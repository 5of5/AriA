//! Round-trip proof: weights trained by this crate load through the runtime
//! `TrainedPredictor` with the load-time ℙ2 projection a measured **no-op** —
//! the loader's defense-in-depth never has to correct what the optimizer's
//! embedded projection already guaranteed.

use aria_engine_backends::data::{dataset_from_bytes, encode_window};
use aria_engine_backends::trained::TrainedPredictor;
use aria_engine_core::condition::Condition;
use aria_engine_core::config::LossLambdas;
use aria_engine_core::engine::Predictor as _;
use aria_training::{train, AdamWParams, TrainingConfig};

fn trained_weights(
    dir: &tempfile::TempDir,
) -> (
    TrainingConfig,
    aria_engine_backends::trained::PredictorWeights,
) {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/docs_excerpt_32k.txt"
    ))
    .unwrap();
    let ds = dataset_from_bytes("docs_excerpt_32k.txt", &bytes, 64, 64).unwrap();
    let data_path = dir.path().join("dataset.json");
    std::fs::write(&data_path, serde_json::to_string(&ds).unwrap()).unwrap();
    let cfg = TrainingConfig {
        data_path,
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
        epochs: 4,
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
    };
    let (_, weights) = train(&cfg).unwrap();
    (cfg, weights)
}

#[test]
fn loader_projection_is_a_noop_on_trained_weights() {
    let dir = tempfile::tempdir().unwrap();
    let (_, weights) = trained_weights(&dir);
    let embed_before = weights.embed.clone();
    let pred_before = weights.predict.token.clone();

    let loaded = TrainedPredictor::from_weights(weights).unwrap();

    // Bound respected under the loader's own audit (ε = 1.0 ⇒ ε/2 = 0.5).
    let lip = loaded.measured_lipschitz().unwrap();
    assert!(lip <= 0.5 + 1e-12, "measured Lip(P) = {lip}");
    let report = loaded.spectral_report().unwrap();
    assert!(report.embed <= 1.0 + 1e-12);

    // No-op proof, functional and bit-exact: the loaded predictor computes
    // exactly matvec with the pre-load matrices — any rescaling by the
    // load-time projection would break bit equality.
    let psi = encode_window(b"the loader must not rescale these trained weights", 64);
    let z_loaded = loaded.embed(&psi);
    let mut flat = Vec::with_capacity(psi.len() * 2);
    for c in &psi {
        flat.push(c.re);
        flat.push(c.im);
    }
    let z_manual: Vec<f64> = embed_before
        .iter()
        .map(|row| row.iter().zip(&flat).map(|(a, b)| a * b).sum())
        .collect();
    assert_eq!(
        z_loaded.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        z_manual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        "embed must be loaded verbatim (projection no-op)"
    );

    let p_loaded = loaded.predict(&z_loaded, Condition::Token);
    let p_manual: Vec<f64> = pred_before
        .iter()
        .map(|row| row.iter().zip(&z_loaded).map(|(a, b)| a * b).sum())
        .collect();
    assert_eq!(
        p_loaded.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        p_manual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        "predict must be loaded verbatim (projection no-op)"
    );
}

#[test]
fn v1_json_file_roundtrips_through_the_runtime_loader() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, weights) = trained_weights(&dir);
    let path = cfg.output_path.unwrap();

    // Measured property (WS-A1 finding): serde_json *emits* shortest-roundtrip
    // floats (ryu) but *parses* them with the fast lossy path unless the
    // `float_roundtrip` feature is enabled — so a v1 JSON file round-trips to
    // within a few units in the 14th significant digit, not bit-exactly.
    // Bit-exact artifact loading is the aria-predictor-v2 safetensors contract
    // (raw LE bytes; WS-A2 checkpoint.rs). The bound below is that parse-loss
    // class, not a training-quality number.
    let from_file = TrainedPredictor::from_file(&path).unwrap();
    let psi = encode_window(b"file and memory must agree to v1 precision", 64);
    let in_memory = TrainedPredictor::from_weights(weights).unwrap();

    let a = from_file.embed(&psi);
    let b = in_memory.embed(&psi);
    for (x, y) in a.iter().zip(&b) {
        let rel = (x - y).abs() / y.abs().max(1e-30);
        assert!(
            rel <= 1e-13,
            "v1 JSON parse drift too large: {x} vs {y} (rel {rel})"
        );
    }
    let pa = from_file.predict(&a, Condition::WorldModel);
    let pb = in_memory.predict(&b, Condition::WorldModel);
    for (x, y) in pa.iter().zip(&pb) {
        let rel = (x - y).abs() / y.abs().max(1e-30);
        assert!(
            rel <= 1e-13,
            "v1 JSON parse drift too large: {x} vs {y} (rel {rel})"
        );
    }
    let lip = from_file.measured_lipschitz().unwrap();
    assert!(lip <= 0.5 + 1e-12, "file-loaded Lip(P) = {lip}");
}
