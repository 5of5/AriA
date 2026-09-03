//! Relocated unit suite: dataset loading, the WS5-faithful batcher and time
//! split, and ConceptNet edge alignment — all through the public API on the
//! checked-in real-text fixture (D4).

use std::path::PathBuf;

use aria_engine_backends::data::dataset_from_bytes;
use aria_engine_core::config::LossLambdas;
use aria_training::dataset::{
    align_corpus, KgEdge, KgEdges, PreparedDataset, KG_EDGES_FORMAT, OPTICAL_DATASET_FORMAT,
};
use aria_training::{fnv1a64_hex, AdamWParams, TrainingConfig};

fn cfg(n_modes: usize, trajectory_len: usize, holdout_frac: f64) -> TrainingConfig {
    TrainingConfig {
        data_path: PathBuf::from("unused.json"),
        corpus_path: None,
        edges_path: None,
        stride: n_modes,
        n_modes,
        latent_dim: 16,
        eps: 1.0,
        lambdas: LossLambdas {
            jepa: 0.70,
            nll: 0.0,
            spectral: 0.15,
            graph: 0.15,
        },
        vocab_size: 256,
        epochs: 1,
        batch_size: 4,
        adamw: AdamWParams::default(),
        seed: 42,
        holdout_frac,
        trajectory_len,
        gamma_tau: 0.5,
        min_rankme_frac: 0.30,
        allow_sub_spec_dims: true,
        output_path: None,
        output_v2_path: None,
        readout_out: None,
    }
}

/// Real bytes for dataset tests: the checked-in docs-corpus excerpt (D4).
fn real_bytes() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/docs_excerpt_32k.txt"
    ))
    .expect("fixture present")
}

#[test]
fn ws5_shape_2862_frames_yield_357_trajectories_and_143_holdout() {
    // Synthesize the WS5 frame count from real bytes: 2862 frames of a
    // 16-mode encoding (frame count is what the split logic consumes).
    let bytes: Vec<u8> = real_bytes()
        .iter()
        .copied()
        .cycle()
        .take(16 * 2862)
        .collect();
    let ds = dataset_from_bytes("ws5-shape", &bytes, 16, 16).unwrap();
    assert_eq!(ds.trajectories[0].len(), 2862);
    let c = cfg(16, 8, 0.4);
    let prepared = PreparedDataset::from_field_dataset(&ds, fnv1a64_hex(&bytes), &c).unwrap();
    assert_eq!(
        prepared.chunks.len(),
        357,
        "2862 frames / 8 = 357 full trajectories"
    );
    assert_eq!(prepared.n_train, 214);
    assert_eq!(
        prepared.chunks.len() - prepared.n_train,
        143,
        "WS5 holdout n = 143"
    );
}

#[test]
fn time_split_holds_the_tail_out_in_corpus_order() {
    let bytes: Vec<u8> = real_bytes().iter().copied().cycle().take(16 * 80).collect();
    let ds = dataset_from_bytes("split", &bytes, 16, 16).unwrap();
    let c = cfg(16, 8, 0.4);
    let p = PreparedDataset::from_field_dataset(&ds, "x".into(), &c).unwrap();
    // Every train chunk's last frame index precedes every holdout chunk's first.
    let train_max = p.chunks[..p.n_train]
        .iter()
        .flat_map(|c| c.iter().copied())
        .max()
        .unwrap();
    let holdout_min = p.chunks[p.n_train..]
        .iter()
        .flat_map(|c| c.iter().copied())
        .min()
        .unwrap();
    assert!(
        train_max < holdout_min,
        "time split must never leak the future"
    );
}

#[test]
fn epoch_order_is_deterministic_per_seed_and_epoch_and_stays_in_train() {
    let bytes: Vec<u8> = real_bytes()
        .iter()
        .copied()
        .cycle()
        .take(16 * 160)
        .collect();
    let ds = dataset_from_bytes("order", &bytes, 16, 16).unwrap();
    let c = cfg(16, 8, 0.4);
    let p = PreparedDataset::from_field_dataset(&ds, "x".into(), &c).unwrap();
    let a = p.epoch_order(42, 3);
    let b = p.epoch_order(42, 3);
    assert_eq!(a, b, "same seed + epoch ⇒ same order");
    assert_ne!(
        p.epoch_order(42, 0),
        p.epoch_order(42, 1),
        "epochs get distinct streams"
    );
    assert!(
        a.iter().all(|&i| i < p.n_train),
        "holdout indices never enter training"
    );
    let mut sorted = a.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        (0..p.n_train).collect::<Vec<_>>(),
        "order is a permutation"
    );
}

#[test]
fn optical_and_unknown_formats_are_refused() {
    let bytes: Vec<u8> = real_bytes().iter().copied().cycle().take(16 * 80).collect();
    let mut ds = dataset_from_bytes("x", &bytes, 16, 16).unwrap();
    ds.format = OPTICAL_DATASET_FORMAT.into();
    let c = cfg(16, 8, 0.4);
    let err = PreparedDataset::from_field_dataset(&ds, "x".into(), &c)
        .unwrap_err()
        .to_string();
    assert!(err.contains("smoke-tests-only"), "got: {err}");

    ds.format = "keras-tfrecord".into();
    assert!(PreparedDataset::from_field_dataset(&ds, "x".into(), &c).is_err());
}

#[test]
fn dimension_and_finiteness_faults_are_rejected() {
    let bytes: Vec<u8> = real_bytes().iter().copied().cycle().take(16 * 80).collect();
    let ds = dataset_from_bytes("x", &bytes, 16, 16).unwrap();

    // n_modes mismatch vs config.
    let c = cfg(32, 8, 0.4);
    assert!(PreparedDataset::from_field_dataset(&ds, "x".into(), &c).is_err());

    // Non-finite frame component.
    let mut bad = ds.clone();
    bad.trajectories[0][0][0] = f64::NAN;
    let c = cfg(16, 8, 0.4);
    assert!(PreparedDataset::from_field_dataset(&bad, "x".into(), &c).is_err());
}

fn tiny_kg() -> KgEdges {
    KgEdges {
        format: KG_EDGES_FORMAT.into(),
        source: "test".into(),
        retrieved: "2026-08-17".into(),
        filter: "test".into(),
        license: "CC-BY-SA 4.0".into(),
        concepts: vec![
            "graph".into(),
            "energy".into(),
            "matrix".into(),
            "unused".into(),
        ],
        relations: vec!["/r/RelatedTo".into()],
        edges: vec![
            KgEdge {
                s: 0,
                e: 1,
                r: 0,
                w: 1.0,
            },
            KgEdge {
                s: 1,
                e: 2,
                r: 0,
                w: 2.5,
            },
            KgEdge {
                s: 0,
                e: 3,
                r: 0,
                w: 1.0,
            }, // endpoint absent from corpus
        ],
    }
}

#[test]
fn alignment_finds_real_tokens_and_reports_coverage() {
    let corpus = b"the graph holds energy; the matrix is a graph of energy states....";
    let kg = tiny_kg();
    // One window covering everything (n_modes ≥ len).
    let a = align_corpus(corpus, 128, 128, 1, &kg).unwrap();
    assert_eq!(a.window_concepts.len(), 1);
    assert_eq!(
        a.window_concepts[0],
        vec![0, 1, 2],
        "graph, energy, matrix present"
    );
    assert!((a.coverage - 1.0).abs() < 1e-12);
    // Edge (0,3) is unusable: 'unused' never occurs.
    assert_eq!(a.usable_edges.len(), 2);
    assert!(a.usable_edges.iter().all(|e| e.e != 3));
}

#[test]
fn alignment_rejects_a_window_count_mismatch() {
    let corpus = b"graph energy graph energy graph energy graph energy";
    let kg = tiny_kg();
    let err = align_corpus(corpus, 16, 16, 999, &kg)
        .unwrap_err()
        .to_string();
    assert!(err.contains("alignment mismatch"), "got: {err}");
}

#[test]
fn alignment_window_boundaries_match_encode_corpus() {
    // The alignment loop must produce exactly as many windows as the encoder
    // produced frames, for overlapping and non-overlapping strides.
    let bytes = real_bytes();
    for (n_modes, stride) in [(64usize, 64usize), (64, 32), (32, 24)] {
        let ds = dataset_from_bytes("b", &bytes, n_modes, stride).unwrap();
        let frames = ds.trajectories[0].len();
        let a = align_corpus(&bytes, n_modes, stride, frames, &tiny_kg()).unwrap();
        assert_eq!(a.window_concepts.len(), frames);
    }
}

#[test]
fn kg_validation_rejects_structural_faults() {
    let mut kg = tiny_kg();
    kg.edges.push(KgEdge {
        s: 9,
        e: 0,
        r: 0,
        w: 1.0,
    });
    assert!(kg.validate().is_err());

    let mut kg = tiny_kg();
    kg.edges.push(KgEdge {
        s: 0,
        e: 0,
        r: 0,
        w: 1.0,
    });
    assert!(kg.validate().unwrap_err().to_string().contains("self-loop"));

    let mut kg = tiny_kg();
    kg.edges.push(KgEdge {
        s: 0,
        e: 1,
        r: 5,
        w: 1.0,
    });
    assert!(kg.validate().is_err());

    let mut kg = tiny_kg();
    kg.edges.push(KgEdge {
        s: 2,
        e: 1,
        r: 0,
        w: f64::NAN,
    });
    assert!(kg.validate().is_err());

    let mut kg = tiny_kg();
    kg.format = "conceptnet-raw".into();
    assert!(kg.validate().is_err());
}
