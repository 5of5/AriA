//! Relocated unit suite: TrainingConfig validation (Δ³ delegated to
//! aria-core), provenance fingerprints, and the deterministic LCG stream.

use std::path::PathBuf;

use aria_engine_core::config::LossLambdas;
use aria_training::{fnv1a64_hex, sha256_hex, AdamWParams, Lcg, TrainingConfig};

fn base_cfg() -> TrainingConfig {
    TrainingConfig {
        data_path: PathBuf::from("unused.json"),
        corpus_path: None,
        edges_path: None,
        stride: 64,
        n_modes: 64,
        latent_dim: 16,
        eps: 1.0,
        lambdas: LossLambdas {
            jepa: 0.70,
            nll: 0.0,
            spectral: 0.15,
            graph: 0.15,
        },
        vocab_size: 256,
        epochs: 3,
        batch_size: 8,
        adamw: AdamWParams::default(),
        seed: 42,
        holdout_frac: 0.4,
        trajectory_len: 8,
        gamma_tau: 0.5,
        min_rankme_frac: 0.30,
        allow_sub_spec_dims: false,
        output_path: None,
        output_v2_path: None,
        readout_out: None,
    }
}

#[test]
fn the_canonical_simplex_validates() {
    base_cfg().validate().unwrap();
}

#[test]
fn delta3_violations_are_rejected_by_the_aria_core_rule() {
    // Sum ≠ 1 — rejected by AriaConfig::validate (one source of truth).
    let mut cfg = base_cfg();
    cfg.lambdas.graph = 0.5;
    let err = cfg.validate().unwrap_err().to_string();
    assert!(err.contains("loss_lambdas sum"), "got: {err}");

    // Negative weight.
    let mut cfg = base_cfg();
    cfg.lambdas.jepa = -0.1;
    cfg.lambdas.graph = 0.85;
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("λᵢ ≥ 0") || err.contains("loss_lambdas.jepa"),
        "got: {err}"
    );

    // Non-finite weight.
    let mut cfg = base_cfg();
    cfg.lambdas.nll = f64::NAN;
    assert!(cfg.validate().is_err());
}

#[test]
fn s_dimension_bounds_come_from_aria_core() {
    // N = 100 is not a power of two — the 𝒮 clause must fire.
    let mut cfg = base_cfg();
    cfg.n_modes = 100;
    let err = cfg.validate().unwrap_err().to_string();
    assert!(err.contains("n_modes"), "got: {err}");

    // d < 8 violates 8 ≤ d ≤ 2N.
    let mut cfg = base_cfg();
    cfg.latent_dim = 4;
    assert!(cfg.validate().is_err());

    // τ outside (0, 1].
    let mut cfg = base_cfg();
    cfg.gamma_tau = 0.0;
    assert!(cfg.validate().is_err());
}

#[test]
fn training_only_domain_is_rejected_with_detail() {
    let mut cfg = base_cfg();
    cfg.epochs = 0;
    assert!(cfg.validate().unwrap_err().to_string().contains("epochs"));

    let mut cfg = base_cfg();
    cfg.batch_size = 0;
    assert!(cfg
        .validate()
        .unwrap_err()
        .to_string()
        .contains("batch_size"));

    let mut cfg = base_cfg();
    cfg.trajectory_len = 1;
    assert!(cfg
        .validate()
        .unwrap_err()
        .to_string()
        .contains("trajectory_len"));

    let mut cfg = base_cfg();
    cfg.holdout_frac = 1.0;
    assert!(cfg
        .validate()
        .unwrap_err()
        .to_string()
        .contains("holdout_frac"));

    let mut cfg = base_cfg();
    cfg.edges_path = Some(PathBuf::from("edges.json"));
    assert!(cfg
        .validate()
        .unwrap_err()
        .to_string()
        .contains("corpus_path"));

    let mut cfg = base_cfg();
    cfg.eps = 0.0;
    assert!(cfg.validate().unwrap_err().to_string().contains("eps"));
}

#[test]
fn serde_defaults_are_the_receipted_protocol() {
    // Production defaults = the gate-optimal 15-epoch RankMe/quality protocol.
    let cfg: TrainingConfig =
        serde_json::from_str(r#"{"data_path":"d.json","n_modes":256,"latent_dim":32}"#).unwrap();
    assert_eq!(
        cfg.epochs, 15,
        "gate-optimal protocol (RankMe frontier, 2026-08-17)"
    );
    assert_eq!(cfg.batch_size, 32);
    assert!((cfg.adamw.lr - 3e-3).abs() < 1e-15);
    assert!((cfg.holdout_frac - 0.4).abs() < 1e-15);
    assert_eq!(cfg.trajectory_len, 8);
    assert_eq!(cfg.stride, 64);
    // Canonical simplex (PRD Workflow A): 0.70 / 0 / 0.15 / 0.15.
    assert!((cfg.lambdas.jepa - 0.70).abs() < 1e-15);
    assert!((cfg.lambdas.nll - 0.0).abs() < 1e-15);
    cfg.validate().unwrap();
}

#[test]
fn fnv_checksum_is_stable_and_input_sensitive() {
    let a = fnv1a64_hex(b"aria");
    assert_eq!(a, fnv1a64_hex(b"aria"));
    assert_ne!(a, fnv1a64_hex(b"arib"));
    // Known FNV-1a vector: empty input = offset basis.
    assert_eq!(fnv1a64_hex(b""), "cbf29ce484222325");
}

#[test]
fn sha256_nist_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    let million = vec![b'a'; 1_000_000];
    assert_eq!(
        sha256_hex(&million),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn sha256_padding_boundaries_are_exact() {
    // Lengths 55/56/63/64/119/120 hit every branch of the final-block logic.
    let cases = [55usize, 56, 63, 64, 119, 120];
    let digests: Vec<String> = cases
        .iter()
        .map(|&n| sha256_hex(&vec![0x61u8; n]))
        .collect();
    for (i, a) in digests.iter().enumerate() {
        assert_eq!(a.len(), 64);
        assert_eq!(a, &sha256_hex(&vec![0x61u8; cases[i]]), "deterministic");
        for b in digests.iter().skip(i + 1) {
            assert_ne!(a, b, "distinct lengths must not collide");
        }
    }
    // Known vector at the 56-byte boundary (double-block padding path).
    assert_eq!(
        sha256_hex(&[b'a'; 56]),
        "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
    );
}

#[test]
fn lcg_is_deterministic_and_in_range() {
    let mut a = Lcg(7);
    let mut b = Lcg(7);
    for _ in 0..100 {
        let (x, y) = (a.unit(), b.unit());
        assert_eq!(x.to_bits(), y.to_bits());
        assert!((-1.0..1.0).contains(&x));
    }
    let mut c = Lcg(9);
    for n in [1usize, 2, 7, 357] {
        for _ in 0..50 {
            assert!(c.index(n) < n);
        }
    }
}
