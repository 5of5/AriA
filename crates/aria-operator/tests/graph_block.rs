//! Wire grammar block: sheet 09 envelopes carry their catalog grammar
//! position (weight = category weight, height = wave ladder, anchors) so the
//! result is a complete graphical query result — consumers never re-derive.

use aria_operator::{run_many, spec_by_id, token_stat, wave_height, RunOpts};

const PAYLOAD: &[u8] = br#"{"nodes":[
  {"id":1,"type":"Person","label":"Ada","notes":"founder","tags":["PERSON_FOUNDER"]},
  {"id":2,"type":"Company","label":"Acme","tags":["COMPANY"]}
],"edges":[{"from":1,"to":2,"type":"WORKS_AT"}]}"#;

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

fn ids() -> Vec<String> {
    ["BIN.PEOPLE", "BIN.TAG.PERSON_FOUNDER", "BIN.REL.WORKS_AT", "BIN.HASH_STAMP"]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

#[test]
fn every_envelope_carries_its_catalog_position() {
    let envs = run_many(&ids(), PAYLOAD, &opts()).unwrap();
    assert_eq!(envs.len(), 4);
    for env in &envs {
        let spec = spec_by_id(&env.binary_id).expect("catalog row");
        let g = env.graph.as_ref().expect("graph block on every envelope (HOST included)");
        assert_eq!(g.class, spec.class);
        assert_eq!(g.layer, spec.layer);
        assert_eq!(g.height, wave_height(spec.wave.as_deref()));
        let shape_consistent = match (g.shape.as_str(), g.weight) {
            ("isolated", 0) | ("uncommon", 1) => true,
            ("common", w) => w >= 2,
            _ => false,
        };
        assert!(shape_consistent, "{} shape/weight", env.binary_id);
        for a in &g.anchors {
            assert_eq!((a.weight, a.height), token_stat(&a.tag), "{} anchor {}", env.binary_id, a.tag);
        }
    }
}

#[test]
fn proposals_still_land_with_graph_block() {
    let envs = run_many(&ids(), PAYLOAD, &opts()).unwrap();
    assert_eq!(envs[0].coverage_state, "proposal"); // PEOPLE: Person node
    assert_eq!(envs[1].coverage_state, "proposal"); // TAG.PERSON_FOUNDER: tag hit
    assert_eq!(envs[2].coverage_state, "proposal"); // REL.WORKS_AT: edge hit
    assert_eq!(envs[3].coverage_state, "limitation"); // HASH_STAMP: host vertical, no Φ
}

#[test]
fn graph_block_is_byte_deterministic() {
    let a = serde_json::to_vec(&run_many(&ids(), PAYLOAD, &opts()).unwrap()).unwrap();
    let b = serde_json::to_vec(&run_many(&ids(), PAYLOAD, &opts()).unwrap()).unwrap();
    assert_eq!(a, b);
}
