//! Relocated unit suite: the RankMe collapse monitor (in-repo Jacobi
//! eigensolver) and the Wilcoxon + bootstrap statistical certification.

use aria_engine_backends::spectral::{power_iteration, DEFAULT_ITERATIONS};
use aria_training::collapse::{
    check_rankme, eigenvalues_symmetric, rankme, DEFAULT_MIN_RANKME_FRAC,
};
use aria_training::eval::wilcoxon_paired;
use aria_training::linalg::zeros;
use aria_training::Lcg;

fn seeded_z(rows: usize, d: usize, seed: u64) -> Vec<Vec<f64>> {
    let mut rng = Lcg(seed);
    (0..rows)
        .map(|_| (0..d).map(|_| rng.unit()).collect())
        .collect()
}

/// Gram matrix built in-test (the production one is private by design).
fn gram(z: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let d = z[0].len();
    let mut g = zeros(d, d);
    for row in z {
        for (i, &zi) in row.iter().enumerate() {
            for (gij, &zj) in g[i].iter_mut().zip(row) {
                *gij += zi * zj;
            }
        }
    }
    g
}

// ---- RankMe / Jacobi ----

#[test]
fn identity_directions_give_full_rank_me() {
    // Z with equal energy in every dimension: p_k uniform ⇒ RankMe = d.
    let d = 8;
    let mut z = Vec::new();
    for i in 0..d {
        let mut row = vec![0.0; d];
        row[i] = 2.0;
        z.push(row);
    }
    let r = rankme(&z);
    assert!((r - d as f64).abs() < 1e-9, "RankMe = {r}, want {d}");
}

#[test]
fn rank_one_z_scores_one() {
    // Every row is a multiple of one direction ⇒ single σ ⇒ RankMe = 1.
    let base: Vec<f64> = vec![0.3, -0.7, 0.2, 0.9];
    let z: Vec<Vec<f64>> = (1..=6)
        .map(|k| base.iter().map(|v| v * f64::from(k)).collect())
        .collect();
    let r = rankme(&z);
    assert!((r - 1.0).abs() < 1e-9, "RankMe = {r}, want 1");
}

#[test]
fn rankme_is_scale_invariant_and_deterministic() {
    let z = seeded_z(64, 16, 7);
    let scaled: Vec<Vec<f64>> = z
        .iter()
        .map(|r| r.iter().map(|v| v * 37.5).collect())
        .collect();
    let a = rankme(&z);
    let b = rankme(&scaled);
    assert!((a - b).abs() < 1e-9, "scale invariance: {a} vs {b}");
    assert_eq!(a.to_bits(), rankme(&z).to_bits(), "deterministic");
    // A full-rank random Z should score high (well above the 0.3·d gate).
    assert!(a > 0.3 * 16.0, "random full-rank Z scores {a}");
}

#[test]
fn jacobi_top_eigenvalue_matches_power_iteration_sigma() {
    // σ₁(Z)² = λ₁(ZᵀZ): cross-check the two in-repo estimators.
    let z = seeded_z(40, 8, 21);
    let g = gram(&z);
    let lambda1 = eigenvalues_symmetric(g.clone())[0];
    let sigma1_g = power_iteration(&g, DEFAULT_ITERATIONS).unwrap();
    // For symmetric PSD G, σ_max(G) = λ_max(G).
    assert!(
        (lambda1 - sigma1_g).abs() / sigma1_g.max(1e-30) < 1e-10,
        "Jacobi λ₁ = {lambda1} vs power-iteration σ = {sigma1_g}"
    );
}

#[test]
fn jacobi_recovers_a_known_spectrum() {
    // Diagonal matrix rotated by a known orthogonal transform: eigenvalues
    // must come back exactly (up to fp) regardless of the rotation.
    let evals = [9.0, 4.0, 1.0, 0.25];
    let (s, c) = (0.6, 0.8); // exact 3-4-5 rotation
    let q = [
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, c, s],
        [0.0, 0.0, -s, c],
    ];
    let mut a = zeros(4, 4);
    for (k, &lambda) in evals.iter().enumerate() {
        for i in 0..4 {
            for j in 0..4 {
                a[i][j] += lambda * q[k][i] * q[k][j];
            }
        }
    }
    let got = eigenvalues_symmetric(a);
    for (g, want) in got.iter().zip(&evals) {
        assert!((g - want).abs() < 1e-12, "eigenvalue {g} vs {want}");
    }
}

#[test]
fn the_gate_fires_on_collapsed_z_and_passes_healthy_z() {
    let collapsed: Vec<Vec<f64>> = (1..=10)
        .map(|k| {
            vec![
                f64::from(k),
                2.0 * f64::from(k),
                -f64::from(k),
                0.5 * f64::from(k),
            ]
        })
        .collect();
    let err = check_rankme(&collapsed, 4, DEFAULT_MIN_RANKME_FRAC)
        .unwrap_err()
        .to_string();
    assert!(err.contains("RankMe"), "got: {err}");
    assert!(err.contains("collapse"), "got: {err}");

    let healthy = seeded_z(64, 4, 3);
    let v = check_rankme(&healthy, 4, DEFAULT_MIN_RANKME_FRAC).unwrap();
    assert!(v >= 1.2);

    // Degenerate inputs score 0 and fail.
    assert!(check_rankme(&[], 4, 0.3).is_err());
    let zeros_z = vec![vec![0.0; 4]; 8];
    assert!(check_rankme(&zeros_z, 4, 0.3).is_err());
}

// ---- Wilcoxon + bootstrap ----

/// Hand-computed anchor: d = 1..30, all positive.
/// W⁺ = Σ1..30 = 465, μ = 232.5, σ² = 30·31·61/24 = 2363.75,
/// z = (465 − 232.5 − 0.5)/√2363.75 = 232/48.6184… = 4.7719,
/// p = ½·erfc(4.7719/√2) ≈ 9.1e-7.
#[test]
fn all_positive_anchor_matches_the_normal_approximation() {
    let persistence: Vec<f64> = (1..=30).map(|i| 10.0 + f64::from(i)).collect();
    let model: Vec<f64> = vec![10.0; 30];
    let r = wilcoxon_paired(&model, &persistence, 42).unwrap();
    assert_eq!(r.n_effective, 30);
    assert!((r.w_plus - 465.0).abs() < 1e-12);
    assert!((r.w_minus - 0.0).abs() < 1e-12);
    assert!((r.z - 4.7719).abs() < 1e-3, "z = {}", r.z);
    assert!(
        r.p_one_sided < 1e-6 && r.p_one_sided > 1e-8,
        "p = {}",
        r.p_one_sided
    );
    assert!((r.median_improvement - 15.5).abs() < 1e-12);
    // CI of the median of 1..30 is inside [1, 30] and positive.
    assert!(r.ci99.0 > 0.0 && r.ci99.1 <= 30.0 && r.ci99.0 <= r.ci99.1);
}

#[test]
fn rank_sum_invariant_and_symmetric_null_is_insignificant() {
    // Antisymmetric differences ⇒ W⁺ ≈ W⁻ ⇒ p far from significance.
    // (i = 0 gives a zero difference, which the wilcox convention drops —
    // the invariant is stated over n_effective.)
    let model: Vec<f64> = (0..40)
        .map(|i| {
            if i % 2 == 0 {
                1.0 + 0.01 * f64::from(i)
            } else {
                1.0 - 0.01 * f64::from(i)
            }
        })
        .collect();
    let persistence = vec![1.0; 40];
    let r = wilcoxon_paired(&model, &persistence, 7).unwrap();
    assert_eq!(r.n_effective, 39, "the single zero difference is dropped");
    let n = r.n_effective as f64;
    assert!((r.w_plus + r.w_minus - n * (n + 1.0) / 2.0).abs() < 1e-9);
    assert!(
        r.p_one_sided > 0.05,
        "symmetric null must not certify, p = {}",
        r.p_one_sided
    );
}

#[test]
fn p_is_monotone_in_the_shift() {
    let model = vec![1.0; 30];
    let mk = |shift: f64| -> f64 {
        let persistence: Vec<f64> = (1..=30)
            .map(|i| 1.0 + shift + 0.001 * f64::from(i))
            .collect();
        wilcoxon_paired(&model, &persistence, 3)
            .unwrap()
            .p_one_sided
    };
    let p_small = mk(0.0005);
    let p_large = mk(0.5);
    assert!(
        p_large <= p_small,
        "stronger shift ⇒ smaller p: {p_large} vs {p_small}"
    );
}

#[test]
fn ties_get_average_ranks_and_the_correction_is_applied() {
    // 15 pairs at +1 and 15 at −1 (all tie in |d|): average rank = 15.5,
    // W⁺ = 15·15.5 = 232.5, z ≈ 0 ⇒ p ≈ 0.5.
    let mut persistence = vec![2.0; 15];
    persistence.extend(vec![0.0; 15]);
    let model = vec![1.0; 30];
    let r = wilcoxon_paired(&model, &persistence, 11).unwrap();
    assert!((r.w_plus - 232.5).abs() < 1e-12);
    assert!((r.p_one_sided - 0.5).abs() < 0.05, "p = {}", r.p_one_sided);
}

#[test]
fn floor_and_shape_faults_are_rejected() {
    let short = vec![1.0; 10];
    assert!(
        wilcoxon_paired(&short, &short, 1).is_err(),
        "n < 30 must reject"
    );
    // All-zero differences also collapse below the floor after zero-drop.
    let same = vec![1.0; 30];
    assert!(wilcoxon_paired(&same, &same, 1).is_err());
    let a = vec![1.0; 30];
    let b = vec![1.0; 29];
    assert!(wilcoxon_paired(&a, &b, 1).is_err());
}

#[test]
fn bootstrap_is_deterministic_and_brackets_the_median() {
    let persistence: Vec<f64> = (1..=50).map(f64::from).collect();
    let model = vec![0.0; 50];
    let r1 = wilcoxon_paired(&model, &persistence, 99).unwrap();
    let r2 = wilcoxon_paired(&model, &persistence, 99).unwrap();
    assert_eq!(r1.ci99.0.to_bits(), r2.ci99.0.to_bits());
    assert_eq!(r1.ci99.1.to_bits(), r2.ci99.1.to_bits());
    assert!(r1.ci99.0 <= r1.median_improvement && r1.median_improvement <= r1.ci99.1);
    // The CI is a genuine sub-range of the data (order statistics of bootstrap
    // medians live strictly inside the sample range for this configuration).
    // Note: different seeds may legitimately produce IDENTICAL endpoints —
    // bootstrap-median order statistics take values on the discrete lattice of
    // the sample, so no cross-seed inequality is asserted.
    assert!(r1.ci99.0 >= 1.0 && r1.ci99.1 <= 50.0);
    assert!(
        r1.ci99.1 - r1.ci99.0 < 49.0,
        "the 99% CI must be informative"
    );
}
