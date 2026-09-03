//! The public telemetry contract — `aria-telemetry-query-v1` and its nested
//! `aria-graph-ipo-v1` graph object.
//!
//! # Why this lives in `aria-backends` and not `aria-core`
//!
//! `aria-core` is the sealed state machine: `Aria.tla` / `AriaV3.tla` are its
//! authority and Inv1–Inv4 are statements about *its* variables. The envelope
//! is a **readout contract** — 𝔸5/𝕃5 put readout maps strictly outside Φ, next
//! to `readout.rs` and `tokenizer.rs`. Keeping product schema out of the
//! formally verified crate is what makes the boundary law (L2) checkable by
//! grep rather than by argument: no symbol defined here is reachable from
//! `Engine::apply`.
//!
//! # The reduction
//!
//! ```text
//! Node(payload, config, seed) = E( I(payload, config), Run_Φ(config, seed), Obs )
//! ```
//!
//! This module is `E`'s vocabulary: the types the projection emits, plus the
//! structural validator a host can run against a document it did not produce.
//! Nothing here is a transition.
//!
//! # Determinism
//!
//! Every collection is ordered (`BTreeMap` / explicitly sorted `Vec`), and
//! `serde_json::Map` is a `BTreeMap` in this workspace (no `preserve_order`
//! feature — verified against `Cargo.lock`), so [`canonical_json`] is a total
//! function producing sorted-key bytes. Equal inputs therefore hash equal on
//! every platform.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use aria_engine_core::graph::{EdgeType, Graph, NodeId, NodeType};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Envelope format tag. The only value `schema` may take.
pub const TELEMETRY_QUERY_V1: &str = "aria-telemetry-query-v1";

/// Nested graph-object format tag (issue `#15`).
pub const GRAPH_IPO_V1: &str = "aria-graph-ipo-v1";

/// Envelope version. Bumped only for a breaking wire change.
pub const TELEMETRY_VERSION: u32 = 1;

/// Why a document was rejected. Reject with detail; never coerce.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IpoError {
    /// A required key is absent.
    #[error("{path}: missing required key '{key}'")]
    MissingKey {
        /// JSON pointer-ish location.
        path: String,
        /// The absent key.
        key: String,
    },
    /// A key held the wrong JSON type.
    #[error("{path}: expected {expected}, found {found}")]
    WrongType {
        /// Location.
        path: String,
        /// What the contract requires.
        expected: String,
        /// What was there.
        found: String,
    },
    /// A format tag did not match.
    #[error("{path}: expected format '{expected}', found '{found}'")]
    BadFormat {
        /// Location.
        path: String,
        /// Required tag.
        expected: String,
        /// Supplied tag.
        found: String,
    },
    /// Structural integrity failure (dangling edge, duplicate id, ragged dim).
    #[error("{0}")]
    Integrity(String),
}

/// Whether a graph element came from the host payload or from the transform.
///
/// This discriminator is the reason the envelope can carry Φ-derived anchors
/// without ever presenting them as host facts. An `Input` element was shaped
/// from bytes the host supplied; a `Transform` element is a deterministic
/// anchor for the observed trajectory and carries no prose and no claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeOrigin {
    /// Shaped from the host payload during Init.
    Input,
    /// Created by Φ (a Match-absorbed latent). Deterministic anchor, not a fact.
    Transform,
}

impl NodeOrigin {
    /// Wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            NodeOrigin::Input => "input",
            NodeOrigin::Transform => "transform",
        }
    }
}

/// A node of the exported graph. Mirrors `GraphNode` plus provenance that
/// deliberately does **not** live on the core type (Inv3 stays a statement
/// about 𝒵, not about product fields).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpoNode {
    /// Arena identity, stable within one envelope.
    pub id: NodeId,
    /// Typed role (spec §5.3).
    #[serde(rename = "type")]
    pub node_type: NodeType,
    /// Embedding in 𝒵. f64 always — reduced precision is view-only.
    pub embedding: Vec<f64>,
    /// Discrete clock value when the node was last written.
    pub timestamp: u64,
    /// Host data or transform anchor.
    pub origin: NodeOrigin,
    /// Host research-binary kind this element routes to, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_type: Option<String>,
}

/// A typed directed edge of the exported graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpoEdge {
    /// Source node id.
    pub from: NodeId,
    /// Target node id.
    pub to: NodeId,
    /// Typed relation.
    #[serde(rename = "type")]
    pub edge_type: EdgeType,
    /// Host data or transform anchor.
    pub origin: NodeOrigin,
    /// Host research-binary kind, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_type: Option<String>,
}

/// `aria-graph-ipo-v1` — the typed graph a graphics host can render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphIpo {
    /// Always [`GRAPH_IPO_V1`].
    pub schema: String,
    /// Nodes in ascending id order.
    pub nodes: Vec<IpoNode>,
    /// Edges in ascending `(from, to, type)` order.
    pub edges: Vec<IpoEdge>,
}

/// Which graph elements came from the host payload.
///
/// A live `Graph` cannot answer this on its own: a node inserted at Init and a
/// node absorbed by Match are the same shape by design (Inv3 is a statement
/// about 𝒵, not about provenance). Ingest is the only layer that knows, so it
/// records the answer here and the projection reads it.
///
/// [`Self::default()`] marks everything [`NodeOrigin::Transform`] — the honest
/// answer for an export with no payload behind it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OriginIndex {
    /// Node ids shaped from the payload during Init.
    pub input_nodes: std::collections::BTreeSet<NodeId>,
    /// Directed endpoint pairs supplied by the host.
    pub input_edges: std::collections::BTreeSet<(NodeId, NodeId)>,
}

impl OriginIndex {
    /// Provenance of a node id.
    pub fn node(&self, id: NodeId) -> NodeOrigin {
        if self.input_nodes.contains(&id) {
            NodeOrigin::Input
        } else {
            NodeOrigin::Transform
        }
    }

    /// Provenance of a directed edge.
    pub fn edge(&self, from: NodeId, to: NodeId) -> NodeOrigin {
        if self.input_edges.contains(&(from, to)) {
            NodeOrigin::Input
        } else {
            NodeOrigin::Transform
        }
    }
}

impl GraphIpo {
    /// Project a live `Graph` into the wire contract.
    ///
    /// Ordering follows the graph's own `BTreeMap`/`BTreeSet` iteration, so the
    /// projection is deterministic without an extra sort.
    pub fn from_graph(g: &Graph, origins: &OriginIndex) -> Self {
        let nodes = g
            .nodes
            .values()
            .map(|n| IpoNode {
                id: n.id,
                node_type: n.node_type.clone(),
                embedding: n.embedding.clone(),
                timestamp: n.timestamp,
                origin: origins.node(n.id),
                binary_type: Some(binary_type_for_node(&n.node_type)),
            })
            .collect();
        let edges = g
            .edges
            .iter()
            .map(|e| IpoEdge {
                from: e.from,
                to: e.to,
                edge_type: e.edge_type.clone(),
                origin: origins.edge(e.from, e.to),
                binary_type: Some(binary_type_for_edge(&e.edge_type)),
            })
            .collect();
        GraphIpo {
            schema: GRAPH_IPO_V1.to_string(),
            nodes,
            edges,
        }
    }

    /// `|V|`.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// `|E|`.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

/// One ingested host identity, preserved in full.
///
/// This is where "remove nothing" is honored: `label`, `notes`, and every key
/// the transform does not understand survive here rather than being dropped
/// when the embedding is computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeRecord {
    /// The graph id this record shaped.
    pub id: NodeId,
    /// The host's own identity value, verbatim, when one was supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<Value>,
    /// Display label, when one was supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Free text the embedding was computed from, unmodified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Every remaining host key, preserved exactly.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub properties: Map<String, Value>,
    /// Host research-binary kind, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_type: Option<String>,
    /// SHA-256 (hex) of this record's canonical JSON — the free anchor.
    pub anchor: String,
}

/// `match` clause. `"*"` means "all"; type filters arrive in a later version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryMatch {
    /// Node selector.
    pub nodes: String,
    /// Edge selector.
    pub edges: String,
}

impl Default for QueryMatch {
    fn default() -> Self {
        QueryMatch {
            nodes: "*".into(),
            edges: "*".into(),
        }
    }
}

/// `where` clause. Empty vectors mean "no filter", never "match nothing".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryWhere {
    /// Restrict to these edge type wire names. Empty = all.
    #[serde(default)]
    pub edge_types: Vec<String>,
    /// Restrict to these binary types. Empty = all.
    #[serde(default)]
    pub binary_types: Vec<String>,
    /// The merge radius τ the run used. Reported, never re-applied to Φ.
    pub tau: f64,
    /// `true` = `graph` is full G; `false` = `graph` is the pruned view.
    pub include_full_graph: bool,
}

impl Default for QueryWhere {
    fn default() -> Self {
        QueryWhere {
            edge_types: Vec::new(),
            binary_types: Vec::new(),
            tau: 0.5,
            include_full_graph: true,
        }
    }
}

/// The structured query a host can store and re-issue. Not openCypher.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryQuery {
    /// Selection.
    #[serde(rename = "match")]
    pub match_clause: QueryMatch,
    /// Filters.
    #[serde(rename = "where")]
    pub where_clause: QueryWhere,
    /// Which envelope keys to include. `query` and `receipt` are always present.
    #[serde(rename = "return")]
    pub return_keys: Vec<String>,
}

impl Default for TelemetryQuery {
    fn default() -> Self {
        TelemetryQuery {
            match_clause: QueryMatch::default(),
            where_clause: QueryWhere::default(),
            return_keys: [
                "query", "graph", "records", "source", "structure", "tags", "ledger", "receipt",
            ]
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        }
    }
}

/// The structural role a column plays, derived from counted facts only.
///
/// No column name, no cell semantics, and no learned weight participates.
/// That is what makes the assignment permutation- and rename-invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnRole {
    /// Present on every row and distinct on every row — a candidate key.
    KeyAnchor,
    /// Distinct wherever present, but with gaps.
    NearKeyAnchor,
    /// Low cardinality, repeated — becomes a shared node; the relation source.
    Facet,
    /// High cardinality but not unique — stays a record property.
    FreeAttribute,
    /// Exactly one distinct value.
    Constant,
    /// Never present.
    Empty,
}

impl ColumnRole {
    /// Wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            ColumnRole::KeyAnchor => "key_anchor",
            ColumnRole::NearKeyAnchor => "near_key_anchor",
            ColumnRole::Facet => "facet",
            ColumnRole::FreeAttribute => "free_attribute",
            ColumnRole::Constant => "constant",
            ColumnRole::Empty => "empty",
        }
    }

    /// Whether this role contributes graph structure (identity or facet nodes).
    pub fn is_structural(self) -> bool {
        matches!(
            self,
            ColumnRole::KeyAnchor | ColumnRole::NearKeyAnchor | ColumnRole::Facet
        )
    }
}

/// Thresholds the role law consulted. Emitted so a host can recompute.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoleThresholds {
    /// Minimum coverage for [`ColumnRole::NearKeyAnchor`].
    pub near_key_coverage: f64,
    /// Absolute cap on distinct values for [`ColumnRole::Facet`].
    pub facet_max_distinct: usize,
    /// Relative cap: `distinct ≤ facet_max_ratio · n_rows`.
    pub facet_max_ratio: f64,
}

impl Default for RoleThresholds {
    fn default() -> Self {
        RoleThresholds {
            near_key_coverage: 0.90,
            facet_max_distinct: 64,
            facet_max_ratio: 0.5,
        }
    }
}

/// Per-column measurement plus the rule that fired — the `explain` block.
///
/// Every field is a count or a ratio of counts. A host can recompute all of
/// them from `source` and falsify the role. That self-refutation property is
/// the difference between telemetry and judgement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnStat {
    /// Column key as it appeared in the payload.
    pub column: String,
    /// Assigned role.
    pub role: ColumnRole,
    /// The predicate that selected the role, as text.
    pub rule: String,
    /// Rows considered.
    pub n_rows: usize,
    /// Rows where the column was non-null and non-empty.
    pub present: usize,
    /// Distinct canonical values among present rows.
    pub distinct: usize,
    /// `present / n_rows`.
    pub coverage: f64,
    /// `distinct / present`, or 0 when `present == 0`.
    pub uniqueness: f64,
    /// Distinct values occurring exactly once among present rows.
    pub singletons: usize,
}

/// A measured functional dependency `from → to`: every distinct `from` value
/// maps to exactly one `to` value wherever both are present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalDep {
    /// Determinant column.
    pub from: String,
    /// Dependent column.
    pub to: String,
    /// Distinct values of `from` that participated.
    pub distinct_from: usize,
    /// Distinct values of `to` that participated.
    pub distinct_to: usize,
    /// Rows where both columns were present.
    pub support: usize,
}

/// Everything the deterministic structure pass measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureReport {
    /// Rows the pass saw.
    pub n_rows: usize,
    /// One entry per column, in ascending column-key order.
    pub columns: Vec<ColumnStat>,
    /// Measured dependencies, in ascending `(from, to)` order.
    pub functional_deps: Vec<FunctionalDep>,
    /// Thresholds in force.
    pub thresholds: RoleThresholds,
    /// Whether the dependency scan ran to completion or hit its bound.
    pub dependency_scan_complete: bool,
}

/// A cluster of the map view. View-only; never a Φ mutation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cluster {
    /// Stable index within this envelope.
    pub id: usize,
    /// Human-readable label from the decomposition.
    pub label: String,
    /// Member node ids, ascending.
    pub node_ids: Vec<NodeId>,
    /// Algebraic connectivity λ₂ of the sub-cluster.
    pub connectivity: f64,
}

/// The probable, pruned, natural tagging of the whole map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaggingState {
    /// Match policy the run used.
    pub policy: String,
    /// Merge radius τ.
    pub tau: f64,
    /// Whether self-loops and duplicate morphs were dropped from the view.
    pub pruned: bool,
    /// Cluster decomposition. `[]` when not computed — never `null`.
    pub clusters: Vec<Cluster>,
    /// Probable edges: host edges plus τ-near pairs, re-checked in f64.
    pub probable_edges: Vec<IpoEdge>,
    /// Relation type → binary types present. The inverted routing index.
    pub binary_index: BTreeMap<String, Vec<String>>,
}

/// Deterministic resource ceilings. A serverless invocation of untrusted JSON
/// must be bounded before allocation, not discovered to be unbounded later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// Maximum accepted payload size in bytes.
    pub max_input_bytes: usize,
    /// Maximum host nodes (rows + facets) admitted to `G₀`.
    pub max_nodes: usize,
    /// Maximum host edges admitted to `G₀`.
    pub max_edges: usize,
    /// Maximum Φ steps.
    pub max_steps: u64,
    /// Maximum column pairs the dependency scan may test.
    pub max_dependency_pairs: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_input_bytes: 32 * 1024 * 1024,
            max_nodes: 65_536,
            max_edges: 262_144,
            max_steps: 262_144,
            max_dependency_pairs: 4_096,
        }
    }
}

/// What the run measured. Reports invariants; never Trust, never a score,
/// never a Goal disposition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryReceipt {
    /// Inv1–Inv4 on the final state.
    pub invariants_ok: bool,
    /// Human-readable failures; empty when `invariants_ok`.
    pub failures: Vec<String>,
    /// Scheduler steps executed.
    pub steps: u64,
    /// Final discrete clock.
    pub t: u64,
    /// Final `|V|`.
    pub node_count: usize,
    /// Final `|E|`.
    pub edge_count: usize,
    /// Nodes shaped from the payload.
    pub input_node_count: usize,
    /// Nodes Φ created.
    pub transform_node_count: usize,
    /// Final `‖ψ‖₂`.
    pub energy: f64,
    /// Final JEPA residual.
    pub residual: f64,
    /// `"sim"` or `"trained"`.
    pub predictor: String,
    /// Match policy wire name.
    pub match_policy: String,
    /// Seed the run used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Optical modes N.
    ///
    /// This and the three fields below exist so the OCID commitment is fully
    /// recomputable from the document alone. Without them a host could check
    /// that the payload and the graph match, but not that the configuration
    /// digest the commitment binds is the one that produced them.
    pub n_modes: usize,
    /// Latent dimension dim(Z).
    pub latent_dim: usize,
    /// Contractivity tolerance ε.
    pub eps: f64,
    /// Schedule string the scheduler ran.
    pub schedule: String,
    /// Ceilings in force for this invocation.
    pub limits: Limits,
}

/// `aria-telemetry-query-v1` — the guaranteed body.
///
/// On success this object, and only this object, reaches the primary sink.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryEnvelope {
    /// Always [`TELEMETRY_QUERY_V1`].
    pub schema: String,
    /// Always [`TELEMETRY_VERSION`].
    pub version: u32,
    /// The normalized structured query. Always present.
    pub query: TelemetryQuery,
    /// The typed graph.
    pub graph: GraphIpo,
    /// Ingested host identities, keyed by graph id.
    pub records: BTreeMap<NodeId, NodeRecord>,
    /// The complete parsed payload.
    pub source: Value,
    /// SHA-256 (hex) of the exact input bytes.
    pub source_sha256: String,
    /// Deterministic structure measurements, when the payload was tabular.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<StructureReport>,
    /// The map view.
    pub tags: TaggingState,
    /// Passive observer ledger, when requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger: Option<Value>,
    /// Invariant and configuration receipt. Always present.
    pub receipt: TelemetryReceipt,
    /// Observation Commitment IDentifier, when the host asked for one.
    ///
    /// Binds payload, configuration, and output under one recomputable hash,
    /// optionally anchored to a verified Ed25519 public key. See
    /// [`crate::ocid`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocid: Option<crate::ocid::Ocid>,
}

// ---------------------------------------------------------------------------
// Canonical bytes and anchors
// ---------------------------------------------------------------------------

/// Canonical JSON bytes: sorted keys, no insignificant whitespace.
///
/// Total for `Value`: `serde_json::Number` cannot hold NaN or ±∞, and this
/// workspace's `serde_json` has no `preserve_order` feature, so `Map` is a
/// `BTreeMap` and key order is the sort order.
pub fn canonical_json(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serializing serde_json::Value is total")
}

/// Lowercase hex SHA-256, using the in-repo FIPS 180-4 implementation.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = crate::observer::sha256(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// The free anchor of any JSON value: SHA-256 over its canonical bytes.
///
/// Identity and provenance only. An anchor is never a semantic label and is
/// never evidence about host truth.
pub fn anchor_of(value: &Value) -> String {
    sha256_hex(&canonical_json(value))
}

// ---------------------------------------------------------------------------
// Binary-type registry (deterministic table, no classifier)
// ---------------------------------------------------------------------------

/// Fallback binary type for an unrecognized `Custom` label.
pub const BINARY_TYPE_CUSTOM: &str = "pcvc.research.custom";

/// Built-in binary type for a node role.
///
/// Resolution order at the call site is: host-supplied value, then a `Custom`
/// label already containing a `.` (treated as qualified), then this table.
pub fn binary_type_for_node(t: &NodeType) -> String {
    match t {
        NodeType::Observation | NodeType::Hypothesis => "pcvc.research.json-map".into(),
        NodeType::Action | NodeType::Goal => "pcvc.research.agent-trace".into(),
        NodeType::InvariantCheckpoint => "pcvc.research.receipt".into(),
        NodeType::Custom(s) => qualified_or_custom(s),
    }
}

/// Built-in binary type for an edge relation.
pub fn binary_type_for_edge(t: &EdgeType) -> String {
    match t {
        EdgeType::CausallyPrecedes | EdgeType::Refines | EdgeType::Contradicts => {
            "pcvc.research.json-map".into()
        }
        EdgeType::TemporalNext => "pcvc.research.agent-trace".into(),
        EdgeType::Custom(s) => qualified_or_custom(s),
    }
}

/// A `Custom` label containing a `.` is already qualified; anything else falls
/// back to [`BINARY_TYPE_CUSTOM`].
fn qualified_or_custom(label: &str) -> String {
    if label.contains('.') {
        label.to_string()
    } else {
        BINARY_TYPE_CUSTOM.to_string()
    }
}

// ---------------------------------------------------------------------------
// Structural validator
// ---------------------------------------------------------------------------

/// Validate a document against `aria-telemetry-query-v1` structurally.
///
/// This is the dependency-free check a host runs on a document it did not
/// produce. It is deliberately not a full JSON Schema engine: taking a runtime
/// schema crate would breach the minimal-dependency doctrine and the wasm32
/// lock for no added assurance on a contract this narrow. The tracked schema
/// files under `schemas/` are the human-readable statement of the same rules.
pub fn validate_envelope(doc: &Value) -> Result<(), IpoError> {
    let root = obj(doc, "$")?;
    expect_format(root, "$", "schema", TELEMETRY_QUERY_V1)?;

    match root.get("version").and_then(Value::as_u64) {
        Some(v) if v == u64::from(TELEMETRY_VERSION) => {}
        Some(v) => {
            return Err(IpoError::Integrity(format!(
                "$.version: expected {TELEMETRY_VERSION}, found {v}"
            )))
        }
        None => return Err(missing("$", "version")),
    }

    for key in ["query", "graph", "records", "source", "source_sha256", "tags", "receipt"] {
        if !root.contains_key(key) {
            return Err(missing("$", key));
        }
    }

    validate_query(root.get("query").unwrap_or(&Value::Null))?;
    let ids = validate_graph(root.get("graph").unwrap_or(&Value::Null))?;
    validate_records(root.get("records").unwrap_or(&Value::Null))?;
    validate_receipt(root.get("receipt").unwrap_or(&Value::Null))?;
    validate_tags(root.get("tags").unwrap_or(&Value::Null), &ids)?;

    let hash = root
        .get("source_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| wrong("$.source_sha256", "a hex string", "non-string"))?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(IpoError::Integrity(
            "$.source_sha256 must be 64 lowercase hex digits".into(),
        ));
    }

    if let Some(structure) = root.get("structure") {
        if !structure.is_null() {
            validate_structure(structure)?;
        }
    }
    Ok(())
}

fn validate_query(v: &Value) -> Result<(), IpoError> {
    let q = obj(v, "$.query")?;
    let m = obj(q.get("match").unwrap_or(&Value::Null), "$.query.match")?;
    for key in ["nodes", "edges"] {
        if !m.get(key).is_some_and(Value::is_string) {
            return Err(wrong(
                &format!("$.query.match.{key}"),
                "a string selector",
                "missing or non-string",
            ));
        }
    }
    let w = obj(q.get("where").unwrap_or(&Value::Null), "$.query.where")?;
    if !w.get("tau").is_some_and(Value::is_number) {
        return Err(wrong("$.query.where.tau", "a number", "missing or non-number"));
    }
    if !w.get("include_full_graph").is_some_and(Value::is_boolean) {
        return Err(wrong(
            "$.query.where.include_full_graph",
            "a boolean",
            "missing or non-boolean",
        ));
    }
    if !q.get("return").is_some_and(Value::is_array) {
        return Err(wrong("$.query.return", "an array", "missing or non-array"));
    }
    Ok(())
}

/// Validates the graph object and returns the node id set for edge checking.
fn validate_graph(v: &Value) -> Result<std::collections::BTreeSet<u64>, IpoError> {
    let g = obj(v, "$.graph")?;
    expect_format(g, "$.graph", "schema", GRAPH_IPO_V1)?;

    let nodes = g
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| wrong("$.graph.nodes", "an array", "missing or non-array"))?;

    let mut ids = std::collections::BTreeSet::new();
    let mut dim: Option<usize> = None;
    for (i, node) in nodes.iter().enumerate() {
        let path = format!("$.graph.nodes[{i}]");
        let n = obj(node, &path)?;
        let id = n
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| wrong(&path, "an unsigned integer 'id'", "missing or non-integer"))?;
        if !ids.insert(id) {
            return Err(IpoError::Integrity(format!("{path}: duplicate node id {id}")));
        }
        if !n.get("type").is_some_and(Value::is_string) {
            return Err(wrong(&path, "a string 'type'", "missing or non-string"));
        }
        if !n.get("timestamp").is_some_and(Value::is_u64) {
            return Err(wrong(&path, "an unsigned 'timestamp'", "missing or invalid"));
        }
        match n.get("origin").and_then(Value::as_str) {
            Some("input" | "transform") => {}
            _ => {
                return Err(wrong(
                    &path,
                    "'origin' of \"input\" or \"transform\"",
                    "missing or unknown",
                ))
            }
        }
        let emb = n
            .get("embedding")
            .and_then(Value::as_array)
            .ok_or_else(|| wrong(&path, "an 'embedding' array", "missing or non-array"))?;
        for (j, component) in emb.iter().enumerate() {
            let f = component.as_f64().ok_or_else(|| {
                wrong(&format!("{path}.embedding[{j}]"), "a finite number", "non-number")
            })?;
            if !f.is_finite() {
                return Err(IpoError::Integrity(format!(
                    "{path}.embedding[{j}] is not finite — not a point of 𝒵"
                )));
            }
        }
        match dim {
            None => dim = Some(emb.len()),
            Some(d) if d == emb.len() => {}
            Some(d) => {
                return Err(IpoError::Integrity(format!(
                    "{path}: embedding dim {} disagrees with {d} — ragged 𝒵",
                    emb.len()
                )))
            }
        }
    }

    let edges = g
        .get("edges")
        .and_then(Value::as_array)
        .ok_or_else(|| wrong("$.graph.edges", "an array", "missing or non-array"))?;
    for (i, edge) in edges.iter().enumerate() {
        let path = format!("$.graph.edges[{i}]");
        let e = obj(edge, &path)?;
        for key in ["from", "to"] {
            let endpoint = e
                .get(key)
                .and_then(Value::as_u64)
                .ok_or_else(|| wrong(&path, &format!("an unsigned '{key}'"), "missing or invalid"))?;
            if !ids.contains(&endpoint) {
                return Err(IpoError::Integrity(format!(
                    "{path}: '{key}' = {endpoint} is a dangling endpoint"
                )));
            }
        }
        if !e.get("type").is_some_and(Value::is_string) {
            return Err(wrong(&path, "a string 'type'", "missing or non-string"));
        }
        match e.get("origin").and_then(Value::as_str) {
            Some("input" | "transform") => {}
            _ => {
                return Err(wrong(
                    &path,
                    "'origin' of \"input\" or \"transform\"",
                    "missing or unknown",
                ))
            }
        }
    }
    Ok(ids)
}

fn validate_records(v: &Value) -> Result<(), IpoError> {
    let records = obj(v, "$.records")?;
    for (key, rec) in records {
        let path = format!("$.records[{key}]");
        if key.parse::<u64>().is_err() {
            return Err(IpoError::Integrity(format!(
                "{path}: record keys must be decimal node ids"
            )));
        }
        let r = obj(rec, &path)?;
        if !r.get("id").is_some_and(Value::is_u64) {
            return Err(wrong(&path, "an unsigned 'id'", "missing or invalid"));
        }
        match r.get("anchor").and_then(Value::as_str) {
            Some(a) if a.len() == 64 && a.bytes().all(|b| b.is_ascii_hexdigit()) => {}
            _ => {
                return Err(wrong(
                    &path,
                    "'anchor' as 64 hex digits",
                    "missing or malformed",
                ))
            }
        }
    }
    Ok(())
}

fn validate_receipt(v: &Value) -> Result<(), IpoError> {
    let r = obj(v, "$.receipt")?;
    if !r.get("invariants_ok").is_some_and(Value::is_boolean) {
        return Err(wrong(
            "$.receipt.invariants_ok",
            "a boolean",
            "missing or non-boolean",
        ));
    }
    for key in [
        "steps",
        "t",
        "node_count",
        "edge_count",
        "input_node_count",
        "transform_node_count",
        // N and dim(Z) are required so the OCID config digest is recomputable
        // from the document alone; without them a commitment could only be
        // partially checked.
        "n_modes",
        "latent_dim",
    ] {
        if !r.get(key).is_some_and(Value::is_u64) {
            return Err(wrong(
                &format!("$.receipt.{key}"),
                "an unsigned integer",
                "missing or invalid",
            ));
        }
    }
    if !r.get("schedule").is_some_and(Value::is_string) {
        return Err(wrong("$.receipt.schedule", "a string", "missing or invalid"));
    }
    if !r.get("failures").is_some_and(Value::is_array) {
        return Err(wrong("$.receipt.failures", "an array", "missing or non-array"));
    }
    if !r.get("limits").is_some_and(Value::is_object) {
        return Err(wrong("$.receipt.limits", "an object", "missing or non-object"));
    }
    Ok(())
}

fn validate_tags(v: &Value, ids: &std::collections::BTreeSet<u64>) -> Result<(), IpoError> {
    let t = obj(v, "$.tags")?;
    for key in ["clusters", "probable_edges"] {
        if !t.get(key).is_some_and(Value::is_array) {
            return Err(wrong(
                &format!("$.tags.{key}"),
                "an array (never null)",
                "missing or non-array",
            ));
        }
    }
    if !t.get("binary_index").is_some_and(Value::is_object) {
        return Err(wrong(
            "$.tags.binary_index",
            "an object",
            "missing or non-object",
        ));
    }
    // Probable edges are a *view over G*: every endpoint must still exist.
    for (i, edge) in t
        .get("probable_edges")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .enumerate()
    {
        let path = format!("$.tags.probable_edges[{i}]");
        let e = obj(edge, &path)?;
        for key in ["from", "to"] {
            let endpoint = e
                .get(key)
                .and_then(Value::as_u64)
                .ok_or_else(|| wrong(&path, &format!("an unsigned '{key}'"), "missing"))?;
            if !ids.contains(&endpoint) {
                return Err(IpoError::Integrity(format!(
                    "{path}: '{key}' = {endpoint} is not a node of $.graph"
                )));
            }
        }
    }
    Ok(())
}

fn validate_structure(v: &Value) -> Result<(), IpoError> {
    let s = obj(v, "$.structure")?;
    if !s.get("n_rows").is_some_and(Value::is_u64) {
        return Err(wrong("$.structure.n_rows", "an unsigned integer", "missing"));
    }
    let columns = s
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| wrong("$.structure.columns", "an array", "missing or non-array"))?;
    for (i, col) in columns.iter().enumerate() {
        let path = format!("$.structure.columns[{i}]");
        let c = obj(col, &path)?;
        for key in ["column", "role", "rule"] {
            if !c.get(key).is_some_and(Value::is_string) {
                return Err(wrong(&path, &format!("a string '{key}'"), "missing"));
            }
        }
        // Every role must be recomputable: the counts that produced it are
        // part of the contract, not optional telemetry decoration.
        for key in ["n_rows", "present", "distinct", "singletons"] {
            if !c.get(key).is_some_and(Value::is_u64) {
                return Err(wrong(&path, &format!("an unsigned '{key}'"), "missing"));
            }
        }
        for key in ["coverage", "uniqueness"] {
            if !c.get(key).is_some_and(Value::is_number) {
                return Err(wrong(&path, &format!("a numeric '{key}'"), "missing"));
            }
        }
    }
    if !s.get("functional_deps").is_some_and(Value::is_array) {
        return Err(wrong(
            "$.structure.functional_deps",
            "an array",
            "missing or non-array",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Validator helpers
// ---------------------------------------------------------------------------

fn obj<'a>(v: &'a Value, path: &str) -> Result<&'a Map<String, Value>, IpoError> {
    v.as_object()
        .ok_or_else(|| wrong(path, "an object", type_name(v)))
}

fn expect_format(
    o: &Map<String, Value>,
    path: &str,
    key: &str,
    want: &str,
) -> Result<(), IpoError> {
    match o.get(key).and_then(Value::as_str) {
        Some(got) if got == want => Ok(()),
        Some(got) => Err(IpoError::BadFormat {
            path: path.to_string(),
            expected: want.to_string(),
            found: got.to_string(),
        }),
        None => Err(missing(path, key)),
    }
}

fn missing(path: &str, key: &str) -> IpoError {
    IpoError::MissingKey {
        path: path.to_string(),
        key: key.to_string(),
    }
}

fn wrong(path: &str, expected: &str, found: &str) -> IpoError {
    IpoError::WrongType {
        path: path.to_string(),
        expected: expected.to_string(),
        found: found.to_string(),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
