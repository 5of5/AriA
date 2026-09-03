//! P3-1 / P3-2 projector surgery. Spec + projector, not 535 src files.
//!
//! S1 family TAG requires the role tag. S2 HOST is empty limitation, no Φ.
//! S3 VERIFY=F is an empty vertical. Residual TAG.PERSON still matches kind.

use aria_operator::{run_binary, run_many, RunOpts};
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

fn mixed() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "notes": "founder", "tags": ["PERSON_FOUNDER"]},
            {"id": 2, "type": "Company", "label": "Acme", "notes": "infra", "tags": ["COMPANY"]},
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
fn family_tag_does_not_fire_on_entity_type_alone() {
    let payload = mixed();
    for id in [
        "BIN.BUYER",
        "BIN.COMPETITOR",
        "BIN.PARTNER",
        "BIN.SELLER",
        "BIN.SYNDICATE",
    ] {
        let env = run_binary(id, &payload, &opts()).unwrap();
        assert_eq!(
            env.coverage_state, "no-finding",
            "{id} must not rewrite Person/Company as a role tag"
        );
        assert!(env.nodes.is_empty(), "{id} leaked {} nodes", env.nodes.len());
    }
}

#[test]
fn buyer_fires_only_on_buyer_tag() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "tags": ["BUYER_TAG"]},
            {"id": 2, "type": "Person", "label": "Bob", "notes": "sounds like a buyer, zero tags"},
            {"id": 3, "type": "Company", "label": "Acme"}
        ]
    }))
    .unwrap();
    let buyer = run_binary("BIN.BUYER", &payload, &opts()).unwrap();
    assert_eq!(buyer.coverage_state, "proposal");
    assert_eq!(buyer.nodes.len(), 1);
    assert_eq!(buyer.nodes[0].id, 1);
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.nodes.len(), 2, "PEOPLE still lists both persons");
    let company = run_binary("BIN.COMPANY", &payload, &opts()).unwrap();
    assert_eq!(company.nodes.len(), 1);
}

#[test]
fn competitor_does_not_rewrite_company() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Company", "label": "Acme"},
            {"id": 2, "type": "Company", "label": "Beta", "tags": ["COMPETITOR_TAG"]}
        ]
    }))
    .unwrap();
    let company = run_binary("BIN.COMPANY", &payload, &opts()).unwrap();
    assert_eq!(company.nodes.len(), 2);
    let comp = run_binary("BIN.COMPETITOR", &payload, &opts()).unwrap();
    assert_eq!(comp.coverage_state, "proposal");
    assert_eq!(comp.nodes.len(), 1);
    assert_eq!(comp.nodes[0].id, 2);
}

#[test]
fn residual_tag_person_still_matches_kind() {
    let env = run_binary("BIN.TAG.PERSON", &mixed(), &opts()).unwrap();
    assert_eq!(env.coverage_state, "proposal");
    assert_eq!(env.nodes.len(), 2);
}

#[test]
fn deep_tag_founder_requires_the_tag() {
    let env = run_binary("BIN.TAG.PERSON_FOUNDER", &mixed(), &opts()).unwrap();
    assert_eq!(env.coverage_state, "proposal");
    assert_eq!(env.nodes.len(), 1);
    assert_eq!(env.nodes[0].id, 1);
}

#[test]
fn host_out_of_phi_empty_limitation() {
    let payload = mixed();
    for id in [
        "BIN.OBSCURA",
        "BIN.HASH_STAMP",
        "BIN.CORPUS_SEARCH",
        "BIN.DOC_EXTRACT",
    ] {
        let env = run_binary(id, &payload, &opts()).unwrap();
        assert_eq!(env.coverage_state, "limitation", "{id}");
        assert!(env.nodes.is_empty(), "{id} leaked research nodes");
        assert!(env.relationships.is_empty());
        assert!(env.telemetry.is_none());
        assert_ne!(env.coverage_state, "truncation");
    }
    let aria = run_binary("BIN.ARIA", &payload, &opts()).unwrap();
    assert_eq!(aria.coverage_state, "proposal");
    assert!(!aria.nodes.is_empty());
}

#[test]
fn run_many_host_does_not_leak_when_named_with_research() {
    let ids = [
        "BIN.PEOPLE",
        "BIN.OBSCURA",
        "BIN.HASH_STAMP",
        "BIN.COMPANY",
        "BIN.DOC_EXTRACT",
        "BIN.BUYER",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    let envs = run_many(&ids, &mixed(), &opts()).unwrap();
    assert_eq!(envs.len(), 6);
    assert_eq!(envs[0].binary_id, "BIN.PEOPLE");
    assert_eq!(envs[0].nodes.len(), 2);
    assert_eq!(envs[1].coverage_state, "limitation");
    assert!(envs[1].nodes.is_empty());
    assert_eq!(envs[2].coverage_state, "limitation");
    assert!(envs[2].nodes.is_empty());
    assert_eq!(envs[3].nodes.len(), 1);
    assert_eq!(envs[4].coverage_state, "limitation");
    assert!(envs[4].nodes.is_empty());
    assert_eq!(envs[5].coverage_state, "no-finding");
}

#[test]
fn company_typed_does_not_light_people() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [{
            "id": 1,
            "type": "Company",
            "label": "Northline Payments Labs",
            "notes": "payments infrastructure in fintech (fabricated)",
            "sector": "fintech"
        }]
    }))
    .unwrap();
    let company = run_binary("BIN.COMPANY", &payload, &opts()).unwrap();
    assert_eq!(company.coverage_state, "proposal");
    assert_eq!(company.nodes.len(), 1);
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.coverage_state, "no-finding");
    let buyer = run_binary("BIN.BUYER", &payload, &opts()).unwrap();
    assert_eq!(buyer.coverage_state, "no-finding");
}

#[test]
fn unstructured_company_notes_cast_company_not_people() {
    // S4 + E10: listed tokens (infrastructure / fintech) tag the observation
    // into the COMPANY family. PEOPLE stays dark. No Person node is minted.
    let payload = serde_json::to_vec(&json!({
        "notes": ["Acme builds payments infrastructure in fintech"]
    }))
    .unwrap();
    let company = run_binary("BIN.COMPANY", &payload, &opts()).unwrap();
    assert_eq!(company.coverage_state, "proposal");
    assert!(!company.nodes.is_empty());
    assert!(company.nodes.iter().all(|n| !n.kind.eq_ignore_ascii_case("person")));
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.coverage_state, "no-finding");
    assert!(people.nodes.iter().all(|n| !n.kind.eq_ignore_ascii_case("person")));
}

#[test]
fn garbage_still_mints_zero_persons() {
    let payload = serde_json::to_vec(&json!({
        "notes": ["qwerty asdf garbage dump — not a person, not a company, 🍕"]
    }))
    .unwrap();
    let ids = ["BIN.PEOPLE", "BIN.BUYER", "BIN.TAG.PERSON", "BIN.ARIA"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let envs = run_many(&ids, &payload, &opts()).unwrap();
    for env in &envs {
        let v = serde_json::to_value(env).unwrap();
        assert!(v.get("trust").is_none());
        for n in &env.nodes {
            assert!(
                !n.kind.eq_ignore_ascii_case("person"),
                "{} guessed a Person from garbage",
                env.binary_id
            );
        }
    }
}
