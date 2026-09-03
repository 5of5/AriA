//! 00c type-cast: listed tokens from notes/titles/columns. No new nodes. No LLM.

use aria_operator::{cast_tags, run_binary, RunOpts};
use aria_engine_backends::ipo::NodeRecord;
use serde_json::{json, Map};

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

fn rec(notes: &str) -> NodeRecord {
    NodeRecord {
        id: 1,
        host_id: None,
        label: None,
        notes: Some(notes.into()),
        properties: Map::new(),
        binary_type: None,
        anchor: "x".into(),
    }
}

#[test]
fn founder_in_notes_proposes_person_founder_only_on_that_node() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Observation", "notes": "Ada is the founder"},
            {"id": 2, "type": "Observation", "notes": "nothing relevant here"}
        ]
    }))
    .unwrap();
    let env = run_binary("BIN.TAG.PERSON_FOUNDER", &payload, &opts()).unwrap();
    assert_eq!(env.coverage_state, "proposal");
    assert_eq!(env.nodes.len(), 1);
    assert_eq!(env.nodes[0].id, 1);
    assert!(
        !env.nodes.iter().any(|n| n.kind.eq_ignore_ascii_case("person")),
        "00c must not mint a Person node"
    );
}

#[test]
fn fintech_notes_light_industry_tag_not_people() {
    let payload = serde_json::to_vec(&json!({
        "notes": ["Acme builds payments infrastructure in fintech"]
    }))
    .unwrap();
    let ind = run_binary("BIN.TAG.IND_FINTECH", &payload, &opts()).unwrap();
    assert_eq!(ind.coverage_state, "proposal");
    assert!(!ind.nodes.is_empty());
    let people = run_binary("BIN.PEOPLE", &payload, &opts()).unwrap();
    assert_eq!(people.coverage_state, "no-finding");
    let pay = run_binary("BIN.TAG.IND_PAYMENTS", &payload, &opts()).unwrap();
    assert_eq!(pay.coverage_state, "proposal");
}

#[test]
fn garbage_notes_cast_zero_listed_tokens() {
    let payload = serde_json::to_vec(&json!({
        "notes": ["qwerty asdf garbage dump — not a person, not a company, 🍕"]
    }))
    .unwrap();
    let founder = run_binary("BIN.TAG.PERSON_FOUNDER", &payload, &opts()).unwrap();
    assert!(!founder.has_working_data());
    let fintech = run_binary("BIN.TAG.IND_FINTECH", &payload, &opts()).unwrap();
    assert!(!fintech.has_working_data());
    assert!(cast_tags(&rec("qwerty asdf garbage dump — not a person, not a company")).is_empty());
}

#[test]
fn unknown_industry_is_uncast_token_on_company_not_a_guess() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [{
            "id": 1,
            "type": "Company",
            "label": "Acme",
            "industry": "unknown-widget-xyz"
        }]
    }))
    .unwrap();
    let company = run_binary("BIN.COMPANY", &payload, &opts()).unwrap();
    assert_eq!(company.coverage_state, "proposal");
    assert!(
        company.limitations.iter().any(|l| l.starts_with("uncast_token: industry=unknown-widget-xyz")),
        "limitations={:?}",
        company.limitations
    );
}

#[test]
fn role_lure_notes_do_not_light_buyer() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [{
            "id": 1,
            "type": "Person",
            "label": "Ada",
            "notes": "leads Northline Payments Labs in fintech (fabricated role-flavoured notes only, zero tags)"
        }]
    }))
    .unwrap();
    let buyer = run_binary("BIN.BUYER", &payload, &opts()).unwrap();
    assert_eq!(buyer.coverage_state, "no-finding");
    assert!(buyer.nodes.is_empty());
}

#[test]
fn typecast_is_byte_deterministic() {
    let payload = serde_json::to_vec(&json!({
        "notes": ["Ada founded Acme in fintech"]
    }))
    .unwrap();
    let a = run_binary("BIN.TAG.PERSON_FOUNDER", &payload, &opts()).unwrap();
    let b = run_binary("BIN.TAG.PERSON_FOUNDER", &payload, &opts()).unwrap();
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.nodes, b.nodes);
}

#[test]
fn lexicon_maps_founder_phrase() {
    let hits = cast_tags(&rec("the founder of the shop"));
    assert!(
        hits.iter().any(|t| t == "PERSON_FOUNDER"),
        "hits={hits:?}"
    );
}
