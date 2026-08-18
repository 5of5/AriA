use aria_engine_core::condition::Condition;
use aria_engine_core::engine::Predictor;
use num_complex::Complex64;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};

/// Simulated predictor backend.
///
/// I: H → Z — random linear isometry (orthonormal rows).
/// P: Z × Condition → Z — random orthogonal matrix scaled to be contractive.
/// d: Z × Z → R — Euclidean distance.
///
/// The predictor scale is chosen so that the worst-case residual jump after an
/// arbitrary unitary OpticalStep is bounded by eps = 1.0:
///     ||z - P(I(U ψ)))|| ≤ ||z|| + ||P(I(U ψ))|| ≤ scale + scale ≤ 1.0
/// when z itself lies in the image of P (norm ≤ scale). This makes Inv2 hold
/// deterministically for the simulated stub while remaining Spec-faithful.
#[derive(Debug)]
pub struct SimPredictor {
    /// Embedding matrix I: H → Z, shape [latent_dim × (2*n_modes)]
    embed_matrix: Vec<Vec<f64>>,
    /// Predictor matrix for token conditioning
    pred_matrix_token: Vec<Vec<f64>>,
    /// Predictor matrix for diffusion conditioning
    pred_matrix_diff: Vec<Vec<f64>>,
    /// Predictor matrix for world_model conditioning
    pred_matrix_wm: Vec<Vec<f64>>,
}

impl SimPredictor {
    /// Lipschitz scale for the predictor. 2*SCALE ≤ eps (eps = 1.0 default).
    const PRED_SCALE: f64 = 0.49;

    pub fn new(n_modes: usize, latent_dim: usize) -> Self {
        let mut rng = StdRng::seed_from_u64(12345);

        let input_dim = 2 * n_modes;
        assert!(
            latent_dim <= input_dim,
            "latent_dim {latent_dim} must be ≤ 2*n_modes {input_dim} for a real isometry stub"
        );

        // I: random isometry / partial isometry (orthonormal rows)
        let embed_matrix = random_isometry(&mut rng, latent_dim, input_dim);

        // P: contractive orthogonal maps, one per conditioning
        let pred_matrix_token = scale(random_orthogonal(&mut rng, latent_dim), Self::PRED_SCALE);
        let pred_matrix_diff = scale(random_orthogonal(&mut rng, latent_dim), Self::PRED_SCALE);
        let pred_matrix_wm = scale(random_orthogonal(&mut rng, latent_dim), Self::PRED_SCALE);

        SimPredictor {
            embed_matrix,
            pred_matrix_token,
            pred_matrix_diff,
            pred_matrix_wm,
        }
    }

    /// Flatten complex vector into real vector [re(0), im(0), re(1), im(1), ...]
    fn flatten_complex(psi: &[Complex64]) -> Vec<f64> {
        let mut v = Vec::with_capacity(psi.len() * 2);
        for c in psi {
            v.push(c.re);
            v.push(c.im);
        }
        v
    }

    /// Matrix-vector multiply: y = M * x
    fn mat_vec(m: &[Vec<f64>], x: &[f64]) -> Vec<f64> {
        m.iter()
            .map(|row| row.iter().zip(x.iter()).map(|(a, b)| a * b).sum())
            .collect()
    }
}

impl Predictor for SimPredictor {
    fn embed(&self, psi: &[Complex64]) -> Vec<f64> {
        let flat = Self::flatten_complex(psi);
        Self::mat_vec(&self.embed_matrix, &flat)
    }

    fn predict(&self, z: &[f64], a: Condition) -> Vec<f64> {
        let matrix = match a {
            Condition::Token => &self.pred_matrix_token,
            Condition::Diffusion => &self.pred_matrix_diff,
            Condition::WorldModel => &self.pred_matrix_wm,
        };
        Self::mat_vec(matrix, z)
    }

    fn dist(&self, a: &[f64], b: &[f64]) -> f64 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

/// Generate a random matrix with orthonormal rows (partial isometry).
fn random_isometry(rng: &mut StdRng, rows: usize, cols: usize) -> Vec<Vec<f64>> {
    assert!(rows <= cols, "rows must be ≤ cols for an isometry");
    let mut m: Vec<Vec<f64>> = (0..rows)
        .map(|_| (0..cols).map(|_| StandardNormal.sample(rng)).collect())
        .collect();
    gram_schmidt_rows(&mut m);
    m
}

/// Generate a random square orthogonal matrix.
fn random_orthogonal(rng: &mut StdRng, n: usize) -> Vec<Vec<f64>> {
    random_isometry(rng, n, n)
}

/// Scale every entry of a matrix by `s`.
fn scale(m: Vec<Vec<f64>>, s: f64) -> Vec<Vec<f64>> {
    m.into_iter()
        .map(|row| row.into_iter().map(|x| x * s).collect())
        .collect()
}

/// Modified Gram-Schmidt orthonormalization of the rows of `m`.
fn gram_schmidt_rows(m: &mut [Vec<f64>]) {
    let rows = m.len();
    for i in 0..rows {
        for j in 0..i {
            let dot: f64 = m[i].iter().zip(&m[j]).map(|(a, b)| a * b).sum();
            if dot != 0.0 {
                for k in 0..m[i].len() {
                    m[i][k] -= dot * m[j][k];
                }
            }
        }
        let norm: f64 = m[i].iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 1e-12 {
            for k in 0..m[i].len() {
                m[i][k] /= norm;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_produces_correct_dim() {
        let p = SimPredictor::new(8, 16);
        let psi: Vec<Complex64> = (0..8)
            .map(|i| Complex64::new(f64::from(i), -f64::from(i)))
            .collect();
        let z = p.embed(&psi);
        assert_eq!(z.len(), 16);
        assert!(z.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn embed_preserves_norm_for_square_isometry() {
        let p = SimPredictor::new(8, 16);
        let psi: Vec<Complex64> = (0..8).map(|_| Complex64::new(1.0, 0.0)).collect();
        let norm_psi = psi
            .iter()
            .map(num_complex::Complex::norm_sqr)
            .sum::<f64>()
            .sqrt();
        let z = p.embed(&psi);
        let norm_z = z.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!(
            (norm_z - norm_psi).abs() < 1e-9,
            "isometry should preserve norm"
        );
    }

    #[test]
    fn predict_contractive() {
        let p = SimPredictor::new(8, 16);
        let z1: Vec<f64> = (0..16).map(|i| f64::from(i).sin()).collect();
        let z2: Vec<f64> = (0..16).map(|i| f64::from(i).cos()).collect();
        let d_in = p.dist(&z1, &z2);
        let pz1 = p.predict(&z1, Condition::Token);
        let pz2 = p.predict(&z2, Condition::Token);
        let d_out = p.dist(&pz1, &pz2);
        assert!(
            d_out <= d_in * SimPredictor::PRED_SCALE + 1e-9,
            "predictor Lipschitz bound violated: {} > {} * {}",
            d_out,
            d_in,
            SimPredictor::PRED_SCALE
        );
    }

    #[test]
    // Exact comparison on purpose: TLA Dist has a zero diagonal — d(z, z)
    // must be exactly 0.0, not approximately.
    #[allow(clippy::float_cmp)]
    fn dist_zero_diagonal() {
        let p = SimPredictor::new(8, 4);
        let z = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(p.dist(&z, &z), 0.0);
    }
}
