//! Statelessness (𝔸T2 / 𝐋T4): M concurrent projections = M sequential bytes.

use aria_operator::{catalog, run_binary, run_many, RunOpts};
use serde_json::json;

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
            {"id": 2, "type": "Company", "label": "Acme", "notes": "infra", "tags": ["COMPANY"]}
        ],
        "edges": [{"from": 1, "to": 2, "type": "WORKS_AT"}]
    }))
    .unwrap()
}

#[test]
fn parallel_projections_match_sequential_hashes() {
    let payload = mixed();
    let ids: Vec<String> = catalog()
        .iter()
        .filter(|s| s.layer != "HOST")
        .take(16)
        .map(|s| s.binary_id.clone())
        .collect();
    let seq = run_many(&ids, &payload, &opts()).unwrap();
    let par: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = ids
            .iter()
            .map(|id| {
                let p = &payload;
                let i = id.clone();
                scope.spawn(move || run_binary(&i, p, &opts()).unwrap())
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(seq.len(), par.len());
    for (s, p) in seq.iter().zip(&par) {
        assert_eq!(s.binary_id, p.binary_id);
        assert_eq!(s.content_hash, p.content_hash, "{}", s.binary_id);
        assert_eq!(s.coverage_state, p.coverage_state);
    }
}

#[test]
fn mixer_depth2_does_not_runaway() {
    let payload = mixed();
    let mixers: Vec<String> = catalog()
        .iter()
        .filter(|s| s.layer == "REFINEMENT")
        .map(|s| s.binary_id.clone())
        .collect();
    assert_eq!(mixers.len(), 25);
    let d1 = run_many(&mixers, &payload, &opts()).unwrap();
    let w1 = d1.iter().filter(|e| e.has_working_data()).count();
    let cb = serde_json::to_vec(&serde_json::json!({
        "schema": "aria-work-v1",
        "phi_once": true,
        "asked": mixers.len(),
        "ops": w1,
        "results": d1.iter().filter(|e| e.has_working_data()).collect::<Vec<_>>(),
    }))
    .unwrap();
    let d2 = run_many(&mixers, &cb, &opts()).unwrap();
    let w2 = d2.iter().filter(|e| e.has_working_data()).count();
    assert!(w2 <= w1, "depth2 {w2} > depth1 {w1}");
}
