//! Production quality: slop in → listed tokens → the right binaries.
//!
//! Not a vibe. Every DEEP_TAG with a lexicon phrase must light its own node
//! when that phrase is buried in garbage. Garbage lights no Person. The
//! worker callback reports tokens, hits, and the binaries that structure them.

use aria_operator::{
    catalog, execute_work, organize_slop, run_many, tag_phrase, RunOpts, WorkRequest,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::time::Instant;

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
fn garbage_slop_has_no_listed_hits_and_mints_no_person() {
    let payload = serde_json::to_vec(&json!({
        "notes": ["qwerty asdf garbage dump — not a person, not a company, 🍕"]
    }))
    .unwrap();
    let org = organize_slop(&payload);
    assert!(org.hits.is_empty(), "garbage produced hits {:?}", org.hits);
    assert!(org.binaries.is_empty());
    let people = run_many(&["BIN.PEOPLE".into(), "BIN.TAG.PERSON_FOUNDER".into()], &payload, &opts())
        .unwrap();
    for env in &people {
        assert!(
            env.nodes.iter().all(|n| !n.kind.eq_ignore_ascii_case("person")),
            "{} minted Person from garbage",
            env.binary_id
        );
        assert!(!env.has_working_data() || env.binary_id == "BIN.ARIA");
    }
}

#[test]
fn mixed_slop_reports_tokens_hits_and_best_binaries() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "notes": "founder of the lab", "tags": ["PERSON_FOUNDER"]},
            {"id": 2, "type": "Company", "label": "Acme", "notes": "payments infrastructure in fintech"}
        ],
        "edges": [{"from": 1, "to": 2, "type": "WORKS_AT"}]
    }))
    .unwrap();
    let org = organize_slop(&payload);
    assert!(org.tokens >= 8, "token count {}", org.tokens);
    assert!(org.hits.iter().any(|h| h == "PERSON_FOUNDER"), "hits {:?}", org.hits);
    assert!(
        org.binaries.iter().any(|b| b == "BIN.TAG.PERSON_FOUNDER"),
        "best binaries {:?}",
        org.binaries
    );
    assert_eq!(org.nodes, 2);
    assert_eq!(org.edges, 1);
    assert!(org.kinds.iter().any(|k| k.eq_ignore_ascii_case("person")));
    assert!(org.kinds.iter().any(|k| k.eq_ignore_ascii_case("company")));

    let ids: Vec<String> = catalog().iter().map(|s| s.binary_id.clone()).collect();
    let req = WorkRequest {
        ops: ids,
        payload: Some(serde_json::from_slice(&payload).unwrap()),
        ..WorkRequest::default()
    };
    let out = execute_work(&req, &opts()).unwrap();
    assert_eq!(out["asked"], 560);
    assert!(out["ops"].as_u64().unwrap() >= 8, "working ops {}", out["ops"]);
    assert_eq!(out["organize"]["tokens"], org.tokens);
    let results = out["results"].as_array().unwrap();
    let got: BTreeSet<&str> = results
        .iter()
        .map(|r| r["binary_id"].as_str().unwrap())
        .collect();
    for must in [
        "BIN.PEOPLE",
        "BIN.COMPANY",
        "BIN.TAG.PERSON_FOUNDER",
        "BIN.REL.WORKS_AT",
        "BIN.ARIA",
    ] {
        assert!(got.contains(must), "missing {must} in {got:?}");
    }
    for r in results {
        assert_eq!(r["coverage_state"], "proposal");
        assert!(r["content_hash"].as_str().is_some_and(|h| h.len() == 64));
        assert!(r.get("trust").is_none());
    }
}

#[test]
fn every_lexicon_deep_tag_finds_its_own_node_in_slop() {
    let mut nodes = Vec::new();
    let mut expect: Vec<(String, u64)> = Vec::new();
    let mut id = 1u64;
    for spec in catalog().iter().filter(|s| s.layer == "DEEP_TAG" && s.taxonomy.is_some()) {
        let Some(tag) = spec.anchor_tags.first() else { continue };
        let phrase = tag_phrase(tag);
        if phrase.chars().count() < 3 {
            continue;
        }
        nodes.push(json!({
            "id": id,
            "type": "Observation",
            "notes": format!("qwerty {phrase} asdf 🍕")
        }));
        expect.push((spec.binary_id.clone(), id));
        id += 1;
    }
    assert!(expect.len() > 200, "lexicon too small: {}", expect.len());
    let payload = serde_json::to_vec(&json!({"nodes": nodes})).unwrap();
    let ids: Vec<String> = expect.iter().map(|(b, _)| b.clone()).collect();
    let t0 = Instant::now();
    let envs = run_many(&ids, &payload, &opts()).unwrap();
    let ms = t0.elapsed().as_millis();
    assert_eq!(envs.len(), expect.len());
    let mut miss = Vec::new();
    for ((bin, nid), env) in expect.iter().zip(envs.iter()) {
        if !env.nodes.iter().any(|n| n.id == *nid) {
            miss.push(format!("{bin} missed node {nid} ({})", env.coverage_state));
        }
        assert!(env.nodes.iter().all(|n| !n.kind.eq_ignore_ascii_case("person")));
    }
    assert!(
        miss.is_empty(),
        "slop miss {}/{}: {}",
        miss.len(),
        expect.len(),
        miss.iter().take(12).cloned().collect::<Vec<_>>().join("; ")
    );
    assert!(
        ms < 20_000,
        "all-lexicon slop identify took {ms}ms (budget 20s debug)"
    );
}

#[test]
fn company_slop_does_not_structure_as_people() {
    let payload = serde_json::to_vec(&json!({
        "notes": ["Acme builds payments infrastructure in fintech"]
    }))
    .unwrap();
    let org = organize_slop(&payload);
    assert!(
        org.binaries.iter().any(|b| b.starts_with("BIN.TAG.IND_") || b.starts_with("BIN.TAG.CO_")),
        "expected industry/company tags in {:?}",
        org.binaries
    );
    assert!(
        !org.binaries.iter().any(|b| b == "BIN.PEOPLE"),
        "PEOPLE must not be a suggested binary for company slop: {:?}",
        org.binaries
    );
    let people = run_many(&["BIN.PEOPLE".into()], &payload, &opts()).unwrap();
    assert!(!people[0].has_working_data());
}

/// The organize hint is a promise, not a guess: every binary it recommends
/// returns working data on that exact payload (a worker following the hint
/// never lands on an empty vertical), and role-TAGs without their `*_TAG`
/// evidence are never recommended (S1).
#[test]
fn organize_hint_never_points_at_an_empty_vertical() {
    let payload = serde_json::to_vec(&json!({
        "nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "notes": "Ada founded Acme"},
            {"id": 2, "type": "Company", "label": "Acme", "tags": ["COMPANY"]}
        ],
        "edges": [{"from": 1, "to": 2, "type": "WORKS_AT"}]
    }))
    .unwrap();
    let report = organize_slop(&payload);
    assert!(!report.binaries.is_empty());
    for role in ["BIN.BUYER", "BIN.COMPETITOR", "BIN.PARTNER", "BIN.SELLER", "BIN.SYNDICATE"] {
        assert!(
            !report.binaries.iter().any(|b| b == role),
            "{role} recommended without its role tag"
        );
    }
    let envs = run_many(&report.binaries, &payload, &opts()).unwrap();
    for e in &envs {
        assert!(
            e.has_working_data(),
            "{} was recommended but returned {} with no data",
            e.binary_id,
            e.coverage_state
        );
    }
    // With the role tag present, the role binary IS recommended and DOES fire.
    let tagged = serde_json::to_vec(&json!({
        "nodes": [{"id": 1, "type": "Person", "label": "Ada", "tags": ["BUYER_TAG"]}]
    }))
    .unwrap();
    let r2 = organize_slop(&tagged);
    assert!(r2.binaries.iter().any(|b| b == "BIN.BUYER"));
    let buyer = run_many(&["BIN.BUYER".to_string()], &tagged, &opts()).unwrap();
    assert!(buyer[0].has_working_data());
}
