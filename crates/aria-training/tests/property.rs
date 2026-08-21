//! Property tests (proptest, seeded): domain rejection never panics, the time
//! split never leaks the future, the embedded projection always exits inside
//! both balls, and alignment never fabricates structure.

use std::path::PathBuf;

use aria_engine_backends::data::dataset_from_bytes;
use aria_engine_backends::spectral::{power_iteration, DEFAULT_ITERATIONS};
use aria_engine_core::config::LossLambdas;
use aria_training::dataset::{align_corpus, KgEdge, KgEdges, PreparedDataset};
use aria_training::loss::{Grads, ModelParams};
use aria_training::{AdamW, AdamWParams, TrainingConfig};
use proptest::prelude::*;

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/docs_excerpt_32k.txt"
    ))
    .unwrap()
}

fn cfg(n_modes: usize, trajectory_len: usize, holdout_frac: f64) -> TrainingConfig {
    TrainingConfig {
        data_path: PathBuf::from("unused"),
        corpus_path: None,
        edges_path: None,
        stride: n_modes,
        n_modes,
        latent_dim: 8,
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

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Δ³ violations and domain faults are rejected as errors — never panics,
    /// never silent acceptance of an off-simplex λ.
    #[test]
    fn config_validation_never_panics_and_enforces_the_simplex(
        jepa in -0.5f64..1.5,
        nll in -0.5f64..1.5,
        spectral in -0.5f64..1.5,
        graph in -0.5f64..1.5,
        holdout in -0.2f64..1.2,
        eps in -1.0f64..3.0,
    ) {
        let mut c = cfg(64, 8, if (0.05..0.95).contains(&holdout) { holdout } else { 0.4 });
        c.lambdas = LossLambdas { jepa, nll, spectral, graph };
        c.holdout_frac = holdout;
        c.eps = eps;
        let result = c.validate();
        let sum = jepa + nll + spectral + graph;
        let on_simplex = jepa >= 0.0 && nll >= 0.0 && spectral >= 0.0 && graph >= 0.0
            && (sum - 1.0).abs() <= 1e-9;
        if !on_simplex {
            prop_assert!(result.is_err(), "off-simplex λ must be rejected");
        }
        if eps <= 0.0 || !(0.0..1.0).contains(&holdout) || holdout == 0.0 {
            prop_assert!(result.is_err());
        }
    }

    /// The WS5 time split never leaks a future frame into training, for any
    /// admissible trajectory length and holdout fraction on real bytes.
    #[test]
    fn time_split_never_leaks_the_future(
        trajectory_len in 2usize..12,
        holdout_pct in 1usize..99,
        n_frames in 30usize..200,
    ) {
        let holdout_frac = holdout_pct as f64 / 100.0;
        let bytes: Vec<u8> = fixture_bytes().into_iter().cycle().take(16 * n_frames).collect();
        let ds = dataset_from_bytes("prop", &bytes, 16, 16).unwrap();
        let c = cfg(16, trajectory_len, holdout_frac);
        match PreparedDataset::from_field_dataset(&ds, "x".into(), &c) {
            Ok(p) => {
                let train_max = p.chunks[..p.n_train].iter().flatten().max().unwrap();
                let holdout_min = p.chunks[p.n_train..].iter().flatten().min().unwrap();
                prop_assert!(train_max < holdout_min);
                // Every epoch order is a permutation of the train indices.
                let order = p.epoch_order(7, 3);
                let mut sorted = order.clone();
                sorted.sort_unstable();
                prop_assert_eq!(sorted, (0..p.n_train).collect::<Vec<_>>());
            }
            Err(e) => {
                // Only the declared degeneracies may reject.
                let msg = e.to_string();
                prop_assert!(
                    msg.contains("degenerate") || msg.contains("no full trajectory"),
                    "unexpected rejection: {}", msg
                );
            }
        }
    }

    /// AdamW + embedded projection exits inside both balls for arbitrary
    /// bounded gradients and hyper-parameters in domain.
    #[test]
    fn every_optimizer_exit_is_inside_both_balls(
        seed in 0u64..1000,
        lr in 1e-4f64..0.8,
        g_scale in 0.0f64..20.0,
        steps in 1usize..12,
    ) {
        let d = 4;
        let input = 8;
        let mut rng_state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
        let mut next = move || {
            rng_state = rng_state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((rng_state >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0) - 1.0
        };
        let mut model = ModelParams {
            embed: (0..d).map(|_| (0..input).map(|_| next() * 0.5).collect()).collect(),
            pred: (0..d).map(|_| (0..d).map(|_| next() * 0.2).collect()).collect(),
        };
        let mut opt = AdamW::new(AdamWParams { lr, ..AdamWParams::default() }, &model);
        let bound = 0.49;
        for _ in 0..steps {
            let grads = Grads {
                embed: (0..d).map(|_| (0..input).map(|_| next() * g_scale).collect()).collect(),
                pred: (0..d).map(|_| (0..d).map(|_| next() * g_scale).collect()).collect(),
            };
            opt.step(&mut model, &grads, bound).unwrap();
            let s_embed = power_iteration(&model.embed, DEFAULT_ITERATIONS).unwrap();
            let s_pred = power_iteration(&model.pred, DEFAULT_ITERATIONS).unwrap();
            prop_assert!(s_embed <= 1.0 + 1e-12, "σ(embed) = {}", s_embed);
            prop_assert!(s_pred <= bound + 1e-12, "σ(pred) = {}", s_pred);
        }
    }

    /// Alignment never fabricates: every usable edge's endpoints exist in the
    /// concept-window map, and every reported concept id is in the table.
    #[test]
    fn alignment_never_fabricates_structure(
        slice_len in 512usize..4096,
        offset in 0usize..16384,
    ) {
        let bytes = fixture_bytes();
        let start = offset.min(bytes.len() - 1);
        let end = (start + slice_len).min(bytes.len());
        let slice = &bytes[start..end];
        if slice.len() < 8 { return Ok(()); }

        let kg = KgEdges {
            format: "aria-kg-edges-v1".into(),
            source: "prop".into(),
            retrieved: "2026-08-17".into(),
            filter: "prop".into(),
            license: "CC-BY-SA 4.0".into(),
            concepts: vec!["the".into(), "graph".into(), "spec".into(), "energy".into(),
                           "aria".into(), "training".into(), "nonexistentzzz".into()],
            relations: vec!["/r/RelatedTo".into()],
            edges: vec![
                KgEdge { s: 0, e: 1, r: 0, w: 1.0 },
                KgEdge { s: 1, e: 2, r: 0, w: 1.0 },
                KgEdge { s: 3, e: 4, r: 0, w: 1.0 },
                KgEdge { s: 5, e: 0, r: 0, w: 1.0 },
                KgEdge { s: 6, e: 0, r: 0, w: 1.0 }, // never matches: token absent
            ],
        };

        // Window count derived with the same rule the encoder uses.
        let n_modes = 32;
        let mut expected = 0usize;
        let mut s = 0usize;
        while s < slice.len() {
            let e = (s + n_modes).min(slice.len());
            if e - s < 8 { break; }
            expected += 1;
            s += n_modes;
        }
        if expected == 0 { return Ok(()); }

        let a = align_corpus(slice, n_modes, n_modes, expected, &kg).unwrap();
        prop_assert_eq!(a.window_concepts.len(), expected);
        let n_concepts = u32::try_from(kg.concepts.len()).expect("tiny table");
        for w in &a.window_concepts {
            prop_assert!(w.iter().all(|&c| c < n_concepts));
        }
        for edge in &a.usable_edges {
            prop_assert!(a.concept_windows.contains_key(&edge.s));
            prop_assert!(a.concept_windows.contains_key(&edge.e));
            prop_assert!(edge.e != 6 && edge.s != 6, "an absent token can never be usable");
        }
        prop_assert!((0.0..=1.0).contains(&a.coverage));
    }
}
