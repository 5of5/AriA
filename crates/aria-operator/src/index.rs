//! Per-run projection index: one O(N + E) pass over the transformed graph,
//! then every operator projects by hash lookup instead of re-scanning.
//!
//! Semantics are the linear projector's, token for token: kind / label /
//! `kind|type` property match for node types; explicit `tags` ∪ 00c casts
//! for anchor tags; edge type for relationships. Byte-identity with the
//! pre-index projector is the referee (`scripts/dump_referee.py`).

use aria_engine_backends::ipo::{IpoEdge, IpoNode, NodeRecord};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Token normalisation shared by every match: lowercase, `-` → `_`.
#[must_use]
pub fn norm(s: &str) -> String {
    s.to_ascii_lowercase().replace('-', "_")
}

/// Precomputed tokens for one node (indices refer to `nodes` order):
/// kind ∪ label ∪ (`kind`|`type` string property) — the `matches_kind` set.
/// The kind-only token lives in `by_kind` (residual TAG kind check).
pub(crate) struct NodeTok {
    pub kindlike: Vec<String>,
    /// Explicit ∪ 00c-cast tags (normalized).
    pub tags: Vec<String>,
}

/// The index. Borrows the transform output; lives for one `run_many`.
pub(crate) struct GraphIndex<'a> {
    pub nodes: &'a [IpoNode],
    pub edges: &'a [IpoEdge],
    pub records: &'a BTreeMap<u64, NodeRecord>,
    pub tok: Vec<NodeTok>,
    by_kind: HashMap<String, Vec<usize>>,
    by_kindlike: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    by_rel: HashMap<String, Vec<usize>>,
    first_prop: HashMap<String, Value>,
    id_to_idx: HashMap<u64, usize>,
}

fn push_unique(map: &mut HashMap<String, Vec<usize>>, key: String, idx: usize) {
    let v = map.entry(key).or_default();
    if v.last() != Some(&idx) {
        v.push(idx);
    }
}

impl<'a> GraphIndex<'a> {
    pub fn build(
        nodes: &'a [IpoNode],
        edges: &'a [IpoEdge],
        records: &'a BTreeMap<u64, NodeRecord>,
    ) -> Self {
        let mut tok = Vec::with_capacity(nodes.len());
        let mut by_kind: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_kindlike: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_tag: HashMap<String, Vec<usize>> = HashMap::new();
        let mut id_to_idx = HashMap::with_capacity(nodes.len());
        for (i, n) in nodes.iter().enumerate() {
            id_to_idx.entry(n.id).or_insert(i);
            let kind = norm(n.node_type.as_str());
            let mut kindlike = vec![kind.clone()];
            let mut tags: Vec<String> = Vec::new();
            if let Some(rec) = records.get(&n.id) {
                if let Some(l) = rec.label.as_deref() {
                    kindlike.push(norm(l));
                }
                if let Some(Value::String(k)) =
                    rec.properties.get("kind").or_else(|| rec.properties.get("type"))
                {
                    kindlike.push(norm(k));
                }
                match rec.properties.get("tags") {
                    Some(Value::Array(arr)) => {
                        tags.extend(arr.iter().filter_map(Value::as_str).map(norm));
                    }
                    Some(Value::String(s)) => tags.push(norm(s)),
                    _ => {
                        if let Some(s) = rec.properties.get("tag").and_then(Value::as_str) {
                            tags.push(norm(s));
                        }
                    }
                }
                tags.extend(crate::typecast::cast_tags(rec).iter().map(|t| norm(t)));
            }
            kindlike.sort_unstable();
            kindlike.dedup();
            tags.sort_unstable();
            tags.dedup();
            push_unique(&mut by_kind, kind.clone(), i);
            for k in &kindlike {
                push_unique(&mut by_kindlike, k.clone(), i);
            }
            for t in &tags {
                push_unique(&mut by_tag, t.clone(), i);
            }
            tok.push(NodeTok { kindlike, tags });
        }
        let mut by_rel: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in edges.iter().enumerate() {
            push_unique(&mut by_rel, norm(e.edge_type.as_str()), i);
        }
        // PROP operators expose the first record (id order) carrying the key.
        let mut first_prop: HashMap<String, Value> = HashMap::new();
        for rec in records.values() {
            for (k, v) in &rec.properties {
                first_prop.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        Self {
            nodes,
            edges,
            records,
            tok,
            by_kind,
            by_kindlike,
            by_tag,
            by_rel,
            first_prop,
            id_to_idx,
        }
    }

    fn gather(map: &HashMap<String, Vec<usize>>, tokens: &[String], out: &mut BTreeSet<usize>) {
        for t in tokens {
            if let Some(v) = map.get(t) {
                out.extend(v.iter().copied());
            }
        }
    }

    /// Nodes whose `node_type` alone matches (residual TAG kind check).
    pub fn nodes_by_kind(&self, tokens: &[String], out: &mut BTreeSet<usize>) {
        Self::gather(&self.by_kind, tokens, out);
    }

    /// Nodes matching by kind, label, or `kind|type` property (`matches_kind`).
    pub fn nodes_by_kindlike(&self, tokens: &[String], out: &mut BTreeSet<usize>) {
        Self::gather(&self.by_kindlike, tokens, out);
    }

    /// Nodes carrying any of these anchor tags (explicit ∪ cast).
    pub fn nodes_by_tag(&self, tokens: &[String], out: &mut BTreeSet<usize>) {
        Self::gather(&self.by_tag, tokens, out);
    }

    /// Edges whose type is in `tokens`; `None` tokens ⇒ every edge.
    pub fn edges_by_rel(&self, tokens: &[String]) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        if tokens.is_empty() {
            out.extend(0..self.edges.len());
        } else {
            Self::gather(&self.by_rel, tokens, &mut out);
        }
        out
    }

    pub fn first_prop(&self, key: &str) -> Option<&Value> {
        self.first_prop.get(key)
    }

    pub fn idx_of(&self, id: u64) -> Option<usize> {
        self.id_to_idx.get(&id).copied()
    }

    /// `matches_kind` for one node against a normalised allow-list.
    pub fn kindlike_hits(&self, idx: usize, allowed: &[String]) -> bool {
        self.tok[idx].kindlike.iter().any(|k| allowed.contains(k))
    }

    /// Explicit ∪ 00c tags on one node (catalog case, not normalized for wire).
    pub fn tags_of(&self, idx: usize) -> &[String] {
        &self.tok[idx].tags
    }

    /// E5: no explicit tags and 00c produced none — family/DEEP_TAG can skip.
    pub fn has_tags(&self) -> bool {
        !self.by_tag.is_empty()
    }
}
