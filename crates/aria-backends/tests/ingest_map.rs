//! T1 — lossless ingest (L5) and bounded admission (L8).
//!
//! The contract under test: nothing the host sent is lost, nothing Aria did
//! not receive is invented, and no untrusted payload can consume unbounded
//! resources.

use aria_engine_backends::ingest::{ingest, PayloadShape};
use aria_engine_backends::ipo::{anchor_of, canonical_json, sha256_hex, Limits, NodeOrigin};
use aria_engine_core::graph::NodeType;
use serde_json::{json, Value};

const N_MODES: usize = 64;
const DIM: usize = 32;

fn bytes_of(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}

fn ingest_value(v: &Value) -> aria_engine_backends::ingest::Ingested {
    ingest(&bytes_of(v), N_MODES, DIM, Limits::default()).expect("ingest must succeed")
}

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name),
    )
    .expect("fixture must be tracked");
    serde_json::from_str(&text).expect("fixture must be valid JSON")
}

// ---------------------------------------------------------------------------
// L5 — losslessness
// ---------------------------------------------------------------------------

/// The headline guarantee: the parsed payload comes back untouched, including
/// keys Aria has no idea about.
#[test]
fn the_complete_payload_survives_verbatim() {
    let payload = json!({
        "nodes": [{
            "id": 1,
            "label": "Stripe",
            "notes": "payments infrastructure",
            "type": "observation",
            "sector": "fintech",
            "arr_usd": 4_200_000_000_i64,
            "nested": { "deep": { "deeper": [1, 2, { "x": true }] } },
            "unknown_to_aria": "kept anyway"
        }],
        "edges": []
    });
    let out = ingest_value(&payload);
    assert_eq!(out.source, payload, "source must equal the parsed input");

    let record = &out.records[&1];
    assert_eq!(record.label.as_deref(), Some("Stripe"));
    assert_eq!(record.notes.as_deref(), Some("payments infrastructure"));
    assert_eq!(record.host_id, Some(json!(1)));
    assert_eq!(record.properties["sector"], json!("fintech"));
    assert_eq!(record.properties["arr_usd"], json!(4_200_000_000_i64));
    assert_eq!(record.properties["nested"]["deep"]["deeper"][2]["x"], json!(true));
    assert_eq!(record.properties["unknown_to_aria"], json!("kept anyway"));
}

/// This is the `dev_seed.rs` bug (issue `#19`) pinned down: label and text
/// used to be consumed to build the embedding and then thrown away, so a host
/// could not recover what a node had been shaped from.
#[test]
fn label_and_text_are_no_longer_discarded_after_embedding() {
    let payload = json!({
        "format": "aria-dev-seed-v1",
        "nodes": [{ "id": 7, "label": "Acme", "ntype": "observation", "text": "the full note body" }],
        "edges": []
    });
    let out = ingest_value(&payload);
    assert_eq!(out.shape, PayloadShape::DevSeed);

    let record = &out.records[&7];
    assert_eq!(record.label.as_deref(), Some("Acme"));
    assert_eq!(
        record.notes.as_deref(),
        Some("the full note body"),
        "the text the embedding came from must remain recoverable"
    );
}

/// The hash is over the bytes the host actually sent, not over a
/// re-serialization, so it is real byte provenance.
#[test]
fn the_source_hash_is_taken_over_the_exact_input_bytes() {
    let spaced = b"{\n  \"notes\" : [ \"a\" ]\n}";
    let out = ingest(spaced, N_MODES, DIM, Limits::default()).unwrap();
    assert_eq!(out.source_sha256, sha256_hex(spaced));
    assert_eq!(out.source_sha256.len(), 64);

    // Semantically identical but differently spelled input hashes differently
    // -- that is the point of byte provenance.
    let tight = br#"{"notes":["a"]}"#;
    let other = ingest(tight, N_MODES, DIM, Limits::default()).unwrap();
    assert_ne!(out.source_sha256, other.source_sha256);
    // ...while the parsed source is equal, which is the semantic guarantee.
    assert_eq!(out.source, other.source);
}

#[test]
fn records_carry_a_recomputable_anchor() {
    let payload = json!({ "nodes": [{ "id": 1, "label": "A" }], "edges": [] });
    let out = ingest_value(&payload);
    let expected = anchor_of(&payload["nodes"][0]);
    assert_eq!(out.records[&1].anchor, expected, "anchor must be recomputable");
}

// ---------------------------------------------------------------------------
// Shape recognition — structural, never semantic
// ---------------------------------------------------------------------------

#[test]
fn an_array_of_row_objects_is_read_as_a_spreadsheet() {
    let out = ingest_value(&fixture("tabular_market_sheet.json"));
    assert_eq!(out.shape, PayloadShape::Tabular);
    let plan = out.plan.expect("a tabular payload carries a structure report");
    assert_eq!(plan.report.n_rows, 8);
}

#[test]
fn an_explicit_graph_is_read_as_a_graph() {
    let out = ingest_value(&fixture("two_cluster_market.json"));
    assert_eq!(out.shape, PayloadShape::Graph);
    assert_eq!(out.g0.node_count(), 6);
    assert_eq!(out.g0.edge_count(), 5);
    assert!(out.plan.is_none(), "an explicit graph has no column structure");
}

#[test]
fn a_notes_array_becomes_one_node_per_note() {
    let out = ingest_value(&json!({ "notes": ["alpha", "beta", "gamma"] }));
    assert_eq!(out.shape, PayloadShape::Notes);
    assert_eq!(out.g0.node_count(), 3);
    assert_eq!(out.records.len(), 3);
    assert_eq!(out.records[&0].notes.as_deref(), Some("alpha"));
}

#[test]
fn a_rows_wrapper_is_equivalent_to_a_bare_array() {
    let bare = ingest_value(&json!([{ "a": 1 }, { "a": 2 }]));
    let wrapped = ingest_value(&json!({ "rows": [{ "a": 1 }, { "a": 2 }] }));
    assert_eq!(bare.shape, PayloadShape::Tabular);
    assert_eq!(wrapped.shape, PayloadShape::Tabular);
    assert_eq!(bare.g0.node_count(), wrapped.g0.node_count());
}

/// Re-entry must terminate. A prior envelope is unwrapped exactly once,
/// through `.source`; unwrapping repeatedly would let a chain of runs bury the
/// real payload.
#[test]
fn a_prior_envelope_is_unwrapped_exactly_once() {
    let inner = json!({ "notes": ["original"] });
    let wrapped = json!({
        "schema": "aria-telemetry-query-v1",
        "version": 1,
        "source": inner.clone()
    });
    let out = ingest_value(&wrapped);
    assert_eq!(out.source, inner, "the real payload must resurface");
    assert_eq!(out.shape, PayloadShape::Notes);

    // Double-wrapped: one unwrap leaves an envelope, which is then read on its
    // own terms rather than unwrapped again.
    let double = json!({
        "schema": "aria-telemetry-query-v1",
        "version": 1,
        "source": wrapped
    });
    let out = ingest(&bytes_of(&double), N_MODES, DIM, Limits::default());
    assert!(
        out.is_err(),
        "one unwrap only; the inner envelope is not a recognized payload shape"
    );
}

#[test]
fn an_unrecognized_payload_is_a_typed_config_error() {
    for bad in [json!(42), json!("a string"), json!({ "mystery": 1 }), json!(null)] {
        let err = ingest(&bytes_of(&bad), N_MODES, DIM, Limits::default())
            .expect_err("must be rejected");
        assert!(
            matches!(err, aria_engine_core::error::AriaError::Config(_)),
            "{bad}: {err}"
        );
    }
}

#[test]
fn malformed_json_is_rejected_before_any_graph_work() {
    let err = ingest(b"{ not json", N_MODES, DIM, Limits::default()).expect_err("must reject");
    assert!(err.to_string().contains("not valid JSON"), "{err}");
}

// ---------------------------------------------------------------------------
// Inv3 at the boundary — rejected before Init, never a partial graph
// ---------------------------------------------------------------------------

/// A relation the host asserted must never be silently dropped: that would be
/// the one loss of information that matters most.
#[test]
fn a_dangling_edge_endpoint_is_rejected_before_init() {
    let payload = json!({
        "nodes": [{ "id": 1, "label": "A" }],
        "edges": [{ "from": 1, "to": 999, "type": "refines" }]
    });
    let err = ingest(&bytes_of(&payload), N_MODES, DIM, Limits::default())
        .expect_err("dangling edge must be rejected");
    assert!(err.to_string().contains("dangling"), "{err}");
}

#[test]
fn a_duplicate_host_id_is_rejected() {
    let payload = json!({
        "nodes": [{ "id": 5, "label": "A" }, { "id": 5, "label": "B" }],
        "edges": []
    });
    let err = ingest(&bytes_of(&payload), N_MODES, DIM, Limits::default())
        .expect_err("duplicate id must be rejected");
    assert!(err.to_string().contains("duplicate id"), "{err}");
}

#[test]
fn every_ingested_graph_is_graph_ok() {
    for name in ["two_cluster_market.json", "tabular_market_sheet.json"] {
        let out = ingest_value(&fixture(name));
        assert!(out.g0.ok(DIM), "{name} must produce a GraphOK G0");
        for node in out.g0.nodes.values() {
            assert_eq!(node.embedding.len(), DIM);
            assert!(node.embedding.iter().all(|v| v.is_finite()));
        }
    }
}

/// An unknown type is a `Custom` label, not an error. Refusing an unfamiliar
/// vocabulary would lock out every host whose taxonomy Aria has not seen.
#[test]
fn an_unknown_node_type_becomes_custom() {
    let out = ingest_value(&json!({
        "nodes": [{ "id": 1, "label": "A", "type": "market_segment" }],
        "edges": []
    }));
    assert_eq!(
        out.g0.nodes[&1].node_type,
        NodeType::Custom("market_segment".into())
    );
}

/// A host id that is not an integer (a ticker, a UUID) still works: an id is
/// allocated, the original is preserved, and edges may reference either form.
#[test]
fn non_integer_host_ids_are_allocated_and_remain_referenceable() {
    let out = ingest_value(&json!({
        "nodes": [
            { "id": "STRP", "label": "Stripe" },
            { "id": "ADYN", "label": "Adyen" }
        ],
        "edges": [{ "from": "STRP", "to": "ADYN", "type": "refines" }]
    }));
    assert_eq!(out.g0.node_count(), 2);
    assert_eq!(out.g0.edge_count(), 1, "the string-keyed edge must resolve");

    let host_ids: Vec<&Value> = out
        .records
        .values()
        .filter_map(|r| r.host_id.as_ref())
        .collect();
    assert!(host_ids.contains(&&json!("STRP")));
    assert!(host_ids.contains(&&json!("ADYN")));
}

#[test]
fn an_edge_naming_an_unknown_host_id_is_rejected() {
    let err = ingest(
        &bytes_of(&json!({
            "nodes": [{ "id": "A", "label": "a" }],
            "edges": [{ "from": "A", "to": "NOPE" }]
        })),
        N_MODES,
        DIM,
        Limits::default(),
    )
    .expect_err("unknown endpoint must be rejected");
    assert!(err.to_string().contains("does not name any ingested node"), "{err}");
}

/// Host-supplied ids and allocated ids must never collide.
#[test]
fn allocation_stays_ahead_of_host_supplied_ids() {
    let out = ingest_value(&json!({
        "nodes": [
            { "id": 100, "label": "explicit" },
            { "id": "opaque", "label": "allocated" },
            { "id": 101, "label": "explicit again" }
        ],
        "edges": []
    }));
    assert_eq!(out.g0.node_count(), 3, "no id collision may drop a node");
}

// ---------------------------------------------------------------------------
// Tabular graph construction — where relations come from
// ---------------------------------------------------------------------------

/// Rows plus facets: 8 rows + (2 sectors + 4 regions + 3 countries + 3 stages)
/// = 8 + 12 = 20 nodes. Every row links to each of its four facet values.
#[test]
fn a_spreadsheet_becomes_rows_linked_through_shared_facets() {
    let out = ingest_value(&fixture("tabular_market_sheet.json"));
    assert_eq!(out.g0.node_count(), 20, "8 rows + 12 distinct facet values");

    let row_nodes = out
        .g0
        .nodes
        .values()
        .filter(|n| n.node_type == NodeType::Observation)
        .count();
    assert_eq!(row_nodes, 8);

    // 8 rows x 4 facet columns = 32 row->facet edges, plus the hierarchy.
    assert!(
        out.g0.edge_count() >= 32,
        "expected at least the row->facet edges, got {}",
        out.g0.edge_count()
    );
}

/// The measured `region → country` dependency must appear as a hierarchy edge,
/// under the documented direction convention: `Refines` reads "the target
/// refines the source", so the edge runs coarse → fine (country → region).
#[test]
fn a_measured_dependency_becomes_a_coarse_to_fine_hierarchy_edge() {
    let out = ingest_value(&fixture("tabular_market_sheet.json"));

    let id_of = |column: &str, value: &str| -> u64 {
        *out.records
            .iter()
            .find(|(_, r)| {
                r.properties.get("column").and_then(Value::as_str) == Some(column)
                    && r.properties.get("value").and_then(Value::as_str) == Some(value)
            })
            .unwrap_or_else(|| panic!("no facet node for {column}={value}"))
            .0
    };

    let us = id_of("country", "US");
    let us_west = id_of("region", "us-west");
    assert!(
        out.g0.edges.iter().any(|e| e.from == us
            && e.to == us_west
            && e.edge_type == aria_engine_core::graph::EdgeType::Refines),
        "expected country(US) -> region(us-west) as Refines"
    );
}

/// Permutation invariance end to end: a shuffled spreadsheet must produce a
/// byte-identical graph, because ids follow content order rather than arrival.
#[test]
fn shuffling_the_spreadsheet_yields_an_identical_graph() {
    let sheet = fixture("tabular_market_sheet.json");
    let mut shuffled = sheet.as_array().unwrap().clone();
    shuffled.reverse();
    shuffled.swap(0, 5);

    let a = ingest_value(&sheet);
    let b = ingest_value(&Value::Array(shuffled));

    assert_eq!(
        canonical_json(&serde_json::to_value(&a.g0).unwrap()),
        canonical_json(&serde_json::to_value(&b.g0).unwrap()),
        "row order must not reach the graph"
    );
    assert_eq!(
        canonical_json(&serde_json::to_value(&a.records).unwrap()),
        canonical_json(&serde_json::to_value(&b.records).unwrap()),
        "row order must not reach the records"
    );
}

/// Row identity is the whole row's anchor and *every* key column is reported,
/// so no column is crowned by name (resolves Q-2026-08-31-1).
#[test]
fn row_identity_reports_all_key_columns_without_choosing_one() {
    let out = ingest_value(&fixture("tabular_market_sheet.json"));
    let row = out
        .records
        .values()
        .find(|r| {
            r.properties.get("ticker").and_then(Value::as_str) == Some("STRP")
        })
        .expect("the Stripe row");

    let host_id = row.host_id.as_ref().expect("key columns exist");
    let keys = host_id.as_object().expect("host_id is an object of key columns");
    assert_eq!(keys["ticker"], json!("STRP"));
    assert_eq!(keys["company"], json!("Stripe"));
    assert_eq!(keys.len(), 2, "both candidate keys, neither elevated");
}

#[test]
fn a_tabular_row_preserves_every_cell_in_its_record() {
    let out = ingest_value(&fixture("tabular_market_sheet.json"));
    let row = out
        .records
        .values()
        .find(|r| r.properties.get("ticker").and_then(Value::as_str) == Some("OWKN"))
        .expect("the Owkin row");
    for key in ["ticker", "company", "sector", "region", "country", "stage", "note"] {
        assert!(row.properties.contains_key(key), "lost cell '{key}'");
    }
    assert_eq!(row.properties["country"], json!("FR"));
}

// ---------------------------------------------------------------------------
// Provenance — nothing Aria did not receive is claimed as host data
// ---------------------------------------------------------------------------

#[test]
fn ingest_marks_exactly_the_host_elements_as_input() {
    let out = ingest_value(&fixture("two_cluster_market.json"));
    assert_eq!(out.origins.input_nodes.len(), 6);
    assert_eq!(out.origins.input_edges.len(), 5);
    for id in out.g0.nodes.keys() {
        assert_eq!(out.origins.node(*id), NodeOrigin::Input);
    }
    // A node the payload never mentioned is not claimed as input.
    assert_eq!(out.origins.node(4_242), NodeOrigin::Transform);
}

// ---------------------------------------------------------------------------
// L8 — bounded admission
// ---------------------------------------------------------------------------

/// The byte ceiling is checked before parsing, so an oversized payload never
/// reaches the allocator.
#[test]
fn an_oversized_payload_is_refused_before_parsing() {
    let limits = Limits {
        max_input_bytes: 16,
        ..Limits::default()
    };
    let err = ingest(
        &bytes_of(&fixture("tabular_market_sheet.json")),
        N_MODES,
        DIM,
        limits,
    )
    .expect_err("must refuse");
    assert!(err.to_string().contains("max_input_bytes"), "{err}");
}

#[test]
fn the_node_ceiling_is_enforced() {
    let limits = Limits {
        max_nodes: 3,
        ..Limits::default()
    };
    let err = ingest(
        &bytes_of(&fixture("tabular_market_sheet.json")),
        N_MODES,
        DIM,
        limits,
    )
    .expect_err("must refuse");
    assert!(err.to_string().contains("max_nodes"), "{err}");
}

#[test]
fn the_edge_ceiling_is_enforced() {
    let limits = Limits {
        max_edges: 2,
        ..Limits::default()
    };
    let err = ingest(
        &bytes_of(&fixture("tabular_market_sheet.json")),
        N_MODES,
        DIM,
        limits,
    )
    .expect_err("must refuse");
    assert!(err.to_string().contains("max_edges"), "{err}");
}

#[test]
fn zero_dimensions_are_refused() {
    let payload = bytes_of(&json!({ "notes": ["a"] }));
    assert!(ingest(&payload, 0, DIM, Limits::default()).is_err());
    assert!(ingest(&payload, N_MODES, 0, Limits::default()).is_err());
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn ingest_is_deterministic_for_identical_input() {
    let payload = fixture("tabular_market_sheet.json");
    let a = ingest_value(&payload);
    let b = ingest_value(&payload);
    assert_eq!(
        canonical_json(&serde_json::to_value(&a.g0).unwrap()),
        canonical_json(&serde_json::to_value(&b.g0).unwrap())
    );
    assert_eq!(a.source_sha256, b.source_sha256);
}

/// Embeddings come from the host's own content through the same deterministic
/// encoder the engine uses — never from entropy, and never a zero vector that
/// would collapse every node onto one point of 𝒵.
#[test]
fn embeddings_are_content_derived_and_distinguish_distinct_content() {
    let out = ingest_value(&json!({ "notes": ["alpha content", "entirely different"] }));
    let embeddings: Vec<&Vec<f64>> = out.g0.nodes.values().map(|n| &n.embedding).collect();
    assert_eq!(embeddings.len(), 2);
    assert_ne!(
        embeddings[0], embeddings[1],
        "different notes must land at different points of Z"
    );
    assert!(embeddings[0].iter().any(|v| v.abs() > 0.0), "not a zero vector");

    // Identical content must land identically.
    let same = ingest_value(&json!({ "notes": ["alpha content", "alpha content"] }));
    let e: Vec<&Vec<f64>> = same.g0.nodes.values().map(|n| &n.embedding).collect();
    assert_eq!(e[0], e[1]);
}
