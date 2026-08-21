//! Streaming long-horizon verification — spec §8 / plan WS6.
//!
//! `aria verify` must run T ≥ 10⁵ steps with memory O(1) in T: the in-memory
//! [`Trace`] is not retained. An optional JSONL sink is write-through. The
//! receipt is the evidence artifact (`aria-verify-receipt-v1`).
//!
//! X1–X5 are finite-window restatements of TRACES.md reject shapes. They are
//! pure functions of the action stream (+ residual for X4) so they can be
//! unit-tested without a 10⁵-step run.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use aria_engine_core::action::Action;
use aria_engine_core::condition::Condition;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::GraphBackend;
use aria_engine_core::error::AriaError;
use aria_engine_core::gates::{GateMonitor, GateReport};
use aria_engine_core::graph::Graph;
use aria_engine_core::invariants;
use aria_engine_core::scheduler::Scheduler;
use aria_engine_core::trace::{initial_graph_of, Trace};
use serde::{Deserialize, Serialize};

use crate::growth::{fit_growth_exponent, log_checkpoints};
use crate::runner::{canonical_psi0, engine_with, validate_config, RefPredictor};
use crate::spectral::SpectralReport;

/// On-disk tag for [`VerifyReceipt`].
pub const RECEIPT_FORMAT: &str = "aria-verify-receipt-v1";

/// Finite-window audit configuration (TRACES.md X1–X5).
#[derive(Debug, Clone, Copy)]
pub struct AuditConfig {
    /// Stutter budget K (𝐂5). Default 2.
    pub k: u64,
    /// Optical-starve window W. Default 64.
    pub w: usize,
    /// Uncapped-diffuse run cap. Default 8.
    pub d_cap: usize,
    /// Residual cold threshold (ε).
    pub eps: f64,
}

impl AuditConfig {
    pub fn from_config(config: &AriaConfig) -> Self {
        Self {
            k: config.stutter_k,
            w: 64,
            d_cap: 8,
            eps: config.eps,
        }
    }
}

/// X1–X5 counters plus the emitted family label.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TraceAudit {
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
    pub x5: u64,
    pub family: String,
}

impl TraceAudit {
    pub fn all_zero(&self) -> bool {
        self.x1 == 0 && self.x2 == 0 && self.x3 == 0 && self.x4 == 0 && self.x5 == 0
    }
}

/// Online X1–X5 auditor. Stores at most `W` action symbols.
#[derive(Debug)]
pub struct StreamingAuditor {
    cfg: AuditConfig,
    consecutive_s: u64,
    consecutive_d: u64,
    window: VecDeque<Action>,
    saw_p_since_cold: bool,
    saw_stutter: bool,
    illegal_family: bool,
    phase: FamilyPhase,
    counts: TraceAudit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FamilyPhase {
    ExpectO,
    ExpectP,
    ExpectM,
    ExpectD,
    AfterD,
}

impl StreamingAuditor {
    pub fn new(cfg: AuditConfig) -> Self {
        Self {
            cfg,
            consecutive_s: 0,
            consecutive_d: 0,
            window: VecDeque::with_capacity(cfg.w),
            saw_p_since_cold: false,
            saw_stutter: false,
            illegal_family: false,
            phase: FamilyPhase::ExpectO,
            counts: TraceAudit {
                family: "W1".into(),
                ..TraceAudit::default()
            },
        }
    }

    pub fn observe(&mut self, action: Action, residual: f64) {
        match action {
            Action::Stutter => {
                self.consecutive_s += 1;
                self.consecutive_d = 0;
                self.saw_stutter = true;
            }
            Action::Diffuse => {
                self.consecutive_s = 0;
                self.consecutive_d += 1;
            }
            _ => {
                self.consecutive_s = 0;
                self.consecutive_d = 0;
            }
        }

        // X1 / X5: any run of consecutive S longer than K.
        if self.consecutive_s > self.cfg.k {
            self.counts.x1 += 1;
            self.counts.x5 += 1;
        }
        // X3: uncapped D.
        if self.consecutive_d > self.cfg.d_cap as u64 {
            self.counts.x3 += 1;
        }

        if self.window.len() == self.cfg.w {
            self.window.pop_front();
        }
        self.window.push_back(action);
        if self.window.len() == self.cfg.w && !self.window.iter().any(|a| *a == Action::OpticalStep)
        {
            self.counts.x2 += 1;
        }

        // Residual-cold: Res ≤ ε. A new hot period needs a Predict before Match.
        if residual <= self.cfg.eps {
            self.saw_p_since_cold = false;
        }
        if action == Action::Predict {
            self.saw_p_since_cold = true;
        }
        if action == Action::Match && residual > self.cfg.eps && !self.saw_p_since_cold {
            self.counts.x4 += 1;
        }

        self.step_family(action);
    }

    fn step_family(&mut self, action: Action) {
        use FamilyPhase::{AfterD, ExpectD, ExpectM, ExpectO, ExpectP};
        match (self.phase, action) {
            (ExpectP, Action::Predict) => self.phase = ExpectM,
            (ExpectM, Action::Match) => self.phase = ExpectD,
            (ExpectD, Action::Diffuse) => self.phase = AfterD,
            (ExpectO | AfterD, Action::OpticalStep) => self.phase = ExpectP,
            (AfterD, Action::Stutter) => {
                if self.consecutive_s > self.cfg.k {
                    self.illegal_family = true;
                }
            }
            _ => self.illegal_family = true,
        }
    }

    pub fn finish(mut self) -> TraceAudit {
        self.counts.family = if self.illegal_family {
            "other".into()
        } else if self.saw_stutter {
            "W2".into()
        } else {
            "W1".into()
        };
        self.counts
    }
}

/// Audit a complete action/residual stream (unit-test seam).
pub fn audit_stream(actions: &[Action], residuals: &[f64], cfg: AuditConfig) -> TraceAudit {
    let mut auditor = StreamingAuditor::new(cfg);
    for (action, res) in actions.iter().zip(residuals) {
        auditor.observe(*action, *res);
    }
    auditor.finish()
}

/// Spec §8 / plan WS6 receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReceipt {
    pub format: String,
    pub git_rev: String,
    pub config_hash: String,
    pub config: serde_json::Value,
    pub steps: u64,
    pub schedule: String,
    pub condition: String,
    pub inv1_max_drift: f64,
    pub inv2_violations: u64,
    pub inv3_violations: u64,
    pub inv4_violations: u64,
    pub sigma_max_audit: Option<SpectralReport>,
    pub graph: VerifyGraph,
    pub trace_audit: TraceAudit,
    pub wall_clock_s: f64,
    pub steps_per_s: f64,
    pub gates: GateReport,
    pub invariants_ok: bool,
    pub predictor: String,
    pub beta_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyGraph {
    pub final_nodes: usize,
    pub final_edges: usize,
    pub measured_beta: f64,
    pub beta_r2: f64,
}

/// Options for [`verify`].
pub struct VerifyOpts {
    pub config: AriaConfig,
    pub steps: u64,
    pub predictor: RefPredictor,
    pub g0: Graph,
    pub audit: AuditConfig,
    pub trace_path: Option<PathBuf>,
    pub receipt_path: Option<PathBuf>,
}

/// Stream T steps, write optional JSONL, return a receipt. Memory is O(W + |G|).
#[allow(clippy::too_many_lines)]
pub fn verify(opts: VerifyOpts) -> Result<VerifyReceipt, AriaError> {
    let VerifyOpts {
        config,
        steps,
        predictor,
        g0,
        audit,
        trace_path,
        receipt_path,
    } = opts;

    config.validate()?;
    validate_config(&config, &predictor)?;

    let condition = config.condition;
    let schedule = config.schedule.clone();
    let stutter_k = config.stutter_k;
    let n_modes = config.n_modes;
    let latent_dim = config.latent_dim;
    let eps = config.eps;
    let eps_energy = config.eps_energy;
    let condition_label = format!("{condition:?}").to_lowercase();
    let predictor_kind = match &predictor {
        RefPredictor::Sim(_) => "sim".to_string(),
        RefPredictor::Trained(_) => "trained".to_string(),
    };
    let strict = config.strict;

    let spectral_report = match &predictor {
        RefPredictor::Trained(p) => Some(
            p.spectral_report()
                .map_err(|e| AriaError::Backend(e.to_string()))?,
        ),
        RefPredictor::Sim(_) => None,
    };

    let engine = engine_with(config.clone(), predictor);
    if !engine.graph_backend().ok(&g0) {
        return Err(AriaError::Config(format!(
            "seed graph fails GraphOK at latent_dim={latent_dim}"
        )));
    }
    let psi0 = canonical_psi0(n_modes);
    let mut state = engine.init(psi0, g0, condition)?;
    let mut scheduler =
        Scheduler::from_string(&schedule, stutter_k).map_err(AriaError::Schedule)?;

    let mut sink = match trace_path {
        Some(ref path) => Some(open_trace_sink(
            path,
            &config,
            &schedule,
            condition,
            // `state.g` is still `G₀` here — Init stores it untouched and the
            // loop has not run yet.
            initial_graph_of(&state.g),
        )?),
        None => None,
    };

    let mut auditor = StreamingAuditor::new(audit);
    let mut monitor = GateMonitor::new(config.gates.clone());
    let checkpoints = log_checkpoints(steps);
    let mut samples: Vec<(u64, usize)> = Vec::new();
    let mut inv1_max_drift = 0.0_f64;
    let mut inv2 = 0u64;
    let mut inv3 = 0u64;
    let mut inv4 = 0u64;
    let mut steps_done = 0u64;

    let t0 = Instant::now();
    for step in 1..=steps {
        let action = scheduler.next_action_budgeted();
        let t_before = state.t;
        state = engine.apply(state, action, condition)?;

        let residual = engine.residual(&state, condition);
        let energy = state.energy();
        let drift = (energy - state.energy_0).abs();
        if drift > inv1_max_drift {
            inv1_max_drift = drift;
        }

        let report = invariants::check_all(&state, residual, eps, eps_energy, n_modes, latent_dim);
        if !report.inv2_ok {
            inv2 += 1;
        }
        if !report.inv3_ok {
            inv3 += 1;
        }
        if !report.inv4_ok {
            inv4 += 1;
        }

        auditor.observe(action, residual);
        monitor.observe(action, &state, residual, eps);
        if let Some(ref mut writer) = sink {
            write_entry(
                writer,
                t_before,
                action,
                residual,
                energy,
                state.g.size(),
                &condition_label,
            )?;
        }
        if checkpoints.binary_search(&step).is_ok() {
            samples.push((step, state.g.node_count()));
        }
        steps_done = step;

        if strict && !report.all_ok() {
            break;
        }
    }
    let wall = t0.elapsed().as_secs_f64();
    if let Some(ref mut writer) = sink {
        writer
            .flush()
            .map_err(|e| AriaError::Backend(e.to_string()))?;
    }

    let final_report = engine.check(&state, condition);
    let trace_audit = auditor.finish();
    let gates = monitor.finish();
    let (beta, r2, beta_note) = match fit_growth_exponent(&samples) {
        Some(fit) => {
            let note = if fit.beta.abs() < 1e-9 {
                Some(format!(
                    "β ≈ 0: |V| saturated under {predictor_kind} + merge \
                     (final |V| = {}). Inside 𝕃3 (β ≤ 1). Not evidence of a \
                     non-collapsing online latent (Q-2026-08-13-7).",
                    state.g.node_count()
                ))
            } else {
                None
            };
            (fit.beta, fit.r_squared, note)
        }
        None => (
            f64::NAN,
            f64::NAN,
            Some("not enough checkpoints to fit β".into()),
        ),
    };

    let receipt = VerifyReceipt {
        format: RECEIPT_FORMAT.into(),
        git_rev: git_rev(),
        config_hash: config_hash(&config),
        config: serde_json::to_value(&config).unwrap_or(serde_json::Value::Null),
        steps: steps_done,
        schedule,
        condition: condition_label,
        inv1_max_drift,
        inv2_violations: inv2,
        inv3_violations: inv3,
        inv4_violations: inv4,
        sigma_max_audit: spectral_report,
        graph: VerifyGraph {
            final_nodes: state.g.node_count(),
            final_edges: state.g.edge_count(),
            measured_beta: beta,
            beta_r2: r2,
        },
        trace_audit,
        wall_clock_s: wall,
        steps_per_s: if wall > 0.0 {
            steps_done as f64 / wall
        } else {
            0.0
        },
        gates,
        invariants_ok: final_report.all_ok() && inv2 == 0 && inv3 == 0 && inv4 == 0,
        predictor: predictor_kind,
        beta_note,
    };

    if let Some(path) = receipt_path {
        let json = serde_json::to_string_pretty(&receipt)
            .map_err(|e| AriaError::Backend(e.to_string()))?;
        std::fs::write(&path, json).map_err(|e| AriaError::Backend(e.to_string()))?;
    }

    Ok(receipt)
}

fn open_trace_sink(
    path: &Path,
    config: &AriaConfig,
    schedule: &str,
    condition: Condition,
    initial_graph: Option<Graph>,
) -> Result<BufWriter<File>, AriaError> {
    let file = File::create(path).map_err(|e| AriaError::Backend(e.to_string()))?;
    let mut writer = BufWriter::new(file);
    let header = Trace::new(
        config.n_modes,
        config.latent_dim,
        config.eps,
        config.seed,
        schedule,
        condition,
        config.match_policy,
        config.diff_policy,
        config.stutter_k,
        config.optical.clone(),
        config.merge_tau,
        initial_graph,
    );
    writer
        .write_all(header.to_jsonl().lines().next().unwrap_or("").as_bytes())
        .map_err(|e| AriaError::Backend(e.to_string()))?;
    writer
        .write_all(b"\n")
        .map_err(|e| AriaError::Backend(e.to_string()))?;
    Ok(writer)
}

fn write_entry(
    writer: &mut BufWriter<File>,
    t: u64,
    action: Action,
    residual: f64,
    energy: f64,
    graph_size: usize,
    condition: &str,
) -> Result<(), AriaError> {
    let line = serde_json::json!({
        "t": t,
        "action": action.symbol(),
        "res": residual,
        "energy": energy,
        "graph_size": graph_size,
        "condition": condition,
    });
    writeln!(writer, "{line}").map_err(|e| AriaError::Backend(e.to_string()))
}

fn git_rev() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn config_hash(config: &AriaConfig) -> String {
    let src = serde_json::to_string(config).unwrap_or_default();
    // FNV-1a 64 — deterministic across processes; not a cryptographic claim.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for b in src.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predictor::SimPredictor;
    use aria_engine_core::policy::MatchPolicy;

    fn cfg() -> AuditConfig {
        AuditConfig {
            k: 2,
            w: 8,
            d_cap: 3,
            eps: 1.0,
        }
    }

    fn opmd_n(n: usize) -> (Vec<Action>, Vec<f64>) {
        let mut actions = Vec::new();
        let mut res = Vec::new();
        for _ in 0..n {
            actions.extend([
                Action::OpticalStep,
                Action::Predict,
                Action::Match,
                Action::Diffuse,
            ]);
            res.extend([0.0, 0.0, 0.0, 0.0]);
        }
        (actions, res)
    }

    #[test]
    fn default_opmd_is_w1_and_x_clean() {
        let (actions, res) = opmd_n(20);
        let audit = audit_stream(&actions, &res, cfg());
        assert!(audit.all_zero(), "{audit:?}");
        assert_eq!(audit.family, "W1");
    }

    #[test]
    fn two_stutters_between_cycles_is_w2() {
        let mut actions = vec![
            Action::OpticalStep,
            Action::Predict,
            Action::Match,
            Action::Diffuse,
            Action::Stutter,
            Action::Stutter,
            Action::OpticalStep,
            Action::Predict,
            Action::Match,
            Action::Diffuse,
        ];
        let res = vec![0.0; actions.len()];
        // pad so the window fills without starving O
        actions.extend([
            Action::OpticalStep,
            Action::Predict,
            Action::Match,
            Action::Diffuse,
        ]);
        let mut res = res;
        res.extend([0.0; 4]);
        let audit = audit_stream(&actions, &res, cfg());
        assert_eq!(audit.x1, 0);
        assert_eq!(audit.x5, 0);
        assert_eq!(audit.family, "W2");
    }

    #[test]
    fn three_stutters_trips_x1_and_x5() {
        let actions = vec![Action::Stutter, Action::Stutter, Action::Stutter];
        let res = vec![0.0; 3];
        let audit = audit_stream(&actions, &res, cfg());
        assert!(audit.x1 >= 1);
        assert!(audit.x5 >= 1);
    }

    #[test]
    fn a_window_without_optical_trips_x2() {
        let actions = vec![Action::Predict; 8];
        let res = vec![0.0; 8];
        let audit = audit_stream(&actions, &res, cfg());
        assert!(audit.x2 >= 1);
    }

    #[test]
    fn four_diffuses_trips_x3_when_cap_is_three() {
        let actions = vec![Action::Diffuse; 4];
        let res = vec![0.0; 4];
        let audit = audit_stream(&actions, &res, cfg());
        assert!(audit.x3 >= 1);
    }

    #[test]
    fn hot_match_without_predict_trips_x4() {
        let actions = vec![Action::OpticalStep, Action::Match];
        let res = vec![2.0, 2.0];
        let audit = audit_stream(&actions, &res, cfg());
        assert_eq!(audit.x4, 1);
    }

    #[test]
    fn predict_before_hot_match_is_clean() {
        let actions = vec![Action::OpticalStep, Action::Predict, Action::Match];
        let res = vec![2.0, 2.0, 2.0];
        let audit = audit_stream(&actions, &res, cfg());
        assert_eq!(audit.x4, 0);
    }

    #[test]
    fn streaming_verify_128_opmd_is_inv_green() {
        let mut config = AriaConfig::test_config();
        config.match_policy = MatchPolicy::Merge;
        config.schedule = "opmd".into();
        let predictor = RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim));
        let receipt = verify(VerifyOpts {
            audit: AuditConfig::from_config(&config),
            config,
            steps: 128,
            predictor,
            g0: Graph::empty(),
            trace_path: None,
            receipt_path: None,
        })
        .unwrap();
        assert!(receipt.invariants_ok, "{receipt:?}");
        assert_eq!(receipt.steps, 128);
        assert_eq!(receipt.inv2_violations, 0);
        assert_eq!(receipt.inv3_violations, 0);
        assert_eq!(receipt.inv4_violations, 0);
        assert!(
            receipt.inv1_max_drift < 1e-7,
            "drift {}",
            receipt.inv1_max_drift
        );
        assert!(receipt.trace_audit.all_zero(), "{:?}", receipt.trace_audit);
        assert_eq!(receipt.trace_audit.family, "W1");
        assert!(receipt.graph.measured_beta.is_finite());
        assert!(receipt.graph.measured_beta <= 1.0);
        assert_eq!(receipt.format, RECEIPT_FORMAT);
    }
}
