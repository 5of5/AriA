//! Trained predictor backend — loads weights learned by the Phase 3 Python loop.
//!
//! The Spec constrains this backend in exactly one way, ℙ2: `E[Lip(P)] ≤ 1`.
//! Training is not trusted to respect that, so the loader *enforces* it by
//! spectral projection. A checkpoint that would break Inv2 is scaled down at
//! load time rather than being allowed to fail mid-run — training must never
//! bypass an invariant.
//!
//! # Why the bound is `ε/2` and not `1`
//!
//! Inv2 is `Res(ψ',z',t') ≤ Res(ψ,z,t) + ε`. The worst case is an OpticalStep,
//! which replaces ψ by an arbitrary unit vector while leaving z fixed:
//!
//! ```text
//! Res(ψ',z,t) = ‖z − P(I(ψ'))‖ ≤ ‖z‖ + ‖P(I(ψ'))‖ ≤ 2·Lip(P)·‖I‖·‖ψ₀‖
//! ```
//!
//! With `‖I‖ ≤ 1` and `‖ψ₀‖ = 1` this is `2·Lip(P)`, so `Lip(P) ≤ ε/2` makes
//! Inv2 hold unconditionally. [`TrainedPredictor::max_residual_jump`] reports
//! that bound so a config can be checked against it before a run.

use std::collections::HashMap;
use std::path::Path;

use aria_engine_core::condition::Condition;
use aria_engine_core::engine::Predictor;
use num_complex::Complex64;
use safetensors::tensor::{serialize, Dtype, TensorView};
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};

use crate::spectral::{
    power_iteration, project_spectral, SpectralError, SpectralReport, DEFAULT_ITERATIONS,
};

/// JSON checkpoint written by `python/training/train_jepa.py --out`.
pub const PREDICTOR_V1_FORMAT: &str = "aria-predictor-v1";
/// Safetensors checkpoint written by `python/training/train_jepa.py --out-v2`.
pub const PREDICTOR_V2_FORMAT: &str = "aria-predictor-v2";

/// On-disk weight format written by `python/training/train_jepa.py`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictorWeights {
    /// Format tag; `aria-predictor-v1` (JSON) or `aria-predictor-v2` (safetensors).
    pub format: String,
    pub n_modes: usize,
    pub latent_dim: usize,
    /// Target Lipschitz bound for P. Keep `≤ eps/2` to guarantee Inv2.
    pub lipschitz_bound: f64,
    /// I : H → Z, shape [latent_dim × 2·n_modes], applied to [re₀, im₀, re₁, …].
    pub embed: Vec<Vec<f64>>,
    /// P : Z × Condition → Z, one [latent_dim × latent_dim] matrix per conditioning.
    pub predict: ConditionedWeights,
}

/// One predictor matrix per conditioning (𝐂2: conditioning, not architecture).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionedWeights {
    pub token: Vec<Vec<f64>>,
    pub diffusion: Vec<Vec<f64>>,
    pub world_model: Vec<Vec<f64>>,
}

/// Error cases when loading a checkpoint.
#[derive(Debug, thiserror::Error)]
pub enum WeightsError {
    #[error("unsupported weight format '{0}' (expected '{PREDICTOR_V1_FORMAT}' or '{PREDICTOR_V2_FORMAT}')")]
    Format(String),
    #[error("safetensors error: {0}")]
    Safe(String),
    #[error("utf-8 error: {0}")]
    Utf8(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{name}: expected shape [{want_rows} × {want_cols}], got [{got_rows} × {got_cols}]")]
    Shape {
        name: &'static str,
        want_rows: usize,
        want_cols: usize,
        got_rows: usize,
        got_cols: usize,
    },
    #[error("{0} contains a non-finite weight")]
    NonFinite(&'static str),
    #[error("lipschitz_bound must be finite and > 0, got {0}")]
    Bound(f64),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("spectral projection failed: {0}")]
    Spectral(#[from] SpectralError),
}

/// A `Predictor` backed by learned weights, with ℙ2 enforced at load time.
#[derive(Debug)]
pub struct TrainedPredictor {
    n_modes: usize,
    latent_dim: usize,
    lipschitz_bound: f64,
    embed: Vec<Vec<f64>>,
    token: Vec<Vec<f64>>,
    diffusion: Vec<Vec<f64>>,
    world_model: Vec<Vec<f64>>,
}

impl TrainedPredictor {
    /// Load a checkpoint from JSON, validating shapes and enforcing ℙ2.
    pub fn from_json(src: &str) -> Result<Self, WeightsError> {
        Self::from_weights(serde_json::from_str(src)?)
    }

    /// Load a checkpoint from a file path (JSON v1 or safetensors v2).
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WeightsError> {
        let bytes = std::fs::read(path)?;
        if looks_like_safetensors(&bytes) {
            Self::from_safetensors(&bytes)
        } else {
            let src = std::str::from_utf8(&bytes).map_err(|e| WeightsError::Utf8(e.to_string()))?;
            Self::from_json(src)
        }
    }

    /// Load an `aria-predictor-v2` safetensors buffer.
    pub fn from_safetensors(bytes: &[u8]) -> Result<Self, WeightsError> {
        let w = PredictorWeights::from_safetensors(bytes)?;
        Self::from_weights_tagged(w, PREDICTOR_V2_FORMAT)
    }

    /// Build from an in-memory JSON v1 checkpoint, validating shapes and enforcing ℙ2.
    pub fn from_weights(w: PredictorWeights) -> Result<Self, WeightsError> {
        Self::from_weights_tagged(w, PREDICTOR_V1_FORMAT)
    }

    fn from_weights_tagged(w: PredictorWeights, expected: &str) -> Result<Self, WeightsError> {
        if w.format != expected {
            return Err(WeightsError::Format(w.format));
        }
        if !w.lipschitz_bound.is_finite() || w.lipschitz_bound <= 0.0 {
            return Err(WeightsError::Bound(w.lipschitz_bound));
        }

        let input_dim = 2 * w.n_modes;
        check_shape("embed", &w.embed, w.latent_dim, input_dim)?;
        check_shape(
            "predict.token",
            &w.predict.token,
            w.latent_dim,
            w.latent_dim,
        )?;
        check_shape(
            "predict.diffusion",
            &w.predict.diffusion,
            w.latent_dim,
            w.latent_dim,
        )?;
        check_shape(
            "predict.world_model",
            &w.predict.world_model,
            w.latent_dim,
            w.latent_dim,
        )?;

        // 𝔸2: I is an isometry, so ‖I‖ ≤ 1. Project if training drifted above it.
        let embed = project_spectral(w.embed, 1.0)?;
        // ℙ2: Lip(P) ≤ lipschitz_bound, enforced rather than trusted.
        let token = project_spectral(w.predict.token, w.lipschitz_bound)?;
        let diffusion = project_spectral(w.predict.diffusion, w.lipschitz_bound)?;
        let world_model = project_spectral(w.predict.world_model, w.lipschitz_bound)?;

        Ok(TrainedPredictor {
            n_modes: w.n_modes,
            latent_dim: w.latent_dim,
            lipschitz_bound: w.lipschitz_bound,
            embed,
            token,
            diffusion,
            world_model,
        })
    }

    pub fn n_modes(&self) -> usize {
        self.n_modes
    }

    pub fn latent_dim(&self) -> usize {
        self.latent_dim
    }

    pub fn lipschitz_bound(&self) -> f64 {
        self.lipschitz_bound
    }

    /// Worst-case residual jump across one action: `2·Lip(P)·‖I‖·‖ψ₀‖`.
    ///
    /// Inv2 holds for every schedule when `eps ≥ max_residual_jump(‖ψ₀‖)`.
    pub fn max_residual_jump(&self, psi_norm: f64) -> Result<f64, SpectralError> {
        Ok(2.0
            * self.lipschitz_bound
            * power_iteration(&self.embed, DEFAULT_ITERATIONS)?
            * psi_norm)
    }

    /// The largest measured Lipschitz constant across the conditioned matrices.
    pub fn measured_lipschitz(&self) -> Result<f64, SpectralError> {
        let sigmas = [&self.token, &self.diffusion, &self.world_model]
            .into_iter()
            .map(|m| power_iteration(m, DEFAULT_ITERATIONS))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sigmas.into_iter().fold(0.0, f64::max))
    }

    /// σ_max per weight matrix — the Phase-1 audit surface (plan WS1).
    ///
    /// Under the load-time hard projection every reported value is ≤ its
    /// bound: `embed ≤ 1.0` (𝔸2) and the three conditioned matrices ≤
    /// `lipschitz_bound` (ℙ2) — ε = 0.0 in weight space (𝕋4). Surfaced by
    /// `aria check --predictor` and the run summary.
    pub fn spectral_report(&self) -> Result<SpectralReport, SpectralError> {
        Ok(SpectralReport {
            embed: power_iteration(&self.embed, DEFAULT_ITERATIONS)?,
            token: power_iteration(&self.token, DEFAULT_ITERATIONS)?,
            diffusion: power_iteration(&self.diffusion, DEFAULT_ITERATIONS)?,
            world_model: power_iteration(&self.world_model, DEFAULT_ITERATIONS)?,
        })
    }

    fn matrix_for(&self, a: Condition) -> &Vec<Vec<f64>> {
        match a {
            Condition::Token => &self.token,
            Condition::Diffusion => &self.diffusion,
            Condition::WorldModel => &self.world_model,
        }
    }
}

impl Predictor for TrainedPredictor {
    fn embed(&self, psi: &[Complex64]) -> Vec<f64> {
        let mut flat = Vec::with_capacity(psi.len() * 2);
        for c in psi {
            flat.push(c.re);
            flat.push(c.im);
        }
        mat_vec(&self.embed, &flat)
    }

    fn predict(&self, z: &[f64], a: Condition) -> Vec<f64> {
        mat_vec(self.matrix_for(a), z)
    }

    fn dist(&self, a: &[f64], b: &[f64]) -> f64 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

impl PredictorWeights {
    /// Serialize as `aria-predictor-v2` safetensors (F64, little-endian).
    pub fn to_safetensors(&self) -> Result<Vec<u8>, WeightsError> {
        self.to_safetensors_with_metadata(&HashMap::new())
    }

    /// Serialize as `aria-predictor-v2` with additional provenance metadata
    /// (ARIA-TRAINING-PRD Quality Gate 6 — cryptographic provenance; consumed
    /// by `aria-training::checkpoint`). Reserved keys (`format`, `n_modes`,
    /// `latent_dim`, `lipschitz_bound`) may not be overridden — attempting to
    /// is rejected rather than silently ignored. Loaders read only the
    /// reserved keys, so extra provenance never affects behavior.
    pub fn to_safetensors_with_metadata(
        &self,
        extra: &HashMap<String, String>,
    ) -> Result<Vec<u8>, WeightsError> {
        const RESERVED: [&str; 4] = ["format", "n_modes", "latent_dim", "lipschitz_bound"];
        if let Some(k) = RESERVED.iter().find(|k| extra.contains_key(**k)) {
            return Err(WeightsError::Invalid(format!(
                "provenance metadata may not override the reserved key '{k}'"
            )));
        }
        let embed = flatten(&self.embed);
        let token = flatten(&self.predict.token);
        let diffusion = flatten(&self.predict.diffusion);
        let world = flatten(&self.predict.world_model);
        let embed_b = f64_to_le(&embed);
        let token_b = f64_to_le(&token);
        let diffusion_b = f64_to_le(&diffusion);
        let world_b = f64_to_le(&world);
        let d = self.latent_dim;
        let input_dim = 2 * self.n_modes;
        let t_embed = view("embed", vec![d, input_dim], &embed_b)?;
        let t_token = view("predict.token", vec![d, d], &token_b)?;
        let t_diff = view("predict.diffusion", vec![d, d], &diffusion_b)?;
        let t_world = view("predict.world_model", vec![d, d], &world_b)?;
        let tensors = [
            ("embed", t_embed),
            ("predict.token", t_token),
            ("predict.diffusion", t_diff),
            ("predict.world_model", t_world),
        ];
        let mut meta = extra.clone();
        meta.insert("format".into(), PREDICTOR_V2_FORMAT.into());
        meta.insert("n_modes".into(), self.n_modes.to_string());
        meta.insert("latent_dim".into(), self.latent_dim.to_string());
        meta.insert(
            "lipschitz_bound".into(),
            format!("{:?}", self.lipschitz_bound),
        );
        serialize(tensors, Some(meta)).map_err(|e| WeightsError::Safe(e.to_string()))
    }

    /// Parse an `aria-predictor-v2` buffer into the in-memory weight struct.
    pub fn from_safetensors(bytes: &[u8]) -> Result<Self, WeightsError> {
        let tensors =
            SafeTensors::deserialize(bytes).map_err(|e| WeightsError::Safe(e.to_string()))?;
        let (_, header) =
            SafeTensors::read_metadata(bytes).map_err(|e| WeightsError::Safe(e.to_string()))?;
        let meta = header
            .metadata()
            .clone()
            .ok_or_else(|| WeightsError::Invalid("missing safetensors metadata".into()))?;
        let format = meta
            .get("format")
            .ok_or_else(|| WeightsError::Invalid("metadata missing 'format'".into()))?
            .clone();
        if format != PREDICTOR_V2_FORMAT {
            return Err(WeightsError::Format(format));
        }
        let n_modes = parse_meta_usize(&meta, "n_modes")?;
        let latent_dim = parse_meta_usize(&meta, "latent_dim")?;
        let lipschitz_bound = parse_meta_f64(&meta, "lipschitz_bound")?;
        let input_dim = 2 * n_modes;
        let embed = unflatten(
            &load_f64(&tensors, "embed", &[latent_dim, input_dim])?,
            latent_dim,
            input_dim,
        );
        let token = unflatten(
            &load_f64(&tensors, "predict.token", &[latent_dim, latent_dim])?,
            latent_dim,
            latent_dim,
        );
        let diffusion = unflatten(
            &load_f64(&tensors, "predict.diffusion", &[latent_dim, latent_dim])?,
            latent_dim,
            latent_dim,
        );
        let world_model = unflatten(
            &load_f64(&tensors, "predict.world_model", &[latent_dim, latent_dim])?,
            latent_dim,
            latent_dim,
        );
        Ok(Self {
            format,
            n_modes,
            latent_dim,
            lipschitz_bound,
            embed,
            predict: ConditionedWeights {
                token,
                diffusion,
                world_model,
            },
        })
    }
}

fn looks_like_safetensors(bytes: &[u8]) -> bool {
    if bytes.len() < 9 {
        return false;
    }
    let header_len = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
    let Ok(n) = usize::try_from(header_len) else {
        return false;
    };
    n > 1 && n < bytes.len().saturating_sub(8) && bytes.get(8) == Some(&b'{')
}

fn flatten(m: &[Vec<f64>]) -> Vec<f64> {
    m.iter().flatten().copied().collect()
}

fn unflatten(v: &[f64], rows: usize, cols: usize) -> Vec<Vec<f64>> {
    v.chunks(cols).take(rows).map(<[f64]>::to_vec).collect()
}

fn f64_to_le(v: &[f64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 8);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn le_to_f64(bytes: &[u8]) -> Result<Vec<f64>, WeightsError> {
    if !bytes.len().is_multiple_of(8) {
        return Err(WeightsError::Invalid(
            "f64 tensor byte length is not a multiple of 8".into(),
        ));
    }
    Ok(bytes
        .chunks_exact(8)
        .map(|c| f64::from_le_bytes(c.try_into().expect("chunks_exact(8)")))
        .collect())
}

fn view<'a>(
    name: &'static str,
    shape: Vec<usize>,
    data: &'a [u8],
) -> Result<TensorView<'a>, WeightsError> {
    TensorView::new(Dtype::F64, shape, data).map_err(|e| WeightsError::Safe(format!("{name}: {e}")))
}

fn load_f64(
    tensors: &SafeTensors<'_>,
    name: &'static str,
    want: &[usize],
) -> Result<Vec<f64>, WeightsError> {
    let t = tensors
        .tensor(name)
        .map_err(|e| WeightsError::Invalid(format!("missing tensor '{name}': {e}")))?;
    if t.dtype() != Dtype::F64 {
        return Err(WeightsError::Invalid(format!(
            "{name}: expected F64, got {:?}",
            t.dtype()
        )));
    }
    if t.shape() != want {
        return Err(WeightsError::Shape {
            name,
            want_rows: want.first().copied().unwrap_or(0),
            want_cols: want.get(1).copied().unwrap_or(0),
            got_rows: t.shape().first().copied().unwrap_or(0),
            got_cols: t.shape().get(1).copied().unwrap_or(0),
        });
    }
    let vals = le_to_f64(t.data())?;
    if vals.iter().any(|x| !x.is_finite()) {
        return Err(WeightsError::NonFinite(name));
    }
    Ok(vals)
}

fn parse_meta_usize(meta: &HashMap<String, String>, key: &str) -> Result<usize, WeightsError> {
    meta.get(key)
        .ok_or_else(|| WeightsError::Invalid(format!("metadata missing '{key}'")))?
        .parse()
        .map_err(|_| WeightsError::Invalid(format!("metadata '{key}' is not a usize")))
}

fn parse_meta_f64(meta: &HashMap<String, String>, key: &str) -> Result<f64, WeightsError> {
    let v: f64 = meta
        .get(key)
        .ok_or_else(|| WeightsError::Invalid(format!("metadata missing '{key}'")))?
        .parse()
        .map_err(|_| WeightsError::Invalid(format!("metadata '{key}' is not an f64")))?;
    if !v.is_finite() {
        return Err(WeightsError::Invalid(format!(
            "metadata '{key}' is not finite"
        )));
    }
    Ok(v)
}

fn check_shape(
    name: &'static str,
    m: &[Vec<f64>],
    rows: usize,
    cols: usize,
) -> Result<(), WeightsError> {
    if m.len() != rows || m.iter().any(|r| r.len() != cols) {
        return Err(WeightsError::Shape {
            name,
            want_rows: rows,
            want_cols: cols,
            got_rows: m.len(),
            got_cols: m.first().map_or(0, std::vec::Vec::len),
        });
    }
    if m.iter().flatten().any(|v| !v.is_finite()) {
        return Err(WeightsError::NonFinite(name));
    }
    Ok(())
}

fn mat_vec(m: &[Vec<f64>], x: &[f64]) -> Vec<f64> {
    m.iter()
        .map(|row| row.iter().zip(x).map(|(a, b)| a * b).sum())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(n: usize) -> Vec<Vec<f64>> {
        (0..n)
            .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
            .collect()
    }

    fn weights(latent_dim: usize, n_modes: usize, scale: f64, bound: f64) -> PredictorWeights {
        let input_dim = 2 * n_modes;
        let embed = (0..latent_dim)
            .map(|i| {
                (0..input_dim)
                    .map(|j| if i == j { 1.0 } else { 0.0 })
                    .collect()
            })
            .collect();
        let p: Vec<Vec<f64>> = identity(latent_dim)
            .into_iter()
            .map(|r| r.into_iter().map(|v| v * scale).collect())
            .collect();
        PredictorWeights {
            format: "aria-predictor-v1".into(),
            n_modes,
            latent_dim,
            lipschitz_bound: bound,
            embed,
            predict: ConditionedWeights {
                token: p.clone(),
                diffusion: p.clone(),
                world_model: p,
            },
        }
    }

    #[test]
    fn spectral_norm_of_identity_is_one() {
        assert!((power_iteration(&identity(6), DEFAULT_ITERATIONS).unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn spectral_norm_of_scaled_identity() {
        let m: Vec<Vec<f64>> = identity(5)
            .into_iter()
            .map(|r| r.into_iter().map(|v| v * 3.5).collect())
            .collect();
        assert!((power_iteration(&m, DEFAULT_ITERATIONS).unwrap() - 3.5).abs() < 1e-12);
    }

    #[test]
    fn loader_projects_an_over_lipschitz_checkpoint() {
        // Training produced Lip(P) = 5.0 but the bound is 0.49.
        let p = TrainedPredictor::from_weights(weights(8, 8, 5.0, 0.49)).unwrap();
        let lip = p.measured_lipschitz().unwrap();
        assert!(lip <= 0.49 + 1e-12, "loader must enforce P2, got {lip}");
    }

    #[test]
    fn loader_leaves_a_compliant_checkpoint_alone() {
        let p = TrainedPredictor::from_weights(weights(8, 8, 0.25, 0.49)).unwrap();
        assert!((p.measured_lipschitz().unwrap() - 0.25).abs() < 1e-12);
    }

    #[test]
    fn max_residual_jump_bounds_eps() {
        let p = TrainedPredictor::from_weights(weights(8, 8, 0.49, 0.49)).unwrap();
        // ‖ψ₀‖ = 1 ⇒ jump ≤ 2·0.49 = 0.98 ≤ ε = 1.0.
        assert!(p.max_residual_jump(1.0).unwrap() <= 1.0);
    }

    #[test]
    fn projection_is_entry_exact_on_the_cyclic_fixture() {
        // The migration fixture: σ(4·I) = 4 exactly under any start vector,
        // so the WS1 estimator must scale 4·I to 0.49 entry-exactly — the
        // same weights the pre-WS1 loader produced (observable behavior
        // stays identical on this fixture, plan WS1 tests-at-risk).
        let p = TrainedPredictor::from_weights(weights(8, 8, 4.0, 0.49)).unwrap();
        let report = p.spectral_report().unwrap();
        for m in [&p.token, &p.diffusion, &p.world_model] {
            for (i, row) in m.iter().enumerate() {
                for (j, v) in row.iter().enumerate() {
                    let want = if i == j { 0.49 } else { 0.0 };
                    assert!((v - want).abs() < 1e-12, "entry [{i}][{j}] = {v}");
                }
            }
        }
        assert!((report.token - 0.49).abs() < 1e-12);
        assert!((report.diffusion - 0.49).abs() < 1e-12);
        assert!((report.world_model - 0.49).abs() < 1e-12);
        assert!((report.embed - 1.0).abs() < 1e-12);
    }

    #[test]
    fn rejects_a_bad_format_tag() {
        let mut w = weights(4, 4, 0.5, 0.49);
        w.format = "keras-h5".into();
        assert!(matches!(
            TrainedPredictor::from_weights(w),
            Err(WeightsError::Format(_))
        ));
    }

    #[test]
    fn rejects_a_shape_mismatch() {
        let mut w = weights(4, 4, 0.5, 0.49);
        w.latent_dim = 5;
        assert!(matches!(
            TrainedPredictor::from_weights(w),
            Err(WeightsError::Shape { .. })
        ));
    }

    #[test]
    fn rejects_non_finite_weights() {
        let mut w = weights(4, 4, 0.5, 0.49);
        w.predict.token[0][0] = f64::NAN;
        assert!(matches!(
            TrainedPredictor::from_weights(w),
            Err(WeightsError::NonFinite(_))
        ));
    }

    #[test]
    fn json_round_trip() {
        let w = weights(4, 4, 0.3, 0.49);
        let src = serde_json::to_string(&w).unwrap();
        let p = TrainedPredictor::from_json(&src).unwrap();
        assert_eq!(p.latent_dim(), 4);
        assert_eq!(p.n_modes(), 4);
    }

    #[test]
    fn safetensors_v2_round_trip_matches_json_v1() {
        let w = weights(8, 8, 0.3, 0.49);
        let bytes = w.to_safetensors().unwrap();
        let from_v2 = TrainedPredictor::from_safetensors(&bytes).unwrap();
        let from_v1 = TrainedPredictor::from_weights(w).unwrap();
        assert_eq!(from_v2.n_modes(), from_v1.n_modes());
        assert_eq!(from_v2.latent_dim(), from_v1.latent_dim());
        assert!(
            (from_v2.measured_lipschitz().unwrap() - from_v1.measured_lipschitz().unwrap()).abs()
                < 1e-12
        );
        let probe = [num_complex::Complex64::new(1.0, 0.0); 8];
        let a = from_v2.embed(&probe);
        let b = from_v1.embed(&probe);
        assert_eq!(a, b);
    }

    #[test]
    fn v2_loader_rejects_a_bad_format_tag() {
        let mut w = weights(4, 4, 0.3, 0.49);
        w.format = PREDICTOR_V2_FORMAT.into();
        let mut bytes = w.to_safetensors().unwrap();
        // Corrupt the format string inside the header.
        if let Some(pos) = bytes
            .windows(PREDICTOR_V2_FORMAT.len())
            .position(|w| w == PREDICTOR_V2_FORMAT.as_bytes())
        {
            bytes[pos] = b'X';
        }
        assert!(TrainedPredictor::from_safetensors(&bytes).is_err());
    }

    #[test]
    fn from_file_sniffs_v2() {
        let dir = std::env::temp_dir();
        let path = dir.join("aria-predictor-v2-sniff-test.safetensors");
        let w = weights(4, 4, 0.25, 0.49);
        std::fs::write(&path, w.to_safetensors().unwrap()).unwrap();
        let p = TrainedPredictor::from_file(&path).unwrap();
        assert_eq!(p.latent_dim(), 4);
        let _ = std::fs::remove_file(&path);
    }
}
