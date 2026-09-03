//! Uniform wire (𝔸T6 / 𝐋T3): every one of the 560 operators serializes the
//! same closed key list in the same order; the only variation allowed is
//! omission of an empty/none optional member. Workers prune once, for all.

use aria_operator::{catalog, run_many, RunOpts, ENVELOPE_KEYS};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn opts() -> RunOpts {
    RunOpts {
        steps: 0,
        seed: Some(1),
        n_modes: Some(16),
        latent_dim: Some(16),
        allow_sub_spec_dims: true,
        include_telemetry: false,
        ..RunOpts::default()
    }
}

fn mixed() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "notes": "founder", "tags": ["PERSON_FOUNDER"]},
            {"id": 2, "type": "Company", "label": "Acme", "notes": "infra", "tags": ["COMPANY"]},
            {"id": 3, "type": "Person", "label": "Bob", "notes": "engineer", "industry": "fintech"}
        ],
        "edges": [
            {"from": 1, "to": 2, "type": "WORKS_AT"},
            {"from": 3, "to": 2, "type": "WORKS_AT"}
        ]
    }))
    .unwrap()
}

/// serde_json is built with `preserve_order` off here, so we check order via
/// the raw text: key positions must be monotonic in canonical order.
fn key_positions(raw: &str) -> Vec<(&'static str, usize)> {
    ENVELOPE_KEYS
        .iter()
        .filter_map(|(k, _)| raw.find(&format!("\"{k}\":")).map(|p| (*k, p)))
        .collect()
}

#[test]
fn all_560_envelopes_share_one_canonical_shape() {
    let ids: Vec<String> = catalog().iter().map(|s| s.binary_id.clone()).collect();
    let envs = run_many(&ids, &mixed(), &opts()).unwrap();
    assert_eq!(envs.len(), 560);
    let canonical: Vec<&str> = ENVELOPE_KEYS.iter().map(|(k, _)| *k).collect();
    let required: Vec<&str> = ENVELOPE_KEYS.iter().filter(|(_, r)| *r).map(|(k, _)| *k).collect();
    let mut key_hist: BTreeMap<String, usize> = BTreeMap::new();
    for env in &envs {
        let raw = serde_json::to_string(env).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let obj = v.as_object().unwrap();
        for k in obj.keys() {
            assert!(canonical.contains(&k.as_str()), "{}: unlisted key {k}", env.binary_id);
            *key_hist.entry(k.clone()).or_insert(0) += 1;
        }
        for r in &required {
            assert!(obj.contains_key(*r), "{}: missing required {r}", env.binary_id);
        }
        // Order: positions strictly increasing along the canonical list.
        let pos = key_positions(&raw);
        assert!(
            pos.windows(2).all(|w| w[0].1 < w[1].1),
            "{}: key order drift: {:?}",
            env.binary_id,
            pos
        );
        // Independence of shape from class: a graph block on every envelope.
        assert!(obj.contains_key("graph"), "{}: graph block missing", env.binary_id);
    }
    // Required keys appear on every envelope; optional keys never on more.
    for r in &required {
        assert_eq!(key_hist[*r], 560, "required key {r} not uniform");
    }
    eprintln!("wire keys over 560: {key_hist:?}");
}

#[test]
fn skeleton_free_callback_prunes_to_same_fields_for_every_operator() {
    // A worker keeps this projection for ANY operator; nothing binary-specific.
    const KEEP: [&str; 7] = [
        "binary_id",
        "coverage_state",
        "nodes",
        "relationships",
        "properties",
        "content_hash",
        "graph",
    ];
    let ids: Vec<String> = catalog().iter().map(|s| s.binary_id.clone()).collect();
    let envs = run_many(&ids, &mixed(), &opts()).unwrap();
    for env in envs.iter().filter(|e| e.has_working_data()) {
        let v = serde_json::to_value(env).unwrap();
        let pruned: serde_json::Map<String, Value> = v
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| KEEP.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(pruned["binary_id"], env.binary_id);
        assert!(pruned.contains_key("content_hash") && pruned.contains_key("graph"));
        assert!(
            pruned.contains_key("nodes") || pruned.contains_key("relationships") || pruned.contains_key("properties"),
            "{} working envelope pruned to nothing",
            env.binary_id
        );
    }
}
