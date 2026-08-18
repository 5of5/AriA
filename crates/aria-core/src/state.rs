use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::graph::Graph;

/// Full Aria state — the discrete Spec variables.
///
/// Observable core: ⟨ψ, z, G, t⟩.
/// prev_res is auxiliary history for Inv2 only (TLA prevRes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// Optical field amplitudes ψ ∈ C^N — 𝔸1
    pub psi: Vec<Complex64>,
    /// JEPA latent z ∈ Z — 𝔸2
    pub z: Vec<f64>,
    /// Experience/thought graph G — 𝔸3
    pub g: Graph,
    /// Discrete step counter t ∈ N
    pub t: u64,
    /// Auxiliary: previous residual for Inv2
    pub prev_res: f64,
    /// Initial field energy ‖ψ₀‖₂ — cached for Inv1
    pub energy_0: f64,
}

impl State {
    /// Compute the current field energy ‖ψ‖₂.
    pub fn energy(&self) -> f64 {
        field_energy(&self.psi)
    }

    /// Compute the current JEPA residual:
    /// Res(ψ, z, t) = d(z, P(I(ψ), a_t))
    ///
    /// This is computed by the Predictor backend; see engine for usage.
    /// Here we provide the distance function used in Inv2 comparison.
    pub fn residual(&self, predicted_z: &[f64]) -> f64 {
        euclidean_distance(&self.z, predicted_z)
    }
}

/// Euclidean distance in Z (compensated summation — see [`field_energy`]).
pub fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    compensated_sqrt(a.iter().zip(b.iter()).map(|(x, y)| (x - y) * (x - y)))
}

/// Compute ‖ψ‖₂ energy of a complex vector.
///
/// Uses Neumaier compensated summation. A plain `sum()` drifts by O(N·ε) per
/// evaluation, and that drift lands directly on Inv1's equality check; at
/// N = 1024 and long runs it eventually exceeds the 1e-10 tolerance even
/// though the underlying unitary evolution is fine. Compensation keeps the
/// error at O(ε) independent of N.
pub fn field_energy(psi: &[Complex64]) -> f64 {
    compensated_sqrt(psi.iter().map(num_complex::Complex::norm_sqr))
}

/// √(Σ terms) with Neumaier compensation.
fn compensated_sqrt<I: Iterator<Item = f64>>(terms: I) -> f64 {
    let mut sum = 0.0f64;
    let mut c = 0.0f64; // running compensation
    for x in terms {
        let t = sum + x;
        if sum.abs() >= x.abs() {
            c += (sum - t) + x;
        } else {
            c += (x - t) + sum;
        }
        sum = t;
    }
    (sum + c).sqrt()
}
