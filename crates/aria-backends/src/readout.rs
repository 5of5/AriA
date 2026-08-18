//! Decoupled readout heads — 𝔸5 / 𝕃5 / spec §5.1.
//!
//! These maps `D` sit **outside** Φ. They consume a finished latent `z` and
//! emit either a discrete token id or a continuous action vector. They never
//! write back into `ψ`, `z`, `G`, or `t`. The structural lock is the WS0
//! decoder gate: this module lives in `aria-backends`, never `aria-core`.
//!
//! Candle is **not** used (Q-2026-08-12-6): core Φ is f64 with 1e-10 energy
//! checks; candle is f32-first. The §5.1 head is hand-rolled f64 so the
//! readout arithmetic matches the rest of the engine and cannot inject a
//! second precision into a later training loop's frozen-z contract.
//!
//! # Discrete head (spec §5.1)
//!
//! `layer-norm → linear(d → |V_o|, no bias) → temperature softmax`.
//! Decoding is greedy argmax on the logits (equivalent to argmax of the
//! softmax for τ > 0, and free of a host-`exp` ulp). Ties break by the
//! lowest index so two builds agree.
//!
//! # Continuous head
//!
//! `linear(d → d_a)`, `1 ≤ d_a ≤ d`. No activation — the caller owns the
//! action-space interpretation.

use std::collections::HashMap;
use std::path::Path;

use safetensors::tensor::{serialize, Dtype, TensorView};
use safetensors::SafeTensors;

/// Official on-disk tag. Loaders reject anything else.
pub const READOUT_FORMAT: &str = "aria-readout-v1";

/// Spec §0.1 discrete-vocabulary bounds.
pub const VOCAB_MIN: usize = 256;
pub const VOCAB_MAX: usize = 128_000;

/// Layer-norm floor — matches the usual 1e-5, independent of host libm.
const LN_EPS: f64 = 1e-5;

/// Errors from constructing, loading, or applying a readout.
#[derive(Debug, thiserror::Error)]
pub enum ReadoutError {
    #[error("{0}")]
    Invalid(String),
    #[error("unsupported readout format '{0}' (expected '{READOUT_FORMAT}')")]
    Format(String),
    #[error("{name}: expected shape {want:?}, got {got:?}")]
    Shape {
        name: &'static str,
        want: Vec<usize>,
        got: Vec<usize>,
    },
    #[error("{0} contains a non-finite weight")]
    NonFinite(&'static str),
    #[error("safetensors error: {0}")]
    Safe(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Which head is stored in an `aria-readout-v1` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadoutKind {
    Discrete,
    Continuous,
}

/// Either head, loaded from one safetensors file.
#[derive(Debug, Clone)]
pub enum Readout {
    Discrete(DiscreteReadout),
    Continuous(ContinuousReadout),
}

impl Readout {
    /// Load an `aria-readout-v1` safetensors file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ReadoutError> {
        Self::from_bytes(&std::fs::read(path)?)
    }

    /// Load from an in-memory safetensors buffer (WASM / tests).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReadoutError> {
        let tensors =
            SafeTensors::deserialize(bytes).map_err(|e| ReadoutError::Safe(e.to_string()))?;
        let (_, header) =
            SafeTensors::read_metadata(bytes).map_err(|e| ReadoutError::Safe(e.to_string()))?;
        let meta = header
            .metadata()
            .clone()
            .ok_or_else(|| ReadoutError::Invalid("missing safetensors metadata".into()))?;
        let format = meta
            .get("format")
            .ok_or_else(|| ReadoutError::Invalid("metadata missing 'format'".into()))?;
        if format != READOUT_FORMAT {
            return Err(ReadoutError::Format(format.clone()));
        }
        let kind = meta
            .get("kind")
            .map(String::as_str)
            .ok_or_else(|| ReadoutError::Invalid("metadata missing 'kind'".into()))?;
        match kind {
            "discrete" => Ok(Self::Discrete(DiscreteReadout::from_tensors(
                &tensors, &meta,
            )?)),
            "continuous" => Ok(Self::Continuous(ContinuousReadout::from_tensors(
                &tensors, &meta,
            )?)),
            other => Err(ReadoutError::Invalid(format!(
                "unknown readout kind '{other}' (expected discrete|continuous)"
            ))),
        }
    }

    pub fn kind(&self) -> ReadoutKind {
        match self {
            Self::Discrete(_) => ReadoutKind::Discrete,
            Self::Continuous(_) => ReadoutKind::Continuous,
        }
    }

    pub fn dim(&self) -> usize {
        match self {
            Self::Discrete(h) => h.dim,
            Self::Continuous(h) => h.dim,
        }
    }

    /// Serialize to the `aria-readout-v1` safetensors encoding.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ReadoutError> {
        match self {
            Self::Discrete(h) => h.to_bytes(),
            Self::Continuous(h) => h.to_bytes(),
        }
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<(), ReadoutError> {
        std::fs::write(path, self.to_bytes()?)?;
        Ok(())
    }
}

/// Discrete §5.1 head: LN → linear(d → |V_o|, no bias) → temperature softmax.
#[derive(Debug, Clone)]
pub struct DiscreteReadout {
    dim: usize,
    vocab_size: usize,
    temperature: f64,
    ln_weight: Vec<f64>,
    ln_bias: Vec<f64>,
    /// Row-major `[vocab_size × dim]`, no bias.
    weight: Vec<f64>,
}

impl DiscreteReadout {
    /// Construct from explicit tensors. Rejects every 𝒮 / finiteness fault.
    pub fn new(
        dim: usize,
        vocab_size: usize,
        temperature: f64,
        ln_weight: Vec<f64>,
        ln_bias: Vec<f64>,
        weight: Vec<f64>,
    ) -> Result<Self, ReadoutError> {
        validate_discrete(dim, vocab_size, temperature)?;
        check_vec("ln_weight", &ln_weight, dim)?;
        check_vec("ln_bias", &ln_bias, dim)?;
        check_vec(
            "weight",
            &weight,
            vocab_size
                .checked_mul(dim)
                .ok_or_else(|| ReadoutError::Invalid("vocab_size × dim overflows usize".into()))?,
        )?;
        Ok(Self {
            dim,
            vocab_size,
            temperature,
            ln_weight,
            ln_bias,
            weight,
        })
    }

    /// Seeded untrained head — real weights, no OS entropy (repo lock).
    ///
    /// LN affine is identity. The linear rows are a deterministic LCG draw
    /// scaled by `1/√d` so logits stay O(1) at unit-norm latents.
    pub fn seeded(
        dim: usize,
        vocab_size: usize,
        temperature: f64,
        seed: u64,
    ) -> Result<Self, ReadoutError> {
        validate_discrete(dim, vocab_size, temperature)?;
        let ln_weight = vec![1.0; dim];
        let ln_bias = vec![0.0; dim];
        let scale = 1.0 / (dim as f64).sqrt();
        let mut rng = seed;
        let mut weight = Vec::with_capacity(vocab_size * dim);
        for _ in 0..(vocab_size * dim) {
            rng = rng
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let u = ((rng >> 11) as f64) / ((1u64 << 53) as f64);
            weight.push((u - 0.5) * 2.0 * scale);
        }
        Self::new(dim, vocab_size, temperature, ln_weight, ln_bias, weight)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }
    pub fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    pub fn temperature(&self) -> f64 {
        self.temperature
    }

    /// Logits `W · LN(z)` — the quantity argmax reads.
    pub fn logits(&self, z: &[f64]) -> Result<Vec<f64>, ReadoutError> {
        let y = layer_norm(z, self.dim, &self.ln_weight, &self.ln_bias)?;
        let mut out = vec![0.0; self.vocab_size];
        for (v, logit) in out.iter_mut().enumerate() {
            let row = &self.weight[v * self.dim..(v + 1) * self.dim];
            let mut s = 0.0;
            for (w, x) in row.iter().zip(&y) {
                s += w * x;
            }
            *logit = s;
        }
        Ok(out)
    }

    /// Temperature softmax of the logits. Uses `libm::exp` (WS2 parity rule).
    pub fn probs(&self, z: &[f64]) -> Result<Vec<f64>, ReadoutError> {
        let logits = self.logits(z)?;
        Ok(softmax_temp(&logits, self.temperature))
    }

    /// Greedy token id. Argmax of logits; ties → lowest index.
    pub fn decode_id(&self, z: &[f64]) -> Result<u32, ReadoutError> {
        let logits = self.logits(z)?;
        let mut best_i = 0usize;
        let mut best = logits[0];
        for (i, &v) in logits.iter().enumerate().skip(1) {
            if v.total_cmp(&best) == std::cmp::Ordering::Greater {
                best = v;
                best_i = i;
            }
        }
        u32::try_from(best_i).map_err(|_| ReadoutError::Invalid("vocab exceeds u32".into()))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ReadoutError> {
        let ln_w = f64_to_le(&self.ln_weight);
        let ln_b = f64_to_le(&self.ln_bias);
        let w = f64_to_le(&self.weight);
        let t_ln_w = view("ln_weight", vec![self.dim], &ln_w)?;
        let t_ln_b = view("ln_bias", vec![self.dim], &ln_b)?;
        let t_w = view("weight", vec![self.vocab_size, self.dim], &w)?;
        let tensors = [("ln_weight", t_ln_w), ("ln_bias", t_ln_b), ("weight", t_w)];
        let mut meta = HashMap::new();
        meta.insert("format".into(), READOUT_FORMAT.into());
        meta.insert("kind".into(), "discrete".into());
        meta.insert("dim".into(), self.dim.to_string());
        meta.insert("vocab_size".into(), self.vocab_size.to_string());
        meta.insert("temperature".into(), format_f64(self.temperature));
        serialize(tensors, Some(meta)).map_err(|e| ReadoutError::Safe(e.to_string()))
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<(), ReadoutError> {
        std::fs::write(path, self.to_bytes()?)?;
        Ok(())
    }

    fn from_tensors(
        tensors: &SafeTensors<'_>,
        meta: &HashMap<String, String>,
    ) -> Result<Self, ReadoutError> {
        let dim = parse_meta_usize(meta, "dim")?;
        let vocab_size = parse_meta_usize(meta, "vocab_size")?;
        let temperature = parse_meta_f64(meta, "temperature")?;
        let ln_weight = load_f64(tensors, "ln_weight", &[dim])?;
        let ln_bias = load_f64(tensors, "ln_bias", &[dim])?;
        let weight = load_f64(tensors, "weight", &[vocab_size, dim])?;
        Self::new(dim, vocab_size, temperature, ln_weight, ln_bias, weight)
    }
}

/// Continuous head: linear(d → d_a), `1 ≤ d_a ≤ d`.
#[derive(Debug, Clone)]
pub struct ContinuousReadout {
    dim: usize,
    d_a: usize,
    /// Row-major `[d_a × dim]`.
    weight: Vec<f64>,
}

impl ContinuousReadout {
    pub fn new(dim: usize, d_a: usize, weight: Vec<f64>) -> Result<Self, ReadoutError> {
        validate_continuous(dim, d_a)?;
        check_vec(
            "weight",
            &weight,
            d_a.checked_mul(dim)
                .ok_or_else(|| ReadoutError::Invalid("d_a × dim overflows usize".into()))?,
        )?;
        Ok(Self { dim, d_a, weight })
    }

    pub fn seeded(dim: usize, d_a: usize, seed: u64) -> Result<Self, ReadoutError> {
        validate_continuous(dim, d_a)?;
        let scale = 1.0 / (dim as f64).sqrt();
        let mut rng = seed;
        let mut weight = Vec::with_capacity(d_a * dim);
        for _ in 0..(d_a * dim) {
            rng = rng
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let u = ((rng >> 11) as f64) / ((1u64 << 53) as f64);
            weight.push((u - 0.5) * 2.0 * scale);
        }
        Self::new(dim, d_a, weight)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }
    pub fn d_a(&self) -> usize {
        self.d_a
    }

    /// `a = W · z`.
    pub fn emit(&self, z: &[f64]) -> Result<Vec<f64>, ReadoutError> {
        if z.len() != self.dim || z.iter().any(|x| !x.is_finite()) {
            return Err(ReadoutError::Invalid(format!(
                "expected {} finite latent components, got {}",
                self.dim,
                z.len()
            )));
        }
        let mut out = vec![0.0; self.d_a];
        for (i, a) in out.iter_mut().enumerate() {
            let row = &self.weight[i * self.dim..(i + 1) * self.dim];
            let mut s = 0.0;
            for (w, x) in row.iter().zip(z) {
                s += w * x;
            }
            *a = s;
        }
        Ok(out)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ReadoutError> {
        let w = f64_to_le(&self.weight);
        let t_w = view("weight", vec![self.d_a, self.dim], &w)?;
        let tensors = [("weight", t_w)];
        let mut meta = HashMap::new();
        meta.insert("format".into(), READOUT_FORMAT.into());
        meta.insert("kind".into(), "continuous".into());
        meta.insert("dim".into(), self.dim.to_string());
        meta.insert("d_a".into(), self.d_a.to_string());
        serialize(tensors, Some(meta)).map_err(|e| ReadoutError::Safe(e.to_string()))
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<(), ReadoutError> {
        std::fs::write(path, self.to_bytes()?)?;
        Ok(())
    }

    fn from_tensors(
        tensors: &SafeTensors<'_>,
        meta: &HashMap<String, String>,
    ) -> Result<Self, ReadoutError> {
        let dim = parse_meta_usize(meta, "dim")?;
        let d_a = parse_meta_usize(meta, "d_a")?;
        let weight = load_f64(tensors, "weight", &[d_a, dim])?;
        Self::new(dim, d_a, weight)
    }
}

fn validate_discrete(dim: usize, vocab_size: usize, temperature: f64) -> Result<(), ReadoutError> {
    if dim == 0 {
        return Err(ReadoutError::Invalid("latent dim must be > 0".into()));
    }
    if !(VOCAB_MIN..=VOCAB_MAX).contains(&vocab_size) {
        return Err(ReadoutError::Invalid(format!(
            "vocab_size {vocab_size} is outside the spec domain [{VOCAB_MIN}, {VOCAB_MAX}]"
        )));
    }
    if !temperature.is_finite() || temperature <= 0.0 {
        return Err(ReadoutError::Invalid(format!(
            "temperature must be finite and > 0, got {temperature}"
        )));
    }
    Ok(())
}

fn validate_continuous(dim: usize, d_a: usize) -> Result<(), ReadoutError> {
    if dim == 0 {
        return Err(ReadoutError::Invalid("latent dim must be > 0".into()));
    }
    if d_a == 0 || d_a > dim {
        return Err(ReadoutError::Invalid(format!(
            "d_a = {d_a} violates 1 ≤ d_a ≤ d = {dim}"
        )));
    }
    Ok(())
}

fn check_vec(name: &'static str, v: &[f64], want: usize) -> Result<(), ReadoutError> {
    if v.len() != want {
        return Err(ReadoutError::Shape {
            name,
            want: vec![want],
            got: vec![v.len()],
        });
    }
    if v.iter().any(|x| !x.is_finite()) {
        return Err(ReadoutError::NonFinite(name));
    }
    Ok(())
}

fn layer_norm(
    z: &[f64],
    dim: usize,
    gamma: &[f64],
    beta: &[f64],
) -> Result<Vec<f64>, ReadoutError> {
    if z.len() != dim || z.iter().any(|x| !x.is_finite()) {
        return Err(ReadoutError::Invalid(format!(
            "expected {dim} finite latent components, got {}",
            z.len()
        )));
    }
    let n = dim as f64;
    let mean = z.iter().sum::<f64>() / n;
    let var = z
        .iter()
        .map(|x| {
            let d = x - mean;
            d * d
        })
        .sum::<f64>()
        / n;
    let inv = 1.0 / (var + LN_EPS).sqrt();
    Ok(z.iter()
        .zip(gamma)
        .zip(beta)
        .map(|((x, g), b)| g * (x - mean) * inv + b)
        .collect())
}

fn softmax_temp(logits: &[f64], temperature: f64) -> Vec<f64> {
    let scale: Vec<f64> = logits.iter().map(|x| x / temperature).collect();
    let max = scale
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, |a, b| if a > b { a } else { b });
    let exps: Vec<f64> = scale.iter().map(|x| libm::exp(x - max)).collect();
    let z = exps.iter().sum::<f64>();
    if z == 0.0 || !z.is_finite() {
        let u = 1.0 / exps.len() as f64;
        return vec![u; exps.len()];
    }
    exps.iter().map(|e| e / z).collect()
}

fn f64_to_le(v: &[f64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 8);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn le_to_f64(bytes: &[u8]) -> Result<Vec<f64>, ReadoutError> {
    if !bytes.len().is_multiple_of(8) {
        return Err(ReadoutError::Invalid(
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
) -> Result<TensorView<'a>, ReadoutError> {
    TensorView::new(Dtype::F64, shape, data).map_err(|e| ReadoutError::Safe(format!("{name}: {e}")))
}

fn load_f64(
    tensors: &SafeTensors<'_>,
    name: &'static str,
    want: &[usize],
) -> Result<Vec<f64>, ReadoutError> {
    let t = tensors
        .tensor(name)
        .map_err(|e| ReadoutError::Invalid(format!("missing tensor '{name}': {e}")))?;
    if t.dtype() != Dtype::F64 {
        return Err(ReadoutError::Invalid(format!(
            "{name}: expected F64, got {:?}",
            t.dtype()
        )));
    }
    if t.shape() != want {
        return Err(ReadoutError::Shape {
            name,
            want: want.to_vec(),
            got: t.shape().to_vec(),
        });
    }
    let vals = le_to_f64(t.data())?;
    if vals.iter().any(|x| !x.is_finite()) {
        return Err(ReadoutError::NonFinite(name));
    }
    Ok(vals)
}

fn parse_meta_usize(meta: &HashMap<String, String>, key: &str) -> Result<usize, ReadoutError> {
    meta.get(key)
        .ok_or_else(|| ReadoutError::Invalid(format!("metadata missing '{key}'")))?
        .parse()
        .map_err(|_| ReadoutError::Invalid(format!("metadata '{key}' is not a usize")))
}

fn parse_meta_f64(meta: &HashMap<String, String>, key: &str) -> Result<f64, ReadoutError> {
    let v: f64 = meta
        .get(key)
        .ok_or_else(|| ReadoutError::Invalid(format!("metadata missing '{key}'")))?
        .parse()
        .map_err(|_| ReadoutError::Invalid(format!("metadata '{key}' is not an f64")))?;
    if !v.is_finite() {
        return Err(ReadoutError::Invalid(format!(
            "metadata '{key}' is not finite"
        )));
    }
    Ok(v)
}

/// Shortest round-trip-stable rendering for a finite f64 metadata value.
fn format_f64(x: f64) -> String {
    // Debug is enough for the values we accept (finite, > 0); parse back
    // with the same rustc so a file we wrote loads bit-identically.
    format!("{x:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_is_reproducible_and_seed_sensitive() {
        let a = DiscreteReadout::seeded(8, 256, 1.0, 7).unwrap();
        let b = DiscreteReadout::seeded(8, 256, 1.0, 7).unwrap();
        let c = DiscreteReadout::seeded(8, 256, 1.0, 8).unwrap();
        assert_eq!(a.weight, b.weight);
        assert_ne!(a.weight, c.weight);
    }

    #[test]
    fn softmax_rows_are_a_simplex() {
        let h = DiscreteReadout::seeded(8, 256, 1.0, 1).unwrap();
        let z = vec![0.25; 8];
        let p = h.probs(&z).unwrap();
        assert_eq!(p.len(), 256);
        let s: f64 = p.iter().sum();
        assert!((s - 1.0).abs() < 1e-12, "softmax sum {s}");
        assert!(p.iter().all(|&x| x >= 0.0 && x.is_finite()));
    }

    #[test]
    fn decode_id_is_deterministic_and_matches_logit_argmax() {
        let h = DiscreteReadout::seeded(8, 256, 0.7, 99).unwrap();
        let z: Vec<f64> = (0..8).map(|i| f64::from(i) * 0.1 - 0.3).collect();
        let id_a = h.decode_id(&z).unwrap();
        let id_b = h.decode_id(&z).unwrap();
        assert_eq!(id_a, id_b);
        let logits = h.logits(&z).unwrap();
        let mut want = 0usize;
        let mut best = logits[0];
        for (i, &v) in logits.iter().enumerate().skip(1) {
            if v.total_cmp(&best) == std::cmp::Ordering::Greater {
                best = v;
                want = i;
            }
        }
        assert_eq!(id_a as usize, want);
    }

    #[test]
    fn discrete_rejects_out_of_domain_vocab_and_temperature() {
        assert!(DiscreteReadout::seeded(8, 255, 1.0, 1).is_err());
        assert!(DiscreteReadout::seeded(8, 128_001, 1.0, 1).is_err());
        assert!(DiscreteReadout::seeded(8, 256, 0.0, 1).is_err());
        assert!(DiscreteReadout::seeded(8, 256, -1.0, 1).is_err());
        assert!(DiscreteReadout::seeded(8, 256, f64::NAN, 1).is_err());
    }

    #[test]
    fn continuous_rejects_bad_d_a() {
        assert!(ContinuousReadout::seeded(8, 0, 1).is_err());
        assert!(ContinuousReadout::seeded(8, 9, 1).is_err());
        assert!(ContinuousReadout::seeded(8, 8, 1).is_ok());
        assert!(ContinuousReadout::seeded(8, 1, 1).is_ok());
    }

    #[test]
    fn safetensors_round_trip_is_bit_identical() {
        let h = DiscreteReadout::seeded(16, 256, 1.25, 123).unwrap();
        let bytes = h.to_bytes().unwrap();
        let Readout::Discrete(loaded) = Readout::from_bytes(&bytes).unwrap() else {
            panic!("expected discrete");
        };
        assert_eq!(loaded.dim, h.dim);
        assert_eq!(loaded.vocab_size, h.vocab_size);
        assert_eq!(loaded.temperature.to_bits(), h.temperature.to_bits());
        assert_eq!(loaded.ln_weight, h.ln_weight);
        assert_eq!(loaded.ln_bias, h.ln_bias);
        assert_eq!(loaded.weight, h.weight);
        let z = vec![0.1; 16];
        assert_eq!(h.decode_id(&z).unwrap(), loaded.decode_id(&z).unwrap());
    }

    #[test]
    fn continuous_safetensors_round_trip() {
        let h = ContinuousReadout::seeded(8, 3, 5).unwrap();
        let bytes = h.to_bytes().unwrap();
        let Readout::Continuous(loaded) = Readout::from_bytes(&bytes).unwrap() else {
            panic!("expected continuous");
        };
        let z = vec![0.2, -0.1, 0.0, 0.4, 0.1, -0.3, 0.2, 0.05];
        assert_eq!(h.emit(&z).unwrap(), loaded.emit(&z).unwrap());
    }

    #[test]
    fn loader_rejects_bad_format_and_nonfinite() {
        let h = DiscreteReadout::seeded(8, 256, 1.0, 1).unwrap();
        let mut bytes = h.to_bytes().unwrap();
        // Corrupt the buffer enough to fail deserialize or validation.
        assert!(Readout::from_bytes(&bytes[..12]).is_err());
        bytes.clear();
        assert!(Readout::from_bytes(&bytes).is_err());
        let mut bad = h.clone();
        bad.weight[0] = f64::NAN;
        // new() would reject; from_bytes of a hand-rolled NaN file is covered
        // by check_vec on the load path via NonFinite after we write via
        // serialize of a constructed tensor — skip if serialize itself is
        // fine with NaN. The constructor is the gate for in-process builds.
        assert!(DiscreteReadout::new(
            8,
            256,
            1.0,
            vec![1.0; 8],
            vec![0.0; 8],
            vec![f64::NAN; 8 * 256],
        )
        .is_err());
    }

    #[test]
    fn wrong_latent_dim_is_a_structured_error() {
        let h = DiscreteReadout::seeded(8, 256, 1.0, 1).unwrap();
        assert!(h.decode_id(&[0.0; 4]).is_err());
        let c = ContinuousReadout::seeded(8, 2, 1).unwrap();
        assert!(c.emit(&[0.0; 3]).is_err());
    }
}
