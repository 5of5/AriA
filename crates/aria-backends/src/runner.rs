//! Shared reference runner — the single code path behind every surface.
//!
//! The CLI, the Python extension, and the WASM module all call [`run`] so that
//! `Exit₂` (notebook ∧ CLI ∧ browser run the same OPMD schedule) is structural
//! rather than a coincidence of three parallel implementations.

use aria_engine_core::condition::Condition;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::{Engine, GraphBackend, Predictor};
use aria_engine_core::error::AriaError;
use aria_engine_core::gates::GateReport;
use aria_engine_core::graph::Graph;
use aria_engine_core::scheduler::Scheduler;
use aria_engine_core::state::State;
use aria_engine_core::trace::Trace;
use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::optical::RefOptical;
use crate::spectral::SpectralReport;
use crate::trained::TrainedPredictor;
use crate::{SimDiffuser, SimGraphBackend, SimPredictor};

/// The predictor a reference run uses: the Phase 1 stub, or Phase 3 weights.
///
/// Both arms satisfy ℙ2 — [`TrainedPredictor`] enforces its Lipschitz bound at
/// load time — so swapping them changes accuracy, never admissibility.
#[derive(Debug)]
pub enum RefPredictor {
    Sim(SimPredictor),
    Trained(TrainedPredictor),
}

impl Predictor for RefPredictor {
    fn embed(&self, psi: &[Complex64]) -> Vec<f64> {
        match self {
            RefPredictor::Sim(p) => p.embed(psi),
            RefPredictor::Trained(p) => p.embed(psi),
        }
    }

    fn predict(&self, z: &[f64], a: Condition) -> Vec<f64> {
        match self {
            RefPredictor::Sim(p) => p.predict(z, a),
            RefPredictor::Trained(p) => p.predict(z, a),
        }
    }

    fn dist(&self, a: &[f64], b: &[f64]) -> f64 {
        match self {
            RefPredictor::Sim(p) => p.dist(a, b),
            RefPredictor::Trained(p) => p.dist(a, b),
        }
    }
}

/// The reference engine: Spec runner + the four simulated operators.
pub type SimEngine = Engine<RefOptical, RefPredictor, SimGraphBackend, SimDiffuser>;

/// Build the reference engine for a config, with the Phase 1 stub predictor.
pub fn sim_engine(config: AriaConfig) -> SimEngine {
    let predictor = RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim));
    engine_with(config, predictor)
}

/// Build the reference engine with an explicit predictor backend.
///
/// This is the Phase 4 backend-swap seam: nothing else in the engine changes.
pub fn engine_with(config: AriaConfig, predictor: RefPredictor) -> SimEngine {
    let seed = config.seed.unwrap_or(42);
    // Plan WS2: FFT phase-mask kernel for N ≥ 256 (spec §0.2 mandate), the
    // Householder reference below; config.optical overrides either way.
    let optical = RefOptical::for_config(&config, seed);
    let graph_backend = SimGraphBackend::with_merge_tau(config.latent_dim, config.merge_tau);
    let diffuser = SimDiffuser::new(config.latent_dim);
    Engine::new(config, optical, predictor, graph_backend, diffuser)
}

/// Canonical synthetic initial field ψ₀ — a normalized deterministic phase ramp.
///
/// Every surface starts from this field so that traces are byte-comparable
/// across CLI, Python, and WASM for the same config.
pub fn canonical_psi0(n_modes: usize) -> Vec<Complex64> {
    let psi: Vec<Complex64> = (0..n_modes)
        .map(|i| {
            let phase = (i as f64) * 0.12345;
            Complex64::new(libm::cos(phase), libm::sin(phase))
        })
        .collect();
    let norm: f64 = psi
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt();
    psi.into_iter()
        .map(|c| c / Complex64::new(norm, 0.0))
        .collect()
}

/// Build the canonical initial state for a config.
pub fn canonical_init(engine: &SimEngine, condition: Condition) -> Result<State, AriaError> {
    let psi0 = canonical_psi0(engine.config().n_modes);
    engine.init(psi0, Graph::empty(), condition)
}

/// Summary of a completed run — the parity surface shared by every binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Number of scheduler steps executed.
    pub steps: u64,
    /// Final discrete step counter t (advances once per Diffuse).
    pub t: u64,
    /// Final graph size |G| = |V| + |E|.
    pub graph_size: usize,
    /// Final node count |V| — the non-collapsing-seam observable.
    #[serde(default)]
    pub node_count: usize,
    /// Final field energy ‖ψ‖₂.
    pub energy: f64,
    /// Final JEPA residual.
    pub residual: f64,
    /// Action symbol sequence, e.g. "OPMDOPMD…".
    pub action_sequence: String,
    /// Whether Inv1–4 all hold on the final state.
    pub invariants_ok: bool,
    /// Human-readable invariant failures (empty when `invariants_ok`).
    pub failures: Vec<String>,
    /// Optional Inv5–Inv11 operating gates. Empty `enabled` means none ran.
    pub gates: GateReport,
    /// σ_max audit per weight matrix (plan WS1) — present iff the run used a
    /// trained predictor. Under hard projection every value is ≤ its bound:
    /// `embed ≤ 1.0`, conditioned matrices ≤ `lipschitz_bound` (𝕋4).
    #[serde(default)]
    pub spectral_report: Option<SpectralReport>,
}

/// Result of [`run`]: the summary plus the full trace.
pub struct RunOutcome {
    pub summary: RunSummary,
    pub trace: Trace,
    pub state: State,
}

/// Run the reference engine from the canonical initial state.
///
/// This is the one function every surface calls. Invariants are checked after
/// every `apply` inside the engine; the summary reports the final check.
pub fn run(config: AriaConfig, steps: u64) -> Result<RunOutcome, AriaError> {
    let predictor = RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim));
    run_with(config, steps, predictor)
}

/// [`run_with`] starting from a caller-supplied `G₀` (Init, not a new action).
pub fn run_with_graph(
    config: AriaConfig,
    steps: u64,
    predictor: RefPredictor,
    g0: Graph,
) -> Result<RunOutcome, AriaError> {
    config.validate()?;
    validate_config(&config, &predictor)?;

    let condition = config.condition;
    let schedule = config.schedule.clone();
    let stutter_k = config.stutter_k;
    let latent_dim = config.latent_dim;

    let spectral_report = match &predictor {
        RefPredictor::Trained(p) => Some(
            p.spectral_report()
                .map_err(|e| AriaError::Backend(e.to_string()))?,
        ),
        RefPredictor::Sim(_) => None,
    };

    let engine = engine_with(config, predictor);
    if !engine.graph_backend().ok(&g0) {
        return Err(AriaError::Config(format!(
            "seed graph fails GraphOK at latent_dim={latent_dim}"
        )));
    }
    let psi0 = canonical_psi0(engine.config().n_modes);
    let state = engine.init(psi0, g0, condition)?;

    let mut scheduler =
        Scheduler::from_string(&schedule, stutter_k).map_err(AriaError::Schedule)?;

    let (final_state, trace, gates) =
        engine.run_monitored(state, &mut scheduler, steps, condition)?;
    let report = engine.check(&final_state, condition);

    let summary = RunSummary {
        steps,
        t: final_state.t,
        graph_size: final_state.g.size(),
        node_count: final_state.g.node_count(),
        energy: final_state.energy(),
        residual: trace.entries.last().map_or(0.0, |e| e.res),
        action_sequence: trace.action_sequence(),
        invariants_ok: report.all_ok(),
        failures: report.failures(),
        gates,
        spectral_report,
    };

    Ok(RunOutcome {
        summary,
        trace,
        state: final_state,
    })
}

/// Run the reference engine with an explicit predictor backend.
pub fn run_with(
    config: AriaConfig,
    steps: u64,
    predictor: RefPredictor,
) -> Result<RunOutcome, AriaError> {
    run_with_graph(config, steps, predictor, Graph::empty())
}

/// Replay a run and return the post-step latent after every action.
///
/// This is the only seam `aria emit` uses to recover `z`. It does not write
/// `z` into the JSONL (goldens stay byte-stable) and it cannot feed a
/// readout gradient — or a readout output — back into Φ (𝕃5).
pub fn latents_of(config: AriaConfig, steps: u64) -> Result<Vec<Vec<f64>>, AriaError> {
    let predictor = RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim));
    latents_with(config, steps, predictor)
}

/// [`latents_of`] with an explicit predictor (trained replay).
pub fn latents_with(
    config: AriaConfig,
    steps: u64,
    predictor: RefPredictor,
) -> Result<Vec<Vec<f64>>, AriaError> {
    config.validate()?;
    validate_config(&config, &predictor)?;
    let condition = config.condition;
    let schedule = config.schedule.clone();
    let stutter_k = config.stutter_k;
    let engine = engine_with(config, predictor);
    let state = canonical_init(&engine, condition)?;
    let mut scheduler =
        Scheduler::from_string(&schedule, stutter_k).map_err(AriaError::Schedule)?;
    let (_, _, _, latents) =
        engine.run_monitored_with_latents(state, &mut scheduler, steps, condition)?;
    Ok(latents)
}

/// Validate a config before any backend is constructed.
///
/// Every surface funnels through `run_with`, so this is the single place that
/// turns bad dimensions into a `Config` error instead of a backend panic —
/// including inside WASM, where a panic is a hard trap. The 𝒮-domain clauses
/// live in [`AriaConfig::validate`] (called first); what remains here are the
/// backend-specific checks that hold regardless of the spec domain.
pub(crate) fn validate_config(
    config: &AriaConfig,
    predictor: &RefPredictor,
) -> Result<(), AriaError> {
    if !config.eps.is_finite() || config.eps < 0.0 {
        return Err(AriaError::Config(format!(
            "eps must be finite and ≥ 0, got {}",
            config.eps
        )));
    }
    match predictor {
        // The stub's isometry needs latent_dim ≤ 2·n_modes (real dimensions of H).
        RefPredictor::Sim(_) => {
            if config.latent_dim > 2 * config.n_modes {
                return Err(AriaError::Config(format!(
                    "latent_dim {} exceeds 2·n_modes {}; the simulated isometry cannot be isometric",
                    config.latent_dim,
                    config.n_modes
                )));
            }
        }
        // A trained checkpoint fixes the dimensions it was learned for.
        RefPredictor::Trained(p) => {
            if p.n_modes() != config.n_modes || p.latent_dim() != config.latent_dim {
                return Err(AriaError::Config(format!(
                    "checkpoint expects N={}, dim(Z)={} but config has N={}, dim(Z)={}",
                    p.n_modes(),
                    p.latent_dim(),
                    config.n_modes,
                    config.latent_dim
                )));
            }
            let jump = p
                .max_residual_jump(1.0)
                .map_err(|e| AriaError::Backend(e.to_string()))?;
            if config.strict && jump > config.eps {
                return Err(AriaError::Config(format!(
                    "trained predictor needs eps ≥ {:.4} (worst-case Inv2 jump), got eps = {}",
                    jump, config.eps
                )));
            }
        }
    }
    Ok(())
}

/// An optical trajectory dataset for the Phase 3 JEPA training loop.
///
/// Each trajectory is a sequence of field snapshots `ψ₀, U(ψ₀), U²(ψ₀), …`
/// flattened as `[re₀, im₀, re₁, im₁, …]`. The learner fits `P ∘ I` so that
/// `P(I(ψₜ), a) ≈ I(ψₜ₊₁)`: pure latent prediction, no decoder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalDataset {
    pub format: String,
    pub n_modes: usize,
    pub seed: u64,
    /// `[trajectory][snapshot][2·n_modes]`
    pub trajectories: Vec<Vec<Vec<f64>>>,
}

/// Generate trajectories by repeatedly applying the simulated optical operator.
///
/// Initial fields are deterministic phase ramps with per-trajectory offsets, so
/// the dataset is fully reproducible from `(n_modes, seed, count, length)`.
pub fn optical_dataset(n_modes: usize, seed: u64, count: usize, length: usize) -> OpticalDataset {
    use aria_engine_core::engine::OpticalBackend;

    // Q-2026-08-13-5: follow the same backend rule as inference — FFT at
    // N ≥ 256, Householder below. The Wilcoxon quality gate uses real
    // spectral-DFT text (`aria dataset --input`), not this synthetic path.
    let mut cfg = AriaConfig::test_config();
    cfg.n_modes = n_modes;
    cfg.seed = Some(seed);
    let optical = crate::optical::RefOptical::for_config(&cfg, seed);

    let trajectories = (0..count)
        .map(|k| {
            let offset = 0.017 * (k as f64 + 1.0);
            let psi: Vec<Complex64> = (0..n_modes)
                .map(|i| {
                    let phase = (i as f64) * 0.12345 + offset;
                    Complex64::new(libm::cos(phase), libm::sin(phase))
                })
                .collect();
            let norm: f64 = psi
                .iter()
                .map(num_complex::Complex::norm_sqr)
                .sum::<f64>()
                .sqrt();
            let mut psi: Vec<Complex64> = psi
                .into_iter()
                .map(|c| c / Complex64::new(norm, 0.0))
                .collect();

            let mut snapshots = Vec::with_capacity(length);
            for t in 0..length {
                snapshots.push(flatten(&psi));
                psi = optical.unitary_step(t as u64, &psi);
            }
            snapshots
        })
        .collect();

    OpticalDataset {
        format: "aria-optical-dataset-v1".into(),
        n_modes,
        seed,
        trajectories,
    }
}

fn flatten(psi: &[Complex64]) -> Vec<f64> {
    let mut v = Vec::with_capacity(psi.len() * 2);
    for c in psi {
        v.push(c.re);
        v.push(c.im);
    }
    v
}

/// Parse a conditioning name. Accepts the TOML/CLI spellings.
pub fn parse_condition(s: &str) -> Result<Condition, AriaError> {
    match s.to_lowercase().as_str() {
        "token" => Ok(Condition::Token),
        "diffusion" => Ok(Condition::Diffusion),
        "world_model" | "worldmodel" => Ok(Condition::WorldModel),
        other => Err(AriaError::Config(format!("unknown condition '{other}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_psi0_is_normalized() {
        let psi = canonical_psi0(64);
        let norm: f64 = psi
            .iter()
            .map(num_complex::Complex::norm_sqr)
            .sum::<f64>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-12);
    }

    #[test]
    fn run_is_deterministic() {
        let mut config = AriaConfig::test_config();
        config.schedule = "opmd".into();
        let a = run(config.clone(), 40).unwrap();
        let b = run(config, 40).unwrap();
        assert_eq!(a.summary.action_sequence, b.summary.action_sequence);
        assert_eq!(a.summary.t, b.summary.t);
        assert_eq!(a.summary.energy.to_bits(), b.summary.energy.to_bits());
        assert_eq!(a.trace.to_jsonl(), b.trace.to_jsonl());
    }

    #[test]
    fn run_opmd_is_green() {
        let config = AriaConfig::test_config();
        let out = run(config, 100).unwrap();
        assert!(out.summary.invariants_ok, "{:?}", out.summary.failures);
        assert_eq!(out.summary.t, 25);
    }

    #[test]
    fn optical_dataset_is_reproducible_and_energy_preserving() {
        let a = optical_dataset(8, 42, 3, 5);
        let b = optical_dataset(8, 42, 3, 5);
        assert_eq!(a.trajectories, b.trajectories);
        assert_eq!(a.trajectories.len(), 3);
        assert_eq!(a.trajectories[0].len(), 5);
        assert_eq!(a.trajectories[0][0].len(), 16);

        // Unitary evolution keeps every snapshot on the unit sphere (𝔸4).
        for traj in &a.trajectories {
            for snap in traj {
                let norm: f64 = snap.iter().map(|x| x * x).sum::<f64>().sqrt();
                assert!((norm - 1.0).abs() < 1e-9, "‖ψ‖ = {norm}");
            }
        }
    }

    #[test]
    fn parse_condition_accepts_all_three() {
        assert_eq!(parse_condition("token").unwrap(), Condition::Token);
        assert_eq!(parse_condition("diffusion").unwrap(), Condition::Diffusion);
        assert_eq!(
            parse_condition("world_model").unwrap(),
            Condition::WorldModel
        );
        assert!(parse_condition("nope").is_err());
    }
}
