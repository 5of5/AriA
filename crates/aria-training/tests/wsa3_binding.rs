//! WS-A3 binding-corpus train: live-docs concatenation through the shipped
//! columnar ingest into `train()`, twice, with RankMe / persistence / σ gates.

use std::path::PathBuf;

use aria_engine_core::config::LossLambdas;
use aria_training::{
    crate_repo_root, ingest_columnar, live_docs_corpus, train, AdamWParams, TrainingConfig,
};

fn edges_fixture() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/conceptnet_edges_v1.json"
    ))
}

fn binding_cfg(dir: &tempfile::TempDir, epochs: usize) -> TrainingConfig {
    let root = crate_repo_root();
    let (files, corpus) = live_docs_corpus(&root).expect("live-docs assemble");
    assert!(
        !files.is_empty() && corpus.len() > 8,
        "live-docs corpus must assemble"
    );
    let (ds, blob) = ingest_columnar("live-docs-binding", &corpus, 256, 64).unwrap();
    assert_eq!(ds.n_modes, 256);
    assert!(!ds.trajectories[0].is_empty());

    let data_path = dir.path().join("dataset.col");
    std::fs::write(&data_path, &blob).unwrap();
    let corpus_path = dir.path().join("corpus.txt");
    std::fs::write(&corpus_path, &corpus).unwrap();

    TrainingConfig {
        data_path,
        corpus_path: Some(corpus_path),
        edges_path: Some(edges_fixture()),
        stride: 64,
        n_modes: 256,
        latent_dim: 32,
        eps: 1.0,
        lambdas: LossLambdas {
            jepa: 0.70,
            nll: 0.0,
            spectral: 0.15,
            graph: 0.15,
        },
        vocab_size: 256,
        epochs,
        batch_size: 32,
        adamw: AdamWParams::default(),
        seed: 42,
        holdout_frac: 0.4,
        trajectory_len: 8,
        gamma_tau: 0.5,
        min_rankme_frac: 0.30,
        allow_sub_spec_dims: false,
        output_path: Some(dir.path().join("weights.json")),
        output_v2_path: Some(dir.path().join("weights.safetensors")),
        readout_out: None,
    }
}

#[test]
fn binding_corpus_train_twice_meets_quality_predicates() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = binding_cfg(&dir, 4);
    let (a, wa) = train(&cfg).unwrap();
    let (b, wb) = train(&cfg).unwrap();

    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb, "equal seed ⇒ bit-identical TrainOutcome");

    let flat =
        |m: &Vec<Vec<f64>>| -> Vec<u64> { m.iter().flatten().map(|v| v.to_bits()).collect() };
    assert_eq!(flat(&wa.embed), flat(&wb.embed));

    assert!(
        a.holdout_residual_final < a.persistence_residual,
        "holdout {} must beat persistence {}",
        a.holdout_residual_final,
        a.persistence_residual
    );
    assert!(
        a.rankme >= a.rankme_gate,
        "RankMe {} < gate {}",
        a.rankme,
        a.rankme_gate
    );
    assert!(a.sigma.embed <= 1.0 + 1e-12);
    assert!(a.sigma.token <= a.lipschitz_bound + 1e-12);
    assert_eq!(a.sigma.token.to_bits(), a.sigma.diffusion.to_bits());
    if a.n_holdout_trajectories >= 30 {
        let w = a.wilcoxon.as_ref().expect("n ≥ 30 ⇒ Wilcoxon");
        assert!(w.median_improvement > 0.0);
        assert!(w.p_one_sided < 0.01);
        assert_eq!(a.gates_pass, Some(true));
    }

    if let Ok(dump) = std::env::var("ARIA_TRAIN_DUMP_DIR") {
        let p = std::path::Path::new(&dump);
        std::fs::create_dir_all(p).unwrap();
        std::fs::write(p.join("train_a3_run1.json"), &ja).unwrap();
        std::fs::write(p.join("train_a3_run2.json"), &jb).unwrap();
        if let Some(v2) = &cfg.output_v2_path {
            let dest = p.join("aria-predictor-v2.safetensors");
            std::fs::copy(v2, dest).unwrap();
        }
    }
}
