//! End-to-end training on the checked-in real-text fixture (D4):
//! `fixtures/docs_excerpt_32k.txt` — 32,768 bytes of the live docs corpus,
//! frozen 2026-08-17. Everything here is deterministic; these assertions
//! either hold forever or fail forever.

use std::path::PathBuf;

use aria_engine_backends::data::dataset_from_bytes;
use aria_engine_core::config::LossLambdas;
use aria_training::{train, AdamWParams, TrainingConfig};

fn fixture_bytes() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/docs_excerpt_32k.txt");
    std::fs::read(path).expect("fixture present")
}

fn edges_fixture() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/conceptnet_edges_v1.json"
    ))
}

/// Write the fixture corpus + its dataset artifact into a temp dir and return
/// a ready config (N = 64, d = 16, stride 64 → 512 frames → 64 trajectories).
fn cfg_in(dir: &tempfile::TempDir, epochs: usize, with_edges: bool) -> TrainingConfig {
    let bytes = fixture_bytes();
    let ds = dataset_from_bytes("docs_excerpt_32k.txt", &bytes, 64, 64).unwrap();
    let data_path = dir.path().join("dataset.json");
    std::fs::write(&data_path, serde_json::to_string(&ds).unwrap()).unwrap();
    let corpus_path = dir.path().join("corpus.txt");
    std::fs::write(&corpus_path, &bytes).unwrap();

    TrainingConfig {
        data_path,
        corpus_path: with_edges.then_some(corpus_path),
        edges_path: with_edges.then(edges_fixture),
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
        epochs,
        batch_size: 8,
        adamw: AdamWParams::default(),
        seed: 42,
        holdout_frac: 0.4,
        trajectory_len: 8,
        gamma_tau: 0.5,
        min_rankme_frac: 0.30,
        allow_sub_spec_dims: true,
        output_path: Some(dir.path().join("weights.json")),
        output_v2_path: None,
        readout_out: None,
    }
}

#[test]
fn training_reduces_holdout_residual_and_respects_both_balls() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = cfg_in(&dir, 8, false);
    let (outcome, weights) = train(&cfg).unwrap();

    // 512 frames → 64 trajectories of 8 → 38 train / 26 holdout (time split).
    assert_eq!(outcome.n_train_trajectories, 38);
    assert_eq!(outcome.n_holdout_trajectories, 26);
    assert_eq!(outcome.transitions_per_trajectory, 7);

    // Training moved the needle on held-out data.
    assert!(
        outcome.holdout_residual_final < outcome.holdout_residual_initial,
        "holdout must fall: {} → {}",
        outcome.holdout_residual_initial,
        outcome.holdout_residual_final
    );
    // And beats copying the current latent (persistence baseline).
    assert!(
        outcome.holdout_residual_final < outcome.persistence_residual,
        "model {} must beat persistence {}",
        outcome.holdout_residual_final,
        outcome.persistence_residual
    );
    // Loss descended across epochs.
    let first = outcome.epoch_loss.first().unwrap().total;
    let last = outcome.epoch_loss.last().unwrap().total;
    assert!(last < first, "epoch loss must descend: {first} → {last}");

    // σ-audit: every exported matrix inside its ball (ℙ2 / 𝔸2).
    assert!(outcome.sigma.embed <= 1.0 + 1e-12);
    for s in [
        outcome.sigma.token,
        outcome.sigma.diffusion,
        outcome.sigma.world_model,
    ] {
        assert!(s <= outcome.lipschitz_bound + 1e-12, "σ = {s} > ε/2");
    }
    // ε = 1.0 ⇒ the theorem bound is exactly ε/2 = 0.5 (WS5's 0.49 was a
    // chosen artifact margin under it, not the formula).
    assert!((outcome.lipschitz_bound - 0.5).abs() < 1e-12);

    // Latent variance telemetry is present and not identically zero
    // (a collapsed representation would zero it — the WS-A2 RankMe gate's
    // cheap shadow).
    assert_eq!(outcome.latent_variance.len(), 16);
    assert!(outcome.latent_variance_mean > 0.0);

    // RankMe gate is the hard abort inside train(); restate the predicate
    // so this test names the production quality bar.
    assert!(
        outcome.rankme >= outcome.rankme_gate,
        "RankMe {} < gate {}",
        outcome.rankme,
        outcome.rankme_gate
    );

    // The artifact was written and tagged v1.
    assert_eq!(weights.format, "aria-predictor-v1");
    assert!(cfg.output_path.as_ref().unwrap().exists());

    // Π_𝒮 wiring proof: the exported embedding is orthogonal to the
    // train-split mean frame (trivial-mode deflation, enforced every step).
    let bytes = fixture_bytes();
    let ds = dataset_from_bytes("docs_excerpt_32k.txt", &bytes, 64, 64).unwrap();
    let frames = &ds.trajectories[0];
    let n_train_frames = outcome.n_train_trajectories * 8;
    let mut mean = vec![0.0; 128];
    for frame in &frames[..n_train_frames] {
        for (m, v) in mean.iter_mut().zip(frame) {
            *m += v;
        }
    }
    let norm = mean.iter().map(|v| v * v).sum::<f64>().sqrt();
    for (i, row) in weights.embed.iter().enumerate() {
        let dot: f64 = row.iter().zip(&mean).map(|(a, b)| a * b).sum::<f64>() / norm;
        assert!(
            dot.abs() < 1e-10,
            "embed row {i} must be ⊥ train mean, got {dot}"
        );
    }
}

#[test]
fn conceptnet_edges_align_and_the_graph_term_is_measured() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = cfg_in(&dir, 3, true);
    let (outcome, _) = train(&cfg).unwrap();

    let coverage = outcome.edge_coverage.expect("alignment configured");
    let usable = outcome.usable_edges.expect("alignment configured");
    // The fixture excerpt was filtered against the live docs vocabulary, and
    // the test corpus IS an excerpt of that corpus: real coverage must exist.
    assert!(
        coverage > 0.5,
        "docs windows must align to ConceptNet concepts, got {coverage}"
    );
    assert!(
        usable > 100,
        "the usable-edge pool must be real, got {usable}"
    );
    assert!(outcome.edges_checksum.is_some());
    assert!(outcome.corpus_checksum.is_some());
    // The graph term was evaluated (value is measured, possibly small — the
    // hinge at γ = τ = 0.5 activates only for far-apart concept means).
    assert!(outcome.epoch_loss.iter().all(|b| b.graph.is_finite()));
}

#[test]
fn two_identical_runs_are_bit_identical() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = cfg_in(&dir, 4, true);
    let (out_a, w_a) = train(&cfg).unwrap();
    let (out_b, w_b) = train(&cfg).unwrap();

    // Outcome JSON equality (covers every measured number).
    let ja = serde_json::to_string(&out_a).unwrap();
    let jb = serde_json::to_string(&out_b).unwrap();
    assert_eq!(ja, jb, "same config + seed ⇒ bit-identical outcome");

    assert!(out_a.holdout_residual_final < out_a.persistence_residual);
    assert!(out_a.rankme >= out_a.rankme_gate);
    assert!(out_a.sigma.embed <= 1.0 + 1e-12);
    assert!(out_a.sigma.token <= out_a.lipschitz_bound + 1e-12);
    assert_eq!(out_a.sigma.token.to_bits(), out_a.sigma.diffusion.to_bits());
    assert_eq!(
        out_a.sigma.token.to_bits(),
        out_a.sigma.world_model.to_bits()
    );

    if let Ok(dump) = std::env::var("ARIA_TRAIN_DUMP_DIR") {
        let dir = std::path::Path::new(&dump);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("train_fixture_run1.json"), &ja).unwrap();
        std::fs::write(dir.join("train_fixture_run2.json"), &jb).unwrap();
    }

    // Weight matrices bit-identical.
    let flat =
        |m: &Vec<Vec<f64>>| -> Vec<u64> { m.iter().flatten().map(|v| v.to_bits()).collect() };
    assert_eq!(flat(&w_a.embed), flat(&w_b.embed));
    assert_eq!(flat(&w_a.predict.token), flat(&w_b.predict.token));
    assert_eq!(flat(&w_a.predict.diffusion), flat(&w_b.predict.diffusion));
    assert_eq!(
        flat(&w_a.predict.world_model),
        flat(&w_b.predict.world_model)
    );

    // A different seed produces different weights (the seed is real).
    let mut cfg2 = cfg.clone();
    cfg2.seed = 43;
    let (_, w_c) = train(&cfg2).unwrap();
    assert_ne!(flat(&w_a.embed), flat(&w_c.embed));
}

#[test]
fn nll_weight_is_rejected_until_ws_a2_wires_readout_training() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = cfg_in(&dir, 1, false);
    cfg.lambdas = LossLambdas {
        jepa: 0.55,
        nll: 0.15,
        spectral: 0.15,
        graph: 0.15,
    };
    let err = train(&cfg).unwrap_err().to_string();
    assert!(
        err.contains("WS-A2"),
        "λ_NLL > 0 must be rejected with detail, got: {err}"
    );
}

#[test]
fn v2_provenance_and_readout_roundtrip_on_the_fixture() {
    use aria_engine_backends::readout::{Readout, ReadoutKind};
    use aria_engine_backends::trained::TrainedPredictor;
    use safetensors::SafeTensors;

    let dir = tempfile::tempdir().unwrap();
    // Overlapping windows (stride 32 < n_modes 64): the next window's first
    // byte lies INSIDE the current window, so the readout task is learnable —
    // the non-overlapping fixture protocol carries no next-window byte signal
    // (measured: holdout NLL 5.589–6.207 vs the 5.545 uniform floor) and the
    // hard gate rightly refuses to write a head there.
    let bytes = fixture_bytes();
    let ds = dataset_from_bytes("docs_excerpt_32k.txt", &bytes, 64, 32).unwrap();
    let data_path = dir.path().join("dataset_stride32.json");
    std::fs::write(&data_path, serde_json::to_string(&ds).unwrap()).unwrap();
    let corpus_path = dir.path().join("corpus_stride32.txt");
    std::fs::write(&corpus_path, &bytes).unwrap();
    let mut cfg = cfg_in(&dir, 4, false);
    cfg.data_path = data_path;
    cfg.stride = 32;
    cfg.corpus_path = Some(corpus_path);
    cfg.output_v2_path = Some(dir.path().join("weights.safetensors"));
    cfg.readout_out = Some(dir.path().join("readout.bin"));
    let (outcome, _) = train(&cfg).unwrap();

    // Overlapping split: 1023 frames → 127 trajectories → 51 holdout ≥ 30,
    // so the Wilcoxon certification applies and is reported.
    assert!(outcome.n_holdout_trajectories >= 30);
    let w = outcome
        .wilcoxon
        .as_ref()
        .expect("n ≥ 30 ⇒ Wilcoxon applies");
    assert!(w.p_one_sided.is_finite());
    assert!(outcome.gates_pass.is_some());
    // RankMe gate passed (or train() would have aborted) and is reported.
    assert!(outcome.rankme >= outcome.rankme_gate);
    assert!((outcome.rankme_gate - 0.30 * 16.0).abs() < 1e-12);

    // v2 artifact: bit-exact runtime load + recoverable provenance.
    let v2_path = cfg.output_v2_path.as_ref().unwrap();
    let bytes = std::fs::read(v2_path).unwrap();
    assert_eq!(
        outcome.artifact_v2_sha256.as_deref().unwrap(),
        aria_training::sha256_hex(&bytes),
        "receipt fingerprint must match the artifact on disk"
    );
    let loaded = TrainedPredictor::from_file(v2_path).unwrap();
    assert!(loaded.measured_lipschitz().unwrap() <= 0.5 + 1e-12);
    let (_, header) = SafeTensors::read_metadata(&bytes).unwrap();
    let meta = header.metadata().clone().expect("metadata present");
    for key in [
        "prov.git_sha",
        "prov.crate_version",
        "prov.seed",
        "prov.dataset_sha256",
        "prov.corpus_sha256",
        "prov.lambdas",
        "prov.protocol",
    ] {
        assert!(meta.contains_key(key), "missing provenance key {key}");
    }
    assert_eq!(meta.get("prov.seed").unwrap(), "42");
    assert_eq!(
        meta.get("prov.dataset_sha256").unwrap(),
        &outcome.dataset_sha256
    );
    // WS-A3 fields are absent, not fabricated.
    assert!(!meta.contains_key("prov.map_revision"));
    assert!(!meta.contains_key("prov.ccv_hash"));

    // Readout pass: gate beaten, artifact loads as a discrete head.
    let r = outcome.readout.as_ref().expect("readout configured");
    assert!(
        r.holdout_nll_final < r.uniform_nll,
        "head must beat the uniform floor"
    );
    assert!(
        r.holdout_nll_final < r.holdout_nll_initial,
        "training must improve NLL"
    );
    let head = Readout::from_file(cfg.readout_out.as_ref().unwrap()).unwrap();
    assert_eq!(head.kind(), ReadoutKind::Discrete);
    assert_eq!(head.dim(), 16);
}

#[test]
fn optical_dataset_is_refused_for_training() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = fixture_bytes();
    let mut ds = dataset_from_bytes("x", &bytes, 64, 64).unwrap();
    ds.format = "aria-optical-dataset-v1".into();
    let data_path = dir.path().join("optical.json");
    std::fs::write(&data_path, serde_json::to_string(&ds).unwrap()).unwrap();
    let mut cfg = cfg_in(&dir, 1, false);
    cfg.data_path = data_path;
    let err = train(&cfg).unwrap_err().to_string();
    assert!(err.contains("smoke-tests-only"), "got: {err}");
}
