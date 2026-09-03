//! 25 sealed map mixers (sheet 05). Remix tagged telemetry; never rewrite source.

use aria_operator::{catalog, execute_work, run_binary, RunOpts, WorkRequest, WORK_V1};
use serde_json::json;

fn opts() -> RunOpts {
    RunOpts {
        steps: 8,
        seed: Some(1),
        n_modes: Some(16),
        latent_dim: Some(16),
        allow_sub_spec_dims: true,
        include_telemetry: false,
        ..RunOpts::default()
    }
}

#[test]
fn twenty_five_refiners_are_named_from_sheet_05() {
    let refs: Vec<_> = catalog()
        .iter()
        .filter(|s| s.layer == "REFINEMENT")
        .map(|s| s.binary_id.as_str())
        .collect();
    assert_eq!(refs.len(), 25);
    assert!(refs.contains(&"BIN.REF.COMPETITIVE_RADAR"));
    assert!(refs.contains(&"BIN.REF.MARKET_INTELLIGENCE_BRIEF"));
    for s in catalog().iter().filter(|s| s.layer == "REFINEMENT") {
        assert_eq!(s.class, "REFINEMENT");
        assert_eq!(s.parent, "MARKET_MAP");
        assert!(s.verify);
        assert!(!s.node_types.is_empty(), "{} has no kinds", s.binary_id);
    }
}

#[test]
fn competitive_radar_slices_competitor_neighborhood_only() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Company", "label": "Acme"},
            {"id": 2, "type": "Company", "label": "Beta", "tags": ["COMPETITOR_TAG"]},
            {"id": 3, "type": "Person", "label": "Ada"},
            {"id": 4, "type": "Claim", "label": " competes"}
        ],
        "edges": [
            {"from": 1, "to": 2, "type": "COMPETES_WITH"},
            {"from": 3, "to": 1, "type": "WORKS_AT"}
        ]
    }))
    .unwrap();
    let env = run_binary("BIN.REF.COMPETITIVE_RADAR", &payload, &opts()).unwrap();
    assert_eq!(env.coverage_state, "proposal");
    assert!(env.nodes.iter().any(|n| n.kind.eq_ignore_ascii_case("company")));
    assert!(env.relationships.iter().any(|r| r.rel_type == "COMPETES_WITH"));
    assert!(
        !env.nodes.iter().any(|n| n.kind.eq_ignore_ascii_case("person")),
        "radar must not pull PEOPLE; that is a different mixer"
    );
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.nodes.len(), 1);
}

#[test]
fn radar_on_people_only_returns_nothing_working() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [{"id": 1, "type": "Person", "label": "Ada"}]
    }))
    .unwrap();
    let env = run_binary("BIN.REF.COMPETITIVE_RADAR", &payload, &opts()).unwrap();
    assert!(!env.has_working_data());
}

#[test]
fn mixer_consumes_work_v1_callback_without_rewriting_source() {
    let inner = WorkRequest {
        ops: vec!["BIN.COMPANY".into(), "BIN.PEOPLE".into()],
        payload: Some(json!({
            "nodes": [
                {"id": 1, "type": "Company", "label": "Acme"},
                {"id": 2, "type": "Person", "label": "Ada"}
            ],
            "edges": [{"from": 2, "to": 1, "type": "WORKS_AT"}]
        })),
        ..WorkRequest::default()
    };
    let callback = execute_work(&inner, &opts()).unwrap();
    assert_eq!(callback["schema"], WORK_V1);
    assert!(callback["ops"].as_u64().unwrap() >= 2);
    let bytes = serde_json::to_vec(&callback).unwrap();
    let radar = run_binary("BIN.REF.COMPETITIVE_RADAR", &bytes, &opts()).unwrap();
    assert!(radar.has_working_data(), "company slice must relate to radar");
    assert!(radar.nodes.iter().any(|n| n.kind.eq_ignore_ascii_case("company")));
    assert!(
        !radar.nodes.iter().any(|n| n.kind.eq_ignore_ascii_case("person")),
        "callback flatten still respects the mixer's neighborhood"
    );
}

#[test]
fn founder_lineage_takes_the_people_company_slice() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "tags": ["PERSON_FOUNDER"]},
            {"id": 2, "type": "Company", "label": "Acme"}
        ],
        "edges": [{"from": 1, "to": 2, "type": "FOUNDED"}]
    }))
    .unwrap();
    let env = run_binary("BIN.REF.FOUNDER_OPERATOR_LINEAGE", &payload, &opts()).unwrap();
    assert_eq!(env.coverage_state, "proposal");
    assert_eq!(env.nodes.len(), 2);
    assert!(env.relationships.iter().any(|r| r.rel_type == "FOUNDED"));
}
