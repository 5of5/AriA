//! AdamW with embedded Lipschitz spectral projection (D1; training PRD
//! §crate architecture: "optimizer.rs — AdamW with embedded Lipschitz
//! spectral projection").
//!
//! The Inductive Safety Theorem (training PRD) is implemented here, not
//! trusted elsewhere: after **every** parameter update the weights are
//! hard-projected back into their balls —
//!
//! ```text
//! W_I ← project_spectral(W_I, 1.0)      (𝔸2: ‖I‖ ≤ 1)
//! W_P ← project_spectral(W_P, ε/2)      (ℙ2: Lip(P) ≤ ε/2 ⇒ Inv2 unconditional)
//! ```
//!
//! using the exact seeded estimator the runtime loader audits with, so a
//! checkpoint produced here loads through `TrainedPredictor` with the
//! projection a measured no-op. AdamW itself is deterministic — bias-corrected
//! first/second moments and decoupled weight decay, no randomness anywhere.

use aria_engine_backends::spectral::{power_iteration, Matrix, SpectralError, DEFAULT_ITERATIONS};
use serde::{Deserialize, Serialize};

use crate::linalg::{dot, zeros};
use crate::loss::{Grads, ModelParams};

/// AdamW hyper-parameters. Defaults are the receipted smoke protocol
/// (2026-08-17, `v0.3.0_wsa1_core_smoke.json`): lr 3e-3, β₁ 0.9, β₂ 0.999,
/// ε 1e-8, weight_decay 0 — the spectral balls are enforced by hard
/// projection, so decay is optional shrinkage, not a safety mechanism.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AdamWParams {
    pub lr: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub eps: f64,
    pub weight_decay: f64,
}

impl Default for AdamWParams {
    fn default() -> Self {
        AdamWParams {
            lr: 3e-3,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            weight_decay: 0.0,
        }
    }
}

impl AdamWParams {
    /// Domain validation — reject-with-detail, never clamp.
    pub fn validate(&self) -> Result<(), String> {
        if !self.lr.is_finite() || self.lr <= 0.0 {
            return Err(format!("adamw.lr = {} must be finite and > 0", self.lr));
        }
        for (name, b) in [("beta1", self.beta1), ("beta2", self.beta2)] {
            if !b.is_finite() || !(0.0..1.0).contains(&b) {
                return Err(format!("adamw.{name} = {b} must lie in [0, 1)"));
            }
        }
        if !self.eps.is_finite() || self.eps <= 0.0 {
            return Err(format!("adamw.eps = {} must be finite and > 0", self.eps));
        }
        if !self.weight_decay.is_finite() || self.weight_decay < 0.0 {
            return Err(format!(
                "adamw.weight_decay = {} must be finite and ≥ 0",
                self.weight_decay
            ));
        }
        Ok(())
    }
}

/// AdamW state over the two Φ-side parameter matrices.
#[derive(Debug, Clone)]
pub struct AdamW {
    params: AdamWParams,
    t: u64,
    m_embed: Matrix,
    v_embed: Matrix,
    m_pred: Matrix,
    v_pred: Matrix,
    /// Unit direction the embedding is kept orthogonal to (trivial-mode
    /// deflation; see [`AdamW::with_deflation`]).
    deflate: Option<Vec<f64>>,
}

impl AdamW {
    /// Fresh state for parameters of the given shapes.
    pub fn new(params: AdamWParams, model: &ModelParams) -> Self {
        let (er, ec) = (model.embed.len(), model.embed.first().map_or(0, Vec::len));
        let (pr, pc) = (model.pred.len(), model.pred.first().map_or(0, Vec::len));
        AdamW {
            params,
            t: 0,
            m_embed: zeros(er, ec),
            v_embed: zeros(er, ec),
            m_pred: zeros(pr, pc),
            v_pred: zeros(pr, pc),
            deflate: None,
        }
    }

    /// Constrain the embedding to stay orthogonal to `direction` after every
    /// step: `W_I · μ̂ = 0` (trivial-mode deflation).
    ///
    /// Why this is part of Π_𝒮 (measured 2026-08-17, WS-A1): the stop-gradient
    /// JEPA flow obeys `E[∂ℒ/∂w] ∝ (c−1)·m² + (c−ρ)·v` per input direction
    /// (gain `w`, predictor gain `c`, mean `m`, variance `v`, lag-1
    /// correlation `ρ`). Variance directions equilibrate at `c = ρ` — no
    /// collapse — but the corpus-mean direction has `c ≤ ε/2 < 1`, so
    /// stop-grad *actively grows* it until it dominates the representation
    /// (measured: ‖μ_z‖² 2.5e-5 → 2.2e-2 within 5 epochs while variance shed
    /// 2.7e-2 → 1.9e-3). The persistence baseline cancels the mean, so the
    /// mean-dominated attractor can never beat it. Deflation removes exactly
    /// that direction — the byte-window analogue of 𝔸4's spectral-lift rule
    /// that excludes the Laplacian's trivial constant eigenmode.
    ///
    /// Theorem compatibility: deflation is an orthogonal projection of each
    /// row, so σ_max can only decrease; composed with the σ-cap the Inductive
    /// Safety Theorem's premise (σ_max(W_I) ≤ 1.0) holds verbatim, and the
    /// σ-cap's global rescale preserves `W μ̂ = 0`. The exported artifact
    /// stays a plain linear map — centering is *inside the matrix*, so the
    /// runtime engine needs no preprocessing and the v1/v2 formats are
    /// untouched.
    ///
    /// A degenerate direction (norm ≤ 1e-12) disables deflation.
    #[must_use]
    pub fn with_deflation(mut self, direction: &[f64]) -> Self {
        let norm = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        self.deflate = (norm > 1e-12).then(|| direction.iter().map(|v| v / norm).collect());
        self
    }

    /// Steps taken so far.
    pub fn steps(&self) -> u64 {
        self.t
    }

    /// One AdamW update followed by the embedded hard projection Π_𝒮:
    /// optional trivial-mode deflation (see [`AdamW::with_deflation`]), then
    /// `embed → project_spectral(·, 1.0)` (𝔸2) and
    /// `pred → project_spectral(·, lip_bound)` (ℙ2 — Inv2 unconditional).
    /// Every exit from this function leaves the parameters inside their
    /// admissible sets, unconditionally.
    ///
    /// Design record (measured 2026-08-17): two manifold-constrained
    /// alternatives — naive Stiefel retraction, and tangent-projected AdamW
    /// with retraction — were measured *worse* (training ascended; AdamW's
    /// per-entry normalization breaks tangency and the retraction converts
    /// the normal component into systematic drift). Orthogonality-by-
    /// construction remains UT-3 / WS-B1 scope; the deflation constraint is
    /// the minimal Π_𝒮 extension that removes the measured mean attractor.
    pub fn step(
        &mut self,
        model: &mut ModelParams,
        grads: &Grads,
        lip_bound: f64,
    ) -> Result<(), SpectralError> {
        self.t += 1;
        let t = self.t;
        let p = self.params;
        adamw_update(
            &mut model.embed,
            &grads.embed,
            &mut self.m_embed,
            &mut self.v_embed,
            t,
            p,
        );
        adamw_update(
            &mut model.pred,
            &grads.pred,
            &mut self.m_pred,
            &mut self.v_pred,
            t,
            p,
        );

        // Embedded projection — the invariant-preserving half of the step.
        if let Some(u) = &self.deflate {
            for row in &mut model.embed {
                let proj = dot(row, u);
                for (r, uv) in row.iter_mut().zip(u) {
                    *r -= proj * uv;
                }
            }
        }
        project_to_estimator_ball(&mut model.embed, 1.0)?;
        project_to_estimator_ball(&mut model.pred, lip_bound)?;
        Ok(())
    }
}

/// Π_𝒮's σ-cap: scale until the *same* seeded estimator the hinge and the
/// runtime loader use reports `σ̂ ≤ bound`. A single [`project_spectral_in_place`]
/// can land 1 ulp over the bound (multiplication is not exactly distributive);
/// the export path already iterated (`project_fixed_point`). Doing it here
/// makes `SpectralEval::CertifiedInBall` an honest post-condition rather than
/// a 1-ulp leap of faith.
pub fn project_to_estimator_ball(w: &mut Matrix, bound: f64) -> Result<(), SpectralError> {
    for _ in 0..8 {
        let sigma = power_iteration(w, DEFAULT_ITERATIONS)?;
        if sigma <= bound || sigma == 0.0 {
            return Ok(());
        }
        let scale = bound / sigma;
        for row in w.iter_mut() {
            for v in row {
                *v *= scale;
            }
        }
    }
    Ok(())
}

/// The textbook AdamW update, entrywise and deterministic. `pub(crate)`:
/// the decoupled readout pass reuses it on θ_D.
pub(crate) fn adamw_update(
    weights: &mut Matrix,
    grads: &Matrix,
    moment1: &mut Matrix,
    moment2: &mut Matrix,
    step: u64,
    hp: AdamWParams,
) {
    // 1 − βᵗ via powi on the exactly-representable exponent.
    let step_i32 = i32::try_from(step.min(u64::from(i32::MAX as u32))).expect("bounded above");
    let bc1 = 1.0 - hp.beta1.powi(step_i32);
    let bc2 = 1.0 - hp.beta2.powi(step_i32);
    for ((wr, gr), (mr, vr)) in weights
        .iter_mut()
        .zip(grads)
        .zip(moment1.iter_mut().zip(moment2.iter_mut()))
    {
        for ((wv, gv), (mv, vv)) in wr.iter_mut().zip(gr).zip(mr.iter_mut().zip(vr.iter_mut())) {
            *mv = hp.beta1 * *mv + (1.0 - hp.beta1) * gv;
            *vv = hp.beta2 * *vv + (1.0 - hp.beta2) * gv * gv;
            let m_hat = *mv / bc1;
            let v_hat = *vv / bc2;
            *wv -= hp.lr * (m_hat / (v_hat.sqrt() + hp.eps) + hp.weight_decay * *wv);
        }
    }
}
