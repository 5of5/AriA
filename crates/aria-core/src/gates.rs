//! Optional operating gates Inv5–Inv11 — Phase 4, configurable, never Spec.
//!
//! FORMAL_SPEC §2.6 lists Inv5–Inv11 as *documentation candidates*. The primary
//! inductive set stays Inv1–Inv4 and nothing here may enlarge it. These gates
//! are therefore:
//!
//! * **off by default** — a run with no `gates` configured behaves exactly as
//!   it did in Phase 1;
//! * **observers, not transitions** — a monitor watches the action stream the
//!   scheduler and engine already produce and never proposes or blocks a step;
//! * **window properties** — several (Inv6, Inv8, Inv11) are only meaningful
//!   over a horizon, so they are evaluated across a sliding window rather than
//!   pointwise;
//! * **separately reported** — a gate breach is a [`GateReport`] entry, not an
//!   `InvViolation`. Only Inv1–4 can abort an `apply`.

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::state::State;

/// One optional operating gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Gate {
    /// Inv5 — consecutive Stutter ≤ K (𝐂5).
    #[serde(rename = "inv5")]
    Inv5StutterBudget,
    /// Inv6 — if Res > ε, a productive action follows within the window (𝐂6).
    #[serde(rename = "inv6")]
    Inv6ResidualProductivity,
    /// Inv7 — the Match policy keeps the dependency spine acyclic (𝐋3).
    #[serde(rename = "inv7")]
    Inv7MergeAcyclicity,
    /// Inv8 — mean residual trend ≥ −tol over the horizon (winning condition).
    #[serde(rename = "inv8")]
    Inv8JepaWindowTrend,
    /// Inv9 — energy conserved under *every* Next disjunct, Stutter included.
    #[serde(rename = "inv9")]
    Inv9EnergyEveryAction,
    /// Inv10 — Match implies GraphOK ∧ TypeOK (ℙ3).
    #[serde(rename = "inv10")]
    Inv10MatchWellTyped,
    /// Inv11 — productive actions are not starved on a fair window.
    #[serde(rename = "inv11")]
    Inv11FairProductivity,
}

impl Gate {
    pub const ALL: [Gate; 7] = [
        Gate::Inv5StutterBudget,
        Gate::Inv6ResidualProductivity,
        Gate::Inv7MergeAcyclicity,
        Gate::Inv8JepaWindowTrend,
        Gate::Inv9EnergyEveryAction,
        Gate::Inv10MatchWellTyped,
        Gate::Inv11FairProductivity,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Gate::Inv5StutterBudget => "inv5",
            Gate::Inv6ResidualProductivity => "inv6",
            Gate::Inv7MergeAcyclicity => "inv7",
            Gate::Inv8JepaWindowTrend => "inv8",
            Gate::Inv9EnergyEveryAction => "inv9",
            Gate::Inv10MatchWellTyped => "inv10",
            Gate::Inv11FairProductivity => "inv11",
        }
    }

    pub fn parse(s: &str) -> Option<Gate> {
        match s.trim().to_lowercase().as_str() {
            "inv5" => Some(Gate::Inv5StutterBudget),
            "inv6" => Some(Gate::Inv6ResidualProductivity),
            "inv7" => Some(Gate::Inv7MergeAcyclicity),
            "inv8" => Some(Gate::Inv8JepaWindowTrend),
            "inv9" => Some(Gate::Inv9EnergyEveryAction),
            "inv10" => Some(Gate::Inv10MatchWellTyped),
            "inv11" => Some(Gate::Inv11FairProductivity),
            _ => None,
        }
    }
}

/// Tunables for the window-based gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    /// Which gates are enabled. Empty means "no gates" — the Phase 1 behavior.
    #[serde(default)]
    pub enabled: Vec<Gate>,
    /// Inv5: maximum consecutive Stutters (𝐂5, default K = 2).
    #[serde(default = "default_stutter_k")]
    pub stutter_k: u64,
    /// Inv6 / Inv11: window length in steps.
    #[serde(default = "default_window")]
    pub window: usize,
    /// Inv8: horizon over which the residual trend is measured.
    #[serde(default = "default_horizon")]
    pub horizon: usize,
    /// Inv8: allowed *relative* rise in the mean residual across the horizon.
    ///
    /// Inv8 detects a sustained increase, not sampling noise. Comparing the
    /// two half-window means of a stationary residual series already swings by
    /// ~12% at `horizon = 32`, so an absolute or very small tolerance would
    /// fire constantly and mean nothing. The default 0.25 sits well clear of
    /// that noise floor while still catching genuine divergence.
    #[serde(default = "default_trend_tol")]
    pub trend_tol: f64,
    /// Inv9: absolute energy tolerance.
    #[serde(default = "default_energy_tol")]
    pub energy_tol: f64,
}

fn default_stutter_k() -> u64 {
    2
}
fn default_window() -> usize {
    8
}
fn default_horizon() -> usize {
    32
}
fn default_trend_tol() -> f64 {
    0.25
}
fn default_energy_tol() -> f64 {
    1e-10
}

impl Default for GateConfig {
    fn default() -> Self {
        GateConfig {
            enabled: Vec::new(),
            stutter_k: default_stutter_k(),
            window: default_window(),
            horizon: default_horizon(),
            trend_tol: default_trend_tol(),
            energy_tol: default_energy_tol(),
        }
    }
}

impl GateConfig {
    /// Parse a comma-separated gate list, e.g. `"inv5,inv7,inv9"` or `"all"`.
    pub fn parse_list(s: &str) -> Result<Vec<Gate>, String> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("none") {
            return Ok(Vec::new());
        }
        if s.eq_ignore_ascii_case("all") {
            return Ok(Gate::ALL.to_vec());
        }
        s.split(',')
            .map(|part| Gate::parse(part).ok_or_else(|| format!("unknown gate '{}'", part.trim())))
            .collect()
    }

    pub fn is_enabled(&self, g: Gate) -> bool {
        self.enabled.contains(&g)
    }
}

/// A single gate breach, with the step at which it was observed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateBreach {
    pub gate: String,
    pub step: u64,
    pub detail: String,
}

/// The outcome of gate monitoring across a run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GateReport {
    pub enabled: Vec<String>,
    pub breaches: Vec<GateBreach>,
}

impl GateReport {
    pub fn all_ok(&self) -> bool {
        self.breaches.is_empty()
    }
}

/// Watches a run and records Inv5–Inv11 breaches.
///
/// The monitor is fed after every `apply`; it never influences the action
/// chosen, so enabling a gate cannot change which behaviors the engine exhibits
/// — only which ones it reports on.
#[derive(Debug)]
pub struct GateMonitor {
    config: GateConfig,
    step: u64,
    consecutive_stutters: u64,
    /// Steps since the residual last exceeded ε with no productive action since.
    hot_residual_age: Option<usize>,
    /// Steps since the last productive action.
    unproductive_run: usize,
    /// Residuals over the current horizon.
    residuals: Vec<f64>,
    breaches: Vec<GateBreach>,
}

impl GateMonitor {
    pub fn new(config: GateConfig) -> Self {
        GateMonitor {
            config,
            step: 0,
            consecutive_stutters: 0,
            hot_residual_age: None,
            unproductive_run: 0,
            residuals: Vec::new(),
            breaches: Vec::new(),
        }
    }

    /// Observe one completed step.
    // One block per operating gate Inv5–Inv11 (FORMAL_SPEC §2.6), kept in a
    // single passive observer so no gate can be accidentally skipped.
    #[allow(clippy::too_many_lines)]
    pub fn observe(&mut self, action: Action, state: &State, residual: f64, eps: f64) {
        let productive = action != Action::Stutter;

        // --- Inv5: consecutive Stutter ≤ K (𝐂5) ---
        if action == Action::Stutter {
            self.consecutive_stutters += 1;
        } else {
            self.consecutive_stutters = 0;
        }
        if self.config.is_enabled(Gate::Inv5StutterBudget)
            && self.consecutive_stutters > self.config.stutter_k
        {
            self.breach(
                Gate::Inv5StutterBudget,
                format!(
                    "{} consecutive Stutters exceeds K = {}",
                    self.consecutive_stutters, self.config.stutter_k
                ),
            );
        }

        // --- Inv6: a hot residual must be answered by P/M/D within the window ---
        let answers_residual = matches!(action, Action::Predict | Action::Match | Action::Diffuse);
        if answers_residual {
            self.hot_residual_age = None;
        } else if let Some(age) = self.hot_residual_age.as_mut() {
            *age += 1;
        }
        if residual > eps && self.hot_residual_age.is_none() && !answers_residual {
            self.hot_residual_age = Some(0);
        }
        if self.config.is_enabled(Gate::Inv6ResidualProductivity) {
            if let Some(age) = self.hot_residual_age {
                if age >= self.config.window {
                    self.breach(
                        Gate::Inv6ResidualProductivity,
                        format!(
                            "Res = {residual:.6} > ε = {eps:.6} for {age} steps with no Predict/Match/Diffuse"
                        ),
                    );
                    self.hot_residual_age = None;
                }
            }
        }

        // --- Inv7: the Match policy keeps the spine acyclic ---
        if self.config.is_enabled(Gate::Inv7MergeAcyclicity)
            && action == Action::Match
            && !state.g.is_acyclic()
        {
            self.breach(
                Gate::Inv7MergeAcyclicity,
                format!(
                    "Match produced a cyclic graph (|V| = {})",
                    state.g.node_count()
                ),
            );
        }

        // --- Inv8: the residual must not trend upward over the horizon ---
        self.residuals.push(residual);
        if self.residuals.len() > self.config.horizon {
            self.residuals.remove(0);
        }
        if self.config.is_enabled(Gate::Inv8JepaWindowTrend)
            && self.residuals.len() == self.config.horizon
        {
            let half = self.config.horizon / 2;
            let early: f64 = self.residuals[..half].iter().sum::<f64>() / half as f64;
            let late: f64 =
                self.residuals[half..].iter().sum::<f64>() / (self.config.horizon - half) as f64;
            // Relative rise, with an absolute floor so a near-zero early mean
            // cannot make any late value look like an infinite increase.
            let allowed = early * (1.0 + self.config.trend_tol) + 1e-12;
            if late > allowed {
                self.breach(
                    Gate::Inv8JepaWindowTrend,
                    format!(
                        "mean residual rose {:.6} → {:.6} (+{:.1}%) over horizon {}, tol {:.0}%",
                        early,
                        late,
                        100.0 * (late - early) / early.max(f64::MIN_POSITIVE),
                        self.config.horizon,
                        100.0 * self.config.trend_tol
                    ),
                );
                self.residuals.clear();
            }
        }

        // --- Inv9: energy is conserved under every disjunct, Stutter included ---
        if self.config.is_enabled(Gate::Inv9EnergyEveryAction) {
            let drift = (state.energy() - state.energy_0).abs();
            if drift > self.config.energy_tol {
                self.breach(
                    Gate::Inv9EnergyEveryAction,
                    format!("{action:?} drifted energy by {drift:.3e}"),
                );
            }
        }

        // --- Inv10: Match implies a well-typed graph (ℙ3) ---
        if self.config.is_enabled(Gate::Inv10MatchWellTyped) && action == Action::Match {
            let dim = state.z.len();
            if !state.g.ok(dim) {
                self.breach(
                    Gate::Inv10MatchWellTyped,
                    "Match produced a graph failing GraphOK".into(),
                );
            }
        }

        // --- Inv11: productive actions must not be starved ---
        if productive {
            self.unproductive_run = 0;
        } else {
            self.unproductive_run += 1;
        }
        if self.config.is_enabled(Gate::Inv11FairProductivity)
            && self.unproductive_run >= self.config.window
        {
            self.breach(
                Gate::Inv11FairProductivity,
                format!(
                    "{} consecutive non-productive steps (window {})",
                    self.unproductive_run, self.config.window
                ),
            );
            self.unproductive_run = 0;
        }

        self.step += 1;
    }

    fn breach(&mut self, gate: Gate, detail: String) {
        self.breaches.push(GateBreach {
            gate: gate.name().to_string(),
            step: self.step,
            detail,
        });
    }

    /// Finish monitoring and produce the report.
    pub fn finish(self) -> GateReport {
        GateReport {
            enabled: self
                .config
                .enabled
                .iter()
                .map(|g| g.name().into())
                .collect(),
            breaches: self.breaches,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{EdgeType, Graph, GraphOp, NodeId, NodeType};
    use num_complex::Complex64;

    /// Add a node with a `dim`-dimensional zero embedding.
    ///
    /// Deliberately committed through the op alphabet so these fixtures
    /// exercise the same path the engine does; `emb_dim` may disagree with the
    /// state's dim(Z) on purpose — that is what Inv10 is asked to catch.
    fn add_test_node(g: &mut Graph, id: NodeId, emb_dim: usize) {
        g.apply(
            &GraphOp::AddNode {
                id,
                ntype: NodeType::Observation,
                emb: vec![0.0; emb_dim],
                ts: 0,
            },
            emb_dim,
        )
        .expect("fixture node must apply");
    }

    fn add_test_edge(g: &mut Graph, from: NodeId, to: NodeId) {
        g.apply(
            &GraphOp::AddEdge {
                from,
                to,
                etype: EdgeType::CausallyPrecedes,
            },
            0,
        )
        .expect("fixture edge must apply");
    }

    fn state(latent_dim: usize) -> State {
        State {
            psi: vec![Complex64::new(1.0, 0.0)],
            z: vec![0.0; latent_dim],
            g: Graph::empty(),
            t: 0,
            prev_res: 0.0,
            energy_0: 1.0,
        }
    }

    fn monitor(gates: &[Gate]) -> GateMonitor {
        GateMonitor::new(GateConfig {
            enabled: gates.to_vec(),
            window: 4,
            horizon: 8,
            ..GateConfig::default()
        })
    }

    #[test]
    fn no_gates_means_no_breaches() {
        let mut m = monitor(&[]);
        let s = state(4);
        for _ in 0..50 {
            m.observe(Action::Stutter, &s, 99.0, 1.0);
        }
        assert!(m.finish().all_ok(), "gates must be off by default");
    }

    #[test]
    fn inv5_flags_an_over_budget_stutter_run() {
        let mut m = monitor(&[Gate::Inv5StutterBudget]);
        let s = state(4);
        for _ in 0..5 {
            m.observe(Action::Stutter, &s, 0.0, 1.0);
        }
        let r = m.finish();
        assert!(!r.all_ok());
        assert!(r.breaches.iter().all(|b| b.gate == "inv5"));
    }

    #[test]
    fn inv5_accepts_a_bounded_stutter_run() {
        let mut m = monitor(&[Gate::Inv5StutterBudget]);
        let s = state(4);
        for _ in 0..10 {
            m.observe(Action::Stutter, &s, 0.0, 1.0);
            m.observe(Action::Stutter, &s, 0.0, 1.0);
            m.observe(Action::OpticalStep, &s, 0.0, 1.0);
        }
        assert!(m.finish().all_ok());
    }

    #[test]
    fn inv6_flags_a_hot_residual_left_unanswered() {
        let mut m = monitor(&[Gate::Inv6ResidualProductivity]);
        let s = state(4);
        for _ in 0..10 {
            m.observe(Action::OpticalStep, &s, 5.0, 1.0);
        }
        let r = m.finish();
        assert!(r.breaches.iter().any(|b| b.gate == "inv6"), "{r:?}");
    }

    #[test]
    fn inv6_accepts_a_hot_residual_answered_by_predict() {
        let mut m = monitor(&[Gate::Inv6ResidualProductivity]);
        let s = state(4);
        for _ in 0..10 {
            m.observe(Action::OpticalStep, &s, 5.0, 1.0);
            m.observe(Action::Predict, &s, 0.0, 1.0);
        }
        assert!(m.finish().all_ok());
    }

    #[test]
    fn inv7_flags_a_cyclic_match_result() {
        let mut m = monitor(&[Gate::Inv7MergeAcyclicity]);
        let mut s = state(2);
        add_test_node(&mut s.g, 1, 2);
        add_test_node(&mut s.g, 2, 2);
        add_test_edge(&mut s.g, 1, 2);
        add_test_edge(&mut s.g, 2, 1);
        m.observe(Action::Match, &s, 0.0, 1.0);
        assert!(m.finish().breaches.iter().any(|b| b.gate == "inv7"));
    }

    #[test]
    fn inv8_ignores_stationary_noise() {
        let mut m = monitor(&[Gate::Inv8JepaWindowTrend]);
        let s = state(4);
        // ±10% oscillation around 1.0 — noise, not divergence.
        for i in 0..200 {
            let r = 1.0 + 0.1 * (f64::from(i) * 0.7).sin();
            m.observe(Action::OpticalStep, &s, r, 1.0);
        }
        assert!(
            m.finish().all_ok(),
            "Inv8 must not fire on stationary noise"
        );
    }

    #[test]
    fn inv8_flags_a_diverging_residual() {
        let mut m = monitor(&[Gate::Inv8JepaWindowTrend]);
        let s = state(4);
        for i in 0..64 {
            m.observe(Action::OpticalStep, &s, 1.0 + f64::from(i), 1.0);
        }
        assert!(m.finish().breaches.iter().any(|b| b.gate == "inv8"));
    }

    #[test]
    fn inv8_accepts_a_falling_residual() {
        let mut m = monitor(&[Gate::Inv8JepaWindowTrend]);
        let s = state(4);
        for i in 0..64 {
            m.observe(Action::OpticalStep, &s, 10.0 / (1.0 + f64::from(i)), 1.0);
        }
        assert!(m.finish().all_ok());
    }

    #[test]
    fn inv9_flags_energy_drift() {
        let mut m = monitor(&[Gate::Inv9EnergyEveryAction]);
        let mut s = state(4);
        s.energy_0 = 2.0; // ‖ψ‖ = 1 ≠ 2
        m.observe(Action::Stutter, &s, 0.0, 1.0);
        assert!(m.finish().breaches.iter().any(|b| b.gate == "inv9"));
    }

    #[test]
    fn inv10_flags_a_match_to_a_bad_graph() {
        let mut m = monitor(&[Gate::Inv10MatchWellTyped]);
        let mut s = state(4);
        // Embedding dimension 2 while dim(Z) = 4.
        add_test_node(&mut s.g, 1, 2);
        m.observe(Action::Match, &s, 0.0, 1.0);
        assert!(m.finish().breaches.iter().any(|b| b.gate == "inv10"));
    }

    #[test]
    fn inv11_flags_starved_productivity() {
        let mut m = monitor(&[Gate::Inv11FairProductivity]);
        let s = state(4);
        for _ in 0..12 {
            m.observe(Action::Stutter, &s, 0.0, 1.0);
        }
        assert!(m.finish().breaches.iter().any(|b| b.gate == "inv11"));
    }

    #[test]
    fn parse_list_handles_names_all_and_none() {
        assert_eq!(GateConfig::parse_list("").unwrap(), vec![]);
        assert_eq!(GateConfig::parse_list("none").unwrap(), vec![]);
        assert_eq!(GateConfig::parse_list("all").unwrap(), Gate::ALL.to_vec());
        assert_eq!(
            GateConfig::parse_list("inv5, inv9").unwrap(),
            vec![Gate::Inv5StutterBudget, Gate::Inv9EnergyEveryAction]
        );
        assert!(GateConfig::parse_list("inv12").is_err());
    }
}
