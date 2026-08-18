//! Experience/thought graph `G = (V, E, ℳ)` — 𝔸3, and the atomic mutation
//! alphabet ℙ3/𝕃6 that is the only sanctioned way to change it.
//!
//! # v2 (plan WS3)
//!
//! - **Typed** nodes and edges (spec §5.3) instead of optional strings, with
//!   `Custom` escapes so no ingestion format is locked out.
//! - **`u64` ids** from a monotone counter instead of formatted strings: an
//!   arena identity that a vector index can key on directly.
//! - **Deterministic containers.** `BTreeMap`/`BTreeSet` rather than
//!   `HashMap`/`HashSet`. This is not a taste choice: std's hashers are
//!   randomly seeded *per process*, so any policy that indexed
//!   `nodes.keys()` — as `SimGraphBackend`'s `one_edit` did — selected a
//!   different node in different processes. Ordered containers make iteration
//!   order, and therefore every trace, reproducible by construction.
//! - **[`GraphOp`] + [`UndoOp`]**: mutations are atomic ops that record a
//!   snapshot-based undo entry, so a failed invariant check rolls the graph
//!   back exactly (𝕃6) without cloning the whole graph per step, and without
//!   unwinding — `panic = "abort"` is set in release, so rollback must be
//!   explicit data, never `catch_unwind`.
//!
//! Inv3's meaning is unchanged: every edge connects existing nodes and every
//! node carries a finite `d`-dimensional embedding in 𝒵.

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Arena identity of a node. Monotone, never reused within a graph.
pub type NodeId = u64;

/// Node types of the typed experience graph (spec §5.3).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum NodeType {
    /// Something observed — the default for latents absorbed by Match.
    #[default]
    Observation,
    /// A proposed explanation.
    Hypothesis,
    /// An action taken or planned.
    Action,
    /// A goal or target state.
    Goal,
    /// A checkpoint recording that invariants held.
    InvariantCheckpoint,
    /// Any type an ingestion format needs that the spec does not name.
    Custom(String),
}

impl NodeType {
    /// Wire name, and the key of the legacy alias table.
    pub fn as_str(&self) -> &str {
        match self {
            NodeType::Observation => "observation",
            NodeType::Hypothesis => "hypothesis",
            NodeType::Action => "action",
            NodeType::Goal => "goal",
            NodeType::InvariantCheckpoint => "invariant_checkpoint",
            NodeType::Custom(s) => s,
        }
    }

    /// Parse a wire or legacy name.
    ///
    /// # Legacy alias table (plan WS3)
    ///
    /// | stored value | v2 type |
    /// |---|---|
    /// | `"latent"` (v0.1.0 Match) | [`NodeType::Observation`] |
    /// | `"observation"` … `"invariant_checkpoint"` | the named variant |
    /// | anything else | [`NodeType::Custom`] |
    ///
    /// Unknown strings become `Custom` rather than an error: a graph that
    /// round-trips through an older or newer writer must still load, because
    /// refusing to load is a worse failure than carrying an opaque label.
    pub fn from_wire(s: &str) -> Self {
        match s {
            // v0.1.0 wrote every Match-absorbed latent as "latent".
            "latent" | "observation" => NodeType::Observation,
            "hypothesis" => NodeType::Hypothesis,
            "action" => NodeType::Action,
            "goal" => NodeType::Goal,
            "invariant_checkpoint" => NodeType::InvariantCheckpoint,
            other => NodeType::Custom(other.to_string()),
        }
    }
}

/// Edge types of the typed experience graph (spec §5.3).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum EdgeType {
    /// Temporal-causal precedence — the default morphism Match lays down.
    #[default]
    CausallyPrecedes,
    /// The endpoints disagree.
    Contradicts,
    /// The target refines the source.
    Refines,
    /// Plain temporal succession.
    TemporalNext,
    /// Any relation an ingestion format needs that the spec does not name.
    Custom(String),
}

impl EdgeType {
    /// Wire name, and the key of the legacy alias table.
    pub fn as_str(&self) -> &str {
        match self {
            EdgeType::CausallyPrecedes => "causally_precedes",
            EdgeType::Contradicts => "contradicts",
            EdgeType::Refines => "refines",
            EdgeType::TemporalNext => "temporal_next",
            EdgeType::Custom(s) => s,
        }
    }

    /// Parse a wire or legacy name. `"morph"` is v0.1.0's only edge label and
    /// maps to [`EdgeType::CausallyPrecedes`]; unknown strings become `Custom`.
    pub fn from_wire(s: &str) -> Self {
        match s {
            "morph" | "causally_precedes" => EdgeType::CausallyPrecedes,
            "contradicts" => EdgeType::Contradicts,
            "refines" => EdgeType::Refines,
            "temporal_next" => EdgeType::TemporalNext,
            other => EdgeType::Custom(other.to_string()),
        }
    }
}

/// Serialize both type enums as plain strings so the wire format stays
/// human-readable and the legacy alias table above is all that a reader needs.
macro_rules! string_serde {
    ($t:ty, $expecting:literal) => {
        impl Serialize for $t {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl Visitor<'_> for V {
                    type Value = $t;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str($expecting)
                    }
                    fn visit_str<E: de::Error>(self, v: &str) -> Result<$t, E> {
                        Ok(<$t>::from_wire(v))
                    }
                }
                d.deserialize_str(V)
            }
        }
    };
}

string_serde!(NodeType, "a node type name");
string_serde!(EdgeType, "an edge type name");

/// A node of the experience graph: a typed, timestamped point of 𝒵.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    /// Arena identity.
    pub id: NodeId,
    /// Embedding ℳ(v) ∈ 𝒵. Inv3 requires `len == latent_dim`, all finite.
    pub embedding: Vec<f64>,
    /// Typed role (spec §5.3).
    #[serde(default)]
    pub node_type: NodeType,
    /// Discrete clock value `t` when this node was last written.
    #[serde(default)]
    pub timestamp: u64,
}

/// A typed directed edge — a morphism of the experience graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node.
    pub from: NodeId,
    /// Target node.
    pub to: NodeId,
    /// Typed relation.
    pub edge_type: EdgeType,
}

/// Experience/thought graph `G` — 𝔸3. Only Match mutates it (FORMAL_SPEC §6.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    /// Nodes by id. Ordered for reproducible iteration.
    ///
    /// Direct mutation bypasses the [`GraphOp`] journal: use it in tests (to
    /// build corrupt states that Inv3 must reject) and in deserialization,
    /// never in the engine.
    pub nodes: BTreeMap<NodeId, GraphNode>,
    /// Typed edges. Ordered for reproducible iteration; a set, so `AddEdge` is
    /// idempotent by construction (ℙ3).
    pub edges: BTreeSet<GraphEdge>,
    /// Next id to hand out. Monotone: ids are never reused, so a stale index
    /// entry can never be confused with a fresh node.
    #[serde(default)]
    next_id: NodeId,
}

/// Why an atomic graph op was refused *before* any mutation happened (𝕃6).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphOpError {
    /// An op referenced a node that is not in `V`.
    #[error("graph op references missing node {0}")]
    MissingNode(NodeId),
    /// `AddNode` collided with a different existing node.
    #[error("node {0} already exists with different content")]
    DuplicateNode(NodeId),
    /// Embedding dimension violates Inv3.
    #[error("node {id}: embedding has dim {got}, expected {want}")]
    Dim {
        /// Offending node.
        id: NodeId,
        /// Dimension supplied.
        got: usize,
        /// Dimension required (`latent_dim`).
        want: usize,
    },
    /// Embedding contains NaN/±∞ — not a point of 𝒵.
    #[error("node {0}: embedding contains a non-finite component")]
    NonFinite(NodeId),
    /// `MergeNodes` needs two distinct nodes.
    #[error("MergeNodes requires distinct nodes, got {0} twice")]
    SelfMerge(NodeId),
}

/// The atomic mutation alphabet — exactly ℙ3's elementary edits plus the
/// 𝕃3 merge. Every op is idempotent and integrity-checked before commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GraphOp {
    /// Insert a typed node.
    AddNode {
        /// Arena identity (allocate with [`Graph::next_id`]).
        id: NodeId,
        /// Typed role.
        ntype: NodeType,
        /// Embedding ℳ(v).
        emb: Vec<f64>,
        /// Clock value.
        ts: u64,
    },
    /// Remove a node and every incident edge (no dangling edges — 𝕃6).
    DeleteNode {
        /// Node to remove.
        id: NodeId,
    },
    /// Insert a typed edge between existing nodes.
    AddEdge {
        /// Source.
        from: NodeId,
        /// Target.
        to: NodeId,
        /// Relation.
        etype: EdgeType,
    },
    /// Remove a typed edge.
    DeleteEdge {
        /// Source.
        from: NodeId,
        /// Target.
        to: NodeId,
        /// Relation.
        etype: EdgeType,
    },
    /// Replace a node's embedding (ℙ3 relabel).
    RelabelNode {
        /// Node to relabel.
        id: NodeId,
        /// New embedding.
        emb: Vec<f64>,
    },
    /// Absorb `merged` into `keep`: re-point `merged`'s edges onto `keep`,
    /// EMA-update `keep`'s embedding, then delete `merged` (𝕃3, spec §5.3).
    MergeNodes {
        /// Surviving node.
        keep: NodeId,
        /// Absorbed node.
        merged: NodeId,
    },
}

/// The exact pre-image needed to reverse one [`GraphOp`].
///
/// Snapshots, not inverse op names: a delete or a merge cannot be reconstructed
/// from its arguments, so the journal carries the removed node, its incident
/// edges, and the surviving node's overwritten embedding.
#[derive(Debug, Clone, PartialEq)]
pub enum UndoOp {
    /// Reverse of `AddNode`.
    DropNode(NodeId),
    /// Reverse of `DeleteNode`.
    RestoreNode {
        /// The removed node, verbatim.
        node: GraphNode,
        /// Its incident edges, verbatim.
        edges: Vec<GraphEdge>,
    },
    /// Reverse of `AddEdge`.
    DropEdge(GraphEdge),
    /// Reverse of `DeleteEdge`.
    RestoreEdge(GraphEdge),
    /// Reverse of `RelabelNode`.
    RestoreEmbedding {
        /// Node relabelled.
        id: NodeId,
        /// Its previous embedding.
        emb: Vec<f64>,
    },
    /// Reverse of `MergeNodes`.
    Unmerge {
        /// The absorbed node, verbatim.
        merged: GraphNode,
        /// The absorbed node's incident edges, verbatim.
        merged_edges: Vec<GraphEdge>,
        /// The surviving node.
        keep: NodeId,
        /// The surviving node's embedding before the EMA update.
        keep_emb: Vec<f64>,
        /// The surviving node's timestamp before the merge.
        keep_ts: u64,
        /// Edges newly created by re-pointing (only these get removed).
        rewired: Vec<GraphEdge>,
    },
    /// The op was already satisfied; nothing to reverse (idempotency).
    Noop,
}

/// EMA weights for the merge embedding update — spec §5.3's normative Rust
/// uses `*e = 0.9 * (*e) + 0.1 * (*c)`.
const MERGE_EMA_KEEP: f64 = 0.9;
const MERGE_EMA_NEW: f64 = 0.1;

impl Graph {
    /// An empty graph.
    pub fn empty() -> Self {
        Graph {
            nodes: BTreeMap::new(),
            edges: BTreeSet::new(),
            next_id: 0,
        }
    }

    /// A graph holding a single seed node.
    pub fn seed(node: GraphNode) -> Self {
        let mut g = Graph::empty();
        g.next_id = node.id + 1;
        g.nodes.insert(node.id, node);
        g
    }

    /// `|V|`.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// `|E|`.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// `|G| = |V| + |E|`.
    pub fn size(&self) -> usize {
        self.node_count() + self.edge_count()
    }

    /// The next id this graph will hand out. Policies allocate from here so ids
    /// stay monotone no matter who proposes the op.
    pub fn next_id(&self) -> NodeId {
        self.next_id
    }

    /// A node by id.
    pub fn node(&self, id: NodeId) -> Option<&GraphNode> {
        self.nodes.get(&id)
    }

    /// A node's embedding ℳ(v).
    pub fn get_embedding(&self, id: NodeId) -> Option<&Vec<f64>> {
        self.nodes.get(&id).map(|n| &n.embedding)
    }

    /// Edges incident to `id` in either direction.
    pub fn incident(&self, id: NodeId) -> Vec<GraphEdge> {
        self.edges
            .iter()
            .filter(|e| e.from == id || e.to == id)
            .cloned()
            .collect()
    }

    /// `GraphOK(G)` — Inv3.
    ///
    /// Every edge connects existing nodes, and every node carries a finite
    /// `latent_dim`-dimensional embedding. Also checks the map key against the
    /// node's own id, which direct field access could desync.
    pub fn ok(&self, latent_dim: usize) -> bool {
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) || !self.nodes.contains_key(&edge.to) {
                return false;
            }
        }
        for (id, node) in &self.nodes {
            if *id != node.id
                || node.embedding.len() != latent_dim
                || !node.embedding.iter().all(|v| v.is_finite())
            {
                return false;
            }
        }
        true
    }

    /// Whether the directed structure is acyclic (Inv7 candidate gate).
    ///
    /// Kahn's algorithm: a DAG admits a topological order covering every node.
    pub fn is_acyclic(&self) -> bool {
        let mut indegree: BTreeMap<NodeId, usize> = self.nodes.keys().map(|&k| (k, 0)).collect();
        let mut out: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();

        for e in &self.edges {
            // Dangling edges are Inv3's failure mode, not this one.
            if !self.nodes.contains_key(&e.from) {
                continue;
            }
            if let Some(d) = indegree.get_mut(&e.to) {
                *d += 1;
                out.entry(e.from).or_default().push(e.to);
            }
        }

        let mut queue: Vec<NodeId> = indegree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&k, _)| k)
            .collect();

        let mut visited = 0usize;
        while let Some(n) = queue.pop() {
            visited += 1;
            for &m in out.get(&n).into_iter().flatten() {
                if let Some(d) = indegree.get_mut(&m) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(m);
                    }
                }
            }
        }

        visited == self.nodes.len()
    }

    /// Validate an embedding against Inv3 before it can enter `G`.
    fn check_embedding(id: NodeId, emb: &[f64], latent_dim: usize) -> Result<(), GraphOpError> {
        if emb.len() != latent_dim {
            return Err(GraphOpError::Dim {
                id,
                got: emb.len(),
                want: latent_dim,
            });
        }
        if !emb.iter().all(|v| v.is_finite()) {
            return Err(GraphOpError::NonFinite(id));
        }
        Ok(())
    }

    /// Apply one atomic op, returning the entry that reverses it.
    ///
    /// Integrity is checked *before* any mutation, so an `Err` leaves `G`
    /// byte-identical and the returned journal stays a faithful pre-image (𝕃6).
    pub fn apply(&mut self, op: &GraphOp, latent_dim: usize) -> Result<UndoOp, GraphOpError> {
        match op {
            GraphOp::AddNode { id, ntype, emb, ts } => {
                self.apply_add_node(*id, ntype.clone(), emb, *ts, latent_dim)
            }
            GraphOp::DeleteNode { id } => Ok(self.apply_delete_node(*id)),
            GraphOp::AddEdge { from, to, etype } => self.apply_add_edge(*from, *to, etype.clone()),
            GraphOp::DeleteEdge { from, to, etype } => {
                Ok(self.apply_delete_edge(*from, *to, etype.clone()))
            }
            GraphOp::RelabelNode { id, emb } => self.apply_relabel_node(*id, emb, latent_dim),
            GraphOp::MergeNodes { keep, merged } => self.apply_merge_nodes(*keep, *merged),
        }
    }

    fn apply_add_node(
        &mut self,
        id: NodeId,
        ntype: NodeType,
        emb: &[f64],
        ts: u64,
        latent_dim: usize,
    ) -> Result<UndoOp, GraphOpError> {
        Self::check_embedding(id, emb, latent_dim)?;
        let candidate = GraphNode {
            id,
            embedding: emb.to_vec(),
            node_type: ntype,
            timestamp: ts,
        };
        if let Some(existing) = self.nodes.get(&id) {
            // Idempotent when identical; a real collision is an error rather
            // than a silent overwrite.
            return if *existing == candidate {
                Ok(UndoOp::Noop)
            } else {
                Err(GraphOpError::DuplicateNode(id))
            };
        }
        self.nodes.insert(id, candidate);
        self.next_id = self.next_id.max(id.saturating_add(1));
        Ok(UndoOp::DropNode(id))
    }

    fn apply_delete_node(&mut self, id: NodeId) -> UndoOp {
        let Some(node) = self.nodes.remove(&id) else {
            return UndoOp::Noop;
        };
        let edges = self.incident(id);
        for e in &edges {
            self.edges.remove(e);
        }
        UndoOp::RestoreNode { node, edges }
    }

    fn apply_add_edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        etype: EdgeType,
    ) -> Result<UndoOp, GraphOpError> {
        if !self.nodes.contains_key(&from) {
            return Err(GraphOpError::MissingNode(from));
        }
        if !self.nodes.contains_key(&to) {
            return Err(GraphOpError::MissingNode(to));
        }
        let edge = GraphEdge {
            from,
            to,
            edge_type: etype,
        };
        if self.edges.contains(&edge) {
            return Ok(UndoOp::Noop);
        }
        self.edges.insert(edge.clone());
        Ok(UndoOp::DropEdge(edge))
    }

    fn apply_delete_edge(&mut self, from: NodeId, to: NodeId, etype: EdgeType) -> UndoOp {
        let edge = GraphEdge {
            from,
            to,
            edge_type: etype,
        };
        if self.edges.remove(&edge) {
            UndoOp::RestoreEdge(edge)
        } else {
            UndoOp::Noop
        }
    }

    fn apply_relabel_node(
        &mut self,
        id: NodeId,
        emb: &[f64],
        latent_dim: usize,
    ) -> Result<UndoOp, GraphOpError> {
        Self::check_embedding(id, emb, latent_dim)?;
        let Some(node) = self.nodes.get_mut(&id) else {
            return Err(GraphOpError::MissingNode(id));
        };
        if node.embedding == emb {
            return Ok(UndoOp::Noop);
        }
        let previous = std::mem::replace(&mut node.embedding, emb.to_vec());
        Ok(UndoOp::RestoreEmbedding { id, emb: previous })
    }

    fn apply_merge_nodes(&mut self, keep: NodeId, merged: NodeId) -> Result<UndoOp, GraphOpError> {
        if keep == merged {
            return Err(GraphOpError::SelfMerge(keep));
        }
        if !self.nodes.contains_key(&keep) {
            return Err(GraphOpError::MissingNode(keep));
        }
        let Some(merged_node) = self.nodes.get(&merged).cloned() else {
            return Err(GraphOpError::MissingNode(merged));
        };

        let merged_edges = self.incident(merged);
        let keep_node = &self.nodes[&keep];
        let keep_emb = keep_node.embedding.clone();
        let keep_ts = keep_node.timestamp;

        // Re-point every edge of `merged` onto `keep`, dropping the self-loops
        // that re-pointing would create, and recording only the edges that did
        // not already exist so undo removes exactly what the merge added.
        let mut rewired = Vec::new();
        for e in &merged_edges {
            self.edges.remove(e);
            let moved = GraphEdge {
                from: if e.from == merged { keep } else { e.from },
                to: if e.to == merged { keep } else { e.to },
                edge_type: e.edge_type.clone(),
            };
            if moved.from == moved.to {
                continue;
            }
            if self.edges.insert(moved.clone()) {
                rewired.push(moved);
            }
        }

        // EMA embedding update + timestamp refresh (spec §5.3).
        let keep_node = self.nodes.get_mut(&keep).expect("checked above");
        for (e, c) in keep_node
            .embedding
            .iter_mut()
            .zip(merged_node.embedding.iter())
        {
            *e = MERGE_EMA_KEEP * *e + MERGE_EMA_NEW * *c;
        }
        keep_node.timestamp = merged_node.timestamp;
        self.nodes.remove(&merged);

        Ok(UndoOp::Unmerge {
            merged: merged_node,
            merged_edges,
            keep,
            keep_emb,
            keep_ts,
            rewired,
        })
    }

    /// Whether two graphs hold the same `(V, E, ℳ)`, ignoring the id counter.
    ///
    /// This is the equality that 𝕃6 transactionality is about: [`Self::undo`]
    /// restores the *graph*, while [`Self::next_id`] stays monotone on purpose
    /// (see [`Self::undo`]), so `==` — which compares the counter too — is the
    /// wrong predicate for "the rollback worked".
    pub fn same_content(&self, other: &Graph) -> bool {
        self.nodes == other.nodes && self.edges == other.edges
    }

    /// Reverse one journal entry.
    ///
    /// Restores `(V, E, ℳ)` exactly. It deliberately does **not** rewind
    /// [`Self::next_id`]: recycling the id of a rolled-back node would let a
    /// stale auxiliary entry (a vector-index tombstone that a revert missed)
    /// alias a genuinely new node. Monotone allocation makes that class of bug
    /// unrepresentable, and an unused id costs nothing.
    pub fn undo(&mut self, entry: &UndoOp) {
        match entry {
            UndoOp::Noop => {}
            UndoOp::DropNode(id) => {
                self.nodes.remove(id);
                self.edges.retain(|e| e.from != *id && e.to != *id);
            }
            UndoOp::RestoreNode { node, edges } => {
                self.nodes.insert(node.id, node.clone());
                for e in edges {
                    self.edges.insert(e.clone());
                }
            }
            UndoOp::DropEdge(edge) => {
                self.edges.remove(edge);
            }
            UndoOp::RestoreEdge(edge) => {
                self.edges.insert(edge.clone());
            }
            UndoOp::RestoreEmbedding { id, emb } => {
                if let Some(node) = self.nodes.get_mut(id) {
                    node.embedding.clone_from(emb);
                }
            }
            UndoOp::Unmerge {
                merged,
                merged_edges,
                keep,
                keep_emb,
                keep_ts,
                rewired,
            } => {
                for e in rewired {
                    self.edges.remove(e);
                }
                if let Some(node) = self.nodes.get_mut(keep) {
                    node.embedding.clone_from(keep_emb);
                    node.timestamp = *keep_ts;
                }
                self.nodes.insert(merged.id, merged.clone());
                for e in merged_edges {
                    self.edges.insert(e.clone());
                }
            }
        }
    }

    /// Apply a batch atomically: on the first failure every applied op is
    /// reversed and `G` is left byte-identical to its pre-state (𝕃6).
    pub fn apply_ops(
        &mut self,
        ops: &[GraphOp],
        latent_dim: usize,
    ) -> Result<Vec<UndoOp>, GraphOpError> {
        let mut journal = Vec::with_capacity(ops.len());
        for op in ops {
            match self.apply(op, latent_dim) {
                Ok(entry) => journal.push(entry),
                Err(e) => {
                    self.undo_ops(&journal);
                    return Err(e);
                }
            }
        }
        Ok(journal)
    }

    /// Reverse a journal, newest entry first.
    pub fn undo_ops(&mut self, journal: &[UndoOp]) {
        for entry in journal.iter().rev() {
            self.undo(entry);
        }
    }
}

impl Default for Graph {
    fn default() -> Self {
        Graph::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIM: usize = 2;

    fn node(id: NodeId, emb: [f64; DIM]) -> GraphNode {
        GraphNode {
            id,
            embedding: emb.to_vec(),
            node_type: NodeType::Observation,
            timestamp: id,
        }
    }

    fn two_node_graph() -> Graph {
        let mut g = Graph::empty();
        g.apply(
            &GraphOp::AddNode {
                id: 1,
                ntype: NodeType::Observation,
                emb: vec![1.0, 0.0],
                ts: 1,
            },
            DIM,
        )
        .unwrap();
        g.apply(
            &GraphOp::AddNode {
                id: 2,
                ntype: NodeType::Hypothesis,
                emb: vec![0.0, 1.0],
                ts: 2,
            },
            DIM,
        )
        .unwrap();
        g.apply(
            &GraphOp::AddEdge {
                from: 1,
                to: 2,
                etype: EdgeType::CausallyPrecedes,
            },
            DIM,
        )
        .unwrap();
        g
    }

    #[test]
    fn graph_ok_accepts_a_well_formed_graph() {
        let g = two_node_graph();
        assert!(g.ok(DIM));
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.size(), 3);
    }

    #[test]
    fn graph_ok_rejects_wrong_dimension_and_dangling_edges() {
        let g = two_node_graph();
        assert!(!g.ok(DIM + 1), "wrong latent_dim must fail Inv3");

        let mut dangling = two_node_graph();
        dangling.edges.insert(GraphEdge {
            from: 1,
            to: 999,
            edge_type: EdgeType::Refines,
        });
        assert!(!dangling.ok(DIM), "dangling edge must fail Inv3");

        let mut desynced = two_node_graph();
        desynced.nodes.insert(7, node(8, [0.0, 0.0]));
        assert!(!desynced.ok(DIM), "key/id desync must fail Inv3");

        let mut nonfinite = two_node_graph();
        nonfinite.nodes.get_mut(&1).unwrap().embedding[0] = f64::NAN;
        assert!(!nonfinite.ok(DIM), "NaN embedding must fail Inv3");
    }

    #[test]
    fn ops_are_integrity_checked_before_mutating() {
        let mut g = two_node_graph();
        let before = g.clone();

        // Wrong dimension.
        assert!(matches!(
            g.apply(
                &GraphOp::AddNode {
                    id: 3,
                    ntype: NodeType::Action,
                    emb: vec![1.0, 2.0, 3.0],
                    ts: 3
                },
                DIM
            ),
            Err(GraphOpError::Dim {
                id: 3,
                got: 3,
                want: 2
            })
        ));
        // Non-finite embedding.
        assert!(matches!(
            g.apply(
                &GraphOp::AddNode {
                    id: 4,
                    ntype: NodeType::Action,
                    emb: vec![f64::INFINITY, 0.0],
                    ts: 4
                },
                DIM
            ),
            Err(GraphOpError::NonFinite(4))
        ));
        // Edge to a missing endpoint.
        assert!(matches!(
            g.apply(
                &GraphOp::AddEdge {
                    from: 1,
                    to: 42,
                    etype: EdgeType::Refines
                },
                DIM
            ),
            Err(GraphOpError::MissingNode(42))
        ));
        // Relabel of a missing node.
        assert!(matches!(
            g.apply(
                &GraphOp::RelabelNode {
                    id: 42,
                    emb: vec![0.0, 0.0]
                },
                DIM
            ),
            Err(GraphOpError::MissingNode(42))
        ));
        // Self-merge.
        assert!(matches!(
            g.apply(&GraphOp::MergeNodes { keep: 1, merged: 1 }, DIM),
            Err(GraphOpError::SelfMerge(1))
        ));

        assert_eq!(g, before, "a refused op must not mutate the graph");
    }

    #[test]
    fn every_op_round_trips_through_its_undo_entry() {
        let dim = DIM;
        let ops = vec![
            GraphOp::AddNode {
                id: 3,
                ntype: NodeType::Goal,
                emb: vec![0.5, 0.5],
                ts: 3,
            },
            GraphOp::DeleteNode { id: 2 },
            // Endpoints must exist in the fresh two-node fixture each
            // iteration starts from, so this is 1→2 with a *new* type (the
            // existing 1→2 edge is CausallyPrecedes, so this really inserts).
            GraphOp::AddEdge {
                from: 1,
                to: 2,
                etype: EdgeType::TemporalNext,
            },
            GraphOp::DeleteEdge {
                from: 1,
                to: 2,
                etype: EdgeType::CausallyPrecedes,
            },
            GraphOp::RelabelNode {
                id: 1,
                emb: vec![9.0, 9.0],
            },
            GraphOp::MergeNodes { keep: 1, merged: 2 },
        ];

        for op in ops {
            let mut g = two_node_graph();
            let before = g.clone();
            let entry = g.apply(&op, dim).expect("op should apply");
            g.undo(&entry);
            assert!(
                g.same_content(&before),
                "undo of {op:?} did not restore (V, E, ℳ):\n  got  {:?}\n  want {:?}",
                g.nodes,
                before.nodes
            );
            assert!(
                g.next_id() >= before.next_id(),
                "undo must never rewind the id counter"
            );
        }
    }

    #[test]
    fn rollback_does_not_recycle_node_ids() {
        // A rolled-back AddNode must not hand its id to the next node: a stale
        // index tombstone could otherwise alias a genuinely new node.
        let mut g = two_node_graph();
        let id = g.next_id();
        let entry = g
            .apply(
                &GraphOp::AddNode {
                    id,
                    ntype: NodeType::Observation,
                    emb: vec![0.0, 0.0],
                    ts: 0,
                },
                DIM,
            )
            .unwrap();
        g.undo(&entry);
        assert!(!g.nodes.contains_key(&id), "node must be gone after undo");
        assert!(
            g.next_id() > id,
            "id {id} was recycled after rollback (next_id = {})",
            g.next_id()
        );
    }

    #[test]
    fn merge_rewires_edges_and_ema_updates_the_survivor() {
        let mut g = two_node_graph();
        // 3 --Refines--> 2, so the merge must re-point it to 1.
        g.apply(
            &GraphOp::AddNode {
                id: 3,
                ntype: NodeType::Action,
                emb: vec![0.0, 0.0],
                ts: 3,
            },
            DIM,
        )
        .unwrap();
        g.apply(
            &GraphOp::AddEdge {
                from: 3,
                to: 2,
                etype: EdgeType::Refines,
            },
            DIM,
        )
        .unwrap();
        let before = g.clone();

        let entry = g
            .apply(&GraphOp::MergeNodes { keep: 1, merged: 2 }, DIM)
            .unwrap();

        assert!(!g.nodes.contains_key(&2), "merged node must be gone");
        assert!(
            g.edges.contains(&GraphEdge {
                from: 3,
                to: 1,
                edge_type: EdgeType::Refines
            }),
            "edge 3→2 must be re-pointed to 3→1, got {:?}",
            g.edges
        );
        assert!(
            !g.edges.iter().any(|e| e.from == 2 || e.to == 2),
            "no edge may reference the merged node"
        );
        assert!(
            !g.edges.iter().any(|e| e.from == e.to),
            "re-pointing must not create a self-loop"
        );
        // EMA: 0.9·[1,0] + 0.1·[0,1] = [0.9, 0.1]; timestamp takes the merged one.
        let keep = &g.nodes[&1];
        assert!((keep.embedding[0] - 0.9).abs() < 1e-15);
        assert!((keep.embedding[1] - 0.1).abs() < 1e-15);
        assert_eq!(keep.timestamp, 2);
        assert!(g.ok(DIM), "merge must preserve Inv3");

        g.undo(&entry);
        assert!(
            g.same_content(&before),
            "unmerge must restore both pre-images exactly"
        );
    }

    #[test]
    fn ops_are_idempotent() {
        let mut g = two_node_graph();
        let add = GraphOp::AddNode {
            id: 1,
            ntype: NodeType::Observation,
            emb: vec![1.0, 0.0],
            ts: 1,
        };
        assert_eq!(g.apply(&add, DIM).unwrap(), UndoOp::Noop);
        let edge = GraphOp::AddEdge {
            from: 1,
            to: 2,
            etype: EdgeType::CausallyPrecedes,
        };
        assert_eq!(g.apply(&edge, DIM).unwrap(), UndoOp::Noop);
        assert_eq!(
            g.apply(&GraphOp::DeleteNode { id: 77 }, DIM).unwrap(),
            UndoOp::Noop
        );
        assert_eq!(
            g.apply(
                &GraphOp::DeleteEdge {
                    from: 1,
                    to: 2,
                    etype: EdgeType::Refines
                },
                DIM
            )
            .unwrap(),
            UndoOp::Noop
        );
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn duplicate_id_with_different_content_is_refused() {
        let mut g = two_node_graph();
        assert!(matches!(
            g.apply(
                &GraphOp::AddNode {
                    id: 1,
                    ntype: NodeType::Goal,
                    emb: vec![7.0, 7.0],
                    ts: 9
                },
                DIM
            ),
            Err(GraphOpError::DuplicateNode(1))
        ));
    }

    #[test]
    fn delete_node_takes_its_edges_and_restore_brings_them_back() {
        let mut g = two_node_graph();
        let before = g.clone();
        let entry = g.apply(&GraphOp::DeleteNode { id: 2 }, DIM).unwrap();
        assert_eq!(g.edge_count(), 0, "incident edges must go with the node");
        assert!(g.ok(DIM), "no dangling edge may survive a delete");
        g.undo(&entry);
        assert!(g.same_content(&before));
    }

    #[test]
    fn batch_apply_is_all_or_nothing() {
        let mut g = two_node_graph();
        let before = g.clone();
        let err = g
            .apply_ops(
                &[
                    GraphOp::AddNode {
                        id: 5,
                        ntype: NodeType::Action,
                        emb: vec![0.1, 0.2],
                        ts: 5,
                    },
                    GraphOp::AddEdge {
                        from: 5,
                        to: 1,
                        etype: EdgeType::Refines,
                    },
                    // Fails: dimension violation, after two successful ops.
                    GraphOp::AddNode {
                        id: 6,
                        ntype: NodeType::Action,
                        emb: vec![0.0],
                        ts: 6,
                    },
                ],
                DIM,
            )
            .expect_err("batch must fail");
        assert!(matches!(err, GraphOpError::Dim { id: 6, .. }));
        assert!(
            g.same_content(&before),
            "a failed batch must roll back completely"
        );

        let journal = g
            .apply_ops(
                &[
                    GraphOp::AddNode {
                        id: 5,
                        ntype: NodeType::Action,
                        emb: vec![0.1, 0.2],
                        ts: 5,
                    },
                    GraphOp::AddEdge {
                        from: 5,
                        to: 1,
                        etype: EdgeType::Refines,
                    },
                ],
                DIM,
            )
            .expect("batch must apply");
        assert_eq!(g.size(), before.size() + 2);
        g.undo_ops(&journal);
        assert!(
            g.same_content(&before),
            "undo_ops must restore the pre-state"
        );
    }

    #[test]
    fn ids_stay_monotone_and_are_never_reused() {
        let mut g = Graph::empty();
        assert_eq!(g.next_id(), 0);
        let id = g.next_id();
        g.apply(
            &GraphOp::AddNode {
                id,
                ntype: NodeType::Observation,
                emb: vec![0.0, 0.0],
                ts: 0,
            },
            DIM,
        )
        .unwrap();
        assert_eq!(g.next_id(), 1);
        g.apply(&GraphOp::DeleteNode { id }, DIM).unwrap();
        assert_eq!(g.next_id(), 1, "deleting must not recycle the id");
    }

    #[test]
    fn acyclicity_detects_cycles_and_self_loops() {
        let g = two_node_graph();
        assert!(g.is_acyclic());

        let mut cyclic = two_node_graph();
        cyclic
            .apply(
                &GraphOp::AddEdge {
                    from: 2,
                    to: 1,
                    etype: EdgeType::CausallyPrecedes,
                },
                DIM,
            )
            .unwrap();
        assert!(!cyclic.is_acyclic());

        let mut loopy = two_node_graph();
        loopy
            .apply(
                &GraphOp::AddEdge {
                    from: 1,
                    to: 1,
                    etype: EdgeType::CausallyPrecedes,
                },
                DIM,
            )
            .unwrap();
        assert!(!loopy.is_acyclic());
    }

    #[test]
    fn legacy_type_names_load_through_the_alias_table() {
        // v0.1.0 wrote node_type "latent" and edge_type "morph".
        assert_eq!(NodeType::from_wire("latent"), NodeType::Observation);
        assert_eq!(EdgeType::from_wire("morph"), EdgeType::CausallyPrecedes);
        // Unknown labels survive as Custom rather than failing the load.
        assert_eq!(
            NodeType::from_wire("market_segment"),
            NodeType::Custom("market_segment".into())
        );

        let legacy = r#"{
            "nodes": { "0": { "id": 0, "embedding": [1.0, 0.0], "node_type": "latent", "timestamp": 3 } },
            "edges": [ { "from": 0, "to": 0, "edge_type": "morph" } ]
        }"#;
        let g: Graph = serde_json::from_str(legacy).expect("legacy graph must deserialize");
        assert_eq!(g.nodes[&0].node_type, NodeType::Observation);
        assert_eq!(
            g.edges.iter().next().unwrap().edge_type,
            EdgeType::CausallyPrecedes
        );
    }

    #[test]
    fn serde_round_trips_and_is_stable() {
        let g = two_node_graph();
        let json = serde_json::to_string(&g).unwrap();
        let back: Graph = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
        // Ordered containers ⇒ byte-stable serialization across runs.
        assert_eq!(json, serde_json::to_string(&back).unwrap());
        assert!(
            json.contains("\"observation\"") && json.contains("\"causally_precedes\""),
            "types must serialize as readable names: {json}"
        );
    }

    #[test]
    fn iteration_order_is_deterministic_regardless_of_insertion_order() {
        // The v0.1.0 HashMap/HashSet graph iterated in a per-process random
        // order, which made `one_edit` pick different nodes in different
        // processes. Ordered containers remove that degree of freedom.
        let mut ascending = Graph::empty();
        for id in 0..32u64 {
            ascending
                .apply(
                    &GraphOp::AddNode {
                        id,
                        ntype: NodeType::Observation,
                        emb: vec![id as f64, 0.0],
                        ts: id,
                    },
                    DIM,
                )
                .unwrap();
        }
        let mut descending = Graph::empty();
        for id in (0..32u64).rev() {
            descending
                .apply(
                    &GraphOp::AddNode {
                        id,
                        ntype: NodeType::Observation,
                        emb: vec![id as f64, 0.0],
                        ts: id,
                    },
                    DIM,
                )
                .unwrap();
        }
        let a: Vec<NodeId> = ascending.nodes.keys().copied().collect();
        let d: Vec<NodeId> = descending.nodes.keys().copied().collect();
        assert_eq!(a, d, "iteration order must not depend on insertion order");
        assert_eq!(a, (0..32u64).collect::<Vec<_>>());
    }
}
