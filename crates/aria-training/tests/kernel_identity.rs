//! Production kernels must match the allocating wrappers bit-for-bit and
//! the certified-in-ball spectral skip must not change Φ-side gradients
//! on weights that actually sit inside both balls.

use aria_engine_backends::data::dataset_from_bytes;
use aria_engine_backends::spectral::{
    power_iteration, project_spectral, project_spectral_in_place, DEFAULT_ITERATIONS,
};
use aria_engine_core::config::LossLambdas;
use aria_training::linalg::{
    dot, l2_sq, mat_t_vec, mat_t_vec_into, matvec, matvec_into, norm2_sq, sub_into,
};
use aria_training::loss::{compute_batch, compute_batch_with, ModelParams, SpectralEval};
use aria_training::{AdamWParams, Lcg, TrainingConfig};
use std::path::PathBuf;

fn seeded_matrix(rows: usize, cols: usize, seed: u64) -> Vec<Vec<f64>> {
    let mut rng = Lcg(seed);
    (0..rows)
        .map(|_| (0..cols).map(|_| rng.unit()).collect())
        .collect()
}

fn seeded_vec(n: usize, seed: u64) -> Vec<f64> {
    let mut rng = Lcg(seed);
    (0..n).map(|_| rng.unit()).collect()
}

fn bits_eq(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        assert_eq!(x.to_bits(), y.to_bits(), "{x} vs {y}");
    }
}

#[test]
fn in_place_kernels_match_allocating_wrappers_bit_for_bit() {
    let matrix = seeded_matrix(8, 32, 3);
    let rhs = seeded_vec(32, 5);
    let left = seeded_vec(8, 7);

    let alloc = matvec(&matrix, &rhs);
    let mut into = vec![0.0; 8];
    matvec_into(&matrix, &rhs, &mut into);
    bits_eq(&alloc, &into);

    let alloc_t = mat_t_vec(&matrix, &left);
    let mut into_t = vec![1.0; 32]; // dirty on purpose — into must zero
    mat_t_vec_into(&matrix, &left, &mut into_t);
    bits_eq(&alloc_t, &into_t);

    let left_v = seeded_vec(16, 11);
    let right_v = seeded_vec(16, 13);
    let mut delta = vec![0.0; 16];
    sub_into(&left_v, &right_v, &mut delta);
    let expect: Vec<f64> = left_v.iter().zip(&right_v).map(|(u, v)| u - v).collect();
    bits_eq(&delta, &expect);
    assert_eq!(
        norm2_sq(&delta).to_bits(),
        delta.iter().map(|v| v * v).sum::<f64>().to_bits()
    );
    assert_eq!(
        l2_sq(&left_v, &right_v).to_bits(),
        norm2_sq(&delta).to_bits()
    );
    assert_eq!(
        dot(&left_v, &right_v).to_bits(),
        left_v
            .iter()
            .zip(&right_v)
            .map(|(u, v)| u * v)
            .sum::<f64>()
            .to_bits()
    );
}

#[test]
fn project_spectral_in_place_matches_owning_wrapper() {
    let raw = seeded_matrix(6, 6, 17);
    let owned = project_spectral(raw.clone(), 0.49).unwrap();
    let mut in_place = raw;
    project_spectral_in_place(&mut in_place, 0.49).unwrap();
    for (ra, rb) in owned.iter().zip(&in_place) {
        bits_eq(ra, rb);
    }
    let sigma = power_iteration(&in_place, DEFAULT_ITERATIONS).unwrap();
    assert!(sigma <= 0.49 + 1e-12, "σ = {sigma}");
}

fn fixture_ds() -> aria_training::PreparedDataset {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/docs_excerpt_32k.txt"
    ))
    .unwrap();
    let raw = dataset_from_bytes("kernel-identity", &bytes, 16, 16).unwrap();
    let cfg = TrainingConfig {
        data_path: PathBuf::from("unused"),
        corpus_path: None,
        edges_path: None,
        stride: 16,
        n_modes: 16,
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
        holdout_frac: 0.4,
        trajectory_len: 4,
        gamma_tau: 0.5,
        min_rankme_frac: 0.30,
        allow_sub_spec_dims: true,
        output_path: None,
        output_v2_path: None,
        readout_out: None,
    };
    aria_training::PreparedDataset::from_field_dataset(&raw, "x".into(), &cfg).unwrap()
}

fn in_ball_params(d: usize, input: usize) -> ModelParams {
    let embed = aria_training::train::orthonormal_rows(d, input, 99, None);
    let pred = aria_training::train::seeded_in_ball(d, 101, 0.49).unwrap();
    let mut params = ModelParams { embed, pred };
    // Same Π_𝒮 fixed point the trainer exits on — a single scale can sit
    // 1 ulp over the bound, which would make Audit and Certified disagree.
    aria_training::project_to_estimator_ball(&mut params.embed, 1.0).unwrap();
    aria_training::project_to_estimator_ball(&mut params.pred, 0.49).unwrap();
    params
}

#[test]
fn certified_in_ball_matches_audit_on_projected_weights() {
    let ds = fixture_ds();
    let params = in_ball_params(8, 32);
    let ids: Vec<usize> = (0..3.min(ds.n_train)).collect();
    let lambdas = LossLambdas {
        jepa: 0.70,
        nll: 0.0,
        spectral: 0.15,
        graph: 0.0,
    };
    let (a, ga) = compute_batch(&params, &ds, &ids, None, &lambdas, 0.49, 0.5).unwrap();
    let (b, gb) = compute_batch_with(
        &params,
        &ds,
        &ids,
        None,
        &lambdas,
        0.49,
        0.5,
        SpectralEval::CertifiedInBall,
    )
    .unwrap();

    // In-ball ⇒ hinge is 0, so skipping the two PIs cannot change the Φ grads.
    assert_eq!(a.spectral.to_bits(), 0.0f64.to_bits());
    assert_eq!(b.spectral.to_bits(), 0.0f64.to_bits());
    assert_eq!(a.jepa.to_bits(), b.jepa.to_bits());
    assert_eq!(a.total.to_bits(), b.total.to_bits());
    for (ra, rb) in ga.embed.iter().zip(&gb.embed) {
        bits_eq(ra, rb);
    }
    for (ra, rb) in ga.pred.iter().zip(&gb.pred) {
        bits_eq(ra, rb);
    }
}

#[test]
fn compute_batch_is_bit_deterministic_across_calls() {
    let ds = fixture_ds();
    let params = in_ball_params(8, 32);
    let ids: Vec<usize> = (0..4.min(ds.n_train)).collect();
    let lambdas = LossLambdas {
        jepa: 1.0,
        nll: 0.0,
        spectral: 0.0,
        graph: 0.0,
    };
    let (l1, g1) = compute_batch(&params, &ds, &ids, None, &lambdas, 0.49, 0.5).unwrap();
    let (l2, g2) = compute_batch(&params, &ds, &ids, None, &lambdas, 0.49, 0.5).unwrap();
    assert_eq!(l1.jepa.to_bits(), l2.jepa.to_bits());
    for (ra, rb) in g1.embed.iter().zip(&g2.embed) {
        bits_eq(ra, rb);
    }
    for (ra, rb) in g1.pred.iter().zip(&g2.pred) {
        bits_eq(ra, rb);
    }
}

#[test]
fn orthonormal_init_and_seeded_pred_sit_inside_the_balls() {
    let embed = aria_training::train::orthonormal_rows(8, 32, 3, None);
    let pred = aria_training::train::seeded_in_ball(8, 5, 0.25).unwrap();
    let s_e = power_iteration(&embed, DEFAULT_ITERATIONS).unwrap();
    let s_p = power_iteration(&pred, DEFAULT_ITERATIONS).unwrap();
    assert!((s_e - 1.0).abs() < 1e-12, "σ(embed) = {s_e}");
    assert!(s_p <= 0.25 + 1e-12, "σ(pred) = {s_p}");
}
