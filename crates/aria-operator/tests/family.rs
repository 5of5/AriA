//! E10 family aggregator: BIN.PEOPLE = union of PEOPLE residuals / DEEP_TAGs.
//! One ingest. No second Φ. REL residuals are not unioned (would leak Company).

use aria_operator::{catalog, family_residuals, run_binary, DISPATCH_JSON, RunOpts};
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

#[test]
fn people_residuals_are_node_tag_rel_and_deep_tag() {
    let kids: Vec<_> = family_residuals("PEOPLE")
        .iter()
        .map(|s| s.binary_id.as_str())
        .collect();
    assert!(kids.contains(&"BIN.NODE.PERSON"));
    assert!(kids.contains(&"BIN.TAG.PERSON"));
    assert!(kids.contains(&"BIN.TAG.PERSON_FOUNDER"));
    assert!(kids.contains(&"BIN.REL.WORKS_AT"));
    assert!(
        !kids.contains(&"BIN.BUYER"),
        "family TAG BUYER is not a residual"
    );
}

#[test]
fn people_unions_founder_cast_on_observation_without_minting_person() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Observation", "notes": "Ada is the founder"},
            {"id": 2, "type": "Observation", "notes": "nothing relevant here"}
        ]
    }))
    .unwrap();
    let founder = run_binary("BIN.TAG.PERSON_FOUNDER", &payload, &opts()).unwrap();
    assert_eq!(founder.coverage_state, "proposal");
    assert_eq!(founder.nodes[0].id, 1);
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.coverage_state, "proposal", "E10: PEOPLE unions PERSON_FOUNDER");
    assert_eq!(people.nodes.len(), 1);
    assert_eq!(people.nodes[0].id, 1);
    assert!(
        !people.nodes.iter().any(|n| n.kind.eq_ignore_ascii_case("person")),
        "00c still must not mint a Person node"
    );
}

#[test]
fn people_stays_dark_on_company_notes() {
    let payload = serde_json::to_vec(&json!({
        "notes": ["Acme builds payments infrastructure in fintech"]
    }))
    .unwrap();
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.coverage_state, "no-finding");
    let company_tag = run_binary("BIN.TAG.IND_FINTECH", &payload, &opts()).unwrap();
    assert_eq!(company_tag.coverage_state, "proposal");
}

#[test]
fn people_does_not_swallow_company_via_works_at() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada"},
            {"id": 2, "type": "Company", "label": "Acme"}
        ],
        "edges": [{"from": 1, "to": 2, "type": "WORKS_AT"}]
    }))
    .unwrap();
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.nodes.len(), 1);
    assert!(people.nodes.iter().all(|n| n.kind.eq_ignore_ascii_case("person")));
    // B0: a PEOPLE envelope does not keep the Company endpoint, so WORKS_AT
    // is dropped here. BIN.REL.WORKS_AT is the residual that carries it.
    assert!(people.relationships.iter().all(|r| {
        people.nodes.iter().any(|n| n.id == r.from) && people.nodes.iter().any(|n| n.id == r.to)
    }));
    let rel = run_binary("BIN.REL.WORKS_AT", &payload, &opts()).unwrap();
    assert!(rel.relationships.iter().any(|r| r.rel_type == "WORKS_AT"));
}

#[test]
fn dispatch_table_covers_every_catalog_row() {
    let v: serde_json::Value = serde_json::from_str(DISPATCH_JSON).unwrap();
    assert_eq!(v["schema"], "aria-dispatch-table-v1");
    assert_eq!(v["catalog"], 560);
    let bins = v["binaries"].as_array().unwrap();
    assert_eq!(bins.len(), catalog().len());
    let ids: std::collections::BTreeSet<_> = bins
        .iter()
        .map(|b| b["binary_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 560);
}