//! Held-out statistical certification (ARIA-TRAINING-PRD §Statistical
//! Certification & Quality Gating): paired Wilcoxon signed-rank test of the
//! trained predictor against the persistence baseline over ≥ 30 held-out
//! trajectories, plus a seeded bootstrap confidence interval of the median
//! improvement — the WS5 receipt fields, computed natively.
//!
//! Method (documented so the receipt is auditable):
//! * differences dᵢ = persistenceᵢ − modelᵢ per held-out trajectory
//!   (positive = the model improves on copying the latent);
//! * zeros dropped (the `wilcox` convention, matching the WS5-era scipy
//!   default), average ranks on ties;
//! * W⁺ = Σ ranks of positive dᵢ; normal approximation with tie correction
//!   `σ² = n(n+1)(2n+1)/24 − Σ(t³−t)/48` and continuity correction;
//! * one-sided p (H₁: median improvement > 0) via `p = ½·erfc(z/√2)`
//!   (`libm::erfc` — the artifact-affecting transcendental discipline);
//! * n ≥ 30 enforced (the PRD's trajectory floor for the gate);
//! * bootstrap: 10,000 seeded-LCG resamples of the median, 99% percentile CI.

use crate::{Lcg, TrainingError};
use serde::{Deserialize, Serialize};

/// Minimum effective sample size the PRD allows for the gate.
pub const MIN_TRAJECTORIES: usize = 30;
/// Bootstrap resamples for the CI (WS5 protocol scale).
pub const BOOTSTRAP_RESAMPLES: usize = 10_000;

/// The certification measurement — every field lands in the receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WilcoxonReport {
    /// Effective n after zero-drop.
    pub n_effective: usize,
    /// Sum of ranks of positive differences.
    pub w_plus: f64,
    /// Sum of ranks of negative differences.
    pub w_minus: f64,
    /// Continuity-corrected z statistic.
    pub z: f64,
    /// One-sided p-value for H₁: median(persistence − model) > 0.
    pub p_one_sided: f64,
    /// Median of the differences (positive = improvement).
    pub median_improvement: f64,
    /// Seeded bootstrap 99% CI of the median improvement.
    pub ci99: (f64, f64),
}

/// Run the paired test on per-trajectory (model, persistence) residuals.
// Exact f64 comparisons are the *semantics* here: the wilcox convention drops
// exactly-zero differences, and tie groups are defined by exact |d| equality.
#[allow(clippy::float_cmp)]
pub fn wilcoxon_paired(
    model: &[f64],
    persistence: &[f64],
    seed: u64,
) -> Result<WilcoxonReport, TrainingError> {
    if model.len() != persistence.len() {
        return Err(TrainingError::Config(format!(
            "paired test needs equal-length samples, got {} vs {}",
            model.len(),
            persistence.len()
        )));
    }
    let diffs: Vec<f64> = persistence.iter().zip(model).map(|(p, m)| p - m).collect();
    if diffs.iter().any(|d| !d.is_finite()) {
        return Err(TrainingError::Config(
            "non-finite residual difference in the paired test".into(),
        ));
    }

    // wilcox convention: drop exact zeros.
    let mut nonzero: Vec<f64> = diffs.iter().copied().filter(|d| *d != 0.0).collect();
    let n = nonzero.len();
    if n < MIN_TRAJECTORIES {
        return Err(TrainingError::Config(format!(
            "Wilcoxon gate needs ≥ {MIN_TRAJECTORIES} non-zero held-out trajectory pairs, got {n} \
             (PRD statistical-certification floor)"
        )));
    }

    // Rank |d| ascending with average ranks for ties.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| nonzero[i].abs().total_cmp(&nonzero[j].abs()));
    let mut ranks = vec![0.0; n];
    let mut tie_correction = 0.0;
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && nonzero[order[j + 1]].abs() == nonzero[order[i]].abs() {
            j += 1;
        }
        // Average rank for the tie group [i, j] (1-based ranks).
        let avg = (i + 1 + j + 1) as f64 / 2.0;
        for &idx in &order[i..=j] {
            ranks[idx] = avg;
        }
        let t = (j - i + 1) as f64;
        if t > 1.0 {
            tie_correction += t * t * t - t;
        }
        i = j + 1;
    }

    let w_plus: f64 = nonzero
        .iter()
        .zip(&ranks)
        .filter(|(d, _)| **d > 0.0)
        .map(|(_, r)| r)
        .sum();
    let nf = n as f64;
    let w_total = nf * (nf + 1.0) / 2.0;
    let w_minus = w_total - w_plus;

    let mean = w_total / 2.0;
    let variance = nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0 - tie_correction / 48.0;
    let sd = variance.sqrt();
    // Continuity correction toward the mean.
    let z = if w_plus > mean {
        (w_plus - mean - 0.5) / sd
    } else {
        (w_plus - mean + 0.5) / sd
    };
    // One-sided: improvement means W⁺ large ⇒ p = P(Z ≥ z).
    let p_one_sided = 0.5 * libm::erfc(z / std::f64::consts::SQRT_2);

    // Median improvement + seeded bootstrap CI over the raw differences
    // (zeros included — the estimand is the median of all paired diffs).
    let median_improvement = median(&mut diffs.clone());
    let ci99 = bootstrap_median_ci(&diffs, seed, BOOTSTRAP_RESAMPLES, 0.99);

    nonzero.clear();
    Ok(WilcoxonReport {
        n_effective: n,
        w_plus,
        w_minus,
        z,
        p_one_sided,
        median_improvement,
        ci99,
    })
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    }
}

/// Percentile bootstrap CI of the median: seeded, deterministic.
fn bootstrap_median_ci(diffs: &[f64], seed: u64, resamples: usize, level: f64) -> (f64, f64) {
    let n = diffs.len();
    let mut rng = Lcg(seed ^ 0xB007_5712_49E3_779B);
    let mut medians = Vec::with_capacity(resamples);
    let mut sample = vec![0.0; n];
    for _ in 0..resamples {
        for slot in &mut sample {
            *slot = diffs[rng.index(n)];
        }
        medians.push(median(&mut sample));
    }
    medians.sort_by(f64::total_cmp);
    let alpha = (1.0 - level) / 2.0;
    // Cast safety: level ∈ (0, 1) ⇒ both products lie in [0, resamples − 1].
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lo_idx = ((resamples - 1) as f64 * alpha).floor() as usize;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let hi_idx = ((resamples - 1) as f64 * (1.0 - alpha)).ceil() as usize;
    (medians[lo_idx], medians[hi_idx.min(resamples - 1)])
}
