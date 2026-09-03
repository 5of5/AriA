//! Catalog integrity and operator uniqueness over the Aria spine.

use aria_engine_backends::ipo::TELEMETRY_QUERY_V1;
use aria_operator::{
    catalog, endpoint_by_binary_id, endpoint_by_operator, run_binary, run_spec, OperatorEnvelope,
    RunOpts, OPERATOR_ENVELOPE_V1,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn opts() -> RunOpts {
    RunOpts {
        steps: 8,
        seed: Some(1),
        n_modes: Some(16),
        latent_dim: Some(16),
        allow_sub_spec_dims: true,
        plan_hash: None,
        requirement_id: None,
        include_telemetry: true,
    }
}

fn spec_json(id: &str) -> String {
    let spec = catalog()
        .iter()
        .find(|s| s.binary_id == id)
        .unwrap_or_else(|| panic!("missing {id}"));
    serde_json::to_string(spec).expect("spec serializes")
}

#[test]
fn catalog_has_560_unique_ids_crates_and_packages() {
    let cat = catalog();
    assert_eq!(cat.len(), 560);
    let ids: BTreeSet<_> = cat.iter().map(|s| s.binary_id.as_str()).collect();
    let crates: BTreeSet<_> = cat.iter().map(|s| s.crate_name.as_str()).collect();
    let pkgs: BTreeSet<_> = cat.iter().map(|s| s.package.as_str()).collect();
    assert_eq!(ids.len(), 560);
    assert_eq!(crates.len(), 560);
    assert_eq!(pkgs.len(), 560);
    assert!(ids.contains("BIN.PEOPLE"));
    assert!(ids.contains("BIN.ARIA"));
    assert!(ids.contains("BIN.DOC_EXTRACT"));
    assert!(ids.contains("BIN.NODE.COMPANY"));
    assert!(ids.contains("BIN.TAG.PERSON_FOUNDER"));
    assert!(ids.contains("BIN.REF.COMPETITIVE_RADAR"));
    assert_eq!(
        cat.iter().filter(|s| s.layer == "REFINEMENT").count(),
        25
    );
}

#[test]
fn people_and_company_are_distinct_closed_json_over_the_same_telemetry_schema() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada"},
            {"id": 2, "type": "Company", "label": "Acme"}
        ],
        "edges": [{"from": 1, "to": 2, "type": "WORKS_AT"}]
    }))
    .unwrap();

    let people: OperatorEnvelope = run_spec(&spec_json("BIN.PEOPLE"), &payload, &opts()).unwrap();
    let company: OperatorEnvelope = run_spec(&spec_json("BIN.COMPANY"), &payload, &opts()).unwrap();

    assert_eq!(people.schema, OPERATOR_ENVELOPE_V1);
    assert_eq!(company.schema, OPERATOR_ENVELOPE_V1);
    assert_eq!(people.binary_id, "BIN.PEOPLE");
    assert_eq!(company.binary_id, "BIN.COMPANY");
    assert_ne!(people.binary_id, company.binary_id);
    assert_ne!(people.crate_name, company.crate_name);

    let pt = people.telemetry.as_ref().expect("telemetry requested");
    let ct = company.telemetry.as_ref().expect("telemetry requested");
    assert_eq!(pt["schema"], TELEMETRY_QUERY_V1);
    assert_eq!(ct["schema"], TELEMETRY_QUERY_V1);
    assert_eq!(pt["source_sha256"], ct["source_sha256"]);

    assert!(people.nodes.iter().all(|n| n.kind.eq_ignore_ascii_case("person")));
    assert!(company.nodes.iter().all(|n| n.kind.eq_ignore_ascii_case("company")));
    assert_eq!(people.nodes.len(), 1);
    assert_eq!(company.nodes.len(), 1);
    assert!(people.as_value_has_no_trust());
}

#[test]
fn aria_transform_operator_passes_the_graph_through() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [{"id": 1, "type": "Person", "label": "Ada"}]
    }))
    .unwrap();
    let env = run_spec(&spec_json("BIN.ARIA"), &payload, &opts()).unwrap();
    assert_eq!(env.binary_id, "BIN.ARIA");
    assert_eq!(env.coverage_state, "proposal");
    assert!(!env.nodes.is_empty());
    assert_eq!(
        env.telemetry.as_ref().expect("telemetry")["schema"],
        TELEMETRY_QUERY_V1
    );
}

#[test]
fn document_extract_stays_verify_false() {
    let payload = b"{\"nodes\":[]}";
    let env = run_spec(&spec_json("BIN.DOC_EXTRACT"), payload, &opts()).unwrap();
    assert!(!env.verify);
    assert_eq!(env.coverage_state, "limitation");
}

#[test]
fn envelope_has_no_trust_use_or_goal_fields() {
    let payload = b"{\"nodes\":[{\"id\":1,\"type\":\"Person\",\"label\":\"Ada\"}]}";
    let env = run_spec(&spec_json("BIN.PEOPLE"), payload, &opts()).unwrap();
    let v = serde_json::to_value(&env).unwrap();
    assert!(v.get("trust").is_none());
    assert!(v.get("Trust").is_none());
    assert!(v.get("use").is_none());
    assert!(v.get("goal_readiness").is_none());
}

trait NoTrust {
    fn as_value_has_no_trust(&self) -> bool;
}

impl NoTrust for OperatorEnvelope {
    fn as_value_has_no_trust(&self) -> bool {
        let v: Value = serde_json::to_value(self).unwrap();
        v.get("trust").is_none() && v.get("coverage_score").is_none()
    }
}

#[test]
fn gateway_run_binary_equals_named_spec() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [{"id": 1, "type": "Person", "label": "Ada"}]
    }))
    .unwrap();
    let via_gw = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    let via_spec = run_spec(&spec_json("BIN.PEOPLE"), &payload, &opts()).unwrap();
    assert_eq!(via_gw.binary_id, via_spec.binary_id);
    assert_eq!(via_gw.nodes, via_spec.nodes);
    assert_eq!(via_gw.content_hash, via_spec.content_hash);
    assert!(run_binary("BIN.NOT_A_THING", &payload, &opts()).is_err());
}

#[test]
fn worker_endpoint_maps_bin_id_to_crate() {
    let ep = endpoint_by_binary_id("BIN.PEOPLE").expect("PEOPLE");
    assert_eq!(ep.package, "aria-telemetry-people");
    assert_eq!(ep.crate_name, "aria_telemetry_people");
    assert_eq!(ep.result_definition_ref, "entity.person");
    assert_eq!(ep.cargo_invoke(), "cargo run -p aria-telemetry-people");
    let by_op = endpoint_by_operator("PEOPLE").expect("op");
    assert_eq!(by_op.binary_id, ep.binary_id);
    assert!(endpoint_by_binary_id("BIN.NOT_A_THING").is_none());
}
