//! Sub-linear memory growth measurement — 𝕃3's `|V| = O(T^β)`, `β ≤ 1`.
//!
//! Lemma 𝕃3 says that merging vertices within τ bounds the experience graph by
//! the sphere-packing capacity of a compact 𝒵, so `|V|` grows sub-linearly in
//! trajectory length. That is an empirical claim about a running engine, and
//! spec §8 predicate 3 requires it to be *measured*, so this module fits the
//! exponent instead of asserting it.
//!
//! The estimator is ordinary least squares on `(ln T, ln |V|)`: taking logs
//! turns `|V| = c·T^β` into a line whose slope is exactly β. R² is reported
//! alongside, because a β with no fit quality behind it is a number, not
//! evidence — a saturating graph (β → 0 at large T) is *better* than the bound
//! but fits the power law poorly, and the caller deserves to see that.

/// Result of fitting `|V| = c·T^β`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrowthFit {
    /// Fitted exponent β. Spec §8 predicate 3 requires `β ≤ 1`.
    pub beta: f64,
    /// Coefficient of determination of the log-log fit, in `[0, 1]`.
    pub r_squared: f64,
    /// Fitted intercept `ln c`.
    pub ln_c: f64,
    /// Samples that entered the fit (`T > 0` and `|V| > 0`).
    pub samples: usize,
}

/// Fit the growth exponent of `(T, |V|)` checkpoints.
///
/// Samples with `T == 0` or `|V| == 0` are dropped: `ln 0` is undefined, and an
/// empty graph carries no growth information. Returns `None` when fewer than
/// two usable samples remain, or when every `T` is identical (a vertical fit
/// has no slope).
pub fn fit_growth_exponent(samples: &[(u64, usize)]) -> Option<GrowthFit> {
    let pts: Vec<(f64, f64)> = samples
        .iter()
        .filter(|(t, v)| *t > 0 && *v > 0)
        .map(|(t, v)| (libm::log(*t as f64), libm::log(*v as f64)))
        .collect();

    if pts.len() < 2 {
        return None;
    }

    let n = pts.len() as f64;
    let mean_x = pts.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = pts.iter().map(|(_, y)| y).sum::<f64>() / n;

    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (x, y) in &pts {
        sxx += (x - mean_x) * (x - mean_x);
        sxy += (x - mean_x) * (y - mean_y);
    }
    if sxx <= 0.0 {
        return None; // every checkpoint at the same T
    }

    let beta = sxy / sxx;
    let ln_c = mean_y - beta * mean_x;

    let mut ss_res = 0.0;
    let mut ss_tot = 0.0;
    for (x, y) in &pts {
        let predicted = ln_c + beta * x;
        ss_res += (y - predicted) * (y - predicted);
        ss_tot += (y - mean_y) * (y - mean_y);
    }
    // A flat |V| (a saturated graph — 𝕃3's ideal) has no variance to explain,
    // so R² is 1 by definition rather than 0/0. The test is *relative*: the
    // mean of k identical logs is not bit-identical to those logs, so `ss_tot`
    // of a perfectly flat series is ~1e-31 rather than exactly 0, and an
    // `ss_tot <= 0.0` guard would fall through and divide two rounding errors.
    let y_scale = mean_y.abs().max(1.0);
    let flat = ss_tot <= f64::EPSILON * y_scale * y_scale * n;
    let r_squared = if flat { 1.0 } else { 1.0 - ss_res / ss_tot };

    Some(GrowthFit {
        beta,
        r_squared,
        ln_c,
        samples: pts.len(),
    })
}

/// Logarithmically spaced checkpoints in `[1, max]` — the sampling schedule the
/// plan specifies for the β fit (`T ∈ {2⁴, 2⁵, …}`).
///
/// Log spacing matters: linear checkpoints over-weight the tail, where a
/// saturating graph is flat, and would bias β downward for the wrong reason.
pub fn log_checkpoints(max: u64) -> Vec<u64> {
    let mut out = Vec::new();
    let mut t = 16u64;
    while t < max {
        out.push(t);
        t *= 2;
    }
    if max > 0 {
        out.push(max);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recover a known exponent from noiseless synthetic data.
    #[test]
    fn recovers_a_known_exponent() {
        for beta in [0.25f64, 0.5, 0.75, 1.0] {
            let samples: Vec<(u64, usize)> = (4..=16u32)
                .map(|e| {
                    let t = 1u64 << e;
                    // Scaled by 100 so integer quantization is a fraction of a
                    // percent: |V| is a count, and rounding 16^0.25 = 2 to an
                    // integer would inject 16% noise at the small end and make
                    // the fixture, not the estimator, set the R² ceiling.
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "synthetic fixture: values are positive, finite and « 2^53"
                    )]
                    let v = (100.0 * (t as f64).powf(beta)).round() as usize;
                    (t, v)
                })
                .collect();
            let fit = fit_growth_exponent(&samples).expect("fit must succeed");
            assert!(
                (fit.beta - beta).abs() < 0.01,
                "recovered β = {:.4}, expected {beta}",
                fit.beta
            );
            assert!(fit.r_squared > 0.999, "R² = {:.4}", fit.r_squared);
        }
    }

    #[test]
    fn linear_growth_gives_beta_one() {
        #[allow(clippy::cast_possible_truncation, reason = "t ≤ 64")]
        let samples: Vec<(u64, usize)> = (1..=64u64).map(|t| (t, t as usize)).collect();
        let fit = fit_growth_exponent(&samples).unwrap();
        assert!((fit.beta - 1.0).abs() < 1e-12, "β = {}", fit.beta);
        assert!((fit.r_squared - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_saturated_graph_reports_zero_growth_not_a_nan() {
        // |V| pinned at 42 — 𝕃3's ideal: growth has stopped entirely.
        let samples: Vec<(u64, usize)> = (1..=32u64).map(|t| (t, 42)).collect();
        let fit = fit_growth_exponent(&samples).unwrap();
        assert!(fit.beta.abs() < 1e-12, "β = {}", fit.beta);
        assert!(
            (fit.r_squared - 1.0).abs() < 1e-12,
            "a flat series must not produce 0/0: R² = {}",
            fit.r_squared
        );
    }

    #[test]
    fn degenerate_inputs_return_none_rather_than_nan() {
        assert!(fit_growth_exponent(&[]).is_none());
        assert!(
            fit_growth_exponent(&[(10, 5)]).is_none(),
            "one point has no slope"
        );
        assert!(
            fit_growth_exponent(&[(0, 0), (0, 5)]).is_none(),
            "T = 0 samples are unusable"
        );
        assert!(
            fit_growth_exponent(&[(8, 3), (8, 9)]).is_none(),
            "a vertical fit has no slope"
        );
    }

    #[test]
    fn zero_valued_samples_are_dropped_not_logged() {
        let samples = [(0u64, 0usize), (16, 4), (64, 8), (256, 16)];
        let fit = fit_growth_exponent(&samples).unwrap();
        assert_eq!(fit.samples, 3, "the (0, 0) sample must be dropped");
        // 4, 8, 16 over 16, 64, 256 is exactly β = 0.5.
        assert!((fit.beta - 0.5).abs() < 1e-12, "β = {}", fit.beta);
    }

    #[test]
    fn checkpoints_are_log_spaced_and_include_the_endpoint() {
        let cps = log_checkpoints(1000);
        assert_eq!(cps, vec![16, 32, 64, 128, 256, 512, 1000]);
        assert_eq!(log_checkpoints(0), Vec::<u64>::new());
        assert_eq!(
            log_checkpoints(8),
            vec![8],
            "max below the first checkpoint"
        );
    }
}
