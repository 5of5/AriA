//! High-assurance spectral primitives (plan WS1 — spec ℙ4, 𝕋4, §0.2, §5.4).
//!
//! Everything the trained backend needs to enforce the ℙ2 Lipschitz bound:
//!
//! - [`power_iteration`] estimates σ_max(W) by 𝕋4's alternating singular-vector
//!   sweeps — `v ← Wᵀu/‖Wᵀu‖₂`, `u ← Wv/‖Wv‖₂`, `σ = uᵀWv` — with a seeded
//!   (deterministic, OS-entropy-free) start vector and `r ∈ [2, 16]` iterations.
//! - [`project_spectral`] is 𝕋4's normalization update `W ← W / max(1.0, σ_max)`
//!   generalized to an arbitrary ball radius `bound`: `W ← W·min(1, bound/σ̂)`.
//!   At `bound = 1.0` it is exactly the spec formula. Under hard projection the
//!   projected matrix satisfies σ̂(W') ≤ bound — ε = 0.0 in weight space.
//!
//! # Cross-language contract (train_jepa.py)
//!
//! `python/training/train_jepa.py` carries a line-by-line identical
//! implementation (same start-vector generator, same sweep order, same
//! `DEFAULT_ITERATIONS`), so the Rust loader and the Python trainer enforce
//! the *same* quantity. The start vector comes from a fixed 64-bit LCG —
//! arithmetic that produces bit-identical values in Rust and Python — because
//! "seeded" must mean reproducible across languages, not just across runs.
//! Two independent estimates can only disagree within the estimation error
//! (~(σ₂/σ₁)^{2r}); identical start vectors remove even that.
//!
//! The estimate converges from below (a Rayleigh-type quotient is always
//! ≤ σ_max), so `project_spectral` under-projects by at most the estimation
//! gap — this is the spec's own definition of the enforced quantity (ℙ4
//! tolerance δ_σ at r iterations), audited by [`SpectralReport`] at runtime.

use serde::{Deserialize, Serialize};

/// A real matrix as the repo represents them everywhere.
pub type Matrix = Vec<Vec<f64>>;

/// Default power-iteration count — the top of the spec's r ∈ [2, 16] range.
/// Must match `POWER_ITERATION_ITERATIONS` in `python/training/train_jepa.py`.
pub const DEFAULT_ITERATIONS: usize = 16;

/// Seed for the start-vector LCG. Fixed, documented, no OS entropy (the repo
/// lock; WASM-compatible). Golden-ratio bits.
pub const START_VECTOR_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// Failure modes of the spectral primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SpectralError {
    /// `r` outside the spec's admissible iteration domain.
    #[error("power iteration count r = {0} violates the spec domain: r ∈ [2, 16] (spec §0.4)")]
    Iterations(usize),
}

/// σ_max estimates for every weight matrix a [`crate::TrainedPredictor`] owns.
///
/// The Phase-1 gate is measurable, not assumed: the σ-audit over a run is
/// these four numbers, each ≤ 1.0 under hard projection (𝕋4).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpectralReport {
    /// σ_max of the isometry I (𝔸2 bound: ≤ 1.0).
    pub embed: f64,
    /// σ_max of the token-conditioned predictor matrix.
    pub token: f64,
    /// σ_max of the diffusion-conditioned predictor matrix.
    pub diffusion: f64,
    /// σ_max of the world_model-conditioned predictor matrix.
    pub world_model: f64,
}

/// Deterministic LCG used for the power-iteration start vector.
///
/// `x_{n+1} = 6364136223846793005·x + 1442695040888963407 (mod 2⁶⁴)` — the
/// MMIX constants — masked to 64 bits, with values drawn as `(x >> 11) / 2⁵³ − 1`
/// (53-bit exact in f64). `python/training/train_jepa.py` implements the same
/// generator, so start vectors are bit-identical across languages.
fn next_lcg(mut x: u64) -> u64 {
    x = x
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    x
}

/// A unit-norm start vector: the seeded LCG stream mapped into [−1, 1)ⁿ.
fn seeded_unit_vector(n: usize) -> Vec<f64> {
    let mut x = START_VECTOR_SEED;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        x = next_lcg(x);
        let unit = ((x >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0) - 1.0;
        v.push(unit);
    }
    let norm = v.iter().map(|a| a * a).sum::<f64>().sqrt();
    if norm > 0.0 {
        for a in &mut v {
            *a /= norm;
        }
    }
    v
}

/// Estimate σ_max(W) by `r` alternating singular-vector sweeps (𝕋4).
///
/// ```text
/// v ← Wᵀu / ‖Wᵀu‖₂   then   u ← Wv / ‖Wv‖₂   ;   σ = uᵀWv = ‖Wv‖₂
/// ```
///
/// `r` must lie in [2, 16] (spec §0.4) — rejected with
/// [`SpectralError::Iterations`] otherwise. The start vector is the seeded
/// deterministic unit vector (no OS entropy). Empty matrices have σ = 0.
///
/// Delegates to [`power_iteration_with_vectors`] so there is one sweep
/// implementation; at equal `r` the σ estimate is bit-identical.
pub fn power_iteration(w: &Matrix, r: usize) -> Result<f64, SpectralError> {
    power_iteration_with_vectors(w, r).map(|(sigma, _, _)| sigma)
}

/// Like [`power_iteration`], but also returns the final singular-vector pair
/// `(u₁, v₁)` alongside σ_max — the subgradient direction `∂σ_max/∂W = u₁·v₁ᵀ`
/// consumed by the training-side spectral penalty (ARIA-TRAINING-PRD §ℒ_Spectral,
/// crates/aria-training). Identical seeded start vector, sweep order, and
/// iteration domain as [`power_iteration`]; at equal `r` the σ estimate is
/// bit-identical (asserted by test). Empty and rank-degenerate matrices return
/// σ = 0 with empty vectors, exactly where [`power_iteration`] returns 0.
///
/// Workspaces (`v`, `u_next`) are allocated once and reused across the `r`
/// sweeps. Each entry is overwritten by assignment (not accumulated) before
/// it is read, so the IEEE stream matches the previous per-sweep allocation.
pub fn power_iteration_with_vectors(
    w: &Matrix,
    r: usize,
) -> Result<(f64, Vec<f64>, Vec<f64>), SpectralError> {
    if !(2..=16).contains(&r) {
        return Err(SpectralError::Iterations(r));
    }
    if w.is_empty() || w[0].is_empty() {
        return Ok((0.0, Vec::new(), Vec::new()));
    }
    let rows = w.len();
    let cols = w[0].len();

    let mut u = seeded_unit_vector(rows);
    let mut v = vec![0.0; cols];
    let mut u_next = vec![0.0; rows];
    let mut sigma = 0.0;

    for _ in 0..r {
        // v = Wᵀ u
        v.fill(0.0);
        for (row, ui) in w.iter().zip(&u) {
            for (acc, a) in v.iter_mut().zip(row) {
                *acc += a * ui;
            }
        }
        let v_norm = v.iter().map(|a| a * a).sum::<f64>().sqrt();
        if v_norm <= 1e-300 {
            return Ok((0.0, Vec::new(), Vec::new()));
        }
        for a in &mut v {
            *a /= v_norm;
        }

        // u = W v ; σ = ‖W v‖₂ (= uᵀWv for the normalized u)
        for (acc, row) in u_next.iter_mut().zip(w) {
            *acc = row.iter().zip(&v).map(|(a, b)| a * b).sum();
        }
        sigma = u_next.iter().map(|a| a * a).sum::<f64>().sqrt();
        if sigma <= 1e-300 {
            return Ok((0.0, Vec::new(), Vec::new()));
        }
        for a in &mut u_next {
            *a /= sigma;
        }
        std::mem::swap(&mut u, &mut u_next);
    }

    Ok((sigma, u, v))
}

/// In-place 𝕋4 normalization: `W ← W·min(1, bound/σ̂(W))`. A matrix already
/// inside the ball (or zero) is left byte-identical. [`project_spectral`] is
/// the owning wrapper. A single scale can land 1 ulp above `bound`; the
/// training-side Π_𝒮 iterates this kernel to the estimator fixed point.
pub fn project_spectral_in_place(w: &mut Matrix, bound: f64) -> Result<(), SpectralError> {
    let sigma = power_iteration(w, DEFAULT_ITERATIONS)?;
    if sigma > bound && sigma != 0.0 {
        let scale = bound / sigma;
        for row in w {
            for v in row {
                *v *= scale;
            }
        }
    }
    Ok(())
}

/// 𝕋4's normalization update, generalized to radius `bound`:
/// `W ← W·min(1, bound/σ̂(W))` — at `bound = 1.0` this is exactly
/// `W ← W / max(1.0, σ_max)`. A matrix already inside the ball (or zero) is
/// returned unchanged; the projected matrix satisfies σ̂(W') ≤ bound.
pub fn project_spectral(mut w: Matrix, bound: f64) -> Result<Matrix, SpectralError> {
    project_spectral_in_place(&mut w, bound)?;
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(n: usize) -> Matrix {
        (0..n)
            .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
            .collect()
    }

    /// Scaled cyclic permutation: every column has exactly one entry `scale`,
    /// so σ_max = `scale` exactly for any start vector.
    fn cyclic(n: usize, scale: f64) -> Matrix {
        (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| if j == (i + 1) % n { scale } else { 0.0 })
                    .collect()
            })
            .collect()
    }

    /// A fixed orthogonal 2×2 rotation of angle `theta`.
    fn rot2(theta: f64) -> Vec<Vec<f64>> {
        let (s, c) = theta.sin_cos();
        vec![vec![c, -s], vec![s, c]]
    }

    /// W = U·diag(1.0, 0.5, 0.25, 0.1)·Vᵀ with U, V block-diagonal rotations —
    /// a 4×4 matrix with a known spectrum and a real spectral gap (σ₂/σ₁ = 0.5).
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
        let s = [1.0, 0.5, 0.25, 0.1];
        // W = U·diag(s)·Vᵀ
        (0..4)
            .map(|i| {
                (0..4)
                    .map(|j| (0..4).map(|k| u[i][k] * s[k] * vt[k][j]).sum())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn rejects_iteration_counts_outside_the_spec_domain() {
        for bad in [0, 1, 17, 128] {
            assert!(matches!(
                power_iteration(&identity(4), bad),
                Err(SpectralError::Iterations(b)) if b == bad
            ));
        }
        for good in [2, 16] {
            assert!(power_iteration(&identity(4), good).is_ok());
        }
    }

    #[test]
    fn identity_has_sigma_one() {
        let sigma = power_iteration(&identity(6), DEFAULT_ITERATIONS).unwrap();
        assert!((sigma - 1.0).abs() < 1e-12, "σ = {sigma}");
    }

    #[test]
    fn scaled_identity_and_cyclic_are_exact() {
        let m = identity(5)
            .into_iter()
            .map(|r| r.into_iter().map(|v| v * 3.5).collect())
            .collect();
        assert!((power_iteration(&m, DEFAULT_ITERATIONS).unwrap() - 3.5).abs() < 1e-12);

        let c = cyclic(16, 0.49);
        assert!((power_iteration(&c, 2).unwrap() - 0.49).abs() < 1e-12);
    }

    #[test]
    fn converges_on_a_matrix_with_a_spectral_gap() {
        // Gap 0.5 ⇒ error at r = 16 is ≈ 0.5³² ≈ 2e-10.
        let sigma = power_iteration(&gap_matrix(), DEFAULT_ITERATIONS).unwrap();
        assert!((sigma - 1.0).abs() < 1e-8, "σ = {sigma}");
    }

    #[test]
    fn is_deterministic() {
        let a = power_iteration(&gap_matrix(), 8).unwrap();
        let b = power_iteration(&gap_matrix(), 8).unwrap();
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "seeded estimator must be reproducible"
        );
    }

    #[test]
    // Exact comparison on purpose: an empty matrix's σ is the constant 0.0
    // returned by power_iteration, not an estimate.
    #[allow(clippy::float_cmp)]
    fn empty_matrices_have_sigma_zero() {
        assert_eq!(power_iteration(&vec![], DEFAULT_ITERATIONS).unwrap(), 0.0);
        assert_eq!(
            power_iteration(&vec![vec![]], DEFAULT_ITERATIONS).unwrap(),
            0.0
        );
    }

    #[test]
    fn non_square_matrices_are_supported() {
        // The isometry shape: latent_dim × 2·n_modes with orthonormal rows.
        // Two orthonormal rows in ℝ⁴ ⇒ σ = 1.
        let m: Matrix = vec![vec![1.0, 0.0, 0.0, 0.0], vec![0.0, 0.6, 0.8, 0.0]];
        assert!((power_iteration(&m, DEFAULT_ITERATIONS).unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn project_spectral_scales_an_over_bound_matrix_to_the_bound() {
        // 𝕋4: 5·I with bound 1.0 → W/max(1, σ) = I, entry-exact.
        let projected = project_spectral(
            identity(4)
                .into_iter()
                .map(|r| r.into_iter().map(|v| v * 5.0).collect())
                .collect(),
            1.0,
        )
        .unwrap();
        let sigma = power_iteration(&projected, DEFAULT_ITERATIONS).unwrap();
        assert!(sigma <= 1.0 + 1e-12, "σ = {sigma}");
        assert_eq!(projected, identity(4), "5·I / max(1, 5) must be exactly I");

        // The ε/2 loader case: bound generalizes the ball radius.
        let projected = project_spectral(
            identity(4)
                .into_iter()
                .map(|r| r.into_iter().map(|v| v * 5.0).collect())
                .collect(),
            0.49,
        )
        .unwrap();
        let sigma = power_iteration(&projected, DEFAULT_ITERATIONS).unwrap();
        assert!((sigma - 0.49).abs() < 1e-12, "σ = {sigma}");
    }

    #[test]
    fn project_spectral_leaves_a_compliant_matrix_unchanged() {
        let m: Matrix = identity(4)
            .into_iter()
            .map(|r| r.into_iter().map(|v| v * 0.25).collect())
            .collect();
        let projected = project_spectral(m.clone(), 0.49).unwrap();
        assert_eq!(
            projected, m,
            "a compliant matrix must come back byte-identical"
        );
    }

    #[test]
    fn project_spectral_handles_the_zero_matrix() {
        let z = vec![vec![0.0; 3]; 3];
        let projected = project_spectral(z.clone(), 1.0).unwrap();
        assert_eq!(projected, z);
    }
}
