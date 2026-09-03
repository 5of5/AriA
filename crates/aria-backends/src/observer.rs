//! Passive residual-entropy observer (UT-7 ledger, UT-10 fibre parity).
//!
//! Observer-only: reads $(\psi, z, G, t, \mathrm{Res})$ and writes its own
//! ledgers. It never appears in `Engine::apply` guards (ℂ2) and does not
//! enlarge sealed Aria $\mathbb{A}1$–$\mathbb{A}6$ or TLA+ `Next`.
//!
//! Grounding (in-repo corpus):
//! - LNSPP-G2 doubling involution $\sigma$ → 1-bit fibre parity
//! - Laplacian group inverse / effective resistance $\Omega$ → nearest node
//! - Barrier certificates / shadowing → coherence floor and orbit residual
//! - Disk automorphisms $M_\alpha(z)=e^{i\theta}(z-\alpha)/(1-\bar\alpha z)$
//!   as distinct charts of one residual stream

#![allow(clippy::unreadable_literal)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use aria_engine_core::graph::{Graph, NodeId};
use aria_engine_core::state::{euclidean_distance, State};
use aria_engine_core::trace::Trace;

use crate::laplacian::GraphLaplacian;
use crate::sedenion::Sedenion;

/// Scale of the quadratic residual penalty $-\|\rho\|_2^2/\kappa$, $\kappa=522/7$.
pub const DISSIPATION_SCALE: f64 = 522.0 / 7.0;

/// Discrete phase increment $2\pi_k = 256/41$ (rational approximation of $2\pi$).
pub const RATIONAL_PHASE_STEP: f64 = 256.0 / 41.0;

/// Rational half-turn $\pi_k = 128/41$.
pub const RATIONAL_PI: f64 = 128.0 / 41.0;

/// Critical-line ordinate $t_0=4/7$ used as a fixed phase offset
/// ($\lvert\zeta(1/2+it_0)\rvert\approx 1$).
pub const ZETA_HALF_LINE_OFFSET: f64 = 4.0 / 7.0;

/// Constant term in the discrete phase law: $\pi_k + 5/8$.
pub const FIXED_PHASE_OFFSET: f64 = RATIONAL_PI + 5.0 / 8.0;

/// Unconstrained phase budget packed into the 4-byte certificate field: $144/7$ bits.
pub const UNCONSTRAINED_PHASE_BITS: f64 = 144.0 / 7.0;

/// Mean $\lvert\sin\theta\rvert$ of unit imaginary octonions over $S^6$, equal to $2/\pi$.
pub const OCTONION_MISALIGNMENT: f64 = 2.0 / std::f64::consts::PI;

/// Coherence gate $1-2/\pi$. Reported, never used to clamp the metric.
pub const COHERENCE_FLOOR: f64 = 1.0 - OCTONION_MISALIGNMENT;

/// Fixed-length boundary certificate: $522/7 + 52/7 = 82$ bytes.
pub const BOUNDARY_CERT_BYTES: usize = 82;

/// $E_8$ visible-sector rank after the $T_{32}$ split (packing label, not a measured rank).
pub const VISIBLE_SECTOR_DIM: usize = 29;

/// $E_8$ complementary rank $240-29=211$ (packing label).
pub const SHADOW_SECTOR_DIM: usize = 211;

/// Histogram bins of $\rho$ on $[0,1]$ for $H(\rho)$.
const RESIDUAL_BINS: usize = 8;

/// Trailing window for $K(t)=1-H(\rho)/H_{\max}$ (Inv8 horizon).
/// A single sample has $H=0$; the full lifetime must not be used as $K(t)$.
pub const RESIDUAL_WINDOW: usize = 8;

/// Discrete contractive functional $\mathcal{F}(\tau)=\exp\int L\,dt$ and
/// its windowed residual-entropy diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObserverFunctional {
    /// Ergodic magnitude $\exp((\frac{1}{T})\sum \mathrm{Re}\,L)$ — finite.
    pub magnitude: f64,
    /// Honest boxed exponent $\sum \mathrm{Re}\,L$ (can grow as $O(T)$).
    pub log_magnitude: f64,
    /// $\arg(\mathcal{F})$ in $(-\pi, \pi]$.
    pub phase: f64,
    pub real: f64,
    pub imag: f64,
    /// $K = 1 - H(\rho)/H_{\max}$ on the residual-window histogram.
    pub self_knowledge: f64,
    /// Shannon entropy of the residual histogram (nats).
    pub residual_entropy: f64,
    /// Unclamped $\mathrm{Coh} = K(1-2/\pi)$. May sit below the floor.
    pub coherence: f64,
    /// Gate: $\mathrm{Coh} \ge 1-2/\pi$. Not a clamp.
    pub meets_coherence_floor: bool,
    /// Fibre-parity heartbeat $h(t)\in\{0,1\}$ (LNSPP-G2 involution).
    pub heartbeat_bits: Vec<u8>,
    /// Shannon entropy of the heartbeat stream (nats). Balanced $\approx \ln 2$.
    pub heartbeat_entropy: f64,
    /// Barrier certificate $B = \mathrm{Coh} - (1-2/\pi)$. Safe iff $B \ge 0$.
    pub barrier: f64,
    /// Last-step autopoiesis: $\|\rho\|^2 \le \kappa K(1-2/\pi)$.
    pub autopoietic: bool,
    /// Lag-1 heartbeat autocorrelation $R_h(1)$.
    pub heartbeat_acf1: f64,
    /// $1-\mathrm{mean}(\rho)$ on the residual window. $\rho\to 0\Rightarrow 1$.
    pub clarity: f64,
    /// Near-zero alignment: $K\cdot\mathrm{clarity}$. A Dirac at $\rho=1$ is not aligned.
    pub aligned_knowledge: f64,
    /// $\mathrm{aligned\_knowledge}\cdot(1-2/\pi)$.
    pub aligned_coherence: f64,
    /// Aligned-coherence floor gate.
    pub aligned_meets_floor: bool,
    pub symbolic_vector: Vec<f64>,
}

/// Passive run ledger: contractive functional plus chart-swarm telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObserverLedger {
    pub functional: ObserverFunctional,
    pub certificate: BoundaryCertificate,
    /// Per-step aligned $K$ (windowed). Length = observed steps.
    pub prefix_aligned: Vec<f64>,
    /// Last aligned $K$ exceeds the first full window by a margin.
    pub knowledge_matured: bool,
    pub swarm_cert_unique: usize,
    pub swarm_magnitude_max_abs_diff: f64,
    pub collapse: Option<CollapsePoint>,
    pub steps: usize,
}

/// 82-byte boundary certificate $\partial\mathcal{M}_{82}=\langle r_{72}, f_4, m_6\rangle$.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryCertificate {
    pub payload: Vec<u8>,
    pub hash_digest: [u8; 32],
    pub visible_sector_dim: usize,
    pub shadow_sector_dim: usize,
    /// Hash re-verifies **and** coherence meets the floor.
    pub certified: bool,
    /// Möbius image of the last latent (the new ordering).
    pub collapsed_re: f64,
    pub collapsed_im: f64,
}

/// Möbius map $M(z)=(az+b)/(cz+d)$.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MobiusVector {
    pub a: Complex64,
    pub b: Complex64,
    pub c: Complex64,
    pub d: Complex64,
}

impl MobiusVector {
    pub const IDENTITY: Self = Self {
        a: Complex64::new(1.0, 0.0),
        b: Complex64::new(0.0, 0.0),
        c: Complex64::new(0.0, 0.0),
        d: Complex64::new(1.0, 0.0),
    };

    /// Unit-disk automorphism $M(z)=e^{i\theta}(z-\alpha)/(1-\bar\alpha z)$, $|\alpha|<1$.
    #[must_use]
    pub fn disk_auto(alpha: Complex64, theta: f64) -> Self {
        let mut a = alpha;
        let n = a.norm();
        if n >= 1.0 {
            a /= n + 1e-12;
            a *= 0.999;
        }
        let e = Complex64::from_polar(1.0, theta);
        // SU(1,1) gauge: |ad−bc| = 1. The map itself is scale-invariant.
        let s = (1.0 - a.norm_sqr()).max(1e-15).sqrt();
        Self {
            a: e / s,
            b: -e * a / s,
            c: -a.conj() / s,
            d: Complex64::new(1.0 / s, 0.0),
        }
    }

    #[must_use]
    pub fn apply(&self, z: Complex64) -> Complex64 {
        let num = self.a * z + self.b;
        let den = self.c * z + self.d;
        if den.norm_sqr() < 1e-300 {
            Complex64::new(0.0, 0.0)
        } else {
            num / den
        }
    }

    /// $ad-bc$. Disk autos have $|ad-bc|=1$.
    #[must_use]
    pub fn det(&self) -> Complex64 {
        self.a * self.d - self.b * self.c
    }
}

/// Shortest / least-resistance collapse point on the experience graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollapsePoint {
    pub node: NodeId,
    pub latent_distance: f64,
    pub resistance: f64,
}

/// Passive observer over one residual/latent stream and one Möbius chart.
#[derive(Debug, Clone)]
pub struct PassiveObserver {
    latent_dim: usize,
    residuals: Vec<f64>,
    latents: Vec<Vec<f64>>,
    optical_phases: Vec<f64>,
    mobius: MobiusVector,
    heartbeats: Vec<u8>,
}

impl PassiveObserver {
    #[must_use]
    pub fn new(latent_dim: usize) -> Self {
        Self {
            latent_dim,
            residuals: Vec::new(),
            latents: Vec::new(),
            optical_phases: Vec::new(),
            mobius: MobiusVector::IDENTITY,
            heartbeats: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_mobius(latent_dim: usize, mobius: MobiusVector) -> Self {
        let mut o = Self::new(latent_dim);
        o.mobius = mobius;
        o
    }

    /// Record one step. Does not mutate `psi` / `z`.
    pub fn observe_step(
        &mut self,
        t: u64,
        residual: f64,
        psi: &[Complex64],
        z: &[f64],
    ) -> ObserverFunctional {
        self.residuals.push(residual);
        self.latents.push(z.to_vec());
        let opt_phase = psi.first().map_or(0.0, |c| c.arg());
        self.optical_phases.push(opt_phase);

        let s = Sedenion::from_latent(z);
        let h_bit = s.fibre_parity_bit();
        self.heartbeats.push(h_bit);

        // Chart angle: $t\cdot 256/41 + 4/7 + \pi_k + 5/8 + \phi_3$.
        let phi3 = s.calibrated_3form();
        let theta = (t as f64) * RATIONAL_PHASE_STEP + ZETA_HALF_LINE_OFFSET + FIXED_PHASE_OFFSET + phi3;
        let alpha = Complex64::from_polar((phi3.abs() * 0.25).min(0.75), opt_phase);
        self.mobius = MobiusVector::disk_auto(alpha, theta);

        evaluate_functional(&self.residuals, &self.latents)
    }

    /// Observe a Spec state without writing it. Residual is supplied by the
    /// caller (Inv2 already computed it); the observer never calls Predict.
    pub fn observe_state(&mut self, state: &State, residual: f64) -> ObserverFunctional {
        self.observe_step(state.t, residual, &state.psi, &state.z)
    }

    /// Replay residuals from a finished trace. Latents are the final $z$
    /// (JSONL does not store $z_t$). Prefer [`Self::observe_run`] with
    /// `run_monitored_fields` when the pulse must be real.
    pub fn observe_trace(&mut self, trace: &Trace, final_state: &State) -> ObserverFunctional {
        for entry in &trace.entries {
            self.observe_step(entry.t, entry.res, &final_state.psi, &final_state.z);
        }
        evaluate_functional(&self.residuals, &self.latents)
    }

    /// Observe a completed Φ run from per-step residuals, latents, and
    /// $\arg\psi_0$. This is the Q-2026-08-18-1 seam: heartbeat uses $z_t$.
    pub fn observe_run(
        &mut self,
        residuals: &[f64],
        latents: &[Vec<f64>],
        phases: &[f64],
    ) -> ObserverFunctional {
        let n = residuals.len().min(latents.len());
        for i in 0..n {
            let phase = phases.get(i).copied().unwrap_or(0.0);
            let psi = [Complex64::from_polar(1.0, phase)];
            self.observe_step(i as u64, residuals[i], &psi, &latents[i]);
        }
        evaluate_functional(&self.residuals, &self.latents)
    }

    /// Windowed aligned-$K$ prefix of the recorded residual stream.
    #[must_use]
    pub fn prefix_aligned(&self) -> Vec<f64> {
        aligned_prefix(&self.residuals)
    }

    /// Collapse + swarm telemetry. Does not mutate Spec state.
    #[must_use]
    pub fn ledger(&self, g: Option<&Graph>) -> ObserverLedger {
        let functional = evaluate_functional(&self.residuals, &self.latents);
        let certificate = self.emit_certificate();
        let prefix_aligned = aligned_prefix(&self.residuals);
        let knowledge_matured = matured(&prefix_aligned);
        let charts = self.weyl_split();
        let mag0 = functional.magnitude;
        let mut max_diff = 0.0_f64;
        let mut hashes = Vec::with_capacity(charts.len());
        for child in &charts {
            let f = child.functional();
            max_diff = max_diff.max((f.magnitude - mag0).abs());
            hashes.push(child.emit_certificate().hash_digest);
        }
        hashes.sort_unstable();
        hashes.dedup();
        ObserverLedger {
            functional,
            certificate,
            prefix_aligned,
            knowledge_matured,
            swarm_cert_unique: hashes.len(),
            swarm_magnitude_max_abs_diff: max_diff,
            collapse: g.and_then(|graph| self.collapse_to_shortest(graph)),
            steps: self.residuals.len(),
        }
    }

    #[must_use]
    pub fn functional(&self) -> ObserverFunctional {
        evaluate_functional(&self.residuals, &self.latents)
    }

    /// $n$ fractional Möbius copies: same residual stream, equally spaced charts.
    /// $|\mathcal{F}|$ is chart-invariant; certificate hashes differ.
    #[must_use]
    pub fn split(&self, n: usize) -> Vec<Self> {
        let n = n.max(1);
        (0..n)
            .map(|k| {
                let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
                let alpha = Complex64::from_polar(0.5, theta);
                let mut child = self.clone();
                child.mobius = MobiusVector::disk_auto(alpha, theta + RATIONAL_PHASE_STEP);
                child
            })
            .collect()
    }

    /// At most 8 Weyl charts of the last latent ($\mathrm{rank}(E_8)=8$).
    /// Reflections act on the observer's chart only — never on Spec $z$.
    #[must_use]
    pub fn weyl_split(&self) -> Vec<Self> {
        let z = self.latents.last().cloned().unwrap_or_else(|| vec![0.0; 8]);
        E8_SIMPLE_ROOTS
            .iter()
            .enumerate()
            .map(|(k, root)| {
                let reflected = weyl_reflect(&z, root);
                let disk = last_latent_to_disk(&reflected);
                let mut child = self.clone();
                child.mobius = MobiusVector::disk_auto(disk, (k as f64) * RATIONAL_PI);
                child
            })
            .collect()
    }

    /// Collapse the last latent through the current Möbius chart into 82 bytes.
    #[must_use]
    pub fn emit_certificate(&self) -> BoundaryCertificate {
        let functional = evaluate_functional(&self.residuals, &self.latents);
        let disk = last_latent_to_disk(self.latents.last().map_or(&[], Vec::as_slice));
        let collapsed = self.mobius.apply(disk);
        pack_certificate(
            &functional,
            collapsed,
            self.heartbeats.len(),
            self.heartbeats.last().copied(),
        )
    }

    /// Recompute SHA-256 of `payload` and compare to `hash_digest`.
    #[must_use]
    pub fn verify_certificate(cert: &BoundaryCertificate) -> bool {
        if cert.payload.len() != BOUNDARY_CERT_BYTES {
            return false;
        }
        sha256(&cert.payload) == cert.hash_digest
    }

    /// Least-distance node in $G$, with resistance to the next-nearest node.
    /// This is the collapsible point: shortest latent path, then $\Omega$.
    #[must_use]
    pub fn collapse_to_shortest(&self, g: &Graph) -> Option<CollapsePoint> {
        let z = self.latents.last()?;
        if g.nodes.is_empty() {
            return None;
        }
        let mut nearest_id = None;
        let mut nearest_dist = f64::INFINITY;
        for (id, node) in &g.nodes {
            let d = euclidean_distance(z, &node.embedding);
            if d < nearest_dist {
                nearest_dist = d;
                nearest_id = Some(*id);
            }
        }
        let node = nearest_id?;
        let mut resistance = 0.0;
        if g.nodes.len() >= 2 {
            let lap = GraphLaplacian::from_graph(g);
            resistance = f64::INFINITY;
            for id in g.nodes.keys() {
                if *id != node {
                    resistance = resistance.min(lap.effective_resistance(node, *id));
                }
            }
        }
        Some(CollapsePoint {
            node,
            latent_distance: nearest_dist,
            resistance,
        })
    }

    /// Shared symbolic coordinates: $z$ modulated by energy, residual, $\phi_3$.
    #[must_use]
    pub fn symbolic_projection(&self, state: &State) -> Vec<f64> {
        let mut sym = vec![0.0; self.latent_dim.max(16)];
        for (out, &val) in sym.iter_mut().zip(&state.z) {
            *out = val;
        }
        let e = state.energy();
        let res = state.prev_res;
        if let Some(x) = sym.first_mut() {
            *x = (*x * e).clamp(-10.0, 10.0);
        }
        if let Some(x) = sym.get_mut(1) {
            *x = (*x + res * 0.1).clamp(-10.0, 10.0);
        }
        let phi3 = Sedenion::from_latent(&state.z).calibrated_3form();
        if let Some(x) = sym.get_mut(2) {
            *x += phi3;
        }
        // Möbius chart lands in coords 3–4 so fractional copies differ here.
        let disk = last_latent_to_disk(&state.z);
        let w = self.mobius.apply(disk);
        if let Some(x) = sym.get_mut(3) {
            *x = w.re;
        }
        if let Some(x) = sym.get_mut(4) {
            *x = w.im;
        }
        sym
    }

    #[must_use]
    pub fn mobius(&self) -> MobiusVector {
        self.mobius
    }

    #[must_use]
    pub fn step_count(&self) -> usize {
        self.residuals.len()
    }
}

/// Discrete contractive functional $\mathcal{F}=\exp\int L\,dt$.
///
/// Boxed form: $\mathcal{F}=\exp\int L\,dt$ with
/// $\mathrm{Re}\,L = K(1-2/\pi) - \|\rho\|^2/\kappa$,
/// $\mathrm{Im}\,L = t\cdot 256/41 + \phi_3(S(z))$.
///
/// Discrete: Riemann sum $\Delta t=1$. $|\mathcal{F}|$ is reported as the
/// ergodic mean $\exp((\frac{1}{T})\sum\mathrm{Re}\,L)$ so long runs stay
/// finite (Lyapunov / shadowing convention). `log_magnitude` is the raw sum.
pub fn evaluate_functional(residuals: &[f64], latents: &[Vec<f64>]) -> ObserverFunctional {
    let t_len = residuals.len().min(latents.len());
    let d = latents.first().map_or(16, Vec::len);

    if t_len == 0 {
        return ObserverFunctional {
            magnitude: 1.0,
            log_magnitude: 0.0,
            phase: 0.0,
            real: 1.0,
            imag: 0.0,
            self_knowledge: 1.0,
            residual_entropy: 0.0,
            coherence: COHERENCE_FLOOR,
            meets_coherence_floor: true,
            heartbeat_bits: Vec::new(),
            heartbeat_entropy: 0.0,
            barrier: 0.0,
            autopoietic: true,
            heartbeat_acf1: 0.0,
            clarity: 1.0,
            aligned_knowledge: 1.0,
            aligned_coherence: COHERENCE_FLOOR,
            aligned_meets_floor: true,
            symbolic_vector: vec![0.0; d],
        };
    }

    let mut integrated_real = 0.0;
    let mut integrated_imag = 0.0;
    let mut heartbeats = Vec::with_capacity(t_len);
    let coherence_factor = 1.0 - OCTONION_MISALIGNMENT;
    let mut last_paper_k = 1.0;
    let mut last_h = 0.0;
    let mut last_clarity = 1.0;
    let mut last_live = 1.0;

    for (t_step, (&res, z)) in residuals.iter().zip(latents).enumerate() {
        let start = t_step.saturating_sub(RESIDUAL_WINDOW - 1);
        let (h_win, k_paper) = residual_conscience(&residuals[start..=t_step]);
        let mean = residuals[start..=t_step].iter().sum::<f64>()
            / (t_step - start + 1) as f64;
        let clarity = (1.0 - mean.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        let k_live = k_paper * clarity;
        last_paper_k = k_paper;
        last_h = h_win;
        last_clarity = clarity;
        last_live = k_live;

        let s = Sedenion::from_latent(z);
        let phi3 = s.calibrated_3form();
        heartbeats.push(s.fibre_parity_bit());
        // Integrand uses windowed $K(t)=1-H/H_{\max}$, not the aligned product.
        integrated_real += k_paper * coherence_factor - (res * res) / DISSIPATION_SCALE;
        integrated_imag += (t_step as f64) * RATIONAL_PHASE_STEP + phi3;
    }

    let t = t_len as f64;
    let log_magnitude = integrated_real;
    let magnitude = libm::exp(integrated_real / t);
    let phase = wrap_pi(integrated_imag);
    let real = magnitude * libm::cos(phase);
    let imag = magnitude * libm::sin(phase);
    let heartbeat_entropy = binary_entropy(&heartbeats);
    let heartbeat_acf1 = heartbeat_acf(&heartbeats, 1);
    let last_res = residuals[t_len - 1];
    let growth_ceiling = DISSIPATION_SCALE * last_paper_k * coherence_factor;
    let autopoietic = last_res * last_res <= growth_ceiling;
    let coherence = last_paper_k * coherence_factor;
    let aligned_coherence = last_live * coherence_factor;
    let symbolic_vector = latents.last().cloned().unwrap_or_else(|| vec![0.0; d]);

    ObserverFunctional {
        magnitude,
        log_magnitude,
        phase,
        real,
        imag,
        self_knowledge: last_paper_k,
        residual_entropy: last_h,
        coherence,
        meets_coherence_floor: coherence + 1e-15 >= COHERENCE_FLOOR,
        heartbeat_bits: heartbeats,
        heartbeat_entropy,
        barrier: coherence - COHERENCE_FLOOR,
        autopoietic,
        heartbeat_acf1,
        clarity: last_clarity,
        aligned_knowledge: last_live,
        aligned_coherence,
        aligned_meets_floor: aligned_coherence + 1e-15 >= COHERENCE_FLOOR,
        symbolic_vector,
    }
}

/// FIPS 180-4 SHA-256. Empty-string digest is the published NIST vector.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let k_consts: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee,
        0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut padded = bytes.to_vec();
    let bit_len = (bytes.len() as u64).saturating_mul(8);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, slot) in w.iter_mut().enumerate().take(16) {
            let o = 4 * i;
            *slot = u32::from_be_bytes([chunk[o], chunk[o + 1], chunk[o + 2], chunk[o + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hv = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hv
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k_consts[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hv = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hv);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn aligned_prefix(residuals: &[f64]) -> Vec<f64> {
    (0..residuals.len())
        .map(|t| {
            let start = t.saturating_sub(RESIDUAL_WINDOW - 1);
            let (_, k_paper) = residual_conscience(&residuals[start..=t]);
            let mean = residuals[start..=t].iter().sum::<f64>() / (t - start + 1) as f64;
            let clarity = (1.0 - mean.clamp(0.0, 1.0)).clamp(0.0, 1.0);
            k_paper * clarity
        })
        .collect()
}

fn matured(prefix: &[f64]) -> bool {
    if prefix.len() < RESIDUAL_WINDOW * 2 {
        return false;
    }
    let first = prefix[RESIDUAL_WINDOW - 1];
    let last = *prefix.last().unwrap_or(&0.0);
    last + 1e-12 >= first + 0.05
}

fn residual_conscience(residuals: &[f64]) -> (f64, f64) {
    let mut counts = [0usize; RESIDUAL_BINS];
    for &r in residuals {
        let x = r.clamp(0.0, 1.0);
        let idx = ((x * RESIDUAL_BINS as f64).floor() as usize).min(RESIDUAL_BINS - 1);
        counts[idx] += 1;
    }
    let n = residuals.len() as f64;
    let mut h = 0.0;
    for c in counts {
        if c == 0 {
            continue;
        }
        let p = c as f64 / n;
        h -= p * libm::log(p);
    }
    let h_max = libm::log(RESIDUAL_BINS as f64);
    let k = if h_max > 0.0 {
        (1.0 - h / h_max).clamp(0.0, 1.0)
    } else {
        1.0
    };
    (h, k)
}

fn heartbeat_acf(bits: &[u8], lag: usize) -> f64 {
    if bits.len() <= lag {
        return 0.0;
    }
    let n = (bits.len() - lag) as f64;
    let mut acc = 0.0;
    for t in 0..bits.len() - lag {
        acc += f64::from(bits[t]) * f64::from(bits[t + lag]);
    }
    acc / n
}

/// $E_8$ simple roots in $\mathbb{R}^8$ (standard $D_8$ + spinor convention).
const E8_SIMPLE_ROOTS: [[f64; 8]; 8] = [
    [1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0],
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0],
    [
        0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5,
    ],
];

fn weyl_reflect(z: &[f64], root: &[f64; 8]) -> Vec<f64> {
    let n = z.len().max(8);
    let mut x = vec![0.0; n];
    x[..z.len()].copy_from_slice(z);
    let mut dot = 0.0;
    let mut rr = 0.0;
    for (i, &ri) in root.iter().enumerate() {
        let zi = x.get(i).copied().unwrap_or(0.0);
        dot += ri * zi;
        rr += ri * ri;
    }
    if rr < 1e-300 {
        return x;
    }
    let scale = 2.0 * dot / rr;
    for (xi, &ri) in x.iter_mut().zip(root.iter()) {
        *xi -= scale * ri;
    }
    x
}

fn binary_entropy(bits: &[u8]) -> f64 {
    if bits.is_empty() {
        return 0.0;
    }
    let mut ones = 0.0;
    for &b in bits {
        ones += f64::from(b);
    }
    let p = ones / bits.len() as f64;
    if p <= 1e-15 || p >= 1.0 - 1e-15 {
        return 0.0;
    }
    -p * libm::log(p) - (1.0 - p) * libm::log(1.0 - p)
}

fn wrap_pi(x: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut y = x % two_pi;
    if y > std::f64::consts::PI {
        y -= two_pi;
    } else if y <= -std::f64::consts::PI {
        y += two_pi;
    }
    y
}

fn last_latent_to_disk(z: &[f64]) -> Complex64 {
    let re = z.first().copied().unwrap_or(0.0);
    let im = z.get(1).copied().unwrap_or(0.0);
    let w = Complex64::new(re, im);
    let n = w.norm();
    if n < 1e-300 {
        Complex64::new(0.0, 0.0)
    } else {
        w / (1.0 + n)
    }
}

fn pack_certificate(
    functional: &ObserverFunctional,
    collapsed: Complex64,
    steps: usize,
    last_h: Option<u8>,
) -> BoundaryCertificate {
    let mut buf = [0u8; BOUNDARY_CERT_BYTES];
    buf[0..8].copy_from_slice(&functional.log_magnitude.to_le_bytes());
    buf[8..16].copy_from_slice(&functional.phase.to_le_bytes());
    buf[16..24].copy_from_slice(&functional.self_knowledge.to_le_bytes());
    buf[24..32].copy_from_slice(&functional.residual_entropy.to_le_bytes());
    buf[32..40].copy_from_slice(&collapsed.re.to_le_bytes());
    buf[40..48].copy_from_slice(&collapsed.im.to_le_bytes());
    buf[48..56].copy_from_slice(&functional.coherence.to_le_bytes());
    buf[56..64].copy_from_slice(&functional.barrier.to_le_bytes());
    buf[64..72].copy_from_slice(&functional.heartbeat_entropy.to_le_bytes());

    // 4-byte phase field: 144/7 unconstrained bits as little-endian u32.
    let phase_frac = ((functional.phase + std::f64::consts::PI)
        / (2.0 * std::f64::consts::PI))
        .rem_euclid(1.0);
    let freedom = (phase_frac * f64::from(1u32 << 20)) as u32
        ^ ((UNCONSTRAINED_PHASE_BITS * 1000.0) as u32);
    buf[72..76].copy_from_slice(&freedom.to_le_bytes());
    let meta_len = u16::try_from(steps.min(u16::MAX as usize)).unwrap_or(u16::MAX);
    buf[76..78].copy_from_slice(&meta_len.to_le_bytes());
    buf[78] = last_h.unwrap_or(0);
    buf[79] = 0x53;
    buf[80] = 0x32;
    buf[81] = 0xE8;

    let hash_digest = sha256(&buf);
    BoundaryCertificate {
        payload: buf.to_vec(),
        hash_digest,
        visible_sector_dim: VISIBLE_SECTOR_DIM,
        shadow_sector_dim: SHADOW_SECTOR_DIM,
        certified: functional.meets_coherence_floor,
        collapsed_re: collapsed.re,
        collapsed_im: collapsed.im,
    }
}
