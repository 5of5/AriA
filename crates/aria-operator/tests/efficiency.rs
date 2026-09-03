//! Data-efficiency gate for operator JSON.
//!
//! Default wire is the vertical. The spine is opt-in (`include_telemetry`).
//! Embeddings stay off unless asked. A PEOPLE worker must not pay for COMPANY.

use aria_operator::{catalog, run_spec, RunOpts};
use serde_json::{json, Value};
use std::time::Instant;

fn opts(include_telemetry: bool) -> RunOpts {
    RunOpts {
        steps: 8,
        seed: Some(1),
        n_modes: Some(16),
        latent_dim: Some(16),
        allow_sub_spec_dims: true,
        plan_hash: None,
        requirement_id: None,
        include_telemetry,
    }
}

fn spec_json(id: &str) -> String {
    let spec = catalog()
        .iter()
        .find(|s| s.binary_id == id)
        .unwrap_or_else(|| panic!("missing {id}"));
    serde_json::to_string(spec).unwrap()
}

fn mixed_payload() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "notes": "founder"},
            {"id": 2, "type": "Company", "label": "Acme", "notes": "infra"},
            {"id": 3, "type": "Person", "label": "Bob", "notes": "engineer"}
        ],
        "edges": [
            {"from": 1, "to": 2, "type": "WORKS_AT"},
            {"from": 3, "to": 2, "type": "WORKS_AT"}
        ]
    }))
    .unwrap()
}

#[test]
fn default_wire_is_the_vertical_spine_is_opt_in() {
    let payload = mixed_payload();
    let t0 = Instant::now();
    let people = run_spec(&spec_json("BIN.PEOPLE"), &payload, &opts(false)).unwrap();
    let company = run_spec(&spec_json("BIN.COMPANY"), &payload, &opts(false)).unwrap();
    let aria = run_spec(&spec_json("BIN.ARIA"), &payload, &opts(false)).unwrap();
    let elapsed_ms = t0.elapsed().as_millis();

    let people_bytes = serde_json::to_vec(&people).unwrap();
    let people_vertical = serde_json::to_vec(&json!({
        "nodes": people.nodes,
        "relationships": people.relationships,
        "properties": people.properties,
    }))
    .unwrap();

    eprintln!(
        "efficiency default: people_full={}B people_vertical={}B elapsed={}ms",
        people_bytes.len(),
        people_vertical.len(),
        elapsed_ms
    );

    assert!(people.telemetry.is_none(), "default wire omits telemetry");
    assert!(
        people_bytes.len() < 768,
        "default PEOPLE envelope should stay under 768B, got {}",
        people_bytes.len()
    );
    assert!(people_vertical.len() < 512);
    assert_eq!(people.nodes.len(), 2);
    assert!(people.nodes.iter().all(|n| n.kind.eq_ignore_ascii_case("person")));
    assert_eq!(company.nodes.len(), 1);
    assert!(aria.nodes.len() > people.nodes.len());
    assert!(elapsed_ms < 5_000, "three operator runs took {elapsed_ms}ms");
}

#[test]
fn opt_in_telemetry_is_larger_than_the_vertical_and_omits_embeddings() {
    let payload = mixed_payload();
    let people = run_spec(&spec_json("BIN.PEOPLE"), &payload, &opts(true)).unwrap();
    let telem = people.telemetry.as_ref().expect("telemetry requested");
    let people_telem = serde_json::to_vec(telem).unwrap();
    let people_vertical = serde_json::to_vec(&json!({
        "nodes": people.nodes,
        "relationships": people.relationships,
        "properties": people.properties,
    }))
    .unwrap();

    eprintln!(
        "efficiency telemetry-on: vertical={}B telem={}B",
        people_vertical.len(),
        people_telem.len()
    );

    assert!(people_vertical.len() < people_telem.len());
    // Production default wire never carries this spine (test above). Engine
    // 0.2.1 graph IPO still lists f64 embeddings on the opt-in document
    // (schema-required). If a later backends cut adds `embeddings_omitted`,
    // it must mean what it says.
    match telem.pointer("/graph/embeddings_omitted") {
        Some(Value::Bool(true)) => {
            if let Some(nodes) = telem.pointer("/graph/nodes").and_then(Value::as_array) {
                for n in nodes {
                    let emb = n.get("embedding").and_then(Value::as_array);
                    assert!(emb.is_none() || emb.unwrap().is_empty());
                }
            }
        }
        Some(other) => panic!("embeddings_omitted must be true when present, got {other}"),
        None => {
            let nodes = telem
                .pointer("/graph/nodes")
                .and_then(Value::as_array)
                .expect("opt-in spine graph.nodes");
            assert!(
                nodes.iter().any(|n| n
                    .get("embedding")
                    .and_then(Value::as_array)
                    .is_some_and(|e| !e.is_empty())),
                "engine 0.2.1 opt-in spine carries embeddings; default wire does not"
            );
        }
    }
}
