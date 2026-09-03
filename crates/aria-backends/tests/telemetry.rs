//! T6 — the callable transform, and the laws it has to satisfy.
//!
//! `transform` is the whole product:
//! `Node(payload, config, seed) = E( I(payload), Run_Φ(config, seed), Obs )`.
//! These tests hold that composition to L3 (conservativity), L4 (determinism),
//! L5 (losslessness), L8 (boundedness), and the zero-authority contract.

use aria_engine_backends::ipo::{
    canonical_json, validate_envelope, Limits, NodeOrigin, TelemetryQuery, TELEMETRY_QUERY_V1,
};
use aria_engine_backends::runner::{run_with_graph, RefPredictor};
use aria_engine_backends::telemetry::{
    apply_return_keys, node_profile_config, transform, TelemetryRequest,
};
use aria_engine_backends::{ingest, SimPredictor};
use aria_engine_core::config::AriaConfig;
use aria_engine_core::policy::MatchPolicy;
use serde_json::{json, Value};

const N_MODES: usize = 64;
const DIM: usize = 32;
const STEPS: u64 = 16;

fn config() -> AriaConfig {
    AriaConfig {
        n_modes: N_MODES,
        latent_dim: DIM,
        seed: Some(1),
        allow_sub_spec_dims: true,
        ..AriaConfig::default()
    }
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name),
    )
    .expect("fixture must be tracked")
}

fn request(payload: Vec<u8>) -> TelemetryRequest {
    TelemetryRequest {
        payload,
        config: config(),
        steps: STEPS,
        predictor: None,
        observe: false,
        limits: Limits::default(),
        respect_config_policy: false,
        query: None,
        ocid: aria_engine_backends::ocid::OcidRequest::default(),
    }
}

fn run_fixture(name: &str) -> aria_engine_backends::ipo::TelemetryEnvelope {
    transform(request(fixture_bytes(name))).expect("transform must succeed")
}

// ---------------------------------------------------------------------------
// The guarantee: exit 0 ⇒ a valid, complete envelope
// ---------------------------------------------------------------------------

#[test]
fn a_spreadsheet_produces_a_valid_envelope() {
    let env = run_fixture("tabular_market_sheet.json");
    assert_eq!(env.schema, TELEMETRY_QUERY_V1);
    assert_eq!(env.version, 1);
    assert!(env.receipt.invariants_ok, "{:?}", env.receipt.failures);

    let doc = serde_json::to_value(&env).unwrap();
    validate_envelope(&doc).expect("the envelope must satisfy its own contract");
}

#[test]
fn an_explicit_graph_produces_a_valid_envelope() {
    let env = run_fixture("two_cluster_market.json");
    let doc = serde_json::to_value(&env).unwrap();
    validate_envelope(&doc).expect("valid");
    assert!(
        env.structure.is_none(),
        "an explicit graph has no column structure to report"
    );
}

/// No prior training is required for a valid body — the single most important
/// property for adoption (`issues.md` overlay; `#17` is a 0.3.0 ship gate, not
/// a node gate).
#[test]
fn no_predictor_file_is_required() {
    let env = run_fixture("tabular_market_sheet.json");
    assert_eq!(env.receipt.predictor, "sim");
    assert!(env.receipt.invariants_ok);
    assert!(env.graph.node_count() > 0);
}

#[test]
fn query_and_receipt_are_always_present() {
    let env = run_fixture("tabular_market_sheet.json");
    let doc = serde_json::to_value(&env).unwrap();
    assert!(doc["query"]["match"]["nodes"].is_string());
    assert!(doc["query"]["where"]["tau"].is_number());
    assert!(doc["receipt"]["invariants_ok"].is_boolean());
}

// ---------------------------------------------------------------------------
// L4 — determinism
// ---------------------------------------------------------------------------

#[test]
fn identical_input_yields_byte_identical_output() {
    let a = run_fixture("tabular_market_sheet.json");
    let b = run_fixture("tabular_market_sheet.json");
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&b).unwrap()
    );
}

/// The strong form of permutation invariance: a shuffled spreadsheet is not
/// merely *similar*, it is byte-identical, because ids follow content order.
/// This is the demonstration that the transform has no arrival-order bias.
#[test]
fn a_shuffled_spreadsheet_yields_a_byte_identical_envelope() {
    let original: Value = serde_json::from_slice(&fixture_bytes("tabular_market_sheet.json")).unwrap();
    let mut rows = original.as_array().unwrap().clone();
    rows.reverse();
    rows.swap(0, 4);
    rows.swap(2, 7);
    let shuffled = Value::Array(rows);

    let a = transform(request(serde_json::to_vec(&original).unwrap())).unwrap();
    let b = transform(request(serde_json::to_vec(&shuffled).unwrap())).unwrap();

    // `source` and its hash necessarily differ (different bytes arrived), so
    // compare everything that describes the *structure* Aria derived.
    assert_eq!(
        canonical_json(&serde_json::to_value(&a.graph).unwrap()),
        canonical_json(&serde_json::to_value(&b.graph).unwrap()),
        "row order must not reach the graph"
    );
    assert_eq!(
        canonical_json(&serde_json::to_value(&a.records).unwrap()),
        canonical_json(&serde_json::to_value(&b.records).unwrap()),
        "row order must not reach the records"
    );
    assert_eq!(
        canonical_json(&serde_json::to_value(&a.structure).unwrap()),
        canonical_json(&serde_json::to_value(&b.structure).unwrap()),
        "row order must not reach the measurements"
    );
    assert_eq!(
        canonical_json(&serde_json::to_value(&a.tags).unwrap()),
        canonical_json(&serde_json::to_value(&b.tags).unwrap()),
        "row order must not reach the map view"
    );
}

#[test]
fn a_different_seed_is_allowed_to_change_the_trajectory_but_not_the_payload() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.config.seed = Some(999);
    let other = transform(req).unwrap();
    let base = run_fixture("tabular_market_sheet.json");

    assert_eq!(other.source, base.source, "the payload is seed-independent");
    assert_eq!(other.source_sha256, base.source_sha256);
    assert_eq!(
        other.receipt.input_node_count, base.receipt.input_node_count,
        "ingest is seed-independent"
    );
}

// ---------------------------------------------------------------------------
// L3 — conservativity: the transform does not alter Φ
// ---------------------------------------------------------------------------

/// The node path must run the *same* action sequence a bare
/// `run_with_graph` would, or telemetry has changed the machine it claims only
/// to observe.
#[test]
fn the_node_path_runs_the_same_action_sequence_as_a_bare_run() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let cfg = node_profile_config(&config());
    let ingested = ingest(&payload, cfg.n_modes, cfg.latent_dim, Limits::default()).unwrap();

    // Mirror the ceiling adjustment `transform` makes so the two runs are
    // configured identically.
    let mut bare_cfg = cfg.clone();
    let admitted = ingested.g0.size() + usize::try_from(STEPS).unwrap();
    if bare_cfg.max_graph_size < admitted {
        bare_cfg.max_graph_size = admitted;
    }
    let predictor = RefPredictor::Sim(SimPredictor::new(bare_cfg.n_modes, bare_cfg.latent_dim));
    let bare = run_with_graph(bare_cfg, STEPS, predictor, ingested.g0.clone()).unwrap();

    let env = transform(request(payload)).unwrap();

    assert_eq!(env.receipt.steps, bare.summary.steps);
    assert_eq!(env.receipt.t, bare.summary.t);
    assert_eq!(env.receipt.node_count, bare.summary.node_count);
    assert_eq!(env.receipt.invariants_ok, bare.summary.invariants_ok);
    assert!(
        (env.receipt.energy - bare.summary.energy).abs() < 1e-15,
        "Inv1 energy must be untouched by the projection"
    );
    assert!((env.receipt.residual - bare.summary.residual).abs() < 1e-15);
}

/// L9 / ℂ2 through the node path: asking for the ledger must not change the run.
#[test]
fn requesting_the_ledger_does_not_change_the_run() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let plain = transform(request(payload.clone())).unwrap();

    let mut observed_req = request(payload);
    observed_req.observe = true;
    let observed = transform(observed_req).unwrap();

    assert!(observed.ledger.is_some(), "the ledger was requested");
    assert!(plain.ledger.is_none());

    assert_eq!(plain.receipt.steps, observed.receipt.steps);
    assert_eq!(plain.receipt.t, observed.receipt.t);
    assert_eq!(plain.receipt.node_count, observed.receipt.node_count);
    assert_eq!(plain.receipt.edge_count, observed.receipt.edge_count);
    assert!((plain.receipt.residual - observed.receipt.residual).abs() < 1e-15);
    assert_eq!(
        canonical_json(&serde_json::to_value(&plain.graph).unwrap()),
        canonical_json(&serde_json::to_value(&observed.graph).unwrap()),
        "the observer must not perturb the graph"
    );
}

#[test]
fn the_ledger_reports_the_steps_it_observed() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.observe = true;
    let env = transform(req).unwrap();
    let ledger = env.ledger.expect("ledger present");
    assert_eq!(ledger["steps"], json!(STEPS));
    assert!(ledger["functional"]["magnitude"].is_number());
    assert!(ledger["certificate"]["certified"].is_boolean());
}

// ---------------------------------------------------------------------------
// L5 — losslessness through the whole pipeline
// ---------------------------------------------------------------------------

#[test]
fn the_payload_survives_the_entire_transform() {
    let bytes = fixture_bytes("tabular_market_sheet.json");
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    let env = transform(request(bytes)).unwrap();

    assert_eq!(env.source, original, "source must equal the parsed input");

    // Every row's every cell is still recoverable after Φ ran.
    let rows: Vec<&aria_engine_backends::ipo::NodeRecord> = env
        .records
        .values()
        .filter(|r| r.properties.contains_key("ticker"))
        .collect();
    assert_eq!(rows.len(), 8);
    for row in rows {
        for key in ["ticker", "company", "sector", "region", "country", "stage", "note"] {
            assert!(row.properties.contains_key(key), "lost '{key}'");
        }
    }
}

#[test]
fn unknown_host_keys_survive() {
    let payload = json!([
        { "k": "a", "aria_has_never_seen_this": { "deep": [1, 2, 3] } },
        { "k": "b", "aria_has_never_seen_this": { "deep": [4] } }
    ]);
    let env = transform(request(serde_json::to_vec(&payload).unwrap())).unwrap();
    assert_eq!(env.source, payload);
    let kept = env
        .records
        .values()
        .filter(|r| r.properties.contains_key("aria_has_never_seen_this"))
        .count();
    assert_eq!(kept, 2);
}

// ---------------------------------------------------------------------------
// Provenance — the transform never claims host authorship
// ---------------------------------------------------------------------------

/// Φ absorbs latents as new nodes under merge. Those must appear as
/// `transform`, never as `input`, and the receipt must count them separately.
#[test]
fn phi_derived_nodes_are_marked_transform_and_counted_separately() {
    let env = run_fixture("tabular_market_sheet.json");

    let input = env
        .graph
        .nodes
        .iter()
        .filter(|n| n.origin == NodeOrigin::Input)
        .count();
    let derived = env
        .graph
        .nodes
        .iter()
        .filter(|n| n.origin == NodeOrigin::Transform)
        .count();

    assert_eq!(input, env.receipt.input_node_count);
    assert_eq!(derived, env.receipt.transform_node_count);
    assert_eq!(input + derived, env.receipt.node_count);
    assert_eq!(input, 20, "8 rows + 12 facet values came from the payload");

    // A transform node has no host record: it was never in the payload.
    for node in env.graph.nodes.iter().filter(|n| n.origin == NodeOrigin::Transform) {
        assert!(
            !env.records.contains_key(&node.id),
            "node {} is Φ-derived and must not carry a host record",
            node.id
        );
    }
}

// ---------------------------------------------------------------------------
// The node profile (issue #11)
// ---------------------------------------------------------------------------

#[test]
fn the_node_profile_uses_merge_without_moving_the_global_default() {
    assert_eq!(
        MatchPolicy::default(),
        MatchPolicy::Identity,
        "the spec-minimal default must not move"
    );
    let profiled = node_profile_config(&config());
    assert_eq!(profiled.match_policy, MatchPolicy::Merge);

    let env = run_fixture("tabular_market_sheet.json");
    assert_eq!(env.receipt.match_policy, "merge");
    assert_eq!(env.tags.policy, "merge");
}

#[test]
fn a_caller_can_pin_its_own_policy() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.config.match_policy = MatchPolicy::Identity;
    req.respect_config_policy = true;
    let env = transform(req).unwrap();
    assert_eq!(env.receipt.match_policy, "identity");
}

// ---------------------------------------------------------------------------
// The map view
// ---------------------------------------------------------------------------

/// A sheet must yield a genuinely useful map: shared facet values are what
/// connect rows, so the edge count has to reflect real structure.
#[test]
fn the_map_is_useful_not_empty() {
    let env = run_fixture("tabular_market_sheet.json");
    assert!(env.receipt.edge_count >= 32, "8 rows x 4 facets");
    assert!(!env.tags.probable_edges.is_empty());
    assert!(!env.tags.binary_index.is_empty());
    for key in ["has_sector", "has_region", "has_country", "has_stage"] {
        assert!(env.tags.binary_index.contains_key(key), "missing {key}");
    }
}

/// The probable view must be *sparse*. A view that connects most pairs asserts
/// that everything relates to everything — measured at 222 edges over 21 nodes
/// with a static τ, which is why eligibility is mutual-kNN ∧ scale-invariant τ.
#[test]
fn the_probable_view_is_sparse_not_near_complete() {
    let env = run_fixture("tabular_market_sheet.json");
    let n = env.graph.node_count();
    let complete = n * (n - 1) / 2;
    let probable = env.tags.probable_edges.len();
    assert!(
        probable * 2 < complete,
        "probable view has {probable} of {complete} possible pairs — too dense to be a signal"
    );
}

/// Clusters are a view, and every member must be a node of the exported graph
/// or a renderer cannot draw them.
#[test]
fn clusters_reference_only_exported_nodes() {
    let env = run_fixture("tabular_market_sheet.json");
    let ids: std::collections::BTreeSet<u64> = env.graph.nodes.iter().map(|n| n.id).collect();
    assert!(!env.tags.clusters.is_empty(), "a connected map should bisect");
    for c in &env.tags.clusters {
        assert!(!c.node_ids.is_empty(), "an empty cluster is not a cluster");
        for id in &c.node_ids {
            assert!(ids.contains(id), "cluster references unknown node {id}");
        }
    }
}

#[test]
fn an_edgeless_graph_reports_no_clusters_rather_than_a_fake_one() {
    let env = transform(request(
        serde_json::to_vec(&json!({ "notes": ["alpha", "beta"] })).unwrap(),
    ))
    .unwrap();
    assert!(
        env.tags.clusters.is_empty(),
        "no structural edges means no cut to report"
    );
    // ...and it is an empty array on the wire, never null.
    let doc = serde_json::to_value(&env).unwrap();
    assert_eq!(doc["tags"]["clusters"], json!([]));
}

// ---------------------------------------------------------------------------
// L8 — boundedness
// ---------------------------------------------------------------------------

#[test]
fn steps_beyond_the_ceiling_are_refused_before_phi() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.steps = 10_000;
    req.limits.max_steps = 100;
    let err = transform(req).expect_err("must refuse");
    assert!(err.to_string().contains("max_steps"), "{err}");
}

#[test]
fn each_ingest_ceiling_is_enforced_through_the_transform() {
    for (label, limits) in [
        (
            "max_input_bytes",
            Limits {
                max_input_bytes: 8,
                ..Limits::default()
            },
        ),
        (
            "max_nodes",
            Limits {
                max_nodes: 2,
                ..Limits::default()
            },
        ),
        (
            "max_edges",
            Limits {
                max_edges: 1,
                ..Limits::default()
            },
        ),
    ] {
        let mut req = request(fixture_bytes("tabular_market_sheet.json"));
        req.limits = limits;
        let err = transform(req).expect_err("must refuse");
        assert!(err.to_string().contains(label), "{label}: {err}");
    }
}

#[test]
fn the_receipt_reports_the_limits_that_bound_the_run() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.limits.max_steps = 64;
    let env = transform(req).unwrap();
    assert_eq!(env.receipt.limits.max_steps, 64);
}

// ---------------------------------------------------------------------------
// Failure modes
// ---------------------------------------------------------------------------

#[test]
fn malformed_and_unrecognized_payloads_are_typed_errors() {
    for bad in [
        b"{ not json".to_vec(),
        serde_json::to_vec(&json!(42)).unwrap(),
        serde_json::to_vec(&json!({ "mystery": true })).unwrap(),
    ] {
        let err = transform(request(bad)).expect_err("must reject");
        assert!(matches!(
            err,
            aria_engine_core::error::AriaError::Config(_)
        ));
    }
}

#[test]
fn a_dangling_edge_is_refused_by_the_transform() {
    let payload = json!({
        "nodes": [{ "id": 1, "label": "A" }],
        "edges": [{ "from": 1, "to": 77 }]
    });
    let err = transform(request(serde_json::to_vec(&payload).unwrap())).expect_err("must reject");
    assert!(err.to_string().contains("dangling"), "{err}");
}

// ---------------------------------------------------------------------------
// The zero-authority contract
// ---------------------------------------------------------------------------

/// The envelope has no field in which to assign Trust, complete a Goal, or
/// score coverage. This test is the structural statement of that: if someone
/// adds one, it fails.
#[test]
fn the_envelope_carries_no_authority_fields() {
    let env = run_fixture("tabular_market_sheet.json");
    let doc = serde_json::to_value(&env).unwrap();
    let text = serde_json::to_string(&doc).unwrap().to_lowercase();

    for forbidden in [
        "\"trust\"",
        "\"trust_state\"",
        "\"goal_complete\"",
        "\"coverage_score\"",
        "\"coverage_completeness\"",
        "\"verdict\"",
        "\"recommendation\"",
        "\"accepted\"",
    ] {
        assert!(
            !text.contains(forbidden),
            "the envelope must not contain {forbidden} — Aria is a transform, not a judge"
        );
    }
    // The receipt reports invariants and nothing that resembles a judgement.
    let receipt = doc["receipt"].as_object().unwrap();
    for key in receipt.keys() {
        assert!(
            !key.contains("trust") && !key.contains("score") && !key.contains("goal"),
            "receipt key '{key}' looks like authority"
        );
    }
}

// ---------------------------------------------------------------------------
// query.return projection
// ---------------------------------------------------------------------------

#[test]
fn return_keys_can_withhold_optional_sections() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.query = Some(TelemetryQuery {
        return_keys: vec!["query".into(), "receipt".into()],
        ..TelemetryQuery::default()
    });
    let env = apply_return_keys(transform(req).unwrap());

    assert!(env.structure.is_none());
    assert!(env.records.is_empty());
    assert_eq!(env.graph.node_count(), 0);
    assert_eq!(env.source, Value::Null);

    // The guaranteed pair survives regardless.
    let doc = serde_json::to_value(&env).unwrap();
    assert!(doc["query"].is_object());
    assert!(doc["receipt"]["invariants_ok"].is_boolean());
}

/// Withholding is applied *after* the full envelope is built and validated, so
/// a projection can never be used to hide a defect.
#[test]
fn the_full_envelope_is_validated_before_anything_is_withheld() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.query = Some(TelemetryQuery {
        return_keys: vec!["query".into(), "receipt".into()],
        ..TelemetryQuery::default()
    });
    // transform() validates internally; if it returns Ok the full document was
    // conformant even though the caller asked for two sections.
    let full = transform(req).unwrap();
    validate_envelope(&serde_json::to_value(&full).unwrap()).expect("full document is valid");
    assert!(full.structure.is_some(), "transform returns the whole body");
}

#[test]
fn tau_is_reported_as_measured_not_as_requested() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"));
    req.config.merge_tau = 0.25;
    req.query = Some(TelemetryQuery::default());
    let env = transform(req).unwrap();
    assert!(
        (env.query.where_clause.tau - 0.25).abs() < 1e-15,
        "the echoed τ must be the radius the run actually used"
    );
    assert!((env.tags.tau - 0.25).abs() < 1e-15);
}
