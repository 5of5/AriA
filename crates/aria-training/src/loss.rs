//! The 4-term hybrid objective ℒ_total on Δ³ (ℙ6) with exact analytic
//! gradients — hand-rolled f64, no autograd dependency.
//!
//! ```text
//! ℒ_total = λ_JEPA·ℒ_JEPA + λ_NLL·ℒ_NLL + λ_Spectral·ℒ_Spectral + λ_Graph·ℒ_Graph
//! ```
//!
//! Gradient map (θ = (W_I, W_P), θ_D = readout weight; 𝔸5: ∂Φ/∂y = 0):
//!
//! * **ℒ_JEPA** `= (1/M) Σ_t ‖W_P·(W_I x_t) − sg(W_I x_{t+1})‖²` with
//!   `r_t = W_P z_t − z̄_{t+1}`:
//!   `∂/∂W_P = (2/M) Σ r_t z_tᵀ`, `∂/∂W_I = (2/M) Σ W_Pᵀ r_t x_tᵀ` — the
//!   stop-gradient means the target branch contributes **nothing** to ∂/∂W_I.
//! * **ℒ_Graph** `= (1/E) Σ_(s,e) max(0, ‖ζ_s − ζ_e‖ − γ)²` over concept mean
//!   latents `ζ_c = mean_{w ∋ c} W_I x_w`: active edges push
//!   `∂/∂ζ_s = (2h/E)·(ζ_s − ζ_e)/‖ζ_s − ζ_e‖`, chained to W_I through each
//!   member window. Trains W_I only.
//! * **ℒ_Spectral** `= Σ max(0, σ_max(W) − bound)²` with subgradient
//!   `∂σ_max/∂W = u₁v₁ᵀ` from `power_iteration_with_vectors` (same seeded
//!   estimator the runtime loader audits with).
//! * **ℒ_NLL** `= −(1/n) Σ ln p_D(y | sg(z))` through the DiscreteReadout
//!   architecture (LN → linear → temperature softmax): gradients reach the
//!   readout weight **only** — never W_I, never W_P (𝔸5, 𝕃5).
//!
//! The per-term values reported in [`LossBreakdown`] are **unweighted**;
//! `total` is the λ-weighted sum. Iteration orders are index- or BTree-driven
//! throughout — no hash-map order ever touches a result.

use aria_engine_backends::spectral::{power_iteration_with_vectors, Matrix, DEFAULT_ITERATIONS};
use aria_engine_core::config::LossLambdas;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::dataset::{EdgeAlignment, PreparedDataset};
use crate::linalg::{
    add_outer, dot, mat_t_vec_into, matvec, matvec_into, norm2_sq, sub_into, zeros,
};
use crate::TrainingError;

/// Whether [`compute_batch_with`] spends the two seeded power-iterations on
/// ℒ_Spectral. After Π_𝒮 the hinge is identically 0 — the training loop
/// passes [`SpectralEval::CertifiedInBall`] so those 32 sweeps are not
/// burned on a closed constraint. Tests that inject out-of-ball weights
/// must use [`SpectralEval::Audit`] (the [`compute_batch`] default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectralEval {
    /// Run the seeded estimator at `DEFAULT_ITERATIONS` and apply the hinge
    /// subgradient `∂σ_max/∂W = u₁v₁ᵀ` when σ̂ exceeds the ball.
    Audit,
    /// Weights already satisfy σ̂(W_I) ≤ 1 and σ̂(W_P) ≤ bound (the Π_𝒮
    /// post-condition). ℒ_Spectral is 0 and contributes no gradient.
    CertifiedInBall,
}

/// Layer-norm floor — parity constant with `aria-backends::readout::LN_EPS`
/// (asserted by the cross-check test against `DiscreteReadout::probs`).
const LN_EPS: f64 = 1e-5;

/// The Φ-side trainable parameters (Phase 1, training PRD §Two-Phase Schedule):
/// the isometry `embed` (d × 2N, 𝔸2 bound σ ≤ 1) and one linear predictor
/// `pred` (d × d, ℙ2 bound σ ≤ ε/2). At export the single trained P fills all
/// three conditioned slots — exactly the WS5 artifact shape (σ = 0.49 ×3);
/// per-conditioning differentiation is Phase 2 (WS-A3, graph corpus).
#[derive(Debug, Clone)]
pub struct ModelParams {
    pub embed: Matrix,
    pub pred: Matrix,
}

/// Gradients aligned with [`ModelParams`].
#[derive(Debug, Clone)]
pub struct Grads {
    pub embed: Matrix,
    pub pred: Matrix,
}

/// Decoupled readout head parameters θ_D (𝔸5): LN affine is frozen identity in
/// WS-A1/A2; the trainable surface is the linear weight (vocab × d), no bias.
#[derive(Debug, Clone)]
pub struct ReadoutParams {
    /// Row-major `[vocab][d]`.
    pub weight: Matrix,
    pub temperature: f64,
}

impl ReadoutParams {
    /// Class probabilities for one latent through the DiscreteReadout
    /// architecture: identity-affine LN (LN_EPS = 1e-5) → linear → temperature
    /// softmax (libm::exp). Bit-parity with
    /// `aria-backends::readout::DiscreteReadout::probs` at identity affine —
    /// the training-side forward and the runtime head compute the same
    /// distribution (asserted by test).
    pub fn probs(&self, z: &[f64]) -> Result<Vec<f64>, TrainingError> {
        let d = self.weight.first().map_or(0, Vec::len);
        if z.len() != d {
            return Err(TrainingError::Config(format!(
                "latent dim {} does not match readout dim {d}",
                z.len()
            )));
        }
        if self.temperature <= 0.0 || !self.temperature.is_finite() {
            return Err(TrainingError::Config(format!(
                "readout temperature {} must be finite and > 0",
                self.temperature
            )));
        }
        let mut ln_z = vec![0.0; d];
        layer_norm_identity_into(z, &mut ln_z);
        let mut logits = vec![0.0; self.weight.len()];
        for (logit, row) in logits.iter_mut().zip(&self.weight) {
            *logit = dot(row, &ln_z);
        }
        let mut probs = vec![0.0; logits.len()];
        softmax_temp_into(&logits, self.temperature, &mut probs);
        Ok(probs)
    }
}

/// Unweighted per-term losses plus the λ-weighted total.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct LossBreakdown {
    pub jepa: f64,
    pub nll: f64,
    pub spectral: f64,
    pub graph: f64,
    pub total: f64,
}

/// Compute ℒ_total and its gradients over one batch of trajectories.
///
/// `align = None` (or λ_Graph = 0) skips the graph term; ℒ_NLL is computed by
/// [`nll_loss_and_grad`] separately because its targets and parameters live
/// outside Φ (WS-A2 wires it into the loop; λ_NLL > 0 is rejected upstream).
///
/// Spectral term is fully audited (two `DEFAULT_ITERATIONS` power-iterations).
/// The training loop uses [`compute_batch_with`] +
/// [`SpectralEval::CertifiedInBall`] after Π_𝒮, which leaves this default
/// for tests that inject out-of-ball weights.
pub fn compute_batch(
    params: &ModelParams,
    ds: &PreparedDataset,
    chunk_ids: &[usize],
    align: Option<&EdgeAlignment>,
    lambdas: &LossLambdas,
    lip_bound: f64,
    gamma: f64,
) -> Result<(LossBreakdown, Grads), TrainingError> {
    compute_batch_with(
        params,
        ds,
        chunk_ids,
        align,
        lambdas,
        lip_bound,
        gamma,
        SpectralEval::Audit,
    )
}

/// [`compute_batch`] with an explicit spectral-evaluation policy.
#[allow(clippy::too_many_arguments)]
pub fn compute_batch_with(
    params: &ModelParams,
    ds: &PreparedDataset,
    chunk_ids: &[usize],
    align: Option<&EdgeAlignment>,
    lambdas: &LossLambdas,
    lip_bound: f64,
    gamma: f64,
    spectral: SpectralEval,
) -> Result<(LossBreakdown, Grads), TrainingError> {
    let d = params.embed.len();
    let input_dim = params.embed.first().map_or(0, Vec::len);
    let mut grads = Grads {
        embed: zeros(d, input_dim),
        pred: zeros(d, d),
    };
    let mut breakdown = LossBreakdown::default();

    // ---- Forward: latents for every frame of every chunk (z = W_I x). ----
    // Latents are cached per global frame index; each batch frame is embedded
    // exactly once even when concepts and transitions both consume it.
    let mut latents: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for &ci in chunk_ids {
        for &f in &ds.chunks[ci] {
            latents
                .entry(f)
                .or_insert_with(|| matvec(&params.embed, &ds.frames[f]));
        }
    }

    // Transition workspaces — allocated once per batch, not per pair.
    let mut p_t = vec![0.0; d];
    let mut r = vec![0.0; d];
    let mut pt_r = vec![0.0; d];

    // ---- ℒ_JEPA + stop-gradient backward. ----
    let mut transitions = 0usize;
    for &ci in chunk_ids {
        transitions += ds.chunks[ci].len() - 1;
    }
    if transitions > 0 && lambdas.jepa > 0.0 {
        let m = transitions as f64;
        let mut loss = 0.0;
        for &ci in chunk_ids {
            let chunk = &ds.chunks[ci];
            for pair in chunk.windows(2) {
                let (f_t, f_next) = (pair[0], pair[1]);
                let z_t = &latents[&f_t];
                let target = &latents[&f_next]; // sg(·): constant in backward
                matvec_into(&params.pred, z_t, &mut p_t);
                sub_into(&p_t, target, &mut r);
                loss += norm2_sq(&r);

                let scale = 2.0 / m * lambdas.jepa;
                // ∂/∂W_P += (2/M)·r zᵀ
                add_outer(&mut grads.pred, scale, &r, z_t);
                // ∂/∂W_I += (2/M)·W_Pᵀ r xᵀ (online branch only)
                mat_t_vec_into(&params.pred, &r, &mut pt_r);
                add_outer(&mut grads.embed, scale, &pt_r, &ds.frames[f_t]);
            }
        }
        breakdown.jepa = loss / m;
    } else if transitions > 0 {
        // Value still reported (unweighted) when λ = 0: measure, don't steer.
        let m = transitions as f64;
        let mut loss = 0.0;
        for &ci in chunk_ids {
            for pair in ds.chunks[ci].windows(2) {
                let z_t = &latents[&pair[0]];
                let target = &latents[&pair[1]];
                matvec_into(&params.pred, z_t, &mut p_t);
                sub_into(&p_t, target, &mut r);
                loss += norm2_sq(&r);
            }
        }
        breakdown.jepa = loss / m;
    }

    // ---- ℒ_Graph over concept mean latents (trains W_I only). ----
    if let Some(align) = align {
        let (loss, grad_updates) = graph_term(ds, chunk_ids, &latents, align, gamma);
        breakdown.graph = loss;
        if lambdas.graph > 0.0 {
            for (frame, g_z) in grad_updates {
                // Chain ∂/∂ζ → ∂/∂W_I through z = W_I x for each member frame.
                add_outer(&mut grads.embed, lambdas.graph, &g_z, &ds.frames[frame]);
            }
        }
    }

    // ---- ℒ_Spectral with u₁v₁ᵀ subgradient. ----
    // After Π_𝒮 both hinges are identically 0 — skip the two full
    // DEFAULT_ITERATIONS sweeps unless an audit is requested.
    match spectral {
        SpectralEval::CertifiedInBall => {
            breakdown.spectral = 0.0;
        }
        SpectralEval::Audit => {
            let (sig_p, u_p, v_p) = power_iteration_with_vectors(&params.pred, DEFAULT_ITERATIONS)?;
            let (sig_i, u_i, v_i) =
                power_iteration_with_vectors(&params.embed, DEFAULT_ITERATIONS)?;
            let hinge_p = (sig_p - lip_bound).max(0.0);
            let hinge_i = (sig_i - 1.0).max(0.0);
            breakdown.spectral = hinge_p * hinge_p + hinge_i * hinge_i;
            if lambdas.spectral > 0.0 {
                if hinge_p > 0.0 {
                    add_outer(
                        &mut grads.pred,
                        lambdas.spectral * 2.0 * hinge_p,
                        &u_p,
                        &v_p,
                    );
                }
                if hinge_i > 0.0 {
                    add_outer(
                        &mut grads.embed,
                        lambdas.spectral * 2.0 * hinge_i,
                        &u_i,
                        &v_i,
                    );
                }
            }
        }
    }

    breakdown.total = lambdas.jepa * breakdown.jepa
        + lambdas.nll * breakdown.nll
        + lambdas.spectral * breakdown.spectral
        + lambdas.graph * breakdown.graph;

    Ok((breakdown, grads))
}

/// The graph Dirichlet hinge over concept mean latents. Returns the unweighted
/// loss and per-frame latent gradients `(frame, ∂ℒ_Graph/∂z_frame)`.
fn graph_term(
    ds: &PreparedDataset,
    chunk_ids: &[usize],
    latents: &BTreeMap<usize, Vec<f64>>,
    align: &EdgeAlignment,
    gamma: f64,
) -> (f64, Vec<(usize, Vec<f64>)>) {
    // Concept → member frames in this batch (deterministic order).
    let mut members: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut batch_frames: Vec<usize> = Vec::new();
    for &ci in chunk_ids {
        batch_frames.extend(ds.chunks[ci].iter().copied());
    }
    batch_frames.sort_unstable();
    batch_frames.dedup();
    for &f in &batch_frames {
        for &c in &align.window_concepts[f] {
            members.entry(c).or_default().push(f);
        }
    }
    if members.is_empty() {
        return (0.0, Vec::new());
    }

    // Mean latent per concept present in the batch.
    let d = latents.values().next().map_or(0, Vec::len);
    let mut means: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    for (&c, frames) in &members {
        let mut acc = vec![0.0; d];
        for &f in frames {
            for (a, z) in acc.iter_mut().zip(&latents[&f]) {
                *a += z;
            }
        }
        let n = frames.len() as f64;
        for a in &mut acc {
            *a /= n;
        }
        means.insert(c, acc);
    }

    // Active edges: both endpoints present in this batch.
    let mut loss = 0.0;
    let mut mean_grads: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    let active: Vec<_> = align
        .usable_edges
        .iter()
        .filter(|e| means.contains_key(&e.s) && means.contains_key(&e.e))
        .collect();
    if active.is_empty() {
        return (0.0, Vec::new());
    }
    let e_count = active.len() as f64;
    let mut diff = vec![0.0; d];
    for edge in active {
        let (zs, ze) = (&means[&edge.s], &means[&edge.e]);
        sub_into(zs, ze, &mut diff);
        let dist = norm2_sq(&diff).sqrt();
        let hinge = (dist - gamma).max(0.0);
        loss += hinge * hinge / e_count;
        if hinge > 0.0 && dist > 0.0 {
            let coeff = 2.0 * hinge / (e_count * dist);
            let gs = mean_grads.entry(edge.s).or_insert_with(|| vec![0.0; d]);
            for (g, v) in gs.iter_mut().zip(&diff) {
                *g += coeff * v;
            }
            let ge = mean_grads.entry(edge.e).or_insert_with(|| vec![0.0; d]);
            for (g, v) in ge.iter_mut().zip(&diff) {
                *g -= coeff * v;
            }
        }
    }

    // Distribute mean gradients to member frames: ∂ζ_c/∂z_w = 1/|S_c|.
    let mut frame_grads: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for (c, g_mean) in mean_grads {
        let frames = &members[&c];
        let inv = 1.0 / frames.len() as f64;
        for &f in frames {
            let fg = frame_grads.entry(f).or_insert_with(|| vec![0.0; d]);
            for (a, g) in fg.iter_mut().zip(&g_mean) {
                *a += g * inv;
            }
        }
    }

    (loss, frame_grads.into_iter().collect())
}

/// ℒ_NLL through the DiscreteReadout architecture on frozen latents:
/// LN(identity affine, LN_EPS = 1e-5) → linear (vocab × d, no bias) →
/// temperature softmax (libm::exp) → mean negative log-likelihood.
///
/// Returns the loss and the gradient **with respect to the readout weight
/// only** — the latents arrive as `sg(z)` and no gradient object for W_I or
/// W_P exists in this function's signature. That is 𝔸5/𝕃5 made structural:
/// `∂Φ/∂y = 0` because no Φ parameter is reachable from here.
pub fn nll_loss_and_grad(
    readout: &ReadoutParams,
    frozen_z: &[Vec<f64>],
    targets: &[u32],
) -> Result<(f64, Matrix), TrainingError> {
    let vocab = readout.weight.len();
    let d = readout.weight.first().map_or(0, Vec::len);
    if frozen_z.len() != targets.len() || frozen_z.is_empty() {
        return Err(TrainingError::Config(format!(
            "NLL needs equally many latents and targets (> 0), got {} / {}",
            frozen_z.len(),
            targets.len()
        )));
    }
    if readout.temperature <= 0.0 || !readout.temperature.is_finite() {
        return Err(TrainingError::Config(format!(
            "readout temperature {} must be finite and > 0",
            readout.temperature
        )));
    }
    let mut grad = zeros(vocab, d);
    let mut loss = 0.0;
    let n = frozen_z.len() as f64;
    let mut ln_z = vec![0.0; d];
    let mut logits = vec![0.0; vocab];
    let mut probs = vec![0.0; vocab];
    for (z, &y) in frozen_z.iter().zip(targets) {
        if z.len() != d {
            return Err(TrainingError::Config(format!(
                "latent dim {} does not match readout dim {d}",
                z.len()
            )));
        }
        let y = y as usize;
        if y >= vocab {
            return Err(TrainingError::Config(format!(
                "target {y} outside vocabulary of size {vocab}"
            )));
        }
        layer_norm_identity_into(z, &mut ln_z);
        for (logit, row) in logits.iter_mut().zip(&readout.weight) {
            *logit = dot(row, &ln_z);
        }
        softmax_temp_into(&logits, readout.temperature, &mut probs);
        loss -= libm::log(probs[y].max(f64::MIN_POSITIVE)) / n;
        for (v, row) in grad.iter_mut().enumerate() {
            let coeff = (probs[v] - if v == y { 1.0 } else { 0.0 }) / (readout.temperature * n);
            for (g, x) in row.iter_mut().zip(&ln_z) {
                *g += coeff * x;
            }
        }
    }
    Ok((loss, grad))
}

/// Identity-affine layer norm — parity with `aria-backends::readout::layer_norm`
/// at γ = 1, β = 0 (the WS-A1 frozen affine).
fn layer_norm_identity_into(z: &[f64], out: &mut [f64]) {
    debug_assert_eq!(out.len(), z.len());
    let n = z.len() as f64;
    let mean = z.iter().sum::<f64>() / n;
    let var = z.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    let inv = 1.0 / (var + LN_EPS).sqrt();
    for (o, x) in out.iter_mut().zip(z) {
        *o = (x - mean) * inv;
    }
}

/// Temperature softmax with the max-shift trick — parity with
/// `aria-backends::readout::softmax_temp` (libm::exp).
///
/// `out` is overwritten in three sequential passes (scale → exp → normalize)
/// matching the previous allocating implementation's IEEE stream.
fn softmax_temp_into(logits: &[f64], temperature: f64, out: &mut [f64]) {
    debug_assert_eq!(out.len(), logits.len());
    for (o, x) in out.iter_mut().zip(logits) {
        *o = *x / temperature;
    }
    let max = out.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    for o in out.iter_mut() {
        *o = libm::exp(*o - max);
    }
    let z = out.iter().sum::<f64>();
    if z == 0.0 || !z.is_finite() {
        let u = 1.0 / out.len() as f64;
        out.fill(u);
        return;
    }
    for o in out.iter_mut() {
        *o /= z;
    }
}
