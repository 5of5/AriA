//! Relocated unit suite: AdamW convergence, the embedded Π_𝒮 (deflation ∘
//! σ-cap) in-ball guarantee, bit-determinism, and hyper-parameter domain.

use aria_engine_backends::spectral::{power_iteration, Matrix, DEFAULT_ITERATIONS};
use aria_training::linalg::{add_outer, matvec, zeros};
use aria_training::loss::{Grads, ModelParams};
use aria_training::{AdamW, AdamWParams, Lcg};

fn model(d: usize, input: usize, seed: u64, scale: f64) -> ModelParams {
    let mut rng = Lcg(seed);
    ModelParams {
        embed: (0..d)
            .map(|_| (0..input).map(|_| rng.unit() * scale).collect())
            .collect(),
        pred: (0..d)
            .map(|_| (0..d).map(|_| rng.unit() * scale).collect())
            .collect(),
    }
}

#[test]
fn adamw_converges_on_a_deterministic_least_squares_problem() {
    // Fit pred to reproduce a fixed target map on fixed inputs — a convex
    // quadratic. The target is projected INSIDE the ε/2 ball first, so the
    // optimum is representable under the embedded projection and AdamW must
    // drive the loss down by orders of magnitude.
    let dim = 6;
    let mut rng = Lcg(3);
    let raw: Matrix = (0..dim)
        .map(|_| (0..dim).map(|_| rng.unit() * 0.3).collect())
        .collect();
    let target: Matrix = aria_engine_backends::spectral::project_spectral(raw, 0.35).unwrap();
    let inputs: Vec<Vec<f64>> = (0..24)
        .map(|_| (0..dim).map(|_| rng.unit()).collect())
        .collect();

    let mut fitted = model(dim, dim, 5, 0.05);
    let mut opt = AdamW::new(
        AdamWParams {
            lr: 5e-3,
            ..AdamWParams::default()
        },
        &fitted,
    );

    let loss_of = |fitted: &ModelParams| -> f64 {
        inputs
            .iter()
            .map(|input| {
                let want = matvec(&target, input);
                let got = matvec(&fitted.pred, input);
                want.iter()
                    .zip(&got)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>()
            })
            .sum::<f64>()
            / inputs.len() as f64
    };

    let initial = loss_of(&fitted);
    for _ in 0..2000 {
        // Analytic gradient of the quadratic wrt pred.
        let mut grads = Grads {
            embed: zeros(dim, dim),
            pred: zeros(dim, dim),
        };
        let count = inputs.len() as f64;
        for input in &inputs {
            let want = matvec(&target, input);
            let got = matvec(&fitted.pred, input);
            let resid: Vec<f64> = got.iter().zip(&want).map(|(a, b)| a - b).collect();
            add_outer(&mut grads.pred, 2.0 / count, &resid, input);
        }
        opt.step(&mut fitted, &grads, 0.49).unwrap();
    }
    let final_loss = loss_of(&fitted);
    assert!(
        final_loss < initial * 1e-3,
        "AdamW must reduce the quadratic by ≥ 1000×: {initial} → {final_loss}"
    );
}

#[test]
fn every_step_exits_inside_both_balls() {
    // Adversarial gradients repeatedly push the weights outward; the embedded
    // projection must keep σ inside the balls after EVERY step.
    let mut m = model(6, 12, 9, 0.4);
    let mut opt = AdamW::new(
        AdamWParams {
            lr: 0.5,
            ..AdamWParams::default()
        },
        &m,
    );
    let bound = 0.49;
    let mut rng = Lcg(31);
    for step in 0..50 {
        let grads = Grads {
            embed: (0..6)
                .map(|_| (0..12).map(|_| rng.unit() * 10.0).collect())
                .collect(),
            pred: (0..6)
                .map(|_| (0..6).map(|_| rng.unit() * 10.0).collect())
                .collect(),
        };
        opt.step(&mut m, &grads, bound).unwrap();
        let s_e = power_iteration(&m.embed, DEFAULT_ITERATIONS).unwrap();
        let s_p = power_iteration(&m.pred, DEFAULT_ITERATIONS).unwrap();
        assert!(s_e <= 1.0 + 1e-12, "step {step}: σ(embed) = {s_e} > 1");
        assert!(
            s_p <= bound + 1e-12,
            "step {step}: σ(pred) = {s_p} > {bound}"
        );
    }
    assert_eq!(opt.steps(), 50);
}

#[test]
fn steps_are_bit_deterministic() {
    let grads = Grads {
        embed: vec![vec![0.3, -0.7]; 2],
        pred: vec![vec![-0.2, 0.11]; 2],
    };
    let run = || {
        let mut m = model(2, 2, 77, 0.2);
        let mut opt = AdamW::new(AdamWParams::default(), &m);
        for _ in 0..25 {
            opt.step(&mut m, &grads, 0.49).unwrap();
        }
        m
    };
    let (a, b) = (run(), run());
    for (ra, rb) in a
        .embed
        .iter()
        .zip(&b.embed)
        .chain(a.pred.iter().zip(&b.pred))
    {
        for (x, y) in ra.iter().zip(rb) {
            assert_eq!(x.to_bits(), y.to_bits());
        }
    }
}

#[test]
fn deflation_keeps_every_row_orthogonal_to_the_direction() {
    let mut rng = Lcg(41);
    let direction: Vec<f64> = (0..12).map(|_| rng.unit()).collect();
    let mut m = model(6, 12, 9, 0.4);
    let mut opt = AdamW::new(
        AdamWParams {
            lr: 0.3,
            ..AdamWParams::default()
        },
        &m,
    )
    .with_deflation(&direction);
    let unit_norm = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
    for step in 0..40 {
        let grads = Grads {
            embed: (0..6)
                .map(|_| (0..12).map(|_| rng.unit() * 5.0).collect())
                .collect(),
            pred: (0..6)
                .map(|_| (0..6).map(|_| rng.unit() * 5.0).collect())
                .collect(),
        };
        opt.step(&mut m, &grads, 0.49).unwrap();
        for (i, row) in m.embed.iter().enumerate() {
            let dot: f64 = row.iter().zip(&direction).map(|(a, b)| a * b).sum::<f64>() / unit_norm;
            assert!(dot.abs() < 1e-12, "step {step} row {i}: W·μ̂ = {dot}");
        }
        let s_e = power_iteration(&m.embed, DEFAULT_ITERATIONS).unwrap();
        assert!(s_e <= 1.0 + 1e-12, "deflation must compose with the σ-cap");
    }
}

#[test]
fn a_degenerate_deflation_direction_behaves_as_no_deflation() {
    // Behavioral proof (no private-field access): stepping with a degenerate
    // (all-zero) deflation direction is bit-identical to stepping without one.
    let grads = Grads {
        embed: vec![vec![0.4, -0.2, 0.9, 0.1]; 3],
        pred: vec![vec![-0.3, 0.25, 0.05]; 3],
    };
    let run = |deflate: bool| {
        let mut m = model(3, 4, 55, 0.3);
        let mut opt = AdamW::new(AdamWParams::default(), &m);
        if deflate {
            opt = opt.with_deflation(&[0.0; 4]);
        }
        for _ in 0..10 {
            opt.step(&mut m, &grads, 0.49).unwrap();
        }
        m
    };
    let (a, b) = (run(true), run(false));
    for (ra, rb) in a
        .embed
        .iter()
        .zip(&b.embed)
        .chain(a.pred.iter().zip(&b.pred))
    {
        for (x, y) in ra.iter().zip(rb) {
            assert_eq!(x.to_bits(), y.to_bits());
        }
    }
}

#[test]
fn params_domain_is_validated() {
    assert!(AdamWParams {
        lr: 0.0,
        ..AdamWParams::default()
    }
    .validate()
    .is_err());
    assert!(AdamWParams {
        beta1: 1.0,
        ..AdamWParams::default()
    }
    .validate()
    .is_err());
    assert!(AdamWParams {
        beta2: -0.1,
        ..AdamWParams::default()
    }
    .validate()
    .is_err());
    assert!(AdamWParams {
        eps: 0.0,
        ..AdamWParams::default()
    }
    .validate()
    .is_err());
    assert!(AdamWParams {
        weight_decay: f64::NAN,
        ..AdamWParams::default()
    }
    .validate()
    .is_err());
    AdamWParams::default().validate().unwrap();
    // The default lr is the receipted smoke protocol.
    assert!((AdamWParams::default().lr - 3e-3).abs() < 1e-15);
}
