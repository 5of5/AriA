//! Run Aria, then project a closed operator document.

use aria_engine_backends::ipo::{canonical_json, sha256_hex, NodeRecord};
use aria_engine_backends::telemetry::{transform, TelemetryRequest};
use aria_engine_core::config::AriaConfig;
use aria_engine_core::error::AriaError;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::envelope::{OperatorEnvelope, OperatorNode, OperatorRel, OperatorSpec};
use crate::index::{norm, GraphIndex};
use crate::{OPERATOR_ENVELOPE_V1, OPERATOR_SCHEMA_VERSION};

/// Options a binary or test may pin. Defaults match `aria node`.
#[derive(Debug, Clone)]
pub struct RunOpts {
    /// Φ steps.
    pub steps: u64,
    /// Seed for byte-determinism.
    pub seed: Option<u64>,
    /// Optical modes. `None` uses `AriaConfig::default()`.
    pub n_modes: Option<usize>,
    /// Latent dim. `None` uses the config default.
    pub latent_dim: Option<usize>,
    /// Test-only 𝒮 escape.
    pub allow_sub_spec_dims: bool,
    /// Bind this plan hash. `None` hashes the payload.
    pub plan_hash: Option<String>,
    /// Coverage key.
    pub requirement_id: Option<String>,
    /// Embed `aria-telemetry-query-v1` under `telemetry` (optional).
    /// Default off: the caller reads the vertical. Workers that need the
    /// spine pass `--telemetry`.
    pub include_telemetry: bool,
}

impl Default for RunOpts {
    fn default() -> Self {
        Self {
            steps: 32,
            seed: Some(1),
            n_modes: None,
            latent_dim: None,
            allow_sub_spec_dims: false,
            plan_hash: None,
            requirement_id: None,
            include_telemetry: false,
        }
    }
}

/// Operator-layer failure. Never a Trust write.
#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
    /// Φ / ingest / config.
    #[error(transparent)]
    Aria(#[from] AriaError),
    /// Spec JSON would not parse.
    #[error("operator spec: {0}")]
    Spec(String),
    /// Envelope serialization.
    #[error("operator json: {0}")]
    Json(#[from] serde_json::Error),
    /// Catalog lookup missed.
    #[error("unknown binary: {0}")]
    UnknownBinary(String),
}

impl OperatorError {
    /// Exit vocabulary: 1 invariant, 2 config/spec, 3 unused here
    /// (I/O is the CLI's job).
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            OperatorError::Aria(AriaError::InvariantViolation(_)) => 1,
            OperatorError::Aria(AriaError::Io(_)) => 3,
            OperatorError::Aria(_)
            | OperatorError::Spec(_)
            | OperatorError::Json(_)
            | OperatorError::UnknownBinary(_) => 2,
        }
    }
}

/// Run the Aria transform and project this crate's closed operator JSON.
pub fn run_spec(
    spec_json: &str,
    payload: &[u8],
    opts: &RunOpts,
) -> Result<OperatorEnvelope, OperatorError> {
    let spec = parse_spec(spec_json)?;
    run_operator(&spec, payload, opts)
}

/// Gateway entry: run any catalog binary by `BIN.*` id.
pub fn run_binary(
    binary_id: &str,
    payload: &[u8],
    opts: &RunOpts,
) -> Result<OperatorEnvelope, OperatorError> {
    let spec = crate::spec_by_id(binary_id)
        .ok_or_else(|| OperatorError::UnknownBinary(binary_id.to_string()))?;
    run_operator(spec, payload, opts)
}

/// One Φ, N projections. Hosted command lists compile through this.
///
/// This is the velocity path: Aria names operators from `commands()`, the
/// gateway transforms the payload once, each binary remains an independent
/// vertical (B0, B2). HOST identities always return an empty limitation
/// envelope and never enter Φ (B6: host tools are not research operators).
pub fn run_many(
    binary_ids: &[String],
    payload: &[u8],
    opts: &RunOpts,
) -> Result<Vec<OperatorEnvelope>, OperatorError> {
    if binary_ids.is_empty() {
        return Ok(Vec::new());
    }
    let specs: Vec<&OperatorSpec> = binary_ids
        .iter()
        .map(|id| {
            crate::spec_by_id(id).ok_or_else(|| OperatorError::UnknownBinary(id.clone()))
        })
        .collect::<Result<_, _>>()?;

    let research = specs.iter().any(|s| !is_host(s));
    let (nodes, edges, records, telem_value) = if research {
        let telem = transform(telemetry_request(payload, opts))?;
        let telem_value = if opts.include_telemetry {
            Some(serde_json::to_value(&telem)?)
        } else {
            None
        };
        (telem.graph.nodes, telem.graph.edges, telem.records, telem_value)
    } else {
        (Vec::new(), Vec::new(), BTreeMap::new(), None)
    };
    // One indexing pass; every projector below is a lookup (ℙT2).
    let ix = GraphIndex::build(&nodes, &edges, &records);

    let mut out = Vec::with_capacity(specs.len());
    for spec in specs {
        out.push(if is_host(spec) {
            host_envelope(spec, payload, opts)
        } else {
            envelope_from(spec, &ix, telem_value.as_ref(), payload, opts)
        });
    }
    Ok(out)
}

fn telemetry_request(payload: &[u8], opts: &RunOpts) -> TelemetryRequest {
    let mut config = AriaConfig::default();
    if let Some(n) = opts.n_modes {
        config.n_modes = n;
    }
    if let Some(d) = opts.latent_dim {
        config.latent_dim = d;
    }
    config.seed = opts.seed;
    config.allow_sub_spec_dims = opts.allow_sub_spec_dims;
    let ingest = flatten_work_telemetry(payload);
    let mut req = TelemetryRequest::new(ingest);
    req.config = config;
    req.steps = opts.steps;
    req
}

/// Map mixers (and any binary) may ingest already-processed `aria-work-v1`
/// callback JSON. Flatten working verticals into a graph. Original payload
/// bytes stay the plan_hash input — this is a view, not a rewrite.
fn flatten_work_telemetry(payload: &[u8]) -> Vec<u8> {
    let Ok(v) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    if v.get("schema").and_then(Value::as_str) != Some(crate::WORK_V1) {
        return payload.to_vec();
    }
    let results = v.get("results").and_then(Value::as_array);
    let Some(results) = results else {
        return payload.to_vec();
    };
    let mut nodes = Vec::new();
    let mut seen = BTreeSet::new();
    let mut edges = Vec::new();
    for r in results {
        if let Some(arr) = r.get("nodes").and_then(Value::as_array) {
            for n in arr {
                let Some(id) = n.get("id").and_then(Value::as_u64) else { continue };
                if !seen.insert(id) {
                    continue;
                }
                let kind = n
                    .get("kind")
                    .or_else(|| n.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("Observation");
                let mut obj = serde_json::json!({"id": id, "type": kind, "kind": kind});
                if let Some(tags) = n.get("tags") {
                    obj["tags"] = tags.clone();
                }
                if let Some(label) = n.get("label").and_then(Value::as_str) {
                    obj["label"] = json!(label);
                }
                nodes.push(obj);
            }
        }
        if let Some(arr) = r.get("relationships").and_then(Value::as_array) {
            for e in arr {
                let (Some(from), Some(to)) = (
                    e.get("from").and_then(Value::as_u64),
                    e.get("to").and_then(Value::as_u64),
                ) else {
                    continue;
                };
                let ty = e
                    .get("type")
                    .or_else(|| e.get("rel_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("RELATED");
                edges.push(serde_json::json!({"from": from, "to": to, "type": ty}));
            }
        }
    }
    serde_json::to_vec(&serde_json::json!({"nodes": nodes, "edges": edges}))
        .unwrap_or_else(|_| payload.to_vec())
}

fn is_host(spec: &OperatorSpec) -> bool {
    spec.layer.eq_ignore_ascii_case("HOST") || spec.class.eq_ignore_ascii_case("HOST")
}

fn is_transform(spec: &OperatorSpec) -> bool {
    spec.layer.eq_ignore_ascii_case("TRANSFORM") || spec.class.eq_ignore_ascii_case("TRANSFORM")
}

fn is_tag_op(spec: &OperatorSpec) -> bool {
    spec.class.eq_ignore_ascii_case("TAG") || spec.layer.eq_ignore_ascii_case("DEEP_TAG")
}

fn is_refinement(spec: &OperatorSpec) -> bool {
    spec.layer.eq_ignore_ascii_case("REFINEMENT") || spec.class.eq_ignore_ascii_case("REFINEMENT")
}

fn parse_spec(spec_json: &str) -> Result<OperatorSpec, OperatorError> {
    let v: Value = serde_json::from_str(spec_json)
        .map_err(|e| OperatorError::Spec(e.to_string()))?;
    OperatorSpec::from_catalog_value(v).map_err(|e| OperatorError::Spec(e.to_string()))
}

fn run_operator(
    spec: &OperatorSpec,
    payload: &[u8],
    opts: &RunOpts,
) -> Result<OperatorEnvelope, OperatorError> {
    if is_host(spec) {
        return Ok(host_envelope(spec, payload, opts));
    }
    let telem = transform(telemetry_request(payload, opts))?;
    let telem_value = if opts.include_telemetry {
        Some(serde_json::to_value(&telem)?)
    } else {
        None
    };
    let ix = GraphIndex::build(&telem.graph.nodes, &telem.graph.edges, &telem.records);
    Ok(envelope_from(spec, &ix, telem_value.as_ref(), payload, opts))
}

fn host_envelope(spec: &OperatorSpec, payload: &[u8], opts: &RunOpts) -> OperatorEnvelope {
    let limitation = if spec.verify {
        "HOST is not a research operator (B6 Observe-first) — empty vertical, no Φ"
    } else {
        "catalog VERIFY=F — operator listed but not product-accepted"
    };
    close_envelope(
        spec,
        Vec::new(),
        Vec::new(),
        Map::new(),
        None,
        payload,
        opts,
        false,
        Some(limitation.into()),
        Vec::new(),
    )
}

fn envelope_from(
    spec: &OperatorSpec,
    ix: &GraphIndex<'_>,
    telem_value: Option<&Value>,
    payload: &[u8],
    opts: &RunOpts,
) -> OperatorEnvelope {
    if is_host(spec) || !spec.verify {
        return host_envelope(spec, payload, opts);
    }
    let (nodes, relationships, properties) = project(spec, ix);

    let truncated = spec.default_limit.is_some_and(|lim| nodes.len() > lim);
    let nodes = match spec.default_limit {
        Some(lim) if nodes.len() > lim => nodes.into_iter().take(lim).collect(),
        _ => nodes,
    };
    let keep: BTreeSet<u64> = nodes.iter().map(|n| n.id).collect();
    let passthrough = is_transform(spec) && spec.pass_through;
    let relationships: Vec<OperatorRel> = if passthrough {
        relationships
    } else {
        relationships
            .into_iter()
            .filter(|r| keep.contains(&r.from) && keep.contains(&r.to))
            .collect()
    };

    let extra = uncast_limitations(spec, &nodes, ix.records);
    close_envelope(
        spec,
        nodes,
        relationships,
        properties,
        telem_value,
        payload,
        opts,
        truncated,
        None,
        extra,
    )
}

fn uncast_limitations(
    spec: &OperatorSpec,
    nodes: &[OperatorNode],
    records: &BTreeMap<u64, NodeRecord>,
) -> Vec<String> {
    if !(spec.layer.eq_ignore_ascii_case("ENTITY") || spec.class.eq_ignore_ascii_case("ENTITY")) {
        return Vec::new();
    }
    if nodes.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for n in nodes {
        let Some(rec) = records.get(&n.id) else { continue };
        for (field, value) in crate::typecast::uncast_fields(rec) {
            out.push(format!("uncast_token: {field}={value}"));
        }
    }
    out.sort();
    out.dedup();
    out.truncate(8);
    out
}

#[allow(clippy::too_many_arguments)] // envelope assembly is a closed struct lowered to args
fn close_envelope(
    spec: &OperatorSpec,
    nodes: Vec<OperatorNode>,
    relationships: Vec<OperatorRel>,
    properties: Map<String, Value>,
    telem_value: Option<&Value>,
    payload: &[u8],
    opts: &RunOpts,
    truncated: bool,
    force_limitation: Option<String>,
    extra_limitations: Vec<String>,
) -> OperatorEnvelope {
    let passthrough = is_transform(spec) && spec.pass_through;
    let (coverage_state, no_finding_reason, mut limitations) = if let Some(lim) = force_limitation {
        ("limitation".to_string(), None, vec![lim])
    } else if nodes.is_empty() && properties.is_empty() && !passthrough {
        (
            "no-finding".to_string(),
            Some(format!(
                "{} declared types not present in the Aria graph (empty is valid)",
                spec.binary_id
            )),
            Vec::new(),
        )
    } else if truncated {
        ("truncation".to_string(), None, Vec::new())
    } else {
        ("proposal".to_string(), None, Vec::new())
    };
    limitations.extend(extra_limitations);

    let payload_obj = serde_json::json!({
        "nodes": nodes,
        "relationships": relationships,
        "properties": properties,
    });
    let content_hash = sha256_hex(&canonical_json(&payload_obj));
    let plan_hash = opts
        .plan_hash
        .clone()
        .unwrap_or_else(|| sha256_hex(payload));
    let requirement_id = opts
        .requirement_id
        .clone()
        .unwrap_or_else(|| "unbound".into());
    let subject_ids: Vec<u64> = nodes.iter().map(|n| n.id).collect();

    OperatorEnvelope {
        schema: OPERATOR_ENVELOPE_V1.into(),
        binary_id: spec.binary_id.clone(),
        operator: spec.operator.clone(),
        schema_version: OPERATOR_SCHEMA_VERSION.into(),
        crate_name: spec.crate_name.clone(),
        plan_hash,
        requirement_id,
        subject_ids,
        result_definition_ref: spec.result_definition_ref.clone(),
        anchor_tags: spec.anchor_tags.clone(),
        neo4j_hit: false,
        nodes,
        relationships,
        properties,
        verify: spec.verify,
        coverage_state,
        no_finding_reason,
        limitations,
        graph: Some(crate::envelope::OperatorGraph::from_spec(spec)),
        content_hash,
        telemetry: telem_value.cloned(),
    }
}

/// Project one operator through the index. Same semantics as the historical
/// linear scan (kind/label/`kind|type` for node types; explicit ∪ cast tags
/// for anchors; edge type for relationships), now O(|matches|) per operator.
/// ENTITY families then union residual/DEEP_TAG verticals (E10) without a
/// second Φ.
#[allow(clippy::too_many_lines)]
fn project(
    spec: &OperatorSpec,
    ix: &GraphIndex<'_>,
) -> (Vec<OperatorNode>, Vec<OperatorRel>, Map<String, Value>) {
    let (mut nodes, relationships, properties) = project_core(spec, ix);
    if spec.layer.eq_ignore_ascii_case("ENTITY") && spec.class.eq_ignore_ascii_case("ENTITY") {
        let mut seen: BTreeSet<u64> = nodes.iter().map(|n| n.id).collect();
        for child in crate::family_residuals(&spec.operator) {
            if child.class.eq_ignore_ascii_case("REL") || child.class.eq_ignore_ascii_case("PROP")
            {
                continue;
            }
            let (extra, _, _) = project_core(child, ix);
            for n in extra {
                if seen.insert(n.id) {
                    nodes.push(n);
                }
            }
        }
        nodes.sort_by_key(|n| n.id);
    }
    (nodes, relationships, properties)
}

fn project_core(
    spec: &OperatorSpec,
    ix: &GraphIndex<'_>,
) -> (Vec<OperatorNode>, Vec<OperatorRel>, Map<String, Value>) {
    let node_of = |i: usize| OperatorNode {
        id: ix.nodes[i].id,
        kind: ix.nodes[i].node_type.as_str().to_string(),
        // Tags ride TAG / DEEP_TAG verticals only — ENTITY envelopes stay
        // inside the 768 B PEOPLE budget (E3). Mixers re-ingest tags from
        // the TAG envelopes on the production callback.
        tags: if is_tag_op(spec) {
            ix.tags_of(i).to_vec()
        } else {
            Vec::new()
        },
    };
    let rel_of = |i: usize| OperatorRel {
        from: ix.edges[i].from,
        to: ix.edges[i].to,
        rel_type: ix.edges[i].edge_type.as_str().to_string(),
    };

    if is_transform(spec) && spec.pass_through {
        let nodes = (0..ix.nodes.len()).map(node_of).collect();
        let rels = (0..ix.edges.len()).map(rel_of).collect();
        return (nodes, rels, Map::new());
    }

    let mut properties = Map::new();
    if let Some(key) = spec.property_key.as_deref() {
        if let Some(v) = ix.first_prop(key) {
            properties.insert(key.to_string(), v.clone());
        }
    }

    // E5: family TAG + DEEP_TAG with no explicit tags and no 00c hits are
    // empty. Residual TAG still matches kind.
    if is_tag_op(spec) && !spec.layer.eq_ignore_ascii_case("RESIDUAL") && !ix.has_tags() {
        return (Vec::new(), Vec::new(), properties);
    }

    let allowed_nodes: Vec<String> = spec.node_types.iter().map(|s| norm(s)).collect();
    let allowed_rels: Vec<String> = spec.relationship_types.iter().map(|s| norm(s)).collect();
    let allowed_tags: Vec<String> = spec.anchor_tags.iter().map(|s| norm(s)).collect();

    let mut cand: BTreeSet<usize> = BTreeSet::new();
    if is_refinement(spec) {
        ix.nodes_by_kindlike(&allowed_nodes, &mut cand);
        ix.nodes_by_tag(&allowed_tags, &mut cand);
    } else if is_tag_op(spec) {
        if spec.layer.eq_ignore_ascii_case("RESIDUAL") {
            // Residual TAG.* organizes by kind or by tag (S5).
            ix.nodes_by_kind(&allowed_tags, &mut cand);
            ix.nodes_by_tag(&allowed_tags, &mut cand);
        } else {
            // Family TAG + DEEP_TAG fire on tag evidence only; role tags fire
            // on anchors that are not their own node types (S1).
            let fire = firing_tags(spec, &allowed_nodes);
            let tags = if fire.is_empty() { &allowed_tags } else { &fire };
            ix.nodes_by_tag(tags, &mut cand);
            if !allowed_nodes.is_empty() {
                cand.retain(|&i| ix.kindlike_hits(i, &allowed_nodes));
            }
        }
    } else if spec.class != "PROP" && spec.class != "REL" && !allowed_nodes.is_empty() {
        ix.nodes_by_kindlike(&allowed_nodes, &mut cand);
    }
    let nodes: Vec<OperatorNode> = cand.iter().map(|&i| node_of(i)).collect();

    let is_rel = spec.class == "REL";
    let keep: HashSet<u64> = if is_rel {
        ix.nodes.iter().map(|n| n.id).collect()
    } else {
        nodes.iter().map(|n| n.id).collect()
    };

    let mut relationships: Vec<OperatorRel> = ix
        .edges_by_rel(&allowed_rels)
        .into_iter()
        .filter(|&i| {
            let e = &ix.edges[i];
            is_rel || (keep.contains(&e.from) && keep.contains(&e.to))
        })
        .map(rel_of)
        .collect();

    // REL operators also list the endpoints they actually used.
    let nodes = if is_rel {
        let used: BTreeSet<usize> = relationships
            .iter()
            .flat_map(|r| [r.from, r.to])
            .filter_map(|id| ix.idx_of(id))
            .collect();
        let nodes: Vec<OperatorNode> = used.into_iter().map(node_of).collect();
        let ids: HashSet<u64> = nodes.iter().map(|n| n.id).collect();
        relationships.retain(|r| ids.contains(&r.from) && ids.contains(&r.to));
        nodes
    } else {
        nodes
    };

    (nodes, relationships, properties)
}

/// Role-tag firing set: anchors that are not the operator's own node types.
/// BUYER anchors BUYER_TAG|PERSON|ACCOUNT with node_types Person|Account → BUYER_TAG.
fn firing_tags(spec: &OperatorSpec, node_types: &[String]) -> Vec<String> {
    spec.anchor_tags
        .iter()
        .map(|t| norm(t))
        .filter(|t| !node_types.contains(t))
        .collect()
}
