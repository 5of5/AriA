//! RankMe representation-collapse monitor (ARIA-TRAINING-PRD §Representation
//! Collapse Monitoring; Garrido et al., ICML 2023, arXiv:2210.02885 — entry
//! in the skill bibliography since WS5).
//!
//! ```text
//! RankMe(Z) = exp(−Σ_k p_k ln p_k),   p_k = σ_k(Z) / Σ_j σ_j(Z)
//! ```
//!
//! over the holdout latent matrix Z ∈ ℝ^{B×d}. σ_k are obtained from the
//! eigenvalues of the Gram matrix ZᵀZ (d ≤ 64 in 𝒮, so the d×d eigenproblem
//! is tiny) via an in-repo **cyclic Jacobi** eigensolver — deterministic
//! sweep order, no external dependency, f64 throughout, `libm::log` for the
//! entropy (artifact-affecting transcendental discipline).
//!
//! Gate (PRD): if RankMe(Z) < α·d (default α = 0.30) the training engine
//! halts with a representation-collapse breach — a hard abort, not a warning.

use aria_engine_backends::spectral::Matrix;

use crate::TrainingError;

/// The PRD default collapse threshold α.
pub const DEFAULT_MIN_RANKME_FRAC: f64 = 0.30;

/// RankMe of a latent matrix `z` (rows = samples, columns = dimensions).
///
/// Degenerate inputs (no rows, or Σσ = 0 i.e. all-zero Z) return 0.0 — the
/// most collapsed possible score, which the gate then rejects loudly.
pub fn rankme(z: &[Vec<f64>]) -> f64 {
    if z.is_empty() || z[0].is_empty() {
        return 0.0;
    }
    let gram = gram(z);
    let eigenvalues = eigenvalues_symmetric(gram);
    // Numerical-rank floor: eigenvalues below 1e-12·λ_max are Jacobi noise on
    // rank-deficient Grams; √ would amplify them into phantom entropy mass
    // (measured: rank-1 Z scored 1.00000016 without the clamp). Relative to
    // λ_max, so RankMe stays scale-invariant.
    let lambda_max = eigenvalues.first().copied().unwrap_or(0.0).max(0.0);
    let floor = 1e-12 * lambda_max;
    let sigmas: Vec<f64> = eigenvalues
        .iter()
        .map(|&l| if l > floor { l.sqrt() } else { 0.0 })
        .collect();
    let total: f64 = sigmas.iter().sum();
    if total <= 0.0 || !total.is_finite() {
        return 0.0;
    }
    let mut entropy = 0.0;
    for s in sigmas {
        let p = s / total;
        if p > 0.0 {
            entropy -= p * libm::log(p);
        }
    }
    libm::exp(entropy)
}

/// The PRD collapse gate: `RankMe(Z) ≥ frac·d` or a hard
/// [`TrainingError::Collapse`]. Returns the measured value on success.
pub fn check_rankme(z: &[Vec<f64>], latent_dim: usize, frac: f64) -> Result<f64, TrainingError> {
    let value = rankme(z);
    let gate = frac * latent_dim as f64;
    if value < gate {
        return Err(TrainingError::Collapse(format!(
            "RankMe = {value:.4} < gate {gate:.4} ({frac} · d = {frac} · {latent_dim}) — \
             representation collapse breach (Garrido et al. 2023); training halted"
        )));
    }
    Ok(value)
}

/// Gram matrix `ZᵀZ` (d×d, symmetric PSD).
///
/// Upper triangle is accumulated and mirrored: each product `z_i z_j`
/// is formed once. IEEE values match the previous dense double loop
/// (`z_i z_j` = `z_j z_i` for finite f64).
#[allow(clippy::needless_range_loop)]
fn gram(z: &[Vec<f64>]) -> Matrix {
    let d = z[0].len();
    let mut g = vec![vec![0.0; d]; d];
    for row in z {
        for i in 0..d {
            let zi = row[i];
            g[i][i] += zi * zi;
            for j in (i + 1)..d {
                let v = zi * row[j];
                g[i][j] += v;
                g[j][i] += v;
            }
        }
    }
    g
}

/// Eigenvalues of a symmetric matrix by cyclic Jacobi rotations.
///
/// Deterministic row-major (p, q) sweep order; convergence when the
/// off-diagonal Frobenius norm falls below `1e-14 · ‖A‖_F` or after 64
/// sweeps (Jacobi converges quadratically — 64 is far beyond need for
/// d ≤ 64). Returns the diagonal in descending order. Public: the crate's
/// in-repo symmetric eigensolver (RankMe here; reusable by later spectral
/// audits).
// (p, q, c, s, t, τ) are the textbook Jacobi-rotation symbols, and the pivot
// indices must index both rows and columns of the symmetric in-place update —
// iterator forms would obscure the algorithm against its references.
#[allow(clippy::many_single_char_names, clippy::needless_range_loop)]
pub fn eigenvalues_symmetric(mut a: Matrix) -> Vec<f64> {
    let d = a.len();
    if d == 1 {
        return vec![a[0][0]];
    }
    let frob: f64 = a
        .iter()
        .flat_map(|r| r.iter())
        .map(|v| v * v)
        .sum::<f64>()
        .sqrt();
    let tol = 1e-14 * frob.max(f64::MIN_POSITIVE);

    for _sweep in 0..64 {
        let mut off = 0.0;
        for p in 0..d {
            for q in (p + 1)..d {
                off += a[p][q] * a[p][q];
            }
        }
        if (2.0 * off).sqrt() <= tol {
            break;
        }
        for p in 0..d {
            for q in (p + 1)..d {
                let apq = a[p][q];
                if apq.abs() <= tol / (d as f64) {
                    continue;
                }
                // Rotation angle: tan(2θ) = 2·a_pq / (a_qq − a_pp).
                let tau = (a[q][q] - a[p][p]) / (2.0 * apq);
                let t = if tau >= 0.0 {
                    1.0 / (tau + (1.0 + tau * tau).sqrt())
                } else {
                    -1.0 / (-tau + (1.0 + tau * tau).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;

                // A ← JᵀAJ, exploiting symmetry: update rows/cols p and q.
                for k in 0..d {
                    if k != p && k != q {
                        let akp = a[k][p];
                        let akq = a[k][q];
                        a[k][p] = c * akp - s * akq;
                        a[p][k] = a[k][p];
                        a[k][q] = s * akp + c * akq;
                        a[q][k] = a[k][q];
                    }
                }
                let app = a[p][p];
                let aqq = a[q][q];
                a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
                a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
                a[p][q] = 0.0;
                a[q][p] = 0.0;
            }
        }
    }

    let mut diag: Vec<f64> = (0..d).map(|i| a[i][i]).collect();
    diag.sort_by(|x, y| y.total_cmp(x));
    diag
}
