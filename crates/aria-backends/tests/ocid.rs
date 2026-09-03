//! O5 — OCID, the Observation Commitment IDentifier.
//!
//! The property under test: a host holding only the envelope and the original
//! payload bytes can prove the graph came from *that* payload under *that*
//! configuration, without trusting Aria and without any key.
//!
//! Signature tests run only with the `ocid-ed25519` feature; the commitment
//! tests run in every build, because the commitment is the in-repo fallback and
//! must work with no dependency at all.

use aria_engine_backends::ipo::{validate_envelope, Limits};
use aria_engine_backends::ocid::{
    commit, config_digest, config_of, graph_digest, signature_verification_available,
    structure_digest, verify_envelope, OcidBinds, OcidConfig, OcidError, OcidRequest, OCID_V1,
};
use aria_engine_backends::telemetry::{transform, TelemetryRequest};
use aria_engine_core::config::AriaConfig;

const N_MODES: usize = 64;
const DIM: usize = 32;
const STEPS: u64 = 16;

/// A well-known Ed25519 test vector (RFC 8032 §7.1, TEST 1): the public key and
/// the signature over the empty message. Hard-coded rather than generated so the
/// test has an external, checkable reference and needs no signing capability.
const RFC8032_PUBLIC: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
const RFC8032_SIG_EMPTY: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name),
    )
    .expect("fixture must be tracked")
}

fn request(payload: Vec<u8>, ocid: OcidRequest) -> TelemetryRequest {
    TelemetryRequest {
        payload,
        config: AriaConfig {
            n_modes: N_MODES,
            latent_dim: DIM,
            seed: Some(1),
            allow_sub_spec_dims: true,
            ..AriaConfig::default()
        },
        steps: STEPS,
        predictor: None,
        observe: false,
        limits: Limits::default(),
        respect_config_policy: false,
        query: None,
        ocid,
    }
}

fn keyless() -> OcidRequest {
    OcidRequest {
        public_key_hex: None,
        signature_hex: None,
    }
}

fn sample_binds() -> OcidBinds {
    OcidBinds {
        domain: OCID_V1.into(),
        public_key: String::new(),
        source_sha256: "a".repeat(64),
        config_sha256: "b".repeat(64),
        graph_sha256: "c".repeat(64),
        structure_sha256: "d".repeat(64),
    }
}

// ---------------------------------------------------------------------------
// The commitment primitive
// ---------------------------------------------------------------------------

#[test]
fn a_commitment_is_64_hex_digits_and_deterministic() {
    let binds = sample_binds();
    let a = commit(&binds);
    assert_eq!(a.len(), 64);
    assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(a, commit(&binds), "the same bindings must commit the same");
}

#[test]
fn changing_any_bound_field_changes_the_commitment() {
    let base = commit(&sample_binds());
    let mutations: Vec<(&str, OcidBinds)> = vec![
        (
            "public_key",
            OcidBinds {
                public_key: "e".repeat(64),
                ..sample_binds()
            },
        ),
        (
            "source",
            OcidBinds {
                source_sha256: "f".repeat(64),
                ..sample_binds()
            },
        ),
        (
            "config",
            OcidBinds {
                config_sha256: "0".repeat(64),
                ..sample_binds()
            },
        ),
        (
            "graph",
            OcidBinds {
                graph_sha256: "1".repeat(64),
                ..sample_binds()
            },
        ),
        (
            "structure",
            OcidBinds {
                structure_sha256: "2".repeat(64),
                ..sample_binds()
            },
        ),
    ];
    for (label, mutated) in mutations {
        assert_ne!(
            base,
            commit(&mutated),
            "changing {label} must change the commitment"
        );
    }
}

/// The reason every field is length-prefixed. With plain concatenation,
/// splitting the same total bytes differently between two adjacent fields
/// produces an identical preimage — a collision an attacker gets to choose.
#[test]
fn length_prefixing_defeats_the_concatenation_collision() {
    let a = OcidBinds {
        public_key: "ab".into(),
        source_sha256: "c".into(),
        ..sample_binds()
    };
    let b = OcidBinds {
        public_key: "a".into(),
        source_sha256: "bc".into(),
        ..sample_binds()
    };
    assert_ne!(
        commit(&a),
        commit(&b),
        "('ab','c') and ('a','bc') must not commit identically"
    );
}

#[test]
fn the_config_digest_covers_every_trajectory_input() {
    let base = OcidConfig {
        n_modes: 64,
        latent_dim: 32,
        eps: 1.0,
        seed: Some(1),
        schedule: "opmd".into(),
        match_policy: "merge".into(),
        merge_tau: 0.5,
        steps: 16,
    };
    let reference = config_digest(&base);

    let variants = [
        OcidConfig { n_modes: 128, ..base.clone() },
        OcidConfig { latent_dim: 64, ..base.clone() },
        OcidConfig { eps: 0.5, ..base.clone() },
        OcidConfig { seed: Some(2), ..base.clone() },
        OcidConfig { schedule: "opd".into(), ..base.clone() },
        OcidConfig { match_policy: "identity".into(), ..base.clone() },
        OcidConfig { merge_tau: 0.25, ..base.clone() },
        OcidConfig { steps: 32, ..base.clone() },
    ];
    for v in variants {
        assert_ne!(
            reference,
            config_digest(&v),
            "a different trajectory must produce a different config digest"
        );
    }
}

// ---------------------------------------------------------------------------
// End to end through the transform
// ---------------------------------------------------------------------------

#[test]
fn no_ocid_is_emitted_unless_asked_for() {
    let env = transform(request(fixture_bytes("tabular_market_sheet.json"), keyless())).unwrap();
    assert!(env.ocid.is_none(), "OCID is opt-in");
}

/// The core O5 guarantee, with no key at all: the commitment binds payload,
/// configuration and output, and a third party can recompute it.
#[test]
fn a_keyless_commitment_verifies_against_the_payload() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let env = transform(request(
        payload.clone(),
        OcidRequest {
            public_key_hex: None,
            signature_hex: Some(String::new()),
        },
    ));
    // A signature without a key is refused, so use the documented keyless form.
    assert!(env.is_err(), "a signature needs a key to check it against");

    let mut req = request(payload.clone(), keyless());
    req.ocid = OcidRequest {
        public_key_hex: Some(RFC8032_PUBLIC.into()),
        signature_hex: None,
    };
    let env = transform(req).unwrap();

    let ocid = env.ocid.as_ref().expect("commitment present");
    assert_eq!(ocid.schema, OCID_V1);
    assert_eq!(ocid.binds.domain, OCID_V1);
    assert_eq!(ocid.key.as_deref(), Some(RFC8032_PUBLIC));
    assert_eq!(
        ocid.signature_verified, None,
        "no signature was supplied, so custody is not claimed"
    );
    assert!(ocid.note.is_some(), "and the document says why");

    verify_envelope(&env, &payload).expect("the commitment must verify");
}

#[test]
fn the_commitment_binds_the_actual_output_digests() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let env = transform(req).unwrap();
    let ocid = env.ocid.as_ref().unwrap();

    assert_eq!(ocid.binds.source_sha256, env.source_sha256);
    assert_eq!(ocid.binds.config_sha256, config_digest(&config_of(&env)));
    assert_eq!(ocid.binds.graph_sha256, graph_digest(&env.graph));
    assert_eq!(
        ocid.binds.structure_sha256,
        structure_digest(env.structure.as_ref())
    );
    assert_eq!(ocid.ocid, commit(&ocid.binds));
}

/// The reason floats are bound at coarse precision rather than bit-exactly.
///
/// Measured against the `serde_json` in this workspace's lock, the parser is
/// off by one ULP on 17-digit decimals: `"0.19565758484969079"` parses to
/// `0x…f6` where the correct value is `0x…f5`. So `f64 -> JSON -> f64` is not
/// the identity, and a bit-exact commitment would mark a faithful copy of the
/// document as a forgery. This test is the regression guard.
#[test]
fn digests_survive_a_json_round_trip() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let env = transform(req).unwrap();

    let text = serde_json::to_string(&env).expect("serialize");
    let back: aria_engine_backends::ipo::TelemetryEnvelope =
        serde_json::from_str(&text).expect("deserialize");

    assert_eq!(
        graph_digest(&env.graph),
        graph_digest(&back.graph),
        "the graph digest must not depend on float rendering"
    );
    assert_eq!(
        structure_digest(env.structure.as_ref()),
        structure_digest(back.structure.as_ref())
    );
    // And the whole commitment still verifies after the trip through text.
    verify_envelope(&back, &payload).expect("a serialized envelope must still verify");
}

/// The deliberate cost of surviving the wire: a one-ULP difference is below the
/// binding precision and is tolerated. This is not a gap in the guarantee —
/// embeddings are a deterministic function of the payload, config and seed that
/// the commitment *does* bind exactly, so a party wanting bit-exact assurance
/// re-runs the transform. Asserting it here keeps the tradeoff explicit rather
/// than accidental.
#[test]
fn a_one_ulp_embedding_change_is_below_the_binding_precision() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload, keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let mut env = transform(req).unwrap();

    let before = graph_digest(&env.graph);
    let original = env.graph.nodes[0].embedding[0];
    env.graph.nodes[0].embedding[0] = f64::from_bits(original.to_bits() + 1);
    assert_eq!(
        before,
        graph_digest(&env.graph),
        "one ULP is finer than the documented binding precision"
    );
}

/// Any *meaningful* change to an embedding is still caught.
#[test]
fn a_meaningful_embedding_change_is_detected() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let mut env = transform(req).unwrap();

    let before = graph_digest(&env.graph);
    // A change at the 8th significant digit — comfortably inside the bound
    // precision, and far smaller than any edit that would move a node.
    env.graph.nodes[0].embedding[0] += 1e-8;

    assert_ne!(before, graph_digest(&env.graph));
    verify_envelope(&env, &payload).expect_err("must fail verification");
}

/// Topology is bound exactly, because integers and strings do round-trip.
#[test]
fn topology_changes_are_bound_exactly() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let env = transform(req).unwrap();
    let before = graph_digest(&env.graph);

    // Drop one edge.
    let mut fewer = env.clone();
    fewer.graph.edges.pop();
    assert_ne!(before, graph_digest(&fewer.graph), "edge removal must show");
    verify_envelope(&fewer, &payload).expect_err("must fail");

    // Relabel one node's provenance: claiming Φ output as host input.
    let mut relabelled = env.clone();
    relabelled.graph.nodes[0].origin = aria_engine_backends::ipo::NodeOrigin::Transform;
    assert_ne!(
        before,
        graph_digest(&relabelled.graph),
        "provenance is part of the commitment"
    );
    verify_envelope(&relabelled, &payload).expect_err("must fail");
}

/// Tampering with the graph after the fact must break verification. This is the
/// property that makes the envelope non-repudiable.
#[test]
fn a_tampered_graph_fails_verification() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let mut env = transform(req).unwrap();

    verify_envelope(&env, &payload).expect("valid before tampering");

    env.graph.nodes[0].embedding[0] += 1.0;
    let err = verify_envelope(&env, &payload).expect_err("tampering must be caught");
    assert!(
        matches!(&err, OcidError::Binding(m) if m.contains("graph digest")),
        "{err}"
    );
}

#[test]
fn a_tampered_structure_report_fails_verification() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let mut env = transform(req).unwrap();

    // Restate a role without its counts changing: exactly the kind of quiet
    // edit a commitment exists to detect.
    if let Some(structure) = env.structure.as_mut() {
        structure.columns[0].role = aria_engine_backends::ipo::ColumnRole::Facet;
    }
    let err = verify_envelope(&env, &payload).expect_err("must be caught");
    assert!(
        matches!(&err, OcidError::Binding(m) if m.contains("structure digest")),
        "{err}"
    );
}

/// A substituted payload must fail even if it is valid JSON of the same shape.
#[test]
fn a_substituted_payload_fails_verification() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let env = transform(req).unwrap();

    let other = fixture_bytes("two_cluster_market.json");
    let err = verify_envelope(&env, &other).expect_err("a different payload must fail");
    assert!(
        matches!(&err, OcidError::Binding(m) if m.contains("hashes to")),
        "{err}"
    );
}

#[test]
fn a_forged_commitment_value_is_detected() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let mut env = transform(req).unwrap();

    env.ocid.as_mut().unwrap().ocid = "0".repeat(64);
    let err = verify_envelope(&env, &payload).expect_err("must be caught");
    assert!(matches!(err, OcidError::Mismatch { .. }), "{err}");
}

/// A run at a different τ must not be able to present a commitment made at
/// another τ, because the config digest is bound.
#[test]
fn swapping_the_commitment_between_two_configurations_is_detected() {
    let payload = fixture_bytes("tabular_market_sheet.json");

    let mut req_a = request(payload.clone(), keyless());
    req_a.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let env_a = transform(req_a).unwrap();

    let mut req_b = request(payload.clone(), keyless());
    req_b.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    req_b.config.merge_tau = 0.25;
    let env_b = transform(req_b).unwrap();

    assert_ne!(
        env_a.ocid.as_ref().unwrap().ocid,
        env_b.ocid.as_ref().unwrap().ocid,
        "different τ must commit differently"
    );

    let mut spliced = env_b.clone();
    spliced.ocid = env_a.ocid.clone();
    let err = verify_envelope(&spliced, &payload).expect_err("splice must be caught");
    assert!(matches!(err, OcidError::Binding(_) | OcidError::Mismatch { .. }), "{err}");
}

#[test]
fn an_envelope_without_an_ocid_cannot_be_verified() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let env = transform(request(payload.clone(), keyless())).unwrap();
    let err = verify_envelope(&env, &payload).expect_err("nothing to verify");
    assert!(matches!(&err, OcidError::Binding(m) if m.contains("no OCID")), "{err}");
}

#[test]
fn an_envelope_with_an_ocid_still_validates_against_the_schema() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload, keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let env = transform(req).unwrap();
    validate_envelope(&serde_json::to_value(&env).unwrap()).expect("valid");
}

// ---------------------------------------------------------------------------
// Malformed requests
// ---------------------------------------------------------------------------

#[test]
fn a_malformed_public_key_is_rejected() {
    for bad in ["deadbeef", &"z".repeat(64), &"a".repeat(63)] {
        let mut req = request(fixture_bytes("tabular_market_sheet.json"), keyless());
        req.ocid.public_key_hex = Some(bad.to_string());
        let err = transform(req).expect_err("must reject");
        assert!(
            err.to_string().contains("public_key") || err.to_string().contains("hex"),
            "{bad}: {err}"
        );
    }
}

#[test]
fn a_signature_without_a_key_is_rejected() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"), keyless());
    req.ocid.signature_hex = Some(RFC8032_SIG_EMPTY.into());
    let err = transform(req).expect_err("must reject");
    assert!(err.to_string().contains("without a public key"), "{err}");
}

// ---------------------------------------------------------------------------
// Ed25519 — the curve actually doing work
// ---------------------------------------------------------------------------

#[test]
fn the_build_reports_whether_it_can_check_signatures() {
    // Truth is whatever this build is; the point is that it is reported rather
    // than silently assumed, so a host is never misled about what was checked.
    assert_eq!(
        signature_verification_available(),
        cfg!(feature = "ocid-ed25519")
    );
}

/// The RFC 8032 TEST 1 vector: this public key and signature are a valid pair
/// over the **empty** message. Verifying them against a non-empty payload must
/// therefore fail — which is exactly the substitution attack the curve check
/// exists to stop.
#[cfg(feature = "ocid-ed25519")]
#[test]
fn a_valid_signature_over_a_different_message_is_rejected() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"), keyless());
    req.ocid = OcidRequest {
        public_key_hex: Some(RFC8032_PUBLIC.into()),
        signature_hex: Some(RFC8032_SIG_EMPTY.into()),
    };
    let err = transform(req).expect_err("the signature does not cover this payload");
    assert!(err.to_string().contains("signature does not verify"), "{err}");
}

/// The same vector against the message it *does* cover verifies, and the
/// envelope then asserts custody.
#[cfg(feature = "ocid-ed25519")]
#[test]
fn the_rfc8032_vector_verifies_over_the_message_it_covers() {
    use aria_engine_backends::ocid::verify_signature;
    verify_signature(RFC8032_PUBLIC, RFC8032_SIG_EMPTY, b"")
        .expect("RFC 8032 TEST 1 must verify over the empty message");
}

#[cfg(feature = "ocid-ed25519")]
#[test]
fn a_corrupted_signature_is_rejected() {
    use aria_engine_backends::ocid::verify_signature;
    let mut corrupted: Vec<char> = RFC8032_SIG_EMPTY.chars().collect();
    corrupted[0] = if corrupted[0] == 'a' { 'b' } else { 'a' };
    let corrupted: String = corrupted.into_iter().collect();

    let err = verify_signature(RFC8032_PUBLIC, &corrupted, b"").expect_err("must reject");
    assert!(matches!(err, OcidError::Signature), "{err}");
}

/// Small-order keys are why `verify_strict` is used rather than `verify`: the
/// crate documents that `verify()` permits weak keys, letting an attacker craft
/// a key and signature that validate against almost any message.
#[cfg(feature = "ocid-ed25519")]
#[test]
fn a_small_order_public_key_is_rejected_outright() {
    use aria_engine_backends::ocid::verify_signature;
    // The identity element: the canonical small-order point.
    let weak = "0100000000000000000000000000000000000000000000000000000000000000";
    let err = verify_signature(weak, RFC8032_SIG_EMPTY, b"").expect_err("weak key must be refused");
    assert!(
        matches!(&err, OcidError::Key(m) if m.contains("weak")),
        "{err}"
    );
}

/// A bogus key must never authenticate anything. Which error surfaces is an
/// implementation detail — dalek may accept a non-canonical encoding at
/// `from_bytes` and only reject it during verification — so this asserts the
/// security property (rejected) rather than the error variant.
#[cfg(feature = "ocid-ed25519")]
#[test]
fn a_non_curve_public_key_never_authenticates() {
    use aria_engine_backends::ocid::verify_signature;
    for bogus in ["f".repeat(64), "0".repeat(64), "9".repeat(64)] {
        let err = verify_signature(&bogus, RFC8032_SIG_EMPTY, b"")
            .expect_err("a bogus key must not verify");
        assert!(
            matches!(err, OcidError::Key(_) | OcidError::Signature),
            "{bogus}: {err}"
        );
    }
}

/// With the verifier absent, a signature request must fail loudly rather than
/// silently producing an unverified commitment a host might trust.
#[cfg(not(feature = "ocid-ed25519"))]
#[test]
fn without_the_feature_a_signature_request_fails_loudly() {
    let mut req = request(fixture_bytes("tabular_market_sheet.json"), keyless());
    req.ocid = OcidRequest {
        public_key_hex: Some(RFC8032_PUBLIC.into()),
        signature_hex: Some(RFC8032_SIG_EMPTY.into()),
    };
    let err = transform(req).expect_err("must not silently skip the check");
    assert!(err.to_string().contains("ocid-ed25519"), "{err}");
}

/// The in-repo fallback: with no dependency at all, the commitment still works.
#[cfg(not(feature = "ocid-ed25519"))]
#[test]
fn the_commitment_fallback_works_without_the_dependency() {
    let payload = fixture_bytes("tabular_market_sheet.json");
    let mut req = request(payload.clone(), keyless());
    req.ocid.public_key_hex = Some(RFC8032_PUBLIC.into());
    let env = transform(req).unwrap();
    verify_envelope(&env, &payload).expect("the commitment needs no curve code");
}
