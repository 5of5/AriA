//! Weight-artifact serialization with cryptographic provenance
//! (ARIA-TRAINING-PRD §Quality Gates, Gate 6): `aria-predictor-v1` (JSON,
//! debug-grade — reloads within ~2 ulp), `aria-predictor-v2` (safetensors,
//! bit-exact raw LE bytes) with embedded provenance metadata, and the
//! `aria-readout-v1` head artifact.
//!
//! Provenance keys are namespaced `prov.*` so they can never collide with the
//! loader's reserved keys (`format`, `n_modes`, `latent_dim`,
//! `lipschitz_bound`); `TrainedPredictor::from_safetensors` reads only the
//! reserved keys, so provenance is structurally passive — it can be recovered
//! with `SafeTensors::read_metadata` but never influences behavior.

use std::collections::HashMap;
use std::path::Path;

use aria_engine_backends::trained::PredictorWeights;
use aria_engine_core::config::LossLambdas;
use serde::{Deserialize, Serialize};

use crate::TrainingError;

/// Everything Gate 6 requires a checkpoint to carry. `map_revision` and
/// `ccv_hash` are `None` until WS-A3's spine exports supply them — absent,
/// not fabricated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub git_sha: String,
    pub crate_version: String,
    pub seed: u64,
    pub dataset_sha256: String,
    pub corpus_sha256: Option<String>,
    pub edges_sha256: Option<String>,
    pub lambdas: LossLambdas,
    /// Human-readable protocol line, e.g.
    /// `n_modes=256 d=32 stride=64 L=8 holdout=0.4 epochs=15 lr=0.003`.
    pub protocol: String,
    pub map_revision: Option<String>,
    pub ccv_hash: Option<String>,
}

impl Provenance {
    /// The `prov.*` metadata map embedded in v2 safetensors artifacts.
    pub fn to_metadata(&self) -> Result<HashMap<String, String>, TrainingError> {
        let mut m = HashMap::new();
        m.insert("prov.git_sha".into(), self.git_sha.clone());
        m.insert("prov.crate_version".into(), self.crate_version.clone());
        m.insert("prov.seed".into(), self.seed.to_string());
        m.insert("prov.dataset_sha256".into(), self.dataset_sha256.clone());
        if let Some(c) = &self.corpus_sha256 {
            m.insert("prov.corpus_sha256".into(), c.clone());
        }
        if let Some(e) = &self.edges_sha256 {
            m.insert("prov.edges_sha256".into(), e.clone());
        }
        m.insert("prov.lambdas".into(), serde_json::to_string(&self.lambdas)?);
        m.insert("prov.protocol".into(), self.protocol.clone());
        if let Some(r) = &self.map_revision {
            m.insert("prov.map_revision".into(), r.clone());
        }
        if let Some(h) = &self.ccv_hash {
            m.insert("prov.ccv_hash".into(), h.clone());
        }
        Ok(m)
    }
}

/// Write the v1 JSON checkpoint (debug-grade precision; see the roundtrip
/// test for the measured ~2 ulp parse drift — v2 is the bit-exact format).
pub fn write_v1(path: &Path, weights: &PredictorWeights) -> Result<(), TrainingError> {
    std::fs::write(path, serde_json::to_string_pretty(weights)?)?;
    Ok(())
}

/// Write the v2 safetensors checkpoint with embedded provenance. Returns the
/// serialized bytes' SHA-256 (the artifact's own fingerprint, for receipts).
///
/// The bytes are canonicalized before writing: `safetensors` serializes its
/// header maps in `HashMap` iteration order, which is nondeterministic across
/// instances (measured: two identical runs produced different file hashes).
/// A canonical artifact re-emits the header with sorted keys so equal weights
/// + equal provenance ⇒ bit-identical files, on every platform, every run.
pub fn write_v2(
    path: &Path,
    weights: &PredictorWeights,
    provenance: &Provenance,
) -> Result<String, TrainingError> {
    let meta = provenance.to_metadata()?;
    let bytes = canonicalize_safetensors(&weights.to_safetensors_with_metadata(&meta)?)?;
    std::fs::write(path, &bytes)?;
    Ok(crate::sha256::sha256_hex(&bytes))
}

/// Rewrite a safetensors buffer with a canonical (sorted-key, compact) header.
/// Tensor offsets are relative to the data section, so re-emitting the header
/// never touches them; the result stays a valid safetensors file.
fn canonicalize_safetensors(bytes: &[u8]) -> Result<Vec<u8>, TrainingError> {
    if bytes.len() < 8 {
        return Err(TrainingError::Dataset(
            "safetensors buffer too short".into(),
        ));
    }
    let header_len = usize::try_from(u64::from_le_bytes(
        bytes[0..8].try_into().expect("8-byte prefix"),
    ))
    .map_err(|_| TrainingError::Dataset("safetensors header length overflow".into()))?;
    let header_end = 8usize
        .checked_add(header_len)
        .filter(|&e| e <= bytes.len())
        .ok_or_else(|| TrainingError::Dataset("safetensors header out of bounds".into()))?;
    // serde_json's default Map is a BTreeMap: parsing and re-emitting sorts
    // every object's keys deterministically (incl. __metadata__).
    let header: serde_json::Value = serde_json::from_slice(&bytes[8..header_end])?;
    let canonical = serde_json::to_vec(&header)?;
    let mut out = Vec::with_capacity(8 + canonical.len() + (bytes.len() - header_end));
    out.extend_from_slice(&(canonical.len() as u64).to_le_bytes());
    out.extend_from_slice(&canonical);
    out.extend_from_slice(&bytes[header_end..]);
    Ok(out)
}
