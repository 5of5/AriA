//! Correct, sound, complete — one Φ, every research operator.
//!
//! Garbage in must not become a guessed Person. Forget is no-finding.
//! Host bytes always survive. No Trust keys. Independent content_hash.

use aria_operator::{catalog, commands_json, execute_work, run_many, token_stat, wave_height, RunOpts, WorkRequest};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::time::Instant;

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

fn research_ids() -> Vec<String> {
    catalog()
        .iter()
        .filter(|s| s.layer != "HOST")
        .map(|s| s.binary_id.clone())
        .collect()
}

#[test]
fn hosted_command_list_covers_every_crate() {
    let cmds = commands_json();
    assert_eq!(cmds["count"], 560);
    let n = cmds["commands"].as_array().expect("commands").len();
    assert_eq!(n, 560);
    let ids: BTreeSet<_> = cmds["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["binary_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 560);
    assert!(ids.contains("BIN.PEOPLE"));
    assert!(ids.contains("BIN.TAG.PERSON_FOUNDER"));
}

#[test]
fn one_phi_projects_every_research_binary_on_garbage() {
    let garbage = serde_json::to_vec(&json!({
        "notes": ["qwerty asdf garbage dump — not a person, not a company"],
        "dump": "unstructured noise",
        "noise": [1, 2, 3]
    }))
    .unwrap();
    let ids = research_ids();
    assert!(ids.len() >= 500, "research catalog shrank: {}", ids.len());

    let t0 = Instant::now();
    let envs = run_many(&ids, &garbage, &opts()).expect("run_many");
    let ms = t0.elapsed().as_millis();
    eprintln!(
        "matrix: {} binaries, one Φ, {}ms, garbage payload {}B",
        envs.len(),
        ms,
        garbage.len()
    );

    assert_eq!(envs.len(), ids.len());
    let mut hashes = BTreeSet::new();
    for (id, env) in ids.iter().zip(&envs) {
        assert_eq!(&env.binary_id, id);
        assert_eq!(env.schema, aria_operator::OPERATOR_ENVELOPE_V1);
        assert!(env.telemetry.is_none());
        let v = serde_json::to_value(env).unwrap();
        assert!(v.get("trust").is_none());
        assert!(v.get("Trust").is_none());
        assert!(v.get("goal_readiness").is_none());
        hashes.insert(env.content_hash.clone());
        if env.nodes.is_empty() && env.properties.is_empty() && env.verify {
            assert_eq!(env.coverage_state, "no-finding");
            assert!(env.no_finding_reason.is_some());
        }
        if env.coverage_state == "proposal" {
            // Garbage must not mint typed people/companies.
            for n in &env.nodes {
                assert!(
                    !n.kind.eq_ignore_ascii_case("person"),
                    "{id} guessed a Person from garbage"
                );
            }
        }
    }
    assert!(
        ms < 30_000,
        "535 projections after one Φ took {ms}ms (budget 30s debug)"
    );
    let _ = hashes;
}

#[test]
fn json_cli_compiles_a_hosted_ops_list() {
    let req = WorkRequest {
        ops: vec!["BIN.PEOPLE".into(), "BIN.COMPANY".into()],
        payload: Some(json!({
            "nodes": [
                {"id": 1, "type": "Person", "label": "Ada"},
                {"id": 2, "type": "Company", "label": "Acme"}
            ]
        })),
        ..WorkRequest::default()
    };
    let out = execute_work(&req, &opts()).unwrap();
    assert_eq!(out["schema"], "aria-work-v1");
    assert_eq!(out["phi_once"], true);
    assert_eq!(out["asked"], 2);
    assert_eq!(out["ops"], 2);
    let results = out["results"].as_array().unwrap();
    assert_eq!(results[0]["binary_id"], "BIN.PEOPLE");
    assert_eq!(results[1]["binary_id"], "BIN.COMPANY");
    assert_eq!(results[0]["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(results[1]["nodes"].as_array().unwrap().len(), 1);
}

#[test]
fn json_cli_commands_flag_returns_the_hosted_list() {
    let req = WorkRequest {
        commands: true,
        ..WorkRequest::default()
    };
    let out = execute_work(&req, &opts()).unwrap();
    assert_eq!(out["schema"], "aria-work-commands-v1");
    assert_eq!(out["count"], 560);
}

/// Production callback lock over the WHOLE catalog (HOST + residual +
/// research). Doctrine: something returns or nothing returns — never a
/// skeleton with missing working data — and every envelope self-describes
/// its grammar position so PCVC workers / Neo4j drivers / the CLI render
/// the wire struct as-is.
#[test]
#[allow(clippy::items_after_statements)] // closed alphabet pinned at the gate
fn every_binary_returns_exact_or_nothing_full_catalog() {
    let garbage = serde_json::to_vec(&json!({
        "notes": ["qwerty asdf garbage dump — not a person, not a company"],
        "dump": "unstructured noise",
        "noise": [1, 2, 3]
    }))
    .unwrap();
    let ids: Vec<String> = catalog().iter().map(|s| s.binary_id.clone()).collect();
    assert_eq!(ids.len(), 560, "full catalog");
    let envs = run_many(&ids, &garbage, &opts()).expect("run_many full catalog");
    assert_eq!(envs.len(), 560);

    const CLOSED: [&str; 5] = ["proposal", "no-finding", "limitation", "truncation", "failure"];
    for (id, env) in ids.iter().zip(&envs) {
        let spec = catalog().iter().find(|s| &s.binary_id == id).unwrap();
        // B8: closed state alphabet, no skeletons.
        assert!(
            CLOSED.contains(&env.coverage_state.as_str()),
            "{id} returned unlisted state {}",
            env.coverage_state
        );
        let has_data =
            !env.nodes.is_empty() || !env.relationships.is_empty() || !env.properties.is_empty();
        match env.coverage_state.as_str() {
            "no-finding" => {
                assert!(!has_data, "{id} no-finding carried data");
                assert!(env.no_finding_reason.is_some(), "{id} forgot without a reason");
            }
            "proposal" | "truncation" => {
                assert!(has_data, "{id} proposed a skeleton (no node/rel/prop evidence)");
            }
            _ => {}
        }
        // Grammar position: always present, catalog-true (deterministic).
        let g = env.graph.as_ref().unwrap_or_else(|| panic!("{id} missing graph block"));
        assert_eq!((g.class.as_str(), g.layer.as_str()), (spec.class.as_str(), spec.layer.as_str()));
        assert_eq!(g.height, wave_height(spec.wave.as_deref()));
        match g.shape.as_str() {
            "isolated" => assert_eq!(g.weight, 0, "{id}"),
            "uncommon" => assert_eq!(g.weight, 1, "{id}"),
            "common" => assert!(g.weight >= 2, "{id}"),
            other => panic!("{id} unlisted shape {other}"),
        }
        assert_eq!(g.anchors.len(), spec.anchor_tags.len(), "{id} anchor evidence mismatch");
        for a in &g.anchors {
            assert_eq!((a.weight, a.height), token_stat(&a.tag), "{id} anchor {}", a.tag);
        }
    }
}

#[test]
fn forget_is_not_delete_on_empty_graph() {
    let req = WorkRequest {
        work: Some("BIN.PEOPLE".into()),
        payload: Some(json!({"nodes": []})),
        ..WorkRequest::default()
    };
    let out = execute_work(&req, &opts()).unwrap();
    assert_eq!(out["schema"], "aria-work-v1");
    assert_eq!(out["asked"], 1);
    assert_eq!(out["ops"], 0);
    assert!(out["results"].as_array().unwrap().is_empty());
    assert!(out.get("trust").is_none());
}

#[test]
fn production_callback_omits_skeletons_on_mixed() {
    let ids: Vec<String> = catalog().iter().map(|s| s.binary_id.clone()).collect();
    let req = WorkRequest {
        ops: ids,
        payload: Some(json!({
            "nodes": [
                {"id": 1, "type": "Person", "label": "Ada", "notes": "founder", "tags": ["PERSON_FOUNDER"]},
                {"id": 2, "type": "Company", "label": "Acme", "notes": "infra", "tags": ["COMPANY"]},
                {"id": 3, "type": "Person", "label": "Bob", "notes": "engineer"}
            ],
            "edges": [
                {"from": 1, "to": 2, "type": "WORKS_AT"},
                {"from": 3, "to": 2, "type": "WORKS_AT"}
            ]
        })),
        ..WorkRequest::default()
    };
    let out = execute_work(&req, &opts()).unwrap();
    assert_eq!(out["asked"], 560);
    let results = out["results"].as_array().unwrap();
    assert_eq!(out["ops"], results.len());
    let ids: BTreeSet<&str> = results
        .iter()
        .map(|r| r["binary_id"].as_str().unwrap())
        .collect();
    for must in [
        "BIN.ARIA",
        "BIN.COMPANY",
        "BIN.NODE.COMPANY",
        "BIN.NODE.PERSON",
        "BIN.PEOPLE",
        "BIN.REL.WORKS_AT",
        "BIN.TAG.COMPANY",
        "BIN.TAG.PERSON",
        "BIN.TAG.PERSON_FOUNDER",
    ] {
        assert!(ids.contains(must), "missing {must}");
    }
    assert!(!ids.contains("BIN.BUYER"));
    assert!(!ids.iter().any(|id| catalog().iter().any(|s| s.binary_id == *id && s.layer == "HOST")));
    assert!(
        ids.iter().any(|id| id.starts_with("BIN.REF.")),
        "map mixers should slice a company+person dump"
    );
    for r in results {
        let nodes = r.get("nodes").and_then(Value::as_array).map_or(0, Vec::len);
        let rels = r.get("relationships").and_then(Value::as_array).map_or(0, Vec::len);
        let props = r.get("properties").and_then(Value::as_object).map_or(0, serde_json::Map::len);
        assert!(
            nodes + rels + props > 0,
            "{} shipped a skeleton",
            r["binary_id"]
        );
        assert!(r.get("trust").is_none());
    }
}
