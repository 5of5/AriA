//! Integration suite for the training-facing spectral subgradient API.
//!
//! Kept outside `src/`: production backend modules contain only product code;
//! tests exercise the public API from the same boundary consumers use.

use aria_engine_backends::spectral::{
    power_iteration, power_iteration_with_vectors, project_spectral, project_spectral_in_place,
    Matrix, SpectralError, DEFAULT_ITERATIONS,
};

fn identity(n: usize) -> Matrix {
    (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect()
}

/// Scaled cyclic permutation: every singular value equals `scale`.
fn cyclic(n: usize, scale: f64) -> Matrix {
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| if j == (i + 1) % n { scale } else { 0.0 })
                .collect()
        })
        .collect()
}

fn rot2(theta: f64) -> Matrix {
    let (s, c) = theta.sin_cos();
    vec![vec![c, -s], vec![s, c]]
}

/// W = U·diag(1, .5, .25, .1)·Vᵀ: known spectrum, nontrivial vectors.
fn gap_matrix() -> Matrix {
    let u1 = rot2(0.31);
    let u2 = rot2(1.17);
    let v1 = rot2(0.77);
    let v2 = rot2(2.09);
    let u = [
        [u1[0][0], u1[0][1], 0.0, 0.0],
        [u1[1][0], u1[1][1], 0.0, 0.0],
        [0.0, 0.0, u2[0][0], u2[0][1]],
        [0.0, 0.0, u2[1][0], u2[1][1]],
    ];
    let vt = [
        [v1[0][0], v1[1][0], 0.0, 0.0],
        [v1[0][1], v1[1][1], 0.0, 0.0],
        [0.0, 0.0, v2[0][0], v2[1][0]],
        [0.0, 0.0, v2[0][1], v2[1][1]],
    ];
    let spectrum = [1.0, 0.5, 0.25, 0.1];
    (0..4)
        .map(|i| {
            (0..4)
                .map(|j| (0..4).map(|k| u[i][k] * spectrum[k] * vt[k][j]).sum())
                .collect()
        })
        .collect()
}

#[test]
fn with_vectors_sigma_is_bit_identical_to_power_iteration() {
    for r in [2, 8, 16] {
        for m in [identity(6), cyclic(16, 0.49), gap_matrix()] {
            let sigma = power_iteration(&m, r).unwrap();
            let (sigma_v, u, v) = power_iteration_with_vectors(&m, r).unwrap();
            assert_eq!(
                sigma.to_bits(),
                sigma_v.to_bits(),
                "σ must be bit-identical at r = {r}"
            );
            assert_eq!(u.len(), m.len());
            assert_eq!(v.len(), m[0].len());
            // Rayleigh identity: uᵀWv = σ for the returned normalized pair.
            let wv: Vec<f64> = m
                .iter()
                .map(|row| row.iter().zip(&v).map(|(a, b)| a * b).sum())
                .collect();
            let uwv: f64 = u.iter().zip(&wv).map(|(a, b)| a * b).sum();
            assert!((uwv - sigma_v).abs() < 1e-12, "uᵀWv = {uwv}, σ = {sigma_v}");
        }
    }
}

#[test]
fn with_vectors_returns_the_top_singular_pair_on_a_gap_matrix() {
    // ∂σ_max/∂W = u₁v₁ᵀ — verify the returned pair is the top pair by
    // first-order perturbation: σ(W + h·u₁v₁ᵀ) − σ(W) ≈ h.
    let m = gap_matrix();
    let (sigma, u, v) = power_iteration_with_vectors(&m, DEFAULT_ITERATIONS).unwrap();
    let h = 1e-6;
    let perturbed: Matrix = m
        .iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .map(|(j, w)| w + h * u[i] * v[j])
                .collect()
        })
        .collect();
    let sigma_p = power_iteration(&perturbed, DEFAULT_ITERATIONS).unwrap();
    assert!(
        ((sigma_p - sigma) / h - 1.0).abs() < 1e-4,
        "first-order gain {} must be ≈ 1",
        (sigma_p - sigma) / h
    );
}

#[test]
fn with_vectors_rejects_bad_iteration_counts_and_handles_empty() {
    assert!(matches!(
        power_iteration_with_vectors(&identity(4), 1),
        Err(SpectralError::Iterations(1))
    ));
    let (sigma, u, v) = power_iteration_with_vectors(&vec![], DEFAULT_ITERATIONS).unwrap();
    assert!(sigma == 0.0 && u.is_empty() && v.is_empty());
}

#[test]
fn in_place_projection_matches_owning_wrapper_and_is_idempotent_in_ball() {
    let m = gap_matrix();
    let owned = project_spectral(m.clone(), 0.49).unwrap();
    let mut in_place = m.clone();
    project_spectral_in_place(&mut in_place, 0.49).unwrap();
    assert_eq!(owned, in_place);

    // A second projection of an in-ball matrix must be a no-op.
    let before = in_place.clone();
    project_spectral_in_place(&mut in_place, 0.49).unwrap();
    assert_eq!(before, in_place);

    let sigma = power_iteration(&in_place, DEFAULT_ITERATIONS).unwrap();
    assert!(sigma <= 0.49 + 1e-12, "σ = {sigma}");
}
