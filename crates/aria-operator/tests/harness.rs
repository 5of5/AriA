//! PCVC Mode 4 harness lane locks: bound result, closed status, silent
//! stderr, bounded output, byte-determinism, protocol errors as JSON.

use aria_operator::{
    dispatch_json, harness_lane, HarnessRequest, DEFAULT_OUTPUT_LIMIT, HARNESS_CAPABILITY,
    HARNESS_REQUEST_V1, HARNESS_RESULT_V1,
};
use serde_json::{json, Value};

const PLAN: &str = "816c0d436f7b8d5747972304e53863190907fe203ca2d17b2b15e431eac3dd9d";

#[allow(clippy::needless_pass_by_value)] // test fixture: call-site clarity
fn request(ops: Vec<&str>, payload: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": HARNESS_REQUEST_V1,
        "capability": HARNESS_CAPABILITY,
        "runId": "6f1d2c1e-0000-4000-8000-000000000001",
        "planHash": PLAN,
        "attemptId": "6f1d2c1e-0000-4000-8000-000000000002",
        "fencingToken": 7,
        "requirementId": "req.people.identity",
        "ops": ops,
        "payload": payload,
        "steps": 0,
        "seed": 1
    }))
    .unwrap()
}

fn people_payload() -> Value {
    json!({"nodes": [
        {"id": 1, "type": "Person", "label": "Ada", "notes": "founder", "tags": ["PERSON_FOUNDER"]},
        {"id": 2, "type": "Company", "label": "Acme", "tags": ["COMPANY"]}
    ], "edges": [{"from": 1, "to": 2, "type": "WORKS_AT"}]})
}

#[test]
fn bound_result_echoes_every_binding_and_carries_the_callback() {
    let (code, out) = harness_lane(&request(vec!["BIN.PEOPLE", "BIN.BUYER"], people_payload()));
    assert_eq!(code, 0);
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["schemaVersion"], HARNESS_RESULT_V1);
    for (k, want) in [
        ("capability", json!(HARNESS_CAPABILITY)),
        ("runId", json!("6f1d2c1e-0000-4000-8000-000000000001")),
        ("planHash", json!(PLAN)),
        ("attemptId", json!("6f1d2c1e-0000-4000-8000-000000000002")),
        ("fencingToken", json!(7)),
        ("requirementId", json!("req.people.identity")),
    ] {
        assert_eq!(v[k], want, "binding {k}");
    }
    assert_eq!(v["status"], "result");
    let cb = &v["callback"];
    assert_eq!(cb["schema"], "aria-work-v1");
    assert_eq!(cb["asked"], 2);
    assert_eq!(cb["ops"], 1, "BUYER has no BUYER_TAG evidence → omitted, not a skeleton");
    assert_eq!(cb["results"][0]["binary_id"], "BIN.PEOPLE");
    assert_eq!(cb["results"][0]["plan_hash"], PLAN, "plan_hash bound into the envelope");
    assert_eq!(cb["results"][0]["requirement_id"], "req.people.identity");
    assert!(out.len() <= DEFAULT_OUTPUT_LIMIT);
}

#[test]
fn no_finding_is_bound_and_empty_not_a_skeleton() {
    let (code, out) = harness_lane(&request(vec!["BIN.PEOPLE"], json!({"nodes": []})));
    assert_eq!(code, 0);
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["status"], "no-finding");
    assert_eq!(v["callback"]["ops"], 0);
    assert!(v["callback"]["results"].as_array().unwrap().is_empty());
    assert!(v.get("limitation").is_none());
}

#[test]
fn engine_rejection_is_a_bound_limitation_with_exit_zero() {
    // duplicate ids: the engine refuses; the harness still gets a bound result
    let (code, out) = harness_lane(&request(
        vec!["BIN.COMPANY"],
        json!({"nodes": [{"id": 7, "type": "Company"}, {"id": 7, "type": "Company"}]}),
    ));
    assert_eq!(code, 0);
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["status"], "limitation");
    assert!(v["limitation"].as_str().unwrap().contains("duplicate id"));
    assert_eq!(v["planHash"], PLAN);
    assert!(v.get("callback").is_none());
}

#[test]
fn protocol_errors_exit_two_with_json_not_stderr() {
    let mut bad: Value = serde_json::from_slice(&request(vec!["BIN.PEOPLE"], json!({}))).unwrap();
    bad["fencingToken"] = json!(0);
    let (code, out) = harness_lane(&serde_json::to_vec(&bad).unwrap());
    assert_eq!(code, 2);
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["status"], "failure");
    assert!(v["error"].as_str().unwrap().contains("fencingToken"));

    let (code, out) = harness_lane(&request(vec!["BIN.NOPE"], json!({})));
    assert_eq!(code, 2);
    assert!(String::from_utf8_lossy(&out).contains("unknown binary BIN.NOPE"));

    let (code, _) = harness_lane(b"not json");
    assert_eq!(code, 2);
}

#[test]
fn output_budget_truncates_deterministically() {
    let mut nodes = Vec::new();
    for i in 0..400u64 {
        nodes.push(json!({"id": i, "type": if i % 2 == 0 { "Person" } else { "Company" }, "label": format!("N{i}")}));
    }
    let mut req: Value =
        serde_json::from_slice(&request(vec!["*"], json!({"nodes": nodes}))).unwrap();
    req["outputLimitBytes"] = json!(4096);
    let raw = serde_json::to_vec(&req).unwrap();
    let (code, out) = harness_lane(&raw);
    assert_eq!(code, 0);
    assert!(out.len() <= 4096, "budget honoured: {} B", out.len());
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert!(matches!(v["status"].as_str(), Some("truncation" | "limitation")));
    if v["status"] == "truncation" {
        assert!(v["droppedVerticals"].as_u64().unwrap() > 0);
    }
    let (_, again) = harness_lane(&raw);
    assert_eq!(out, again, "same request bytes ⇒ same result bytes");
}

#[test]
fn request_parse_is_closed_and_validated() {
    let ok = HarnessRequest::parse(&request(vec!["BIN.PEOPLE"], json!({}))).unwrap();
    assert_eq!(ok.output_limit_bytes, DEFAULT_OUTPUT_LIMIT);
    let mut extra: Value = serde_json::from_slice(&request(vec!["BIN.PEOPLE"], json!({}))).unwrap();
    extra["surprise"] = json!(1);
    assert!(HarnessRequest::parse(&serde_json::to_vec(&extra).unwrap()).is_err(), "closed model");
    let mut cap: Value = serde_json::from_slice(&request(vec!["BIN.PEOPLE"], json!({}))).unwrap();
    cap["capability"] = json!("company-search");
    assert!(HarnessRequest::parse(&serde_json::to_vec(&cap).unwrap()).is_err());
}

#[test]
fn dispatch_descriptor_covers_the_catalog_with_graph_positions() {
    let d = dispatch_json();
    assert_eq!(d["schema"], "aria-dispatch-v1");
    assert_eq!(d["capability"]["name"], HARNESS_CAPABILITY);
    assert_eq!(d["catalog"], 560);
    let bins = d["binaries"].as_array().unwrap();
    assert_eq!(bins.len(), 560);
    assert!(bins.iter().all(|b| b["graph"]["shape"].is_string() && b["resultDefinitionRef"].is_string()));
    assert_eq!(d["capability"]["output_limit_bytes"], DEFAULT_OUTPUT_LIMIT);
}
