//! T0 — the public telemetry contract.
//!
//! These tests own the wire format. They must fail if the envelope shape, the
//! canonical-byte rule, the anchor derivation, or the structural validator
//! drifts, because those four things are what a host depends on
//! without being able to inspect Aria's internals.
//!
//! Production `src/` carries no test modules (objective O7); everything that
//! exercises this contract lives here.

use std::collections::{BTreeMap, BTreeSet};

use aria_engine_backends::ipo::{
    anchor_of, binary_type_for_edge, binary_type_for_node, canonical_json, sha256_hex,
    validate_envelope, Cluster, ColumnRole, ColumnStat, FunctionalDep, GraphIpo, IpoEdge, IpoError,
    Limits, NodeOrigin, NodeRecord, OriginIndex, QueryMatch, QueryWhere, RoleThresholds,
    StructureReport, TaggingState, TelemetryEnvelope, TelemetryQuery, TelemetryReceipt,
    BINARY_TYPE_CUSTOM, GRAPH_IPO_V1, TELEMETRY_QUERY_V1, TELEMETRY_VERSION,
};
use aria_engine_core::graph::{EdgeType, Graph, GraphOp, NodeType};
use serde_json::{json, Value};

const DIM: usize = 4;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The two-cluster topology the spectral suite already uses: three fintech
/// nodes, three healthcare nodes, `Refines` inside each cluster and one
/// `CausallyPrecedes` bridge between them.
fn two_cluster_graph() -> Graph {
    let points: [(u64, [f64; DIM]); 6] = [
        (1, [1.00, 0.90, 0.00, 0.00]),
        (2, [0.95, 1.00, 0.05, 0.00]),
        (3, [0.90, 0.95, 0.00, 0.05]),
        (4, [0.00, 0.05, 1.00, 0.90]),
        (5, [0.05, 0.00, 0.95, 1.00]),
        (6, [0.00, 0.00, 0.90, 0.95]),
    ];
    let mut g = Graph::empty();
    for (id, emb) in points {
        g.apply(
            &GraphOp::AddNode {
                id,
                ntype: NodeType::Observation,
                emb: emb.to_vec(),
                ts: id,
            },
            DIM,
        )
        .expect("fixture node must apply");
    }
    for (from, to, etype) in [
        (1, 2, EdgeType::Refines),
        (2, 3, EdgeType::Refines),
        (4, 5, EdgeType::Refines),
        (5, 6, EdgeType::Refines),
        (3, 4, EdgeType::CausallyPrecedes),
    ] {
        g.apply(&GraphOp::AddEdge { from, to, etype }, DIM)
            .expect("fixture edge must apply");
    }
    g
}

/// Every node and edge of the two-cluster fixture is host-supplied.
fn all_input_origins(g: &Graph) -> OriginIndex {
    OriginIndex {
        input_nodes: g.nodes.keys().copied().collect(),
        input_edges: g.edges.iter().map(|e| (e.from, e.to)).collect(),
    }
}

/// A single ingested record, as the reference envelope carries it.
fn reference_records() -> BTreeMap<u64, NodeRecord> {
    let mut records = BTreeMap::new();
    let record_body = json!({ "id": 1, "label": "Stripe", "sector": "fintech" });
    records.insert(
        1,
        NodeRecord {
            id: 1,
            host_id: Some(json!(1)),
            label: Some("Stripe".into()),
            notes: None,
            properties: json!({ "sector": "fintech" })
                .as_object()
                .cloned()
                .expect("object"),
            binary_type: Some("pcvc.research.json-map".into()),
            anchor: anchor_of(&record_body),
        },
    );
    records
}

/// The structure report the reference envelope reports.
fn reference_structure() -> StructureReport {
    StructureReport {
        n_rows: 8,
        columns: vec![ColumnStat {
            column: "ticker".into(),
            role: ColumnRole::KeyAnchor,
            rule: "coverage == 1.0 && uniqueness == 1.0".into(),
            n_rows: 8,
            present: 8,
            distinct: 8,
            coverage: 1.0,
            uniqueness: 1.0,
            singletons: 8,
        }],
        functional_deps: vec![FunctionalDep {
            from: "region".into(),
            to: "country".into(),
            distinct_from: 3,
            distinct_to: 3,
            support: 8,
        }],
        thresholds: RoleThresholds::default(),
        dependency_scan_complete: true,
    }
}

/// A complete, valid envelope. Built in Rust rather than checked in as JSON so
/// it cannot silently drift away from the types it is supposed to describe.
fn valid_envelope() -> TelemetryEnvelope {
    let g = two_cluster_graph();
    let origins = all_input_origins(&g);
    let graph = GraphIpo::from_graph(&g, &origins);

    let source = json!({
        "nodes": [{ "id": 1, "label": "Stripe", "sector": "fintech" }],
        "edges": []
    });

    let records = reference_records();

    let mut binary_index = BTreeMap::new();
    binary_index.insert(
        "refines".to_string(),
        vec!["pcvc.research.json-map".to_string()],
    );

    TelemetryEnvelope {
        schema: TELEMETRY_QUERY_V1.into(),
        version: TELEMETRY_VERSION,
        query: TelemetryQuery::default(),
        graph,
        records,
        source: source.clone(),
        source_sha256: sha256_hex(&canonical_json(&source)),
        structure: Some(reference_structure()),
        tags: TaggingState {
            policy: "merge".into(),
            tau: 0.5,
            pruned: true,
            clusters: vec![Cluster {
                id: 0,
                label: "Market_Root".into(),
                node_ids: vec![1, 2, 3],
                connectivity: 0.42,
            }],
            probable_edges: vec![IpoEdge {
                from: 1,
                to: 2,
                edge_type: EdgeType::Refines,
                origin: NodeOrigin::Input,
                binary_type: Some("pcvc.research.json-map".into()),
            }],
            binary_index,
        },
        ledger: None,
        receipt: TelemetryReceipt {
            invariants_ok: true,
            failures: Vec::new(),
            steps: 32,
            t: 8,
            node_count: 6,
            edge_count: 5,
            input_node_count: 6,
            transform_node_count: 0,
            energy: 1.0,
            residual: 0.0,
            predictor: "sim".into(),
            match_policy: "merge".into(),
            seed: Some(1),
            n_modes: 64,
            latent_dim: DIM,
            eps: 1.0,
            schedule: "opmd".into(),
            limits: Limits::default(),
        },
        ocid: None,
    }
}

fn envelope_value() -> Value {
    serde_json::to_value(valid_envelope()).expect("envelope must serialize")
}

// ---------------------------------------------------------------------------
// Canonical bytes, hashes, anchors
// ---------------------------------------------------------------------------

/// The whole determinism story rests on `serde_json::Map` being a `BTreeMap`
/// in this workspace (no `preserve_order` feature). If that ever changes, the
/// envelope stops hashing stably across platforms and this test is the alarm.
#[test]
fn canonical_json_sorts_keys_regardless_of_literal_order() {
    let a: Value = serde_json::from_str(r#"{"z":1,"a":2,"m":{"y":3,"b":4}}"#).unwrap();
    let b: Value = serde_json::from_str(r#"{"a":2,"m":{"b":4,"y":3},"z":1}"#).unwrap();
    let bytes_a = canonical_json(&a);
    assert_eq!(bytes_a, canonical_json(&b), "key order must not survive");
    assert_eq!(
        String::from_utf8(bytes_a).unwrap(),
        r#"{"a":2,"m":{"b":4,"y":3},"z":1}"#
    );
}

#[test]
fn canonical_json_is_insensitive_to_input_whitespace() {
    let tight: Value = serde_json::from_str(r#"{"a":[1,2],"b":"x"}"#).unwrap();
    let loose: Value =
        serde_json::from_str("{\n  \"a\" : [ 1 , 2 ]\n,\n  \"b\":  \"x\"  }").unwrap();
    assert_eq!(canonical_json(&tight), canonical_json(&loose));
}

#[test]
fn sha256_hex_matches_the_published_nist_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn anchor_is_stable_across_key_order_and_sensitive_to_content() {
    let a: Value = serde_json::from_str(r#"{"label":"Stripe","sector":"fintech"}"#).unwrap();
    let reordered: Value = serde_json::from_str(r#"{"sector":"fintech","label":"Stripe"}"#).unwrap();
    let changed: Value = serde_json::from_str(r#"{"label":"Stripe","sector":"health"}"#).unwrap();

    assert_eq!(anchor_of(&a), anchor_of(&reordered));
    assert_ne!(anchor_of(&a), anchor_of(&changed));
    assert_eq!(anchor_of(&a).len(), 64);
    assert!(anchor_of(&a).bytes().all(|b| b.is_ascii_hexdigit()));
}

// ---------------------------------------------------------------------------
// Graph → IPO projection
// ---------------------------------------------------------------------------

#[test]
fn projection_preserves_node_and_edge_counts() {
    let g = two_cluster_graph();
    let ipo = GraphIpo::from_graph(&g, &all_input_origins(&g));

    assert_eq!(ipo.schema, GRAPH_IPO_V1);
    assert_eq!(ipo.node_count(), g.node_count());
    assert_eq!(ipo.edge_count(), g.edge_count());
    assert_eq!(ipo.node_count(), 6);
    assert_eq!(ipo.edge_count(), 5);
}

#[test]
fn projection_keeps_embeddings_f64_and_uniform() {
    let g = two_cluster_graph();
    let ipo = GraphIpo::from_graph(&g, &all_input_origins(&g));
    for node in &ipo.nodes {
        assert_eq!(node.embedding.len(), DIM);
        assert!(node.embedding.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn projection_is_deterministic_and_id_ordered() {
    let g = two_cluster_graph();
    let origins = all_input_origins(&g);
    let first = GraphIpo::from_graph(&g, &origins);
    let second = GraphIpo::from_graph(&g, &origins);
    assert_eq!(canonical_json(&serde_json::to_value(&first).unwrap()),
               canonical_json(&serde_json::to_value(&second).unwrap()));

    let ids: Vec<u64> = first.nodes.iter().map(|n| n.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "nodes must be emitted in ascending id order");
}

/// The `origin` discriminator is what lets the envelope carry Φ-derived
/// anchors without presenting them as host facts. A node absent from the host
/// set must never be labelled `input`.
#[test]
fn origin_index_separates_host_data_from_transform_anchors() {
    let mut g = two_cluster_graph();
    // Simulate a Match-absorbed latent: a node the payload never mentioned.
    g.apply(
        &GraphOp::AddNode {
            id: 99,
            ntype: NodeType::Observation,
            emb: vec![0.5; DIM],
            ts: 42,
        },
        DIM,
    )
    .unwrap();
    g.apply(
        &GraphOp::AddEdge {
            from: 1,
            to: 99,
            etype: EdgeType::CausallyPrecedes,
        },
        DIM,
    )
    .unwrap();

    // Host supplied only the original six nodes and five edges.
    let origins = OriginIndex {
        input_nodes: (1..=6).collect(),
        input_edges: [(1, 2), (2, 3), (4, 5), (5, 6), (3, 4)].into_iter().collect(),
    };
    let ipo = GraphIpo::from_graph(&g, &origins);

    let derived = ipo.nodes.iter().find(|n| n.id == 99).expect("node 99");
    assert_eq!(derived.origin, NodeOrigin::Transform);
    assert!(ipo
        .nodes
        .iter()
        .filter(|n| n.id <= 6)
        .all(|n| n.origin == NodeOrigin::Input));

    let bridge = ipo
        .edges
        .iter()
        .find(|e| e.from == 1 && e.to == 99)
        .expect("edge 1->99");
    assert_eq!(
        bridge.origin,
        NodeOrigin::Transform,
        "an edge the host never supplied must not be labelled input"
    );
}

#[test]
fn default_origin_index_claims_nothing_as_host_input() {
    let g = two_cluster_graph();
    let ipo = GraphIpo::from_graph(&g, &OriginIndex::default());
    assert!(
        ipo.nodes.iter().all(|n| n.origin == NodeOrigin::Transform),
        "with no payload behind it, an export must not claim host provenance"
    );
}

// ---------------------------------------------------------------------------
// Binary-type registry
// ---------------------------------------------------------------------------

#[test]
fn binary_type_table_is_the_documented_mapping() {
    assert_eq!(
        binary_type_for_node(&NodeType::Observation),
        "pcvc.research.json-map"
    );
    assert_eq!(
        binary_type_for_node(&NodeType::Goal),
        "pcvc.research.agent-trace"
    );
    assert_eq!(
        binary_type_for_node(&NodeType::InvariantCheckpoint),
        "pcvc.research.receipt"
    );
    assert_eq!(
        binary_type_for_edge(&EdgeType::TemporalNext),
        "pcvc.research.agent-trace"
    );
    assert_eq!(
        binary_type_for_edge(&EdgeType::Refines),
        "pcvc.research.json-map"
    );
}

/// A `Custom` label containing a dot is already namespaced by the host and is
/// passed through; a bare label is not guessed at.
#[test]
fn qualified_custom_labels_pass_through_bare_ones_fall_back() {
    assert_eq!(
        binary_type_for_node(&NodeType::Custom("pcvc.research.orderbook".into())),
        "pcvc.research.orderbook"
    );
    assert_eq!(
        binary_type_for_edge(&EdgeType::Custom("company.owns".into())),
        "company.owns"
    );
    assert_eq!(
        binary_type_for_node(&NodeType::Custom("market_segment".into())),
        BINARY_TYPE_CUSTOM
    );
}

// ---------------------------------------------------------------------------
// Envelope round-trip
// ---------------------------------------------------------------------------

#[test]
fn envelope_round_trips_through_serde() {
    let original = valid_envelope();
    let text = serde_json::to_string(&original).expect("serialize");
    let back: TelemetryEnvelope = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(original, back);
}

#[test]
fn envelope_serialization_is_byte_stable() {
    let a = serde_json::to_vec(&valid_envelope()).unwrap();
    let b = serde_json::to_vec(&valid_envelope()).unwrap();
    assert_eq!(a, b);
}

/// `query` and `receipt` are guaranteed present. A host that receives exit 0
/// may rely on them without a null check.
#[test]
fn envelope_always_carries_query_and_receipt() {
    let doc = envelope_value();
    assert!(doc.get("query").is_some_and(Value::is_object));
    assert!(doc.get("receipt").is_some_and(Value::is_object));
    assert_eq!(doc["schema"], TELEMETRY_QUERY_V1);
    assert_eq!(doc["version"], 1);
}

/// `clusters` and `probable_edges` are arrays even when empty — never null.
/// A renderer should not have to distinguish "no clusters" from "absent".
#[test]
fn empty_tag_collections_serialize_as_arrays_not_null() {
    let mut env = valid_envelope();
    env.tags.clusters.clear();
    env.tags.probable_edges.clear();
    let doc = serde_json::to_value(&env).unwrap();
    assert_eq!(doc["tags"]["clusters"], json!([]));
    assert_eq!(doc["tags"]["probable_edges"], json!([]));
    validate_envelope(&doc).expect("empty views are still valid");
}

// ---------------------------------------------------------------------------
// Validator — acceptance
// ---------------------------------------------------------------------------

#[test]
fn the_reference_envelope_validates() {
    validate_envelope(&envelope_value()).expect("reference envelope must validate");
}

#[test]
fn structure_is_optional() {
    let mut env = valid_envelope();
    env.structure = None;
    let doc = serde_json::to_value(&env).unwrap();
    assert!(doc.get("structure").is_none(), "None must be omitted");
    validate_envelope(&doc).expect("an explicit-graph payload has no structure report");
}

// ---------------------------------------------------------------------------
// Validator — rejection. Each case is a way a host could be lied to.
// ---------------------------------------------------------------------------

fn reject(mutate: impl FnOnce(&mut Value)) -> IpoError {
    let mut doc = envelope_value();
    mutate(&mut doc);
    validate_envelope(&doc).expect_err("document should have been rejected")
}

#[test]
fn a_non_object_document_is_rejected() {
    assert!(matches!(
        validate_envelope(&json!([1, 2, 3])),
        Err(IpoError::WrongType { .. })
    ));
}

#[test]
fn a_wrong_schema_tag_is_rejected() {
    let err = reject(|d| d["schema"] = json!("aria-dev-seed-v1"));
    assert!(matches!(err, IpoError::BadFormat { .. }), "{err}");
}

#[test]
fn a_wrong_version_is_rejected() {
    let err = reject(|d| d["version"] = json!(2));
    assert!(matches!(err, IpoError::Integrity(_)), "{err}");
}

#[test]
fn every_required_top_level_key_is_enforced() {
    for key in [
        "query",
        "graph",
        "records",
        "source",
        "source_sha256",
        "tags",
        "receipt",
    ] {
        let err = reject(|d| {
            d.as_object_mut().unwrap().remove(key);
        });
        match err {
            IpoError::MissingKey { key: k, .. } => assert_eq!(k, key),
            other => panic!("removing '{key}' gave {other}"),
        }
    }
}

#[test]
fn a_wrong_nested_graph_schema_tag_is_rejected() {
    let err = reject(|d| d["graph"]["schema"] = json!("something-else"));
    assert!(matches!(err, IpoError::BadFormat { .. }), "{err}");
}

/// Inv3's meaning projected onto the wire: an edge may not reference a node
/// that is not in the document.
#[test]
fn a_dangling_edge_endpoint_is_rejected() {
    let err = reject(|d| d["graph"]["edges"][0]["to"] = json!(4_242));
    assert!(
        matches!(&err, IpoError::Integrity(m) if m.contains("dangling")),
        "{err}"
    );
}

#[test]
fn a_duplicate_node_id_is_rejected() {
    let err = reject(|d| {
        let first = d["graph"]["nodes"][0].clone();
        d["graph"]["nodes"].as_array_mut().unwrap().push(first);
    });
    assert!(
        matches!(&err, IpoError::Integrity(m) if m.contains("duplicate node id")),
        "{err}"
    );
}

/// All node embeddings must share one dimension: a ragged export is not a
/// subset of 𝒵 and would break any consumer doing linear algebra on it.
#[test]
fn a_ragged_embedding_dimension_is_rejected() {
    let err = reject(|d| d["graph"]["nodes"][1]["embedding"] = json!([0.1, 0.2]));
    assert!(
        matches!(&err, IpoError::Integrity(m) if m.contains("ragged")),
        "{err}"
    );
}

#[test]
fn a_non_finite_embedding_component_is_rejected() {
    // JSON has no NaN literal, so a host would smuggle one as a string.
    let err = reject(|d| d["graph"]["nodes"][0]["embedding"] = json!([0.1, "NaN", 0.3, 0.4]));
    assert!(matches!(err, IpoError::WrongType { .. }), "{err}");
}

#[test]
fn an_unknown_origin_value_is_rejected() {
    let err = reject(|d| d["graph"]["nodes"][0]["origin"] = json!("inferred"));
    assert!(matches!(err, IpoError::WrongType { .. }), "{err}");
}

#[test]
fn a_malformed_source_hash_is_rejected() {
    for bad in [json!("deadbeef"), json!("Z".repeat(64)), json!(42)] {
        let err = reject(|d| d["source_sha256"] = bad);
        assert!(
            matches!(err, IpoError::Integrity(_) | IpoError::WrongType { .. }),
            "{err}"
        );
    }
}

#[test]
fn a_non_numeric_record_key_is_rejected() {
    let err = reject(|d| {
        let rec = d["records"]["1"].clone();
        d["records"].as_object_mut().unwrap().insert("stripe".into(), rec);
    });
    assert!(
        matches!(&err, IpoError::Integrity(m) if m.contains("decimal node ids")),
        "{err}"
    );
}

#[test]
fn a_record_without_an_anchor_is_rejected() {
    let err = reject(|d| {
        d["records"]["1"].as_object_mut().unwrap().remove("anchor");
    });
    assert!(matches!(err, IpoError::WrongType { .. }), "{err}");
}

/// `tags` is a *view over G*. A probable edge pointing at a node that is not
/// in the exported graph would make the view unrenderable.
#[test]
fn a_probable_edge_outside_the_graph_is_rejected() {
    let err = reject(|d| d["tags"]["probable_edges"][0]["from"] = json!(9_999));
    assert!(
        matches!(&err, IpoError::Integrity(m) if m.contains("not a node")),
        "{err}"
    );
}

#[test]
fn null_tag_collections_are_rejected() {
    let err = reject(|d| d["tags"]["clusters"] = Value::Null);
    assert!(matches!(err, IpoError::WrongType { .. }), "{err}");
}

#[test]
fn a_receipt_missing_its_counts_is_rejected() {
    for key in [
        "steps",
        "t",
        "node_count",
        "edge_count",
        "input_node_count",
        "transform_node_count",
    ] {
        let err = reject(|d| {
            d["receipt"].as_object_mut().unwrap().remove(key);
        });
        assert!(matches!(err, IpoError::WrongType { .. }), "{key}: {err}");
    }
}

#[test]
fn a_receipt_without_limits_is_rejected() {
    let err = reject(|d| {
        d["receipt"].as_object_mut().unwrap().remove("limits");
    });
    assert!(matches!(err, IpoError::WrongType { .. }), "{err}");
}

/// Every structural role must ship the counts that produced it. A role without
/// its `explain` numbers is an unfalsifiable assertion, which is exactly what
/// this contract exists to prevent.
#[test]
fn a_column_stat_missing_its_counts_is_rejected() {
    for key in ["n_rows", "present", "distinct", "singletons"] {
        let err = reject(|d| {
            d["structure"]["columns"][0]
                .as_object_mut()
                .unwrap()
                .remove(key);
        });
        assert!(matches!(err, IpoError::WrongType { .. }), "{key}: {err}");
    }
    for key in ["coverage", "uniqueness"] {
        let err = reject(|d| {
            d["structure"]["columns"][0]
                .as_object_mut()
                .unwrap()
                .remove(key);
        });
        assert!(matches!(err, IpoError::WrongType { .. }), "{key}: {err}");
    }
}

#[test]
fn a_column_stat_missing_its_rule_is_rejected() {
    let err = reject(|d| {
        d["structure"]["columns"][0]
            .as_object_mut()
            .unwrap()
            .remove("rule");
    });
    assert!(matches!(err, IpoError::WrongType { .. }), "{err}");
}

#[test]
fn a_query_without_its_clauses_is_rejected() {
    for path in ["match", "where", "return"] {
        let err = reject(|d| {
            d["query"].as_object_mut().unwrap().remove(path);
        });
        assert!(matches!(err, IpoError::WrongType { .. }), "{path}: {err}");
    }
}

// ---------------------------------------------------------------------------
// Defaults and role helpers
// ---------------------------------------------------------------------------

#[test]
fn query_defaults_select_everything_and_return_the_full_body() {
    let q = TelemetryQuery::default();
    assert_eq!(q.match_clause, QueryMatch::default());
    assert_eq!(q.match_clause.nodes, "*");
    assert_eq!(q.match_clause.edges, "*");
    assert!(q.where_clause.edge_types.is_empty(), "empty = no filter");
    assert!(q.where_clause.include_full_graph);
    for key in ["query", "graph", "records", "source", "tags", "receipt"] {
        assert!(q.return_keys.iter().any(|k| k == key), "missing {key}");
    }
}

#[test]
fn where_defaults_report_the_shipped_tau() {
    assert!((QueryWhere::default().tau - 0.5).abs() < 1e-15);
}

#[test]
fn only_identity_and_facet_roles_contribute_structure() {
    assert!(ColumnRole::KeyAnchor.is_structural());
    assert!(ColumnRole::NearKeyAnchor.is_structural());
    assert!(ColumnRole::Facet.is_structural());
    assert!(!ColumnRole::FreeAttribute.is_structural());
    assert!(!ColumnRole::Constant.is_structural());
    assert!(!ColumnRole::Empty.is_structural());
}

#[test]
fn role_and_origin_wire_names_are_snake_case_and_stable() {
    assert_eq!(ColumnRole::KeyAnchor.as_str(), "key_anchor");
    assert_eq!(ColumnRole::NearKeyAnchor.as_str(), "near_key_anchor");
    assert_eq!(ColumnRole::FreeAttribute.as_str(), "free_attribute");
    assert_eq!(NodeOrigin::Input.as_str(), "input");
    assert_eq!(NodeOrigin::Transform.as_str(), "transform");

    // The serde form must agree with `as_str`, or the validator and the
    // serializer would disagree about the same document.
    assert_eq!(
        serde_json::to_value(ColumnRole::KeyAnchor).unwrap(),
        json!("key_anchor")
    );
    assert_eq!(
        serde_json::to_value(NodeOrigin::Transform).unwrap(),
        json!("transform")
    );
}

/// The ceilings are part of the contract: a host reading the receipt must be
/// able to see what bound the invocation.
#[test]
fn default_limits_are_bounded_and_reported() {
    let l = Limits::default();
    assert!(l.max_input_bytes > 0 && l.max_input_bytes <= 64 * 1024 * 1024);
    assert!(l.max_nodes > 0);
    assert!(l.max_edges >= l.max_nodes);
    assert!(l.max_steps > 0);
    assert!(l.max_dependency_pairs > 0);

    let doc = envelope_value();
    assert_eq!(
        doc["receipt"]["limits"]["max_input_bytes"],
        json!(l.max_input_bytes)
    );
}

#[test]
fn default_thresholds_are_in_their_documented_ranges() {
    let t = RoleThresholds::default();
    assert!(t.near_key_coverage > 0.0 && t.near_key_coverage <= 1.0);
    assert!(t.facet_max_ratio > 0.0 && t.facet_max_ratio <= 1.0);
    assert!(t.facet_max_distinct > 0);
}

// ---------------------------------------------------------------------------
// Tracked contract artifacts
// ---------------------------------------------------------------------------

/// Issue `#15` requires the host contract to live in tracked crate files, not
/// only in gitignored PDFs. These are the files a stranger reads.
#[test]
fn the_schema_files_are_tracked_and_parse() {
    for name in [
        "aria-telemetry-query-v1.json",
        "aria-graph-ipo-v1.json",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("schemas")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be tracked: {e}", path.display()));
        let schema: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} must be valid JSON: {e}", path.display()));
        assert_eq!(schema["title"], name.trim_end_matches(".json"));
    }
}

#[test]
fn the_input_fixtures_are_tracked_and_shaped_as_documented() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");

    let graph_form: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("two_cluster_market.json")).unwrap())
            .unwrap();
    assert_eq!(graph_form["nodes"].as_array().unwrap().len(), 6);
    assert_eq!(graph_form["edges"].as_array().unwrap().len(), 5);

    let sheet: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("tabular_market_sheet.json")).unwrap(),
    )
    .unwrap();
    let rows = sheet.as_array().expect("the sheet fixture is an array of rows");
    assert_eq!(rows.len(), 8);

    // The fixture must actually exercise the role law: a unique key column, a
    // repeated facet column, and a real functional dependency.
    let tickers: BTreeSet<&str> = rows.iter().map(|r| r["ticker"].as_str().unwrap()).collect();
    assert_eq!(tickers.len(), rows.len(), "ticker must be a key anchor");

    let sectors: BTreeSet<&str> = rows.iter().map(|r| r["sector"].as_str().unwrap()).collect();
    assert!(
        sectors.len() > 1 && sectors.len() < rows.len(),
        "sector must be a facet, not a key and not a constant"
    );

    // region → country: every region maps to exactly one country.
    let mut region_to_country: BTreeMap<&str, &str> = BTreeMap::new();
    for row in rows {
        let region = row["region"].as_str().unwrap();
        let country = row["country"].as_str().unwrap();
        assert_eq!(
            *region_to_country.entry(region).or_insert(country),
            country,
            "fixture must contain the region -> country dependency"
        );
    }
}
