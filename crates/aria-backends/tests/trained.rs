//! Phase 3 — the trained predictor must be Spec-admissible, not just accurate.
//!
//! These tests use a synthetic checkpoint so they run without PyTorch. The
//! end-to-end learning demonstration (held-out residual actually falls) lives in
//! `python/tests/test_training.py`.

use aria_engine_backends::runner::{self, RefPredictor};
use aria_engine_backends::trained::{ConditionedWeights, PredictorWeights};
use aria_engine_backends::TrainedPredictor;
use aria_engine_core::config::AriaConfig;

const N_MODES: usize = 8;
const LATENT_DIM: usize = 16;

/// A deterministic, well-shaped checkpoint whose P matrices have norm `scale`.
fn checkpoint(scale: f64, bound: f64) -> PredictorWeights {
    let input_dim = 2 * N_MODES;

    // I: a partial isometry — the first `LATENT_DIM` rows of the identity.
    let embed: Vec<Vec<f64>> = (0..LATENT_DIM)
        .map(|i| {
            (0..input_dim)
                .map(|j| if i == j { 1.0 } else { 0.0 })
                .collect()
        })
        .collect();

    // P: a scaled cyclic permutation, so its spectral norm is exactly `scale`.
    let p: Vec<Vec<f64>> = (0..LATENT_DIM)
        .map(|i| {
            (0..LATENT_DIM)
                .map(|j| {
                    if j == (i + 1) % LATENT_DIM {
                        scale
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect();

    PredictorWeights {
        format: "aria-predictor-v1".into(),
        n_modes: N_MODES,
        latent_dim: LATENT_DIM,
        lipschitz_bound: bound,
        embed,
        predict: ConditionedWeights {
            token: p.clone(),
            diffusion: p.clone(),
            world_model: p,
        },
    }
}

fn config() -> AriaConfig {
    AriaConfig {
        n_modes: N_MODES,
        latent_dim: LATENT_DIM,
        ..AriaConfig::test_config()
    }
}

#[test]
fn trained_predictor_runs_1000_opmd_steps_green() {
    let trained = TrainedPredictor::from_weights(checkpoint(0.49, 0.49)).unwrap();
    assert!(trained.max_residual_jump(1.0).unwrap() <= config().eps);

    let outcome = runner::run_with(config(), 1000, RefPredictor::Trained(trained)).unwrap();

    assert!(
        outcome.summary.invariants_ok,
        "trained weights violated an invariant: {:?}",
        outcome.summary.failures
    );
    assert_eq!(outcome.summary.t, 250);
    assert_eq!(outcome.summary.action_sequence, "OPMD".repeat(250));
}

#[test]
fn swapping_the_predictor_needs_no_spec_change() {
    // Exit4 in miniature: same config, same schedule, different backend.
    let stub = runner::run(config(), 200).unwrap();
    let trained = TrainedPredictor::from_weights(checkpoint(0.4, 0.49)).unwrap();
    let learned = runner::run_with(config(), 200, RefPredictor::Trained(trained)).unwrap();

    assert!(stub.summary.invariants_ok);
    assert!(learned.summary.invariants_ok);
    assert_eq!(
        stub.summary.action_sequence,
        learned.summary.action_sequence
    );
    assert_eq!(stub.summary.t, learned.summary.t);
    assert_eq!(stub.summary.graph_size, learned.summary.graph_size);
}

#[test]
fn an_over_lipschitz_checkpoint_is_projected_not_trusted() {
    // Training claims Lip(P) = 4.0 while declaring a 0.49 bound. The loader
    // must clamp it, and the run must still be green.
    let trained = TrainedPredictor::from_weights(checkpoint(4.0, 0.49)).unwrap();
    assert!(trained.measured_lipschitz().unwrap() <= 0.49 + 1e-12);

    let outcome = runner::run_with(config(), 400, RefPredictor::Trained(trained)).unwrap();
    assert!(
        outcome.summary.invariants_ok,
        "{:?}",
        outcome.summary.failures
    );
}

#[test]
fn all_conditionings_share_the_one_architecture() {
    // 𝐂2 / A4: conditioning selects a matrix, never a second engine.
    for name in ["token", "diffusion", "world_model"] {
        let mut cfg = config();
        cfg.condition = runner::parse_condition(name).unwrap();
        let trained = TrainedPredictor::from_weights(checkpoint(0.45, 0.49)).unwrap();
        let outcome = runner::run_with(cfg, 200, RefPredictor::Trained(trained)).unwrap();
        assert!(
            outcome.summary.invariants_ok,
            "{name}: {:?}",
            outcome.summary.failures
        );
    }
}

#[test]
fn sigma_audit_holds_across_a_10k_step_run() {
    // Phase 1 acceptance: σ_max(W) ≤ 1.0 — ε = 0.0 in weight space under the
    // hard projection (𝕋4) — audited across a T ≥ 10⁴ run with the trained
    // backend. Weights are static during a run, so the audit samples the
    // estimator before the run, on the summary (per-run), and after; every
    // sample must sit at or below 1.0.
    let predictor = TrainedPredictor::from_weights(checkpoint(4.0, 0.49)).unwrap();
    let pre = predictor.spectral_report().unwrap();
    for (name, sigma) in [
        ("embed", pre.embed),
        ("token", pre.token),
        ("diffusion", pre.diffusion),
        ("world_model", pre.world_model),
    ] {
        assert!(sigma <= 1.0 + 1e-12, "pre-run σ({name}) = {sigma}");
    }

    let outcome = runner::run_with(config(), 10_000, RefPredictor::Trained(predictor)).unwrap();
    assert!(
        outcome.summary.invariants_ok,
        "{:?}",
        outcome.summary.failures
    );
    assert_eq!(outcome.summary.t, 2500);
    assert_eq!(outcome.summary.action_sequence, "OPMD".repeat(2500));

    // The summary carries the audit — the gate is measurable, not assumed.
    let post = outcome
        .summary
        .spectral_report
        .expect("trained runs must carry the σ audit");
    for (name, sigma) in [
        ("embed", post.embed),
        ("token", post.token),
        ("diffusion", post.diffusion),
        ("world_model", post.world_model),
    ] {
        assert!(sigma <= 1.0 + 1e-12, "post-run σ({name}) = {sigma}");
    }
    // The loaded matrices satisfy their declared bound, not just 1.0:
    // σ(embed) ≤ 1.0 (𝔸2), σ(conditioned) ≤ 0.49 (ℙ2).
    assert!(
        (post.token - 0.49).abs() < 1e-12,
        "σ(token) = {}",
        post.token
    );
    assert!((post.diffusion - 0.49).abs() < 1e-12);
    assert!((post.world_model - 0.49).abs() < 1e-12);
    assert!(
        (post.embed - 1.0).abs() < 1e-12,
        "σ(embed) = {}",
        post.embed
    );
}
