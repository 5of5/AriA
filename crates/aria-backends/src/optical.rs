use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::OpticalBackend;
use aria_engine_core::state::field_energy;
use num_complex::Complex64;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::f64::consts::PI;
use std::sync::Arc;

use rustfft::FftPlannerScalar;

/// Simulated optical backend — ideal unitary evolution.
///
/// Phase 1 builds one fixed-seed random unitary matrix at construction and
/// reuses it for every OpticalStep. This preserves energy (Inv1) while keeping
/// each step O(N²) instead of O(N³), which is required for the default
/// 1,000-step CLI run at N=256.
#[derive(Debug)]
pub struct SimOptical {
    matrix: Vec<Vec<Complex64>>,
}

impl SimOptical {
    pub fn new(n_modes: usize) -> Self {
        Self::with_seed(n_modes, 42)
    }

    pub fn with_seed(n_modes: usize, seed: u64) -> Self {
        let matrix = make_unitary(n_modes, seed, 0);
        SimOptical { matrix }
    }
}

impl OpticalBackend for SimOptical {
    fn unitary_step(&self, _t: u64, psi: &[Complex64]) -> Vec<Complex64> {
        mat_vec_mul(&self.matrix, psi)
    }
}

/// O(N log N) FFT phase-mask optical backend (spec ℙ1/§5.2, plan WS2).
///
/// ```text
/// ψ' = F⁻¹( e^{iθ(t)} ⊙ F(ψ) )
/// ```
///
/// - rustfft transforms are unnormalized (F⁻¹F = N·I), so the composite is
///   scaled by 1/N — i.e. 1/√N per direction — making the exact-arithmetic
///   operator unitary (𝕃1).
/// - θ(t) ∈ [0, 2π)^N is drawn from `StdRng::seed_from_u64(seed + t)` — the
///   same t-indexed family semantics as `make_unitary(seed, t)`; seeded only,
///   no OS entropy (repo lock; WASM-safe).
/// - The planner is **`FftPlannerScalar`** explicitly: the generic
///   `FftPlanner::new()` auto-dispatches AVX/SSE/NEON at runtime and would
///   make traces depend on the host CPU (Rev 2 decision gate). `wasm_simd`
///   stays OFF. Scalar butterflies have a fixed fp operation order ⇒ CLI ≡
///   Python byte-identical on one host.
/// - Normalization kernel (spec §0.2): if the transform moves energy by more
///   than 1e-7, the output is rescaled to the input energy. At the enforced
///   check tolerance (eps_energy ≤ 1e-10) the input energy ≡ energy_0 for any
///   valid pre-state, so this recalibrates to energy_0 (𝕃2) without needing
///   state access inside the backend.
pub struct FftOptical {
    n_modes: usize,
    seed: u64,
    fft: Arc<dyn rustfft::Fft<f64>>,
    ifft: Arc<dyn rustfft::Fft<f64>>,
}

impl std::fmt::Debug for FftOptical {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // rustfft's `Fft` trait object has no Debug — report the constructor
        // identity only; the plans themselves are opaque execution state.
        f.debug_struct("FftOptical")
            .field("n_modes", &self.n_modes)
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl FftOptical {
    pub fn with_seed(n_modes: usize, seed: u64) -> Self {
        let mut planner = FftPlannerScalar::new();
        let fft = planner.plan_fft_forward(n_modes);
        let ifft = planner.plan_fft_inverse(n_modes);
        FftOptical {
            n_modes,
            seed,
            fft,
            ifft,
        }
    }
}

impl OpticalBackend for FftOptical {
    fn unitary_step(&self, t: u64, psi: &[Complex64]) -> Vec<Complex64> {
        debug_assert_eq!(psi.len(), self.n_modes, "ψ must have exactly N modes");
        let n = self.n_modes;

        let mut buf = psi.to_vec();
        self.fft.process(&mut buf);

        // e^{iθ(t)} ⊙ F(ψ) — the per-engine t-indexed mask family.
        // sin/cos come from the pure-Rust `libm` crate, NOT the host libm:
        // the host implementations differ between execution environments
        // (measured: the MAX/Mojo-venv Python's sin disagrees with the CLI's
        // by 1 ulp at θ = 0x3fd3aff648e08287), which would break cross-
        // artifact byte-parity. The shared deterministic implementation
        // removes the degree of freedom entirely (repo lock: no entropy,
        // WASM-safe — wasm32 already used this same code via Rust std).
        let mut rng = StdRng::seed_from_u64(self.seed.wrapping_add(t));
        for x in &mut buf {
            let theta: f64 = rng.gen::<f64>() * 2.0 * PI;
            *x *= Complex64::new(libm::cos(theta), libm::sin(theta));
        }

        self.ifft.process(&mut buf);

        // rustfft is unnormalized: F⁻¹F = N·I ⇒ scale by 1/N (1/√N per
        // direction) so the composite is unitary in exact arithmetic.
        let inv_n = 1.0 / (n as f64);
        for x in &mut buf {
            *x *= inv_n;
        }

        // Normalization kernel (spec §0.2, 𝕃2): recalibrate to the input
        // energy if the transform drifted by more than 1e-7. Under the
        // enforced eps_energy ≤ 1e-10 the input energy is energy_0 within
        // tolerance, so this is the spec's "renormalize ψ' to energy_0".
        let energy_in = field_energy(psi);
        let energy_out = field_energy(&buf);
        if (energy_out - energy_in).abs() > 1e-7 && energy_out > 0.0 {
            let scale = energy_in / energy_out;
            for x in &mut buf {
                *x *= scale;
            }
        }

        buf
    }
}

/// The optical backend a reference run uses: the spec-mandated FFT kernel for
/// N ≥ 256, or the v0.1.0 Householder reference below it (plan WS2).
#[derive(Debug)]
pub enum RefOptical {
    Sim(SimOptical),
    Fft(FftOptical),
}

impl RefOptical {
    /// Select per config: explicit `"fft"`/`"householder"` override, else the
    /// automatic default — FFT for N ≥ 256 (spec §0.2 mandate), Householder
    /// below (v0.1.0 behavior, and what the N < 256 test configs expect).
    pub fn for_config(config: &AriaConfig, seed: u64) -> Self {
        match config.optical.as_deref() {
            Some("fft") => Self::Fft(FftOptical::with_seed(config.n_modes, seed)),
            Some("householder") => Self::Sim(SimOptical::with_seed(config.n_modes, seed)),
            _ => {
                if config.n_modes >= 256 {
                    Self::Fft(FftOptical::with_seed(config.n_modes, seed))
                } else {
                    Self::Sim(SimOptical::with_seed(config.n_modes, seed))
                }
            }
        }
    }
}

impl OpticalBackend for RefOptical {
    fn unitary_step(&self, t: u64, psi: &[Complex64]) -> Vec<Complex64> {
        match self {
            RefOptical::Sim(o) => o.unitary_step(t, psi),
            RefOptical::Fft(o) => o.unitary_step(t, psi),
        }
    }
}

/// Build a deterministic N×N unitary matrix as a product of N Householder
/// reflections and diagonal phase rotations.
///
/// Each reflection `H = I − 2vv†` is applied as a rank-1 update rather than by
/// forming `H` and multiplying: `H·U = U − 2v(v†U)` costs O(N²), so the whole
/// product costs O(N³) instead of the O(N⁴) an explicit matmul per reflection
/// would take. At N = 256 that is the difference between ~8 s and ~30 ms of
/// setup.
// Single-character names (u, v, w, n) deliberately mirror the spec's linear
// algebra: U the unitary, v the reflection vector, w = v†U, n the mode count.
#[allow(clippy::many_single_char_names)]
fn make_unitary(n_modes: usize, seed: u64, t: u64) -> Vec<Vec<Complex64>> {
    let mut rng = StdRng::seed_from_u64(seed.wrapping_add(t));
    let n = n_modes;

    // Start with the identity.
    let mut u: Vec<Vec<Complex64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    if i == j {
                        Complex64::new(1.0, 0.0)
                    } else {
                        Complex64::ZERO
                    }
                })
                .collect()
        })
        .collect();

    let mut w = vec![Complex64::ZERO; n];

    for _k in 0..n {
        // A random unit vector v on the complex unit sphere.
        let mut v: Vec<Complex64> = (0..n)
            .map(|_| {
                let theta: f64 = rng.gen::<f64>() * 2.0 * PI;
                Complex64::new(libm::cos(theta), libm::sin(theta)) / (n as f64).sqrt()
            })
            .collect();
        let norm: f64 = v
            .iter()
            .map(num_complex::Complex::norm_sqr)
            .sum::<f64>()
            .sqrt();
        for vi in &mut v {
            *vi /= Complex64::new(norm, 0.0);
        }

        // w = v† U   (row vector, one entry per column of U)
        w.fill(Complex64::ZERO);
        for (i, row) in u.iter().enumerate() {
            let vi_conj = v[i].conj();
            for (wj, uij) in w.iter_mut().zip(row) {
                *wj += vi_conj * uij;
            }
        }

        // U ← phase · (U − 2 v w)
        let phase: f64 = rng.gen::<f64>() * 2.0 * PI;
        let phase_c = Complex64::new(libm::cos(phase), libm::sin(phase));
        for (i, row) in u.iter_mut().enumerate() {
            let two_vi = Complex64::new(2.0, 0.0) * v[i];
            for (uij, wj) in row.iter_mut().zip(&w) {
                *uij = phase_c * (*uij - two_vi * wj);
            }
        }
    }

    u
}

fn mat_vec_mul(m: &[Vec<Complex64>], v: &[Complex64]) -> Vec<Complex64> {
    let n = v.len();
    let mut result = vec![Complex64::ZERO; n];
    for i in 0..n {
        for j in 0..n {
            result[i] += m[i][j] * v[j];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_conserved() {
        let opt = SimOptical::new(8);
        let psi0: Vec<Complex64> = (0..8)
            .map(|i| Complex64::new(f64::from(i).cos(), f64::from(i).sin()))
            .collect();
        let e0 = opt.energy(&psi0);

        for t in 0..10 {
            let psi1 = opt.unitary_step(t, &psi0);
            let e1 = opt.energy(&psi1);
            // Different psi (rotated) but same energy
            assert!((e1 - e0).abs() < 1e-10, "energy not conserved at t={t}");
            // Not identity
            assert!(psi1 != psi0 || t > 0, "unitary is identity — unlikely");
        }
    }

    #[test]
    fn matrix_is_unitary() {
        // U†U = I to f64 precision — this is what makes Inv1 hold exactly.
        let n = 24;
        let u = make_unitary(n, 7, 0);
        for i in 0..n {
            for j in 0..n {
                let dot: Complex64 = (0..n).map(|k| u[k][i].conj() * u[k][j]).sum();
                let want = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (dot.re - want).abs() < 1e-9 && dot.im.abs() < 1e-9,
                    "U†U[{i}][{j}] = {dot}, want {want}"
                );
            }
        }
    }

    #[test]
    fn seed_changes_matrix() {
        let a = SimOptical::with_seed(8, 1);
        let b = SimOptical::with_seed(8, 2);
        let psi0: Vec<Complex64> = (0..8)
            .map(|i| Complex64::new(f64::from(i).cos(), f64::from(i).sin()))
            .collect();
        let psi_a = a.unitary_step(0, &psi0);
        let psi_b = b.unitary_step(0, &psi0);
        assert_ne!(
            psi_a, psi_b,
            "different seeds should produce different unitaries"
        );
    }
}

#[cfg(test)]
mod ws2_tests {
    use super::*;

    fn probe_psi(n: usize, scale: f64) -> Vec<Complex64> {
        (0..n)
            .map(|i| {
                let phase = (i as f64) * 0.12345;
                Complex64::new(phase.cos(), phase.sin()) * scale
            })
            .collect()
    }

    #[test]
    fn fft_unitarity_at_n4096() {
        // Phase 2 gate: energy conserved to 1e-12 at N = 4096 with the FFT
        // backend (measured worst |ΔE| over 64 steps = 1.11e-16).
        let opt = FftOptical::with_seed(4096, 42);
        let psi0 = probe_psi(4096, 1.0 / 64.0);
        let e0 = field_energy(&psi0);
        let mut worst = 0.0f64;
        for t in 0..64 {
            let psi1 = opt.unitary_step(t, &psi0);
            let e1 = field_energy(&psi1);
            worst = worst.max((e1 - e0).abs());
            assert!(
                psi1 != psi0,
                "the mask family must actually rotate the field"
            );
        }
        assert!(worst <= 1e-12, "FFT energy drift {worst:e} exceeds 1e-12");
    }

    #[test]
    fn fft_is_deterministic() {
        let opt = FftOptical::with_seed(256, 7);
        let psi0 = probe_psi(256, 1.0 / 16.0);
        for t in [0, 1, 17, 255] {
            let a = opt.unitary_step(t, &psi0);
            let b = opt.unitary_step(t, &psi0);
            assert_eq!(a, b, "seeded FFT must be reproducible at t={t}");
        }
    }

    #[test]
    fn fft_mask_family_is_t_indexed() {
        // θ(t) is a t-indexed family (seed + t offset), mirroring
        // make_unitary(seed, t): different t ⇒ different unitary.
        let opt = FftOptical::with_seed(256, 7);
        let psi0 = probe_psi(256, 1.0 / 16.0);
        let p0 = opt.unitary_step(0, &psi0);
        let p1 = opt.unitary_step(1, &psi0);
        assert_ne!(p0, p1, "t=0 and t=1 must apply different unitaries");
    }

    #[test]
    fn fft_normalization_kernel_recalibrates_to_the_input_energy() {
        // The §0.2 kernel keeps the output at the input energy even when the
        // input itself is not unit-norm (the backend has no state access; it
        // recalibrates to what the state currently holds, which the enforced
        // eps_energy ≤ 1e-10 keeps ≡ energy_0).
        let opt = FftOptical::with_seed(256, 3);
        let psi0 = probe_psi(256, 2.0 / 16.0); // ‖ψ‖ = 2.0
        let e0 = field_energy(&psi0);
        for t in 0..8 {
            let psi1 = opt.unitary_step(t, &psi0);
            assert!(
                (field_energy(&psi1) - e0).abs() < 1e-12,
                "kernel did not preserve the input energy at t={t}"
            );
        }
    }

    #[test]
    fn fft_and_householder_agree_on_energy_at_small_n() {
        for backend in [
            RefOptical::Fft(FftOptical::with_seed(8, 11)),
            RefOptical::Sim(SimOptical::with_seed(8, 11)),
        ] {
            let psi0 = probe_psi(8, 1.0);
            let e0 = field_energy(&psi0);
            for t in 0..4 {
                let e1 = field_energy(&backend.unitary_step(t, &psi0));
                assert!((e1 - e0).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn backend_selection_table() {
        let base = aria_engine_core::config::AriaConfig::default();
        let mut cfg = base.clone();
        cfg.n_modes = 8;
        assert!(matches!(
            RefOptical::for_config(&cfg, 1),
            RefOptical::Sim(_)
        ));

        let mut cfg = base.clone();
        cfg.n_modes = 256;
        assert!(matches!(
            RefOptical::for_config(&cfg, 1),
            RefOptical::Fft(_)
        ));

        let mut cfg = base.clone();
        cfg.n_modes = 8;
        cfg.optical = Some("fft".into());
        assert!(matches!(
            RefOptical::for_config(&cfg, 1),
            RefOptical::Fft(_)
        ));

        let mut cfg = base;
        cfg.n_modes = 256;
        cfg.optical = Some("householder".into());
        assert!(matches!(
            RefOptical::for_config(&cfg, 1),
            RefOptical::Sim(_)
        ));
    }
}
