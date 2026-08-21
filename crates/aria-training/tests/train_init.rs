//! Relocated unit suite: the deterministic initializers train() builds on —
//! seeded orthonormal rows (with optional mean deflation) and the in-ball
//! predictor start.

use aria_engine_backends::spectral::{power_iteration, DEFAULT_ITERATIONS};
use aria_training::train::{orthonormal_rows, seeded_in_ball};
use aria_training::Lcg;

#[test]
fn orthonormal_rows_have_unit_norm_and_zero_cross_dot() {
    let rows = orthonormal_rows(8, 32, 42, None);
    for (i, a) in rows.iter().enumerate() {
        let n: f64 = a.iter().map(|v| v * v).sum::<f64>().sqrt();
        assert!((n - 1.0).abs() < 1e-12, "row {i} norm {n}");
        for b in rows.iter().skip(i + 1) {
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            assert!(dot.abs() < 1e-12, "rows must be orthogonal, dot = {dot}");
        }
    }
    // σ_max of an orthonormal-row matrix is exactly 1.
    let sigma = power_iteration(&rows, DEFAULT_ITERATIONS).unwrap();
    assert!((sigma - 1.0).abs() < 1e-10, "σ = {sigma}");
}

#[test]
fn deflated_init_is_orthogonal_to_the_mean_direction() {
    let mut rng = Lcg(99);
    let mean: Vec<f64> = (0..32).map(|_| rng.unit()).collect();
    let rows = orthonormal_rows(8, 32, 42, Some(&mean));
    let norm = mean.iter().map(|v| v * v).sum::<f64>().sqrt();
    for (i, row) in rows.iter().enumerate() {
        let dot: f64 = row.iter().zip(&mean).map(|(a, b)| a * b).sum::<f64>() / norm;
        assert!(dot.abs() < 1e-12, "row {i} must be ⊥ mean, got {dot}");
        let n: f64 = row.iter().map(|v| v * v).sum::<f64>().sqrt();
        assert!((n - 1.0).abs() < 1e-12);
    }
    // Still exactly orthonormal ⇒ σ_max = 1.
    let sigma = power_iteration(&rows, DEFAULT_ITERATIONS).unwrap();
    assert!((sigma - 1.0).abs() < 1e-10);
    // A degenerate deflation direction is ignored, not fatal.
    let rows = orthonormal_rows(4, 16, 7, Some(&[0.0; 16]));
    assert_eq!(rows.len(), 4);
}

#[test]
fn init_is_deterministic_per_seed() {
    let a = orthonormal_rows(4, 16, 7, None);
    let b = orthonormal_rows(4, 16, 7, None);
    assert_eq!(a, b);
    assert_ne!(
        orthonormal_rows(4, 16, 7, None),
        orthonormal_rows(4, 16, 8, None)
    );

    let p = seeded_in_ball(6, 11, 0.245).unwrap();
    let q = seeded_in_ball(6, 11, 0.245).unwrap();
    assert_eq!(p, q);
    let sigma = power_iteration(&p, DEFAULT_ITERATIONS).unwrap();
    assert!(sigma <= 0.245 + 1e-12);
}
