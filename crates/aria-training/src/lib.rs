//! aria-training — the native Rust training engine for the Aria transformer.
//!
//! Milestone M1 of the Aria program ladder (ARIA-TRAINING-PRD v7.3, WS-A1).
//! This crate converts admissible corpora into `aria-predictor-v1/v2` weight
//! artifacts by minimizing the 4-term hybrid objective on the probability
//! simplex Δ³ (ℙ6):
//!
//! ```text
//! ℒ_total = λ_JEPA·ℒ_JEPA + λ_NLL·ℒ_NLL + λ_Spectral·ℒ_Spectral + λ_Graph·ℒ_Graph
//! ```
//!
//! Invariant preservation is by construction, not by trust (Inductive Safety
//! Theorem, training PRD): after **every** optimizer step the parameters are
//! hard-projected back into the contractivity ball — `σ_max(W_P) ≤ ε/2` (ℙ2,
//! Inv2) and `σ_max(W_I) ≤ 1.0` (𝔸2) — via the same seeded power iteration the
//! runtime loader enforces (`aria-backends::spectral`). Readout gradients are
//! isolated from Φ parameters (𝔸5, 𝕃5): `∂Φ/∂y = 0` structurally.
//!
//! Everything is deterministic f64: seeded LCG only (no OS entropy), exact
//! analytic gradients for the linear maps (no autograd dependency), stable
//! iteration orders (no hash-map iteration touches results).
//!
//! WS-A1 scope (user decisions D1–D4, CHANGELOG 2026-08-17): serde dataset
//! path, 4-term loss, AdamW with embedded projection, seeded training loop,
//! ConceptNet edge substrate for ℒ_Graph. RankMe / Wilcoxon gates, checkpoint
//! v2 provenance, CLI `aria train`, and `aria.train(cfg)` land in WS-A2.

pub mod checkpoint;
pub mod collapse;
pub mod dataset;
pub mod eval;
pub mod ingest;
pub mod linalg;
pub mod loss;
pub mod optimizer;
pub mod sha256;
pub mod train;

use std::path::PathBuf;

use aria_engine_backends::spectral::{SpectralError, SpectralReport};
use aria_engine_backends::trained::WeightsError;
use aria_engine_core::config::{AriaConfig, LossLambdas};
use serde::{Deserialize, Serialize};

pub use checkpoint::Provenance;
pub use collapse::{check_rankme, rankme, DEFAULT_MIN_RANKME_FRAC};
pub use dataset::{EdgeAlignment, KgEdges, PreparedDataset};
pub use eval::{wilcoxon_paired, WilcoxonReport, MIN_TRAJECTORIES};
pub use ingest::{
    assert_columnar_roundtrip, crate_repo_root, ingest_columnar, ingest_with_hash,
    live_docs_corpus, COLUMNAR_MAGIC,
};
pub use loss::{LossBreakdown, ModelParams, ReadoutParams, SpectralEval};
pub use optimizer::{project_to_estimator_ball, AdamW, AdamWParams};
pub use sha256::sha256_hex;
pub use train::train;

/// Failure modes of the training engine. Reject-with-detail, never clamp.
#[derive(Debug, thiserror::Error)]
pub enum TrainingError {
    #[error("config: {0}")]
    Config(String),
    #[error("dataset: {0}")]
    Dataset(String),
    /// Representation-collapse breach — the RankMe gate fired (PRD: the
    /// training engine halts and reports; this is a hard abort, not advice).
    #[error("collapse: {0}")]
    Collapse(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("spectral: {0}")]
    Spectral(#[from] SpectralError),
    #[error("weights: {0}")]
    Weights(#[from] WeightsError),
}

/// Configuration for one training run (`aria train` / `aria.train(cfg)` carry
/// exactly these fields from WS-A2 onward).
///
/// Δ³ and 𝒮 validation is **delegated to aria-core**: [`TrainingConfig::validate`]
/// builds the mirror [`AriaConfig`] and runs its `validate()`, so the simplex
/// rule (λᵢ ≥ 0, Σλᵢ = 1 within 1e-9) and the dimension bounds
/// (N ∈ {2^k : k ∈ [4,14]}, 8 ≤ d ≤ 2N, τ ∈ (0,1], 256 ≤ |V_o| ≤ 128000) are
/// the engine's own rules, not a re-implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// Dataset artifact (`aria-text-dataset-v1` JSON). `aria-optical-dataset-v1`
    /// is refused: the admission table marks it smoke-tests-only, never training data.
    pub data_path: PathBuf,
    /// Raw corpus bytes the dataset was generated from — required iff
    /// `edges_path` is set (window→concept alignment re-derives the exact
    /// window boundaries from these bytes).
    #[serde(default)]
    pub corpus_path: Option<PathBuf>,
    /// Typed knowledge-graph edge list (`aria-kg-edges-v1` JSON; amendment D3):
    /// the ℒ_Graph substrate for smoke/regression runs and unit tests.
    #[serde(default)]
    pub edges_path: Option<PathBuf>,
    /// Window stride used when the dataset was generated (`aria dataset --stride`).
    /// Only consumed by edge alignment; a mismatch is detected and rejected.
    #[serde(default = "default_stride")]
    pub stride: usize,
    /// Number of optical modes N (window size in bytes).
    pub n_modes: usize,
    /// Latent dimension d = dim(𝒵).
    pub latent_dim: usize,
    /// Contractivity tolerance ε (Inv2). The enforced Lipschitz bound is ε/2.
    #[serde(default = "default_eps")]
    pub eps: f64,
    /// Loss weights λ ∈ Δ³ (aria-core type; ℙ6).
    #[serde(default = "default_lambdas")]
    pub lambdas: LossLambdas,
    /// Discrete readout vocabulary |V_o| (only consumed by ℒ_NLL).
    #[serde(default = "default_vocab_size")]
    pub vocab_size: usize,
    /// Training epochs (full passes over the train split). Default is the
    /// gate-optimal receipted protocol (measured 2026-08-17: RankMe decays
    /// monotonically with epochs — 18.15@10, 13.78@15, 10.25@30, 8.19@50,
    /// 5.80@600 on the docs corpus — while the persistence ratio stays
    /// ≈ 2.6–2.9×; 15 epochs maximizes gate margin at near-best quality.
    /// The 600-epoch WS-A1 protocol predates the RankMe gate.)
    #[serde(default = "default_epochs")]
    pub epochs: usize,
    /// Trajectories per batch (receipted protocol default: 32).
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// AdamW hyper-parameters (D1).
    #[serde(default)]
    pub adamw: AdamWParams,
    /// Seed for every stochastic choice (init, epoch shuffles). No OS entropy.
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// Fraction of trajectories held out from the END of the corpus
    /// (WS5 time-split protocol; the statistical referee split).
    #[serde(default = "default_holdout_frac")]
    pub holdout_frac: f64,
    /// Frames per trajectory (WS5: 8).
    #[serde(default = "default_trajectory_len")]
    pub trajectory_len: usize,
    /// Target merge distance γ for ℒ_Graph — the Match policy's τ.
    #[serde(default = "default_gamma_tau")]
    pub gamma_tau: f64,
    /// RankMe collapse-gate fraction α (PRD default 0.30): training halts if
    /// RankMe(holdout Z) < α·d.
    #[serde(default = "default_min_rankme_frac")]
    pub min_rankme_frac: f64,
    /// Escape hatch mirroring `AriaConfig::allow_sub_spec_dims` — tests only.
    #[serde(default)]
    pub allow_sub_spec_dims: bool,
    /// Where to write the `aria-predictor-v1` JSON checkpoint (debug-grade).
    #[serde(default)]
    pub output_path: Option<PathBuf>,
    /// Where to write the `aria-predictor-v2` safetensors checkpoint with
    /// embedded `prov.*` provenance (the bit-exact production artifact).
    #[serde(default)]
    pub output_v2_path: Option<PathBuf>,
    /// Where to write the `aria-readout-v1` head trained by the decoupled
    /// pass (D7). Requires `corpus_path` (next-window byte targets).
    #[serde(default)]
    pub readout_out: Option<PathBuf>,
}

fn default_stride() -> usize {
    64
}
fn default_eps() -> f64 {
    1.0
}
fn default_lambdas() -> LossLambdas {
    // The PRD Workflow-A canonical simplex (D2): JEPA 0.70, NLL 0, Spectral
    // 0.15, Graph 0.15. All four terms are implemented and tested; NLL joins
    // the training loop when WS-A2 wires readout targets.
    LossLambdas {
        jepa: 0.70,
        nll: 0.0,
        spectral: 0.15,
        graph: 0.15,
    }
}
fn default_vocab_size() -> usize {
    256
}
fn default_epochs() -> usize {
    15
}
fn default_batch_size() -> usize {
    32
}
fn default_seed() -> u64 {
    42
}
fn default_holdout_frac() -> f64 {
    0.4
}
fn default_trajectory_len() -> usize {
    8
}
fn default_gamma_tau() -> f64 {
    0.5
}
fn default_min_rankme_frac() -> f64 {
    collapse::DEFAULT_MIN_RANKME_FRAC
}

impl TrainingConfig {
    /// Validate against the 𝒮 hard bounds and the training-only domain.
    ///
    /// The Δ³ / dimension / τ / vocabulary clauses run inside
    /// [`AriaConfig::validate`] on a mirror config — one source of truth.
    pub fn validate(&self) -> Result<(), TrainingError> {
        let mirror = AriaConfig {
            n_modes: self.n_modes,
            latent_dim: self.latent_dim,
            eps: self.eps,
            merge_tau: self.gamma_tau,
            loss_lambdas: self.lambdas.clone(),
            vocab_size: self.vocab_size,
            allow_sub_spec_dims: self.allow_sub_spec_dims,
            seed: Some(self.seed),
            ..AriaConfig::default()
        };
        mirror
            .validate()
            .map_err(|e| TrainingError::Config(e.to_string()))?;

        if !self.eps.is_finite() || self.eps <= 0.0 {
            return Err(TrainingError::Config(format!(
                "eps = {} must be finite and > 0 (Inv2 tolerance; bound = eps/2)",
                self.eps
            )));
        }
        if self.epochs == 0 {
            return Err(TrainingError::Config("epochs must be ≥ 1".into()));
        }
        if self.batch_size == 0 {
            return Err(TrainingError::Config("batch_size must be ≥ 1".into()));
        }
        if self.trajectory_len < 2 {
            return Err(TrainingError::Config(format!(
                "trajectory_len = {} must be ≥ 2 (a trajectory needs at least one transition)",
                self.trajectory_len
            )));
        }
        if self.stride == 0 {
            return Err(TrainingError::Config("stride must be ≥ 1".into()));
        }
        if !self.holdout_frac.is_finite() || self.holdout_frac <= 0.0 || self.holdout_frac >= 1.0 {
            return Err(TrainingError::Config(format!(
                "holdout_frac = {} must lie in (0, 1) — the WS5 time-split referee needs both splits non-empty",
                self.holdout_frac
            )));
        }
        self.adamw.validate().map_err(TrainingError::Config)?;
        if !self.min_rankme_frac.is_finite()
            || self.min_rankme_frac <= 0.0
            || self.min_rankme_frac > 1.0
        {
            return Err(TrainingError::Config(format!(
                "min_rankme_frac = {} must lie in (0, 1] (PRD default 0.30)",
                self.min_rankme_frac
            )));
        }
        if self.edges_path.is_some() && self.corpus_path.is_none() {
            return Err(TrainingError::Config(
                "edges_path is set but corpus_path is not: window→concept alignment \
                 re-derives window boundaries from the raw corpus bytes"
                    .into(),
            ));
        }
        if self.readout_out.is_some() && self.corpus_path.is_none() {
            return Err(TrainingError::Config(
                "readout_out is set but corpus_path is not: the decoupled readout pass \
                 takes next-window byte targets from the raw corpus bytes (D7)"
                    .into(),
            ));
        }
        Ok(())
    }

    /// The enforced Lipschitz bound for the predictor matrices: ε/2 (ℙ2).
    pub fn lipschitz_bound(&self) -> f64 {
        self.eps / 2.0
    }
}

/// Per-epoch and final measurements of one training run — everything the
/// smoke receipt archives. Claims only what WS-A1 measures: loss descent,
/// holdout-vs-persistence direction, σ-audit, coverage, determinism inputs.
/// RankMe and Wilcoxon numbers are deliberately absent (WS-A2 gates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainOutcome {
    /// Format tag of this record.
    pub format: String,
    /// Seed the run used (init + epoch shuffles).
    pub seed: u64,
    /// Epochs actually run.
    pub epochs_run: usize,
    /// Mean loss breakdown per epoch, in order.
    pub epoch_loss: Vec<LossBreakdown>,
    /// Mean total loss of the final epoch.
    pub final_loss: f64,
    /// Residual metric definition, recorded so the receipt is self-describing.
    pub residual_metric: String,
    /// Holdout residual of the untrained (seeded) model, before epoch 1.
    pub holdout_residual_initial: f64,
    /// Holdout residual after the final epoch.
    pub holdout_residual_final: f64,
    /// Persistence baseline (z'_{t+1} = z_t) on the same holdout transitions,
    /// under the final embedding.
    pub persistence_residual: f64,
    /// σ_max audit of the exported weights (embed + three conditioned slots).
    pub sigma: SpectralReport,
    /// The enforced bound ε/2 the σ values must sit under.
    pub lipschitz_bound: f64,
    /// Per-dimension variance of holdout latents under the final embedding —
    /// collapse telemetry (the RankMe gate itself is WS-A2).
    pub latent_variance: Vec<f64>,
    /// Mean of `latent_variance`.
    pub latent_variance_mean: f64,
    /// Fraction of corpus windows that aligned to ≥ 1 KG concept (None when
    /// no edge substrate was configured).
    pub edge_coverage: Option<f64>,
    /// Number of fixture edges with both endpoints present somewhere in the
    /// corpus (the usable-edge pool for ℒ_Graph).
    pub usable_edges: Option<usize>,
    /// RankMe of the holdout latents under the final embedding (Garrido 2023).
    /// The run aborts before this outcome exists if the gate fires.
    pub rankme: f64,
    /// The gate RankMe was checked against (min_rankme_frac · d).
    pub rankme_gate: f64,
    /// Paired Wilcoxon certification vs the persistence baseline (None when
    /// the holdout has fewer than [`MIN_TRAJECTORIES`] trajectories — the
    /// gate is then not applicable and nothing is claimed).
    pub wilcoxon: Option<WilcoxonReport>,
    /// PRD promotion gate: Some(p < 0.01 ∧ median improvement > 0), None when
    /// Wilcoxon was not applicable.
    pub gates_pass: Option<bool>,
    /// Decoupled readout pass measurements (None when not configured).
    pub readout: Option<ReadoutOutcome>,
    /// FNV-1a-64 checksum of the dataset artifact bytes.
    pub dataset_checksum: String,
    /// SHA-256 of the dataset artifact bytes (Gate 6).
    pub dataset_sha256: String,
    /// FNV-1a-64 checksum of the edge-list artifact bytes.
    pub edges_checksum: Option<String>,
    /// SHA-256 of the edge-list artifact bytes.
    pub edges_sha256: Option<String>,
    /// FNV-1a-64 checksum of the raw corpus bytes used for alignment.
    pub corpus_checksum: Option<String>,
    /// SHA-256 of the raw corpus bytes.
    pub corpus_sha256: Option<String>,
    /// SHA-256 of the written v2 safetensors artifact (None when not written).
    pub artifact_v2_sha256: Option<String>,
    /// Dataset artifact format tag.
    pub dataset_format: String,
    /// Dataset source string (as recorded by `aria dataset`).
    pub dataset_source: String,
    /// Source corpus size in bytes (as recorded in the dataset artifact).
    pub source_bytes: usize,
    /// Train / holdout split sizes in trajectories.
    pub n_train_trajectories: usize,
    pub n_holdout_trajectories: usize,
    /// Transitions per trajectory (trajectory_len − 1).
    pub transitions_per_trajectory: usize,
}

/// Format tag for [`TrainOutcome`] (v2 = the WS-A2 superset of the WS-A1
/// receipt schema: gates, SHA-256 provenance, readout pass).
pub const TRAIN_OUTCOME_FORMAT: &str = "aria-train-outcome-v2";

/// Measurements of the decoupled readout pass (D7): θ_D trained on frozen
/// train-split latents, evaluated on holdout NLL against the uniform floor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadoutOutcome {
    /// Holdout NLL of the seeded (untrained) head.
    pub holdout_nll_initial: f64,
    /// Holdout NLL after the pass.
    pub holdout_nll_final: f64,
    /// The uniform-distribution floor ln |V_o| the head must beat.
    pub uniform_nll: f64,
    /// Steps taken by the θ_D optimizer.
    pub steps: usize,
}

/// FNV-1a 64-bit checksum, hex-encoded — deterministic provenance fingerprint
/// for WS-A1 receipts. This is integrity bookkeeping, not cryptography; the
/// cryptographic upgrade decision is Q-A2-a (WS-A2 checkpoint provenance).
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The crate's deterministic random stream — the same MMIX LCG constants the
/// repo's spectral start vector and seeded readout use (no OS entropy,
/// wasm-safe arithmetic, bit-identical across platforms). Public because it
/// is the *only* admissible randomness source for anything this crate or its
/// consumers derive from a `TrainingConfig::seed`.
#[derive(Debug, Clone, Copy)]
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    /// Uniform draw in [−1, 1) with 53-bit mantissa exactness.
    pub fn unit(&mut self) -> f64 {
        let x = self.next_u64();
        ((x >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0) - 1.0
    }

    /// Uniform index in `0..n` (n ≥ 1) via 128-bit multiply-shift (unbiased
    /// enough for shuffle purposes and fully deterministic).
    pub fn index(&mut self, n: usize) -> usize {
        debug_assert!(n >= 1);
        let x = self.next_u64();
        usize::try_from((u128::from(x) * (n as u128)) >> 64).expect("index < n ≤ usize::MAX")
    }
}
