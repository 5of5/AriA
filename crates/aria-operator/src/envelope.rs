//! Closed operator document and the frozen
//! catalog row that defines one crate.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::OPERATOR_ENVELOPE_V1;

/// One catalog operator. Frozen into each crate's `spec.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorSpec {
    /// `BIN.*` identity.
    pub binary_id: String,
    /// Operator enum/name on the envelope.
    pub operator: String,
    /// Family / HOST / TRANSFORM / RESIDUAL / DEEP_TAG.
    pub layer: String,
    /// NODE / REL / TAG / PROP / ENTITY / …
    pub class: String,
    /// Parent family or envelope, when declared.
    #[serde(default)]
    pub parent: String,
    /// Catalog crate field (underscored).
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Cargo package name (hyphenated).
    pub package: String,
    /// `aria/people@v1` (or `pcvc/…@dev` for host tools).
    #[serde(default)]
    pub telemetry_fork: String,
    /// Sheet VERIFY T/F.
    pub verify: bool,
    /// Whether a Neo4j template is declared (availability, never Trust).
    pub neo4j_pass: bool,
    /// Result-limit when the sheet gave a number.
    #[serde(default)]
    pub default_limit: Option<usize>,
    /// Node kinds this operator may emit.
    #[serde(default)]
    pub node_types: Vec<String>,
    /// Relationship kinds this operator may emit.
    #[serde(default)]
    pub relationship_types: Vec<String>,
    /// Declared anchor tags.
    #[serde(default)]
    pub anchor_tags: Vec<String>,
    /// Plan result type this operator is allowed to satisfy.
    #[serde(default)]
    pub result_definition_ref: String,
    /// Catalog retrieval step (memory / lookup / lift / …).
    #[serde(default)]
    pub retrieval_step: String,
    /// TRANSFORM and HOST pass the Aria graph through without a type filter.
    #[serde(default)]
    pub pass_through: bool,
    /// PROP operators expose this property key only.
    #[serde(default)]
    pub property_key: Option<String>,
    /// Deep-tag taxonomy block.
    #[serde(default)]
    pub taxonomy: Option<String>,
    /// Residual/deep wave letter.
    #[serde(default)]
    pub wave: Option<String>,
}

impl OperatorSpec {
    /// Parse one catalog / spec.json object.
    pub fn from_catalog_value(v: Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(v)
    }
}

/// One typed node on the operator envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorNode {
    /// Arena id (stable within the envelope).
    pub id: u64,
    /// Declared kind for this operator.
    #[serde(rename = "kind")]
    pub kind: String,
    /// Explicit ∪ 00c-cast tags on this node. Omitted when empty so the
    /// default PEOPLE vertical does not grow. Mixers re-ingest this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// One typed relationship on the operator envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorRel {
    /// Source id.
    pub from: u64,
    /// Target id.
    pub to: u64,
    /// Declared rel type.
    #[serde(rename = "type")]
    pub rel_type: String,
}

/// One anchor token with its catalog weight and wave height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorWeight {
    /// The declared anchor tag (02 column A).
    pub tag: String,
    /// Category weight: # of research binaries declaring this token
    /// (category weight: how many operators declare the token).
    pub weight: u32,
    /// Wave height ladder: A→1, B→2, C→3, D→4; 0 when no wave is declared.
    pub height: u8,
}

/// Grammar position of this operator's return — a deterministic function of
/// the frozen catalog, never of the payload. Carried on the wire so any
/// graph consumer renders the result as-is instead
/// of re-deriving the grammar. "Node vs rel vs property" is `class`; the
/// common/uncommon split is `shape`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorGraph {
    /// Catalog class (NODE / REL / TAG / PROP / ENTITY / …).
    pub class: String,
    /// Catalog layer (ENTITY / RESIDUAL / DEEP_TAG / HOST / TRANSFORM / …).
    pub layer: String,
    /// Category weight: max declaring count across this operator's tokens.
    pub weight: u32,
    /// Height: this operator's own wave on the ladder (unwaved → 0).
    pub height: u8,
    /// `common` when weight ≥ 2, `uncommon` at 1, `isolated` at 0.
    pub shape: String,
    /// Declared anchors with their weights/heights, catalog order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<AnchorWeight>,
}

impl OperatorGraph {
    /// Grammar block for one catalog row; all numbers come from the static
    /// token statistics over the frozen catalog.
    #[must_use]
    pub fn from_spec(spec: &OperatorSpec) -> Self {
        let hit = |t: &str| crate::token_stat(t);
        let weight = spec
            .anchor_tags
            .iter()
            .chain(spec.node_types.iter())
            .chain(spec.relationship_types.iter())
            .map(|t| hit(t).0)
            .chain(spec.property_key.as_deref().map(|k| hit(k).0))
            .max()
            .unwrap_or(0);
        let height = crate::wave_height(spec.wave.as_deref());
        let shape = match weight {
            0 => "isolated",
            1 => "uncommon",
            _ => "common",
        }
        .to_string();
        let anchors = spec
            .anchor_tags
            .iter()
            .map(|t| {
                let (w, h) = hit(t);
                AnchorWeight { tag: t.clone(), weight: w, height: h }
            })
            .collect();
        Self {
            class: spec.class.clone(),
            layer: spec.layer.clone(),
            weight,
            height,
            shape,
            anchors,
        }
    }
}

/// Closed operator JSON. Unknown fields are never written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorEnvelope {
    /// Always [`OPERATOR_ENVELOPE_V1`].
    pub schema: String,
    /// Catalog `BIN.*`.
    pub binary_id: String,
    /// Catalog operator name.
    pub operator: String,
    /// Envelope semver.
    pub schema_version: String,
    /// Catalog crate name.
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Observation Plan bind (hex). Payload hash when unbound.
    pub plan_hash: String,
    /// Coverage key. `"unbound"` when the host did not supply one.
    #[serde(default, skip_serializing_if = "is_unbound")]
    pub requirement_id: String,
    /// Subject ids looked up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_ids: Vec<u64>,
    /// Plan result type.
    #[serde(rename = "resultDefinitionRef")]
    pub result_definition_ref: String,
    /// Tags that justified this return.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchor_tags: Vec<String>,
    /// Neo4j availability only. Never Trust.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub neo4j_hit: bool,
    /// Operator-typed nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<OperatorNode>,
    /// Operator-typed relationships.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<OperatorRel>,
    /// Operator-typed properties (PROP binaries; otherwise empty object).
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub properties: Map<String, Value>,
    /// Sheet VERIFY bit after projection.
    pub verify: bool,
    /// `proposal` | `no-finding` | `limitation` | `truncation` | `failure`.
    pub coverage_state: String,
    /// Required when `nodes` is empty and state is no-finding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_finding_reason: Option<String>,
    /// Distinct from failure.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub limitations: Vec<String>,
    /// Hash of the operator payload (nodes + rels + properties), not notes.
    pub content_hash: String,
    /// Grammar position of this return (weight / height / anchors / shape),
    /// deterministic from the frozen catalog. Graph-first renderers read it;
    /// additive to the envelope's required keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<OperatorGraph>,
    /// Shared Aria spine. Optional. Not an API contract.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<Value>,
}

/// Canonical serialized key order of every envelope (all 560 operators emit
/// this shape; members marked `?` are omitted when empty/none). Workers prune
/// from this list — the shape is the same for every operator, so pruning is
/// one operation, never per-binary (𝔸T6 uniform shape).
pub const ENVELOPE_KEYS: &[(&str, bool)] = &[
    ("schema", true),
    ("binary_id", true),
    ("operator", true),
    ("schema_version", true),
    ("crate", true),
    ("plan_hash", true),
    ("requirement_id", false),
    ("subject_ids", false),
    ("resultDefinitionRef", true),
    ("anchor_tags", false),
    ("neo4j_hit", false),
    ("nodes", false),
    ("relationships", false),
    ("properties", false),
    ("verify", true),
    ("coverage_state", true),
    ("no_finding_reason", false),
    ("limitations", false),
    ("content_hash", true),
    ("graph", false),
    ("telemetry", false),
];

impl OperatorEnvelope {
    /// Format tag constructor helper.
    #[must_use]
    pub fn schema_tag() -> &'static str {
        OPERATOR_ENVELOPE_V1
    }

    /// Working vertical: at least one node, relationship, or property.
    /// Empty no-finding / HOST limitation / empty pass-through are skeletons
    /// and must not ship on the production callback.
    #[must_use]
    pub fn has_working_data(&self) -> bool {
        !self.nodes.is_empty() || !self.relationships.is_empty() || !self.properties.is_empty()
    }
}

fn is_unbound(s: &str) -> bool {
    s == "unbound"
}
