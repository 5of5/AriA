//! Organize report: what slop the worker sent, which listed tokens fired,
//! which binaries will structure them. Observer only.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

use crate::typecast::{cast_lexicon, casts_in_value, tokenize, CAST_PROPS};
use crate::specs_for_tag;

/// What the node determined from the host payload **before** pruning.
/// Workers use this to know depth, token count, and which binaries to ask.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct OrganizeReport {
    /// Alphanumeric tokens scanned (notes, labels, 00c fields, tags).
    pub tokens: u32,
    /// Closed-vocabulary tags that fired (explicit ∪ 00c). Catalog case.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hits: Vec<String>,
    /// Designated fields with no lexicon hit (`field=value`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uncast: Vec<String>,
    /// Catalog identities that declare those hits (best structure for this slop).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<String>,
    /// Host nodes (or notes-as-nodes).
    pub nodes: usize,
    /// Host edges.
    pub edges: usize,
    /// Distinct host kinds (`type` / `kind`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
}

/// Read the payload the way 00c does. No Φ. No new nodes.
#[must_use]
pub fn organize_slop(payload: &[u8]) -> OrganizeReport {
    let Ok(v) = serde_json::from_slice::<Value>(payload) else {
        return OrganizeReport::default();
    };
    let mut texts = Vec::new();
    let mut explicit: BTreeSet<String> = BTreeSet::new();
    let mut uncast: BTreeSet<String> = BTreeSet::new();
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    let mut nodes = 0usize;
    let mut edges = 0usize;
    harvest(
        &v,
        &mut texts,
        &mut explicit,
        &mut uncast,
        &mut kinds,
        &mut nodes,
        &mut edges,
    );

    let mut tokens = 0u32;
    let mut tok_lists = Vec::with_capacity(texts.len());
    for t in &texts {
        let toks = tokenize(t);
        tokens = tokens.saturating_add(u32::try_from(toks.len()).unwrap_or(u32::MAX));
        tok_lists.push(toks);
    }
    let lex = cast_lexicon();
    let mut hits: BTreeSet<String> = explicit;
    let l = lex
        .keys()
        .map(|p| p.split_whitespace().count())
        .max()
        .unwrap_or(1);
    for toks in &tok_lists {
        let mut gram = String::new();
        for i in 0..toks.len() {
            gram.clear();
            for n in 0..l.min(toks.len() - i) {
                if n > 0 {
                    gram.push(' ');
                }
                gram.push_str(&toks[i + n]);
                if let Some(tags) = lex.get(gram.as_str()) {
                    hits.extend(tags.iter().cloned());
                }
            }
        }
    }

    // A binary is recommended only if it would actually fire on these hits.
    // Family role-TAGs (BUYER, COMPETITOR, …) fire on their *_TAG anchors
    // only — never on the entity kinds they also list (S1). Recommending
    // them on a bare "COMPANY" hit would send workers to empty verticals.
    let hits_norm: BTreeSet<String> = hits.iter().map(|h| crate::index::norm(h)).collect();
    let mut binaries: BTreeSet<(u8, String)> = BTreeSet::new();
    for tag in &hits {
        for spec in specs_for_tag(tag) {
            if spec.layer.eq_ignore_ascii_case("TAG") {
                let node_types: Vec<String> =
                    spec.node_types.iter().map(|t| crate::index::norm(t)).collect();
                let fires = spec
                    .anchor_tags
                    .iter()
                    .map(|t| crate::index::norm(t))
                    .filter(|t| !node_types.contains(t))
                    .any(|t| hits_norm.contains(&t));
                if !fires {
                    continue;
                }
            }
            let rank = match spec.layer.as_str() {
                "DEEP_TAG" => 0,
                "RESIDUAL" => 1,
                "ENTITY" | "TAG" => 2,
                "REFINEMENT" => 3,
                _ => 4,
            };
            binaries.insert((rank, spec.binary_id.clone()));
        }
        if let Some(parent) = specs_for_tag(tag)
            .iter()
            .find(|s| !s.parent.is_empty())
            .map(|s| s.parent.as_str())
        {
            if let Some(fam) = crate::spec_by_operator(parent) {
                if fam.layer.eq_ignore_ascii_case("ENTITY") {
                    binaries.insert((2, fam.binary_id.clone()));
                }
            }
        }
    }

    OrganizeReport {
        tokens,
        hits: hits.into_iter().collect(),
        uncast: uncast.into_iter().collect(),
        binaries: binaries.into_iter().map(|(_, id)| id).collect(),
        nodes,
        edges,
        kinds: kinds.into_iter().collect(),
    }
}

fn harvest(
    v: &Value,
    texts: &mut Vec<String>,
    explicit: &mut BTreeSet<String>,
    uncast: &mut BTreeSet<String>,
    kinds: &mut BTreeSet<String>,
    nodes: &mut usize,
    edges: &mut usize,
) {
    match v {
        Value::Object(map) => {
            if let Some(arr) = map.get("nodes").and_then(Value::as_array) {
                *nodes += arr.len();
                for n in arr {
                    harvest_node(n, texts, explicit, uncast, kinds);
                }
            } else if let Some(arr) = map.get("notes").and_then(Value::as_array) {
                *nodes += arr.len();
                for n in arr {
                    // Strings are pushed by harvest_node; objects may carry
                    // `text` instead of `notes` — push that once, here.
                    if let Value::Object(o) = n {
                        if o.get("notes").is_none() {
                            push_text(n, texts);
                        }
                    }
                    harvest_node(n, texts, explicit, uncast, kinds);
                }
            }
            if let Some(arr) = map.get("edges").and_then(Value::as_array) {
                *edges += arr.len();
            }
            if map.get("nodes").is_none() && map.get("notes").is_none() {
                if let Some(s) = map.get("notes").and_then(Value::as_str) {
                    texts.push(s.to_string());
                    *nodes += 1;
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                harvest(
                    item, texts, explicit, uncast, kinds, nodes, edges,
                );
            }
        }
        Value::String(s) => texts.push(s.clone()),
        _ => {}
    }
}

fn harvest_node(
    n: &Value,
    texts: &mut Vec<String>,
    explicit: &mut BTreeSet<String>,
    uncast: &mut BTreeSet<String>,
    kinds: &mut BTreeSet<String>,
) {
    let Some(obj) = n.as_object() else {
        if let Value::String(s) = n {
            texts.push(s.clone());
        }
        return;
    };
    if let Some(k) = obj
        .get("type")
        .or_else(|| obj.get("kind"))
        .and_then(Value::as_str)
    {
        kinds.insert(k.to_string());
    }
    if let Some(s) = obj.get("label").and_then(Value::as_str) {
        if !s.is_empty() {
            texts.push(s.to_string());
        }
    }
    if let Some(s) = obj.get("notes").and_then(Value::as_str) {
        if !s.is_empty() {
            texts.push(s.to_string());
        }
    }
    match obj.get("tags") {
        Some(Value::Array(arr)) => {
            for t in arr.iter().filter_map(Value::as_str) {
                if !t.is_empty() {
                    explicit.insert(t.to_string());
                    texts.push(t.to_string());
                }
            }
        }
        Some(Value::String(s)) if !s.is_empty() => {
            explicit.insert(s.clone());
            texts.push(s.clone());
        }
        _ => {}
    }
    for key in CAST_PROPS {
        if let Some(Value::String(s)) = obj.get(*key) {
            if s.is_empty() {
                continue;
            }
            texts.push(s.clone());
            if crate::typecast::UNCAST_PROPS.contains(key) && !casts_in_value(s) {
                uncast.insert(format!("{key}={s}"));
            }
        }
    }
}

fn push_text(v: &Value, texts: &mut Vec<String>) {
    match v {
        Value::String(s) => texts.push(s.clone()),
        Value::Object(o) => {
            if let Some(s) = o.get("notes").or_else(|| o.get("text")).and_then(Value::as_str) {
                texts.push(s.to_string());
            }
        }
        _ => {}
    }
}
