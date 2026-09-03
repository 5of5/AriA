//! T1 — lossless payload ingest. This is `I` in
//! `Node = E ∘ Run_Φ ∘ I`, and it is **Init only**: it produces the `G₀` that
//! `Engine::init` receives and never becomes a transition. There is no sixth
//! action here and no path from this module into `Engine::apply`.
//!
//! # The losslessness contract (L5)
//!
//! Nothing in the payload is dropped. Concretely:
//!
//! - the complete parsed payload is returned in [`Ingested::source`];
//! - SHA-256 of the exact input bytes is returned in [`Ingested::source_sha256`];
//! - `label`, `notes`, the host's own id, and **every key this module does not
//!   understand** survive in [`NodeRecord`];
//! - `GraphNode` is untouched — it stays `{id, embedding, node_type,
//!   timestamp}`, so Inv3 remains a statement about 𝒵 and not about product
//!   fields. That separation is the whole reason the payload can be preserved
//!   without weakening the invariant.
//!
//! The historical bug this replaces: `dev_seed.rs` computed an embedding from
//! `text` and then discarded both `label` and `text`, so a host could not
//! recover what a node had been shaped from (issue `#19`).
//!
//! # Bounded before allocation (L8)
//!
//! Every ceiling in [`Limits`] is checked before the corresponding allocation,
//! and a breach is an `AriaError::Config` — never a partial graph and never an
//! OOM. A serverless invocation of untrusted JSON has to be safe by
//! construction, not by hope.

use std::collections::{BTreeMap, BTreeSet};

use aria_engine_core::engine::Predictor;
use aria_engine_core::error::AriaError;
use aria_engine_core::graph::{EdgeType, Graph, GraphOp, NodeId, NodeType};
use serde_json::{Map, Value};

use crate::data::encode_window;
use crate::ipo::{
    anchor_of, binary_type_for_node, canonical_json, sha256_hex, Limits, NodeRecord, OriginIndex,
    TELEMETRY_QUERY_V1,
};
use crate::predictor::SimPredictor;
use crate::structure::{analyze, distinct_values, present_cell, Row, TabularPlan};

/// Wire keys this module interprets structurally. Everything else on a node
/// object is preserved verbatim in `NodeRecord::properties`.
const RESERVED_NODE_KEYS: [&str; 7] = ["id", "label", "notes", "text", "type", "ntype", "binary_type"];

/// Facet lookup: `(column, canonical value bytes)` → the shared node's id.
type FacetIds = BTreeMap<(String, Vec<u8>), NodeId>;

/// The shape a payload was recognized as. Recorded so the envelope can say
/// how it read the input rather than leaving the host to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadShape {
    /// `{ "nodes": [...], "edges": [...] }` — an explicit typed graph.
    Graph,
    /// `[ {...}, {...} ]` or `{ "rows": [...] }` — a literal spreadsheet.
    Tabular,
    /// `{ "notes": [...] }` — one node per note.
    Notes,
    /// A serialized engine `Graph` (compatibility reader).
    RawGraph,
    /// `aria-dev-seed-v1` (compatibility reader).
    DevSeed,
}

impl PayloadShape {
    /// Wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            PayloadShape::Graph => "graph",
            PayloadShape::Tabular => "tabular",
            PayloadShape::Notes => "notes",
            PayloadShape::RawGraph => "raw_graph",
            PayloadShape::DevSeed => "dev_seed",
        }
    }
}

/// Everything Init produced from the payload.
#[derive(Debug, Clone)]
pub struct Ingested {
    /// `G₀`, already GraphOK at `latent_dim`.
    pub g0: Graph,
    /// One entry per ingested host identity.
    pub records: BTreeMap<NodeId, NodeRecord>,
    /// The complete parsed payload.
    pub source: Value,
    /// SHA-256 (hex) of the exact input bytes.
    pub source_sha256: String,
    /// Which graph elements are host data (feeds `origin` in the export).
    pub origins: OriginIndex,
    /// How the payload was read.
    pub shape: PayloadShape,
    /// Structure measurements, present for [`PayloadShape::Tabular`].
    pub plan: Option<TabularPlan>,
}

/// Parse and shape a payload into `G₀`.
///
/// `bytes` is the exact input; `source_sha256` is taken over it before parsing
/// so the hash is of what the host actually sent, not of a re-serialization.
pub fn ingest(
    bytes: &[u8],
    n_modes: usize,
    latent_dim: usize,
    limits: Limits,
) -> Result<Ingested, AriaError> {
    if bytes.len() > limits.max_input_bytes {
        return Err(AriaError::Config(format!(
            "payload is {} bytes, exceeding max_input_bytes {} — refusing before allocation",
            bytes.len(),
            limits.max_input_bytes
        )));
    }
    if n_modes == 0 || latent_dim == 0 {
        return Err(AriaError::Config(
            "n_modes and latent_dim must be > 0".into(),
        ));
    }

    let source_sha256 = sha256_hex(bytes);
    let parsed: Value = serde_json::from_slice(bytes)
        .map_err(|e| AriaError::Config(format!("payload is not valid JSON: {e}")))?;

    // Re-entry: a prior envelope is unwrapped exactly once, through `.source`.
    // Unwrapping repeatedly would let a chain of runs bury the real payload.
    let source = unwrap_prior_envelope(parsed);

    let mut ctx = IngestCtx::new(n_modes, latent_dim, limits);
    let shape = ctx.dispatch(&source)?;

    if !ctx.g0.ok(latent_dim) {
        return Err(AriaError::Config(
            "ingest produced a graph that fails GraphOK — refusing to start Φ".into(),
        ));
    }

    Ok(Ingested {
        g0: ctx.g0,
        records: ctx.records,
        source,
        source_sha256,
        origins: OriginIndex {
            input_nodes: ctx.input_nodes,
            input_edges: ctx.input_edges,
        },
        shape,
        plan: ctx.plan,
    })
}

/// If the payload is itself an `aria-telemetry-query-v1` document, take its
/// `source`. One level only.
fn unwrap_prior_envelope(v: Value) -> Value {
    let is_envelope = v
        .get("schema")
        .and_then(Value::as_str)
        .is_some_and(|s| s == TELEMETRY_QUERY_V1);
    if is_envelope {
        if let Some(inner) = v.get("source") {
            return inner.clone();
        }
    }
    v
}

/// Accumulator for one ingest. Holds the graph under construction plus the
/// provenance sets the export needs.
struct IngestCtx {
    g0: Graph,
    records: BTreeMap<NodeId, NodeRecord>,
    input_nodes: BTreeSet<NodeId>,
    input_edges: BTreeSet<(NodeId, NodeId)>,
    plan: Option<TabularPlan>,
    predictor: SimPredictor,
    n_modes: usize,
    latent_dim: usize,
    limits: Limits,
    next_id: NodeId,
    /// Canonical bytes of each host-supplied `id` → the graph id it received.
    /// Lets an edge reference a non-integer host id (a ticker, a UUID) without
    /// the host having to know Aria's internal numbering.
    host_id_map: BTreeMap<Vec<u8>, NodeId>,
}

impl IngestCtx {
    fn new(n_modes: usize, latent_dim: usize, limits: Limits) -> Self {
        IngestCtx {
            g0: Graph::empty(),
            records: BTreeMap::new(),
            input_nodes: BTreeSet::new(),
            input_edges: BTreeSet::new(),
            plan: None,
            predictor: SimPredictor::new(n_modes, latent_dim),
            n_modes,
            latent_dim,
            limits,
            next_id: 0,
            host_id_map: BTreeMap::new(),
        }
    }

    /// Recognize the payload shape and build `G₀` accordingly.
    ///
    /// Recognition is structural: it asks what keys exist, never what they
    /// mean. The order tries the most specific shapes first.
    fn dispatch(&mut self, source: &Value) -> Result<PayloadShape, AriaError> {
        // A serialized engine Graph: already-embedded nodes keyed by id.
        if let Some(obj) = source.as_object() {
            if obj.contains_key("nodes") && obj["nodes"].is_object() {
                self.ingest_raw_graph(source)?;
                return Ok(PayloadShape::RawGraph);
            }
            if obj.get("format").and_then(Value::as_str) == Some(crate::dev_seed::DEV_SEED_FORMAT) {
                self.ingest_node_array(
                    obj.get("nodes").and_then(Value::as_array).unwrap_or(&Vec::new()),
                )?;
                self.ingest_edge_array(
                    obj.get("edges").and_then(Value::as_array).unwrap_or(&Vec::new()),
                )?;
                return Ok(PayloadShape::DevSeed);
            }
            if let Some(nodes) = obj.get("nodes").and_then(Value::as_array) {
                self.ingest_node_array(nodes)?;
                self.ingest_edge_array(
                    obj.get("edges").and_then(Value::as_array).unwrap_or(&Vec::new()),
                )?;
                return Ok(PayloadShape::Graph);
            }
            if let Some(notes) = obj.get("notes").and_then(Value::as_array) {
                self.ingest_notes(notes)?;
                return Ok(PayloadShape::Notes);
            }
            if let Some(rows) = obj.get("rows").and_then(Value::as_array) {
                self.ingest_tabular(rows)?;
                return Ok(PayloadShape::Tabular);
            }
        }
        if let Some(rows) = source.as_array() {
            self.ingest_tabular(rows)?;
            return Ok(PayloadShape::Tabular);
        }
        Err(AriaError::Config(
            "unrecognized payload: expected an array of row objects, or an object with \
             'nodes', 'notes', or 'rows'"
                .into(),
        ))
    }

    /// Allocate the next graph id, enforcing the node ceiling.
    fn alloc(&mut self) -> Result<NodeId, AriaError> {
        if self.g0.node_count() >= self.limits.max_nodes {
            return Err(AriaError::Config(format!(
                "payload exceeds max_nodes {} — refusing before allocation",
                self.limits.max_nodes
            )));
        }
        let id = self.next_id;
        self.next_id += 1;
        Ok(id)
    }

    /// Embed text with the same deterministic encoder the engine uses.
    ///
    /// Reuses `data::encode_window` + `SimPredictor::embed` — the exact path
    /// `dev_seed` and the training dataset take, so an ingested node sits in
    /// the same 𝒵 as anything else the engine produces. No training required.
    fn embed(&self, text: &str) -> Vec<f64> {
        let mut window = text.as_bytes().to_vec();
        window.resize(self.n_modes, 0);
        let psi = encode_window(&window[..self.n_modes], self.n_modes);
        self.predictor.embed(&psi)
    }

    /// Commit one typed node plus its record.
    fn push_node(
        &mut self,
        id: NodeId,
        node_type: NodeType,
        embedding: Vec<f64>,
        timestamp: u64,
        record: NodeRecord,
    ) -> Result<(), AriaError> {
        self.g0
            .apply(
                &GraphOp::AddNode {
                    id,
                    ntype: node_type,
                    emb: embedding,
                    ts: timestamp,
                },
                self.latent_dim,
            )
            .map_err(|e| AriaError::Config(format!("node {id}: {e}")))?;
        self.input_nodes.insert(id);
        self.records.insert(id, record);
        Ok(())
    }

    /// Commit one typed edge, enforcing the edge ceiling.
    fn push_edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        etype: EdgeType,
    ) -> Result<(), AriaError> {
        if self.g0.edge_count() >= self.limits.max_edges {
            return Err(AriaError::Config(format!(
                "payload exceeds max_edges {} — refusing before allocation",
                self.limits.max_edges
            )));
        }
        self.g0
            .apply(&GraphOp::AddEdge { from, to, etype }, self.latent_dim)
            .map_err(|e| AriaError::Config(format!("edge {from} -> {to}: {e}")))?;
        self.input_edges.insert((from, to));
        Ok(())
    }

    // -- explicit graph -----------------------------------------------------

    /// `{"nodes": [ {...} ], "edges": [ {...} ]}`.
    ///
    /// Host ids are honored when they are unsigned integers; anything else
    /// (a string sku, a UUID) gets an allocated id while the original value is
    /// preserved in `host_id`, so nothing is lost and nothing is invented.
    fn ingest_node_array(&mut self, nodes: &[Value]) -> Result<(), AriaError> {
        // Reserve the entire host id space *first*. Advancing the allocator
        // node-by-node is not enough: a payload of
        // `[{id:100}, {id:"opaque"}, {id:101}]` would allocate 101 for the
        // opaque node and then collide with the third node's own id. The
        // allocator must start above every host id in the payload, not just
        // above the ones seen so far.
        let host_ceiling = nodes
            .iter()
            .filter_map(|n| n.get("id").and_then(Value::as_u64))
            .max();
        if let Some(max_id) = host_ceiling {
            self.next_id = self.next_id.max(
                max_id
                    .checked_add(1)
                    .ok_or_else(|| AriaError::Config("host id u64::MAX is reserved".into()))?,
            );
        }

        for (i, raw) in nodes.iter().enumerate() {
            let obj = raw.as_object().ok_or_else(|| {
                AriaError::Config(format!("nodes[{i}] must be an object, found {raw}"))
            })?;
            let host_id = obj.get("id").cloned();
            let id = match host_id.as_ref().and_then(Value::as_u64) {
                Some(v) => v,
                None => self.alloc()?,
            };
            if self.g0.nodes.contains_key(&id) {
                return Err(AriaError::Config(format!(
                    "nodes[{i}]: duplicate id {id} — host ids must be unique"
                )));
            }
            if let Some(hid) = &host_id {
                self.host_id_map.insert(canonical_json(hid), id);
            }

            let label = string_field(obj, "label");
            let body = string_field(obj, "notes").or_else(|| string_field(obj, "text"));
            let node_type = obj
                .get("type")
                .or_else(|| obj.get("ntype"))
                .and_then(Value::as_str)
                .map_or(NodeType::Observation, NodeType::from_wire);

            // Embedding source: notes if present, else label, else the whole
            // object's canonical bytes. Always *something* derived from the
            // host's own content — never a random or zero vector.
            let text = body
                .clone()
                .or_else(|| label.clone())
                .unwrap_or_else(|| String::from_utf8_lossy(&canonical_json(raw)).into_owned());
            let embedding = self.embed(&text);

            let record = build_record(id, host_id, label, body, obj, &node_type);
            self.push_node(id, node_type, embedding, 0, record)?;
        }
        Ok(())
    }

    /// `{"edges": [ {"from":…, "to":…, "type":…} ]}`.
    fn ingest_edge_array(&mut self, edges: &[Value]) -> Result<(), AriaError> {
        for (i, raw) in edges.iter().enumerate() {
            let obj = raw.as_object().ok_or_else(|| {
                AriaError::Config(format!("edges[{i}] must be an object, found {raw}"))
            })?;
            let from = self.resolve_endpoint(obj, "from", i)?;
            let to = self.resolve_endpoint(obj, "to", i)?;
            let etype = obj
                .get("type")
                .or_else(|| obj.get("rel"))
                .and_then(Value::as_str)
                .map_or(EdgeType::CausallyPrecedes, EdgeType::from_wire);
            self.push_edge(from, to, etype)?;
        }
        Ok(())
    }

    /// Map an edge endpoint to a graph id, honoring the host id map.
    ///
    /// A dangling endpoint is a hard error *before* Init rather than a dropped
    /// edge: silently discarding a relation the host asserted would violate
    /// losslessness in the one direction that matters most.
    fn resolve_endpoint(
        &self,
        obj: &Map<String, Value>,
        key: &str,
        i: usize,
    ) -> Result<NodeId, AriaError> {
        let raw = obj
            .get(key)
            .ok_or_else(|| AriaError::Config(format!("edges[{i}] missing '{key}'")))?;
        let id = match raw.as_u64() {
            Some(v) => v,
            None => *self
                .host_id_map
                .get(&canonical_json(raw))
                .ok_or_else(|| {
                    AriaError::Config(format!(
                        "edges[{i}]: '{key}' = {raw} does not name any ingested node"
                    ))
                })?,
        };
        if !self.g0.nodes.contains_key(&id) {
            return Err(AriaError::Config(format!(
                "edges[{i}]: '{key}' = {id} is a dangling endpoint"
            )));
        }
        Ok(id)
    }

    // -- notes --------------------------------------------------------------

    /// `{"notes": ["...", {"label":…,"notes":…}]}` — one node per note.
    fn ingest_notes(&mut self, notes: &[Value]) -> Result<(), AriaError> {
        for raw in notes {
            let id = self.alloc()?;
            let (label, text, obj) = match raw {
                Value::String(s) => (None, s.clone(), Map::new()),
                Value::Object(o) => {
                    let label = string_field(o, "label");
                    let text = string_field(o, "notes")
                        .or_else(|| string_field(o, "text"))
                        .or_else(|| label.clone())
                        .unwrap_or_default();
                    (label, text, o.clone())
                }
                other => (
                    None,
                    String::from_utf8_lossy(&canonical_json(other)).into_owned(),
                    Map::new(),
                ),
            };
            let embedding = self.embed(&text);
            let node_type = NodeType::Observation;
            let record = build_record(id, None, label, Some(text), &obj, &node_type);
            self.push_node(id, node_type, embedding, 0, record)?;
        }
        Ok(())
    }

    // -- raw engine graph ---------------------------------------------------

    /// A serialized engine `Graph`: embeddings already exist, so nothing is
    /// recomputed. Records carry only ids — this shape has no host prose.
    fn ingest_raw_graph(&mut self, source: &Value) -> Result<(), AriaError> {
        let g: Graph = serde_json::from_value(source.clone())
            .map_err(|e| AriaError::Config(format!("payload is not a serialized Graph: {e}")))?;
        if g.node_count() > self.limits.max_nodes {
            return Err(AriaError::Config(format!(
                "graph has {} nodes, exceeding max_nodes {}",
                g.node_count(),
                self.limits.max_nodes
            )));
        }
        if g.edge_count() > self.limits.max_edges {
            return Err(AriaError::Config(format!(
                "graph has {} edges, exceeding max_edges {}",
                g.edge_count(),
                self.limits.max_edges
            )));
        }
        for (id, node) in &g.nodes {
            self.input_nodes.insert(*id);
            self.records.insert(
                *id,
                NodeRecord {
                    id: *id,
                    host_id: Some(Value::from(*id)),
                    label: None,
                    notes: None,
                    properties: Map::new(),
                    binary_type: Some(binary_type_for_node(&node.node_type)),
                    anchor: anchor_of(&Value::from(*id)),
                },
            );
        }
        for e in &g.edges {
            self.input_edges.insert((e.from, e.to));
        }
        self.next_id = g.next_id();
        self.g0 = g;
        Ok(())
    }

    // -- tabular ------------------------------------------------------------

    /// A literal spreadsheet. Rows become nodes; facet values become shared
    /// nodes; row→facet edges are the relations; measured functional
    /// dependencies become facet→facet hierarchy edges.
    fn ingest_tabular(&mut self, rows: &[Value]) -> Result<(), AriaError> {
        let parsed: Vec<Row> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                r.as_object().cloned().ok_or_else(|| {
                    AriaError::Config(format!("rows[{i}] must be an object, found {r}"))
                })
            })
            .collect::<Result<_, _>>()?;

        let plan = analyze(
            &parsed,
            crate::ipo::RoleThresholds::default(),
            self.limits.max_dependency_pairs,
        );

        // Four distinct phases, in order: identities, shared values, the
        // relations that connect them, and the hierarchy among the values.
        let row_ids = self.tabular_row_nodes(&parsed, &plan)?;
        let facet_ids = self.tabular_facet_nodes(&parsed, &plan)?;
        self.link_rows_to_facets(&parsed, &plan, &row_ids, &facet_ids)?;
        self.link_facet_hierarchy(&parsed, &plan, &facet_ids)?;

        self.plan = Some(plan);
        Ok(())
    }

    /// One node per row, in canonical content order.
    ///
    /// Content order rather than arrival order is what makes the whole
    /// envelope permutation-invariant: shuffle the sheet and the same rows
    /// receive the same ids.
    fn tabular_row_nodes(
        &mut self,
        parsed: &[Row],
        plan: &TabularPlan,
    ) -> Result<BTreeMap<usize, NodeId>, AriaError> {
        let mut row_ids = BTreeMap::new();
        for &arrival in &plan.canonical_rows {
            let row = &parsed[arrival];
            let id = self.alloc()?;
            row_ids.insert(arrival, id);

            let row_value = Value::Object(row.clone());
            // Identity is the whole row's anchor, and every key-anchor column
            // is reported — no column is crowned by name (Q-2026-08-31-1).
            let host_id = if plan.key_columns.is_empty() {
                None
            } else {
                let mut keys = Map::new();
                for c in &plan.key_columns {
                    if let Some(v) = present_cell(row, c) {
                        keys.insert(c.clone(), v.clone());
                    }
                }
                Some(Value::Object(keys))
            };

            let embedding = self.embed(&String::from_utf8_lossy(&canonical_json(&row_value)));
            let record = NodeRecord {
                id,
                host_id,
                label: None,
                notes: None,
                // The entire row is preserved: nothing is reserved away here,
                // because every cell is host data.
                properties: row.clone(),
                binary_type: Some(binary_type_for_node(&NodeType::Observation)),
                anchor: anchor_of(&row_value),
            };
            self.push_node(id, NodeType::Observation, embedding, 0, record)?;
        }
        Ok(row_ids)
    }

    /// One shared node per distinct value of each facet column.
    fn tabular_facet_nodes(
        &mut self,
        parsed: &[Row],
        plan: &TabularPlan,
    ) -> Result<FacetIds, AriaError> {
        let mut facet_ids: FacetIds = BTreeMap::new();
        for column in &plan.facet_columns {
            for value in distinct_values(parsed, column) {
                let id = self.alloc()?;
                let node_type = NodeType::Custom(column.clone());
                let label = value.as_str().map_or_else(
                    || String::from_utf8_lossy(&canonical_json(&value)).into_owned(),
                    ToString::to_string,
                );
                let embedding = self.embed(&format!("{column}={label}"));

                let mut properties = Map::new();
                properties.insert("column".into(), Value::from(column.clone()));
                properties.insert("value".into(), value.clone());
                let anchor_body = Value::Object(properties.clone());

                let record = NodeRecord {
                    id,
                    host_id: Some(value.clone()),
                    label: Some(label),
                    notes: None,
                    properties,
                    binary_type: Some(binary_type_for_node(&node_type)),
                    anchor: anchor_of(&anchor_body),
                };
                facet_ids.insert((column.clone(), canonical_json(&value)), id);
                self.push_node(id, node_type, embedding, 0, record)?;
            }
        }
        Ok(facet_ids)
    }

    /// Row → facet edges. **This is where relations come from**: two rows that
    /// share a facet value are now two hops apart through that shared node.
    fn link_rows_to_facets(
        &mut self,
        parsed: &[Row],
        plan: &TabularPlan,
        row_ids: &BTreeMap<usize, NodeId>,
        facet_ids: &FacetIds,
    ) -> Result<(), AriaError> {
        for (&arrival, &row_id) in row_ids {
            let row = &parsed[arrival];
            for column in &plan.facet_columns {
                let Some(v) = present_cell(row, column) else {
                    continue;
                };
                if let Some(&facet_id) = facet_ids.get(&(column.clone(), canonical_json(v))) {
                    self.push_edge(row_id, facet_id, EdgeType::Custom(format!("has_{column}")))?;
                }
            }
        }
        Ok(())
    }

    /// Facet → facet hierarchy from the measured dependencies.
    ///
    /// Direction convention, tested: `a → b` means every `a` determines one
    /// `b`, so `b` is the *coarser* class. `EdgeType::Refines` is documented as
    /// "the target refines the source", so the edge runs coarse → fine. For
    /// `region → country` that is `country -> region`: "region refines
    /// country", which is the reading a renderer expects.
    fn link_facet_hierarchy(
        &mut self,
        parsed: &[Row],
        plan: &TabularPlan,
        facet_ids: &FacetIds,
    ) -> Result<(), AriaError> {
        for dep in &plan.report.functional_deps {
            for fine in distinct_values(parsed, &dep.from) {
                let Some(&fine_id) = facet_ids.get(&(dep.from.clone(), canonical_json(&fine)))
                else {
                    continue;
                };
                // The single `to` value this `from` value determines. The
                // dependency held, so the first witness is the only answer.
                let coarse = parsed.iter().find_map(|row| {
                    let a = present_cell(row, &dep.from)?;
                    if canonical_json(a) != canonical_json(&fine) {
                        return None;
                    }
                    present_cell(row, &dep.to).cloned()
                });
                let Some(coarse) = coarse else { continue };
                if let Some(&coarse_id) = facet_ids.get(&(dep.to.clone(), canonical_json(&coarse)))
                {
                    self.push_edge(coarse_id, fine_id, EdgeType::Refines)?;
                }
            }
        }
        Ok(())
    }

}

/// Build a record, preserving every non-reserved host key.
///
/// Free function rather than a method: it reads only the payload object, so
/// giving it access to the whole ingest context would be a wider capability
/// than the job needs.
fn build_record(
    id: NodeId,
    host_id: Option<Value>,
    label: Option<String>,
    notes: Option<String>,
    obj: &Map<String, Value>,
    node_type: &NodeType,
) -> NodeRecord {
    let mut properties = Map::new();
    for (k, v) in obj {
        if !RESERVED_NODE_KEYS.contains(&k.as_str()) {
            properties.insert(k.clone(), v.clone());
        }
    }
    let binary_type = obj
        .get("binary_type")
        .and_then(Value::as_str)
        .map_or_else(|| binary_type_for_node(node_type), ToString::to_string);
    NodeRecord {
        id,
        host_id,
        label,
        notes,
        properties,
        binary_type: Some(binary_type),
        anchor: anchor_of(&Value::Object(obj.clone())),
    }
}

/// A non-empty string field, trimmed of nothing (host text is preserved
/// exactly); absent when missing, null, or whitespace-only.
fn string_field(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(ToString::to_string)
}
