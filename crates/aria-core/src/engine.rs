//! Aria Engine — Spec-faithful state machine.
//!
//! Implements Init, Next, Spec per FORMAL_SPEC.md.
//! Checks Inv1–4 after every apply.
//! The scheduler is policy, not Spec.

use std::fmt::Debug;

use crate::action::Action;
use crate::condition::Condition;
use crate::config::AriaConfig;
use crate::error::AriaError;
use crate::gates::{GateMonitor, GateReport};
use crate::graph::{Graph, GraphOp, UndoOp};
use crate::invariants;
use crate::invariants::InvariantReport;
use crate::policy::{DiffPolicy, MatchPolicy};
use crate::scheduler::Scheduler;
use crate::state::State;
use crate::trace::Trace;

/// [`Engine::run_monitored_with_latents`] return type — factored out so the
/// signature stays inside clippy's type-complexity budget.
pub type MonitoredLatents = (State, Trace, GateReport, Vec<Vec<f64>>);

/// Optical backend trait — PRD §5.3
pub trait OpticalBackend: Debug + Send + Sync {
    /// Apply unitary step: ψ' = U_t(ψ)
    fn unitary_step(&self, t: u64, psi: &[num_complex::Complex64]) -> Vec<num_complex::Complex64>;
    /// Field energy: ‖ψ‖₂ — Neumaier-compensated (spec §0.2, plan WS2).
    ///
    /// One shared default so every backend measures the Inv1 quantity with
    /// the same summation; the pre-WS2 per-backend uncompensated sums are gone.
    fn energy(&self, psi: &[num_complex::Complex64]) -> f64 {
        crate::state::field_energy(psi)
    }
}

/// Predictor backend trait — PRD §5.3
pub trait Predictor: Debug + Send + Sync {
    /// Isometry I: H → Z
    fn embed(&self, psi: &[num_complex::Complex64]) -> Vec<f64>;
    /// Predictor P: Z × Condition → Z
    fn predict(&self, z: &[f64], a: Condition) -> Vec<f64>;
    /// Distance in latent space
    fn dist(&self, a: &[f64], b: &[f64]) -> f64;
}

/// Graph backend trait — PRD §5.3
///
/// # Ops, not graphs (plan WS3)
///
/// v0.1.0 returned a whole new `Graph`, which cost two clones of `G` per Match
/// and made every run `O(T²)` in graph size. A backend now *proposes* the
/// atomic ops of `ED(G ⊕ z, G*)` and the engine applies them transactionally
/// against a snapshot journal (𝕃6), so a step costs the size of its edit, not
/// the size of memory.
pub trait GraphBackend: Debug + Send + Sync {
    /// Propose the ops realizing `G' = ED(G ⊕ z, policy, G*)` at clock `t`.
    ///
    /// Absorbing `z` is part of Match, so the ops include it: a policy that
    /// merges `z` into an existing node emits `MergeNodes`, one that appends
    /// emits `AddNode`. Allocate new ids from [`Graph::next_id`].
    fn edit_ops(
        &self,
        g: &Graph,
        z: &[f64],
        policy: MatchPolicy,
        target: Option<&Graph>,
        t: u64,
    ) -> Vec<GraphOp>;

    /// GraphOK check
    fn ok(&self, g: &Graph) -> bool;

    /// Mirror committed ops into auxiliary structures (e.g. a vector index).
    ///
    /// `g` is the **post**-state, so a merge's EMA-updated embedding is
    /// readable. Default: nothing to mirror.
    fn commit_ops(&self, _ops: &[GraphOp], _g: &Graph) {}

    /// Mirror a rollback, so auxiliary structures never diverge from `G`.
    ///
    /// `journal` is the undo record that was just replayed; `g` is the restored
    /// graph. Default: nothing to mirror.
    fn revert_ops(&self, _journal: &[UndoOp], _g: &Graph) {}
}

/// Diffuser backend trait — PRD §5.3
pub trait Diffuser: Debug + Send + Sync {
    /// Diffusion step: z' = Diff_G(z)
    fn diffuse(&self, g: &Graph, z: &[f64], policy: DiffPolicy) -> Vec<f64>;
}

/// Aria Engine — the Spec state machine with pluggable backends.
#[derive(Debug)]
pub struct Engine<O, P, G, D>
where
    O: OpticalBackend,
    P: Predictor,
    G: GraphBackend,
    D: Diffuser,
{
    config: AriaConfig,
    optical: O,
    predictor: P,
    graph_backend: G,
    diffuser: D,
}

impl<O, P, GB, D> Engine<O, P, GB, D>
where
    O: OpticalBackend,
    P: Predictor,
    GB: GraphBackend,
    D: Diffuser,
{
    /// Create a new engine with backends and config.
    pub fn new(
        config: AriaConfig,
        optical: O,
        predictor: P,
        graph_backend: GB,
        diffuser: D,
    ) -> Self {
        Engine {
            config,
            optical,
            predictor,
            graph_backend,
            diffuser,
        }
    }

    /// Init: create the initial state per FORMAL_SPEC §5.
    ///
    /// ψ = ψ₀, z = I(ψ₀), G = G₀, t = 0,
    /// prevRes = d(I(ψ₀), P(I(ψ₀), a(0)))
    ///
    /// Validates shapes up front: a field of the wrong length is a config
    /// error, not a downstream panic or a silently truncated mat-vec.
    pub fn init(
        &self,
        psi0: Vec<num_complex::Complex64>,
        g0: Graph,
        a0: Condition,
    ) -> Result<State, AriaError> {
        // The 𝒮 hard bounds (spec §0.1/§0.4) gate every engine construction;
        // the per-clause messages say exactly which bound was violated.
        self.config.validate()?;
        if psi0.len() != self.config.n_modes {
            return Err(AriaError::Config(format!(
                "ψ₀ has {} modes but config.n_modes = {}",
                psi0.len(),
                self.config.n_modes
            )));
        }
        for (i, c) in psi0.iter().enumerate() {
            if !c.re.is_finite() || !c.im.is_finite() {
                return Err(AriaError::Config(format!("ψ₀[{i}] = {c} is not finite")));
            }
        }

        let energy_0 = self.optical.energy(&psi0);
        let z0 = self.predictor.embed(&psi0);
        if z0.len() != self.config.latent_dim {
            return Err(AriaError::Config(format!(
                "embed(ψ₀) has dim {} but config.latent_dim = {} — backend/config mismatch",
                z0.len(),
                self.config.latent_dim
            )));
        }
        let p0 = self.predictor.predict(&z0, a0);
        if p0.len() != self.config.latent_dim {
            return Err(AriaError::Config(format!(
                "P(z) has dim {} but config.latent_dim = {} — backend/config mismatch",
                p0.len(),
                self.config.latent_dim
            )));
        }
        let prev_res = self.predictor.dist(&z0, &p0);

        Ok(State {
            psi: psi0,
            z: z0,
            g: g0,
            t: 0,
            prev_res,
            energy_0,
        })
    }

    /// Apply a single named action to the state.
    ///
    /// Returns the new state on success, or an invariant violation.
    /// This is the Spec's Next relation turned into a deterministic function.
    // One match arm per named action, mirroring FORMAL_SPEC §6 one-to-one;
    // splitting the Next relation across helpers would hurt Spec readability.
    // The Match arm is restructured in plan_v0.2.0.md WS3 (edit-ops journal).
    #[allow(clippy::too_many_lines)]
    pub fn apply(
        &self,
        mut state: State,
        action: Action,
        a: Condition,
    ) -> Result<State, AriaError> {
        match action {
            Action::OpticalStep => {
                // ψ' = U_t(ψ); UNCHANGED ⟨z, G, t⟩ — FORMAL_SPEC §6.1
                let prev_psi = state.psi.clone();
                let prev_prev_res = state.prev_res;
                let pre_residual = self.compute_residual(&state, a);

                state.psi = self.optical.unitary_step(state.t, &state.psi);
                // TLA history obligation: prevRes' = Res(psi, z, t)
                state.prev_res = pre_residual;

                let post_residual = self.compute_residual(&state, a);
                let report = invariants::check_all(
                    &state,
                    post_residual,
                    self.config.eps,
                    self.config.eps_energy,
                    self.config.n_modes,
                    self.config.latent_dim,
                );
                if self.config.strict && !report.all_ok() {
                    if let Some(v) = invariants::violation_from_report(
                        &report,
                        action,
                        state.energy(),
                        state.energy_0,
                        post_residual,
                        state.prev_res,
                        self.config.eps,
                    ) {
                        state.psi = prev_psi;
                        state.prev_res = prev_prev_res;
                        return Err(AriaError::InvariantViolation(v));
                    }
                }
            }

            Action::Predict => {
                // z' = P(I(ψ), a_t); UNCHANGED ⟨ψ, G, t⟩ — FORMAL_SPEC §6.2
                let prev_z = state.z.clone();
                let prev_prev_res = state.prev_res;
                let pre_residual = self.compute_residual(&state, a);

                state.z = self.predictor.predict(&self.predictor.embed(&state.psi), a);
                state.prev_res = pre_residual;

                let post_residual = self.compute_residual(&state, a);
                let report = invariants::check_all(
                    &state,
                    post_residual,
                    self.config.eps,
                    self.config.eps_energy,
                    self.config.n_modes,
                    self.config.latent_dim,
                );
                if self.config.strict && !report.all_ok() {
                    if let Some(v) = invariants::violation_from_report(
                        &report,
                        action,
                        state.energy(),
                        state.energy_0,
                        post_residual,
                        state.prev_res,
                        self.config.eps,
                    ) {
                        state.z = prev_z;
                        state.prev_res = prev_prev_res;
                        return Err(AriaError::InvariantViolation(v));
                    }
                }
            }

            Action::Match => {
                // G' = ED(G ⊕ z, G*); UNCHANGED ⟨ψ, z, t⟩ — FORMAL_SPEC §6.3
                //
                // Transactional, clone-free (plan WS3): the backend proposes
                // atomic ops, `apply_ops` commits them all-or-nothing against a
                // snapshot journal, and any failure below replays that journal.
                // `panic = "abort"` in release forbids unwind-based cleanup, so
                // every rollback here is explicit data.
                let prev_prev_res = state.prev_res;
                let pre_residual = self.compute_residual(&state, a);

                let ops = self.graph_backend.edit_ops(
                    &state.g,
                    &state.z,
                    self.config.match_policy,
                    None,
                    state.t,
                );

                let journal = state
                    .g
                    .apply_ops(&ops, self.config.latent_dim)
                    .map_err(|e| AriaError::Backend(format!("Match op refused: {e}")))?;

                // Enforce max graph size — the projected size is the committed
                // one, so an over-cap edit is rolled back rather than pre-guessed.
                if state.g.size() > self.config.max_graph_size {
                    let size = state.g.size();
                    state.g.undo_ops(&journal);
                    self.graph_backend.revert_ops(&journal, &state.g);
                    return Err(AriaError::Schedule(format!(
                        "graph size {} exceeds max {}",
                        size, self.config.max_graph_size
                    )));
                }

                self.graph_backend.commit_ops(&ops, &state.g);
                state.prev_res = pre_residual;

                let post_residual = self.compute_residual(&state, a);
                let report = invariants::check_all(
                    &state,
                    post_residual,
                    self.config.eps,
                    self.config.eps_energy,
                    self.config.n_modes,
                    self.config.latent_dim,
                );
                if self.config.strict && !report.all_ok() {
                    if let Some(v) = invariants::violation_from_report(
                        &report,
                        action,
                        state.energy(),
                        state.energy_0,
                        post_residual,
                        state.prev_res,
                        self.config.eps,
                    ) {
                        state.g.undo_ops(&journal);
                        self.graph_backend.revert_ops(&journal, &state.g);
                        state.prev_res = prev_prev_res;
                        return Err(AriaError::InvariantViolation(v));
                    }
                }
            }

            Action::Diffuse => {
                // z' = Diff_G(z); t' = t+1; UNCHANGED ⟨ψ, G⟩ — FORMAL_SPEC §6.4
                let prev_z = state.z.clone();
                let prev_t = state.t;
                let prev_prev_res = state.prev_res;
                let pre_residual = self.compute_residual(&state, a);

                state.z = self
                    .diffuser
                    .diffuse(&state.g, &state.z, self.config.diff_policy);
                state.t = state
                    .t
                    .checked_add(1)
                    .ok_or_else(|| AriaError::Backend("t overflowed u64".into()))?;
                state.prev_res = pre_residual;

                let post_residual = self.compute_residual(&state, a);
                let report = invariants::check_all(
                    &state,
                    post_residual,
                    self.config.eps,
                    self.config.eps_energy,
                    self.config.n_modes,
                    self.config.latent_dim,
                );
                if self.config.strict && !report.all_ok() {
                    if let Some(v) = invariants::violation_from_report(
                        &report,
                        action,
                        state.energy(),
                        state.energy_0,
                        post_residual,
                        state.prev_res,
                        self.config.eps,
                    ) {
                        state.z = prev_z;
                        state.t = prev_t;
                        state.prev_res = prev_prev_res;
                        return Err(AriaError::InvariantViolation(v));
                    }
                }
            }

            Action::Stutter => {
                // UNCHANGED all vars — TLA stuttering (including prevRes)
                let residual = self.compute_residual(&state, a);

                let report = invariants::check_all(
                    &state,
                    residual,
                    self.config.eps,
                    self.config.eps_energy,
                    self.config.n_modes,
                    self.config.latent_dim,
                );
                if self.config.strict && !report.all_ok() {
                    if let Some(v) = invariants::violation_from_report(
                        &report,
                        action,
                        state.energy(),
                        state.energy_0,
                        residual,
                        state.prev_res,
                        self.config.eps,
                    ) {
                        return Err(AriaError::InvariantViolation(v));
                    }
                }
            }
        }

        Ok(state)
    }

    /// Step one full Φ-cycle: OpticalStep → Predict → Match → Diffuse (𝐂4).
    ///
    /// This is the preferred schedule. Each sub-step checks invariants.
    /// Returns the state after the full cycle, or the first invariant violation.
    pub fn step_phi(&self, state: State, a: Condition) -> Result<State, AriaError> {
        let s = state;
        let s = self.apply(s, Action::OpticalStep, a)?;
        let s = self.apply(s, Action::Predict, a)?;
        let s = self.apply(s, Action::Match, a)?;
        let s = self.apply(s, Action::Diffuse, a)?;
        Ok(s)
    }

    /// Run the engine for a number of steps with a scheduler.
    ///
    /// Returns the final state and a trace of all steps.
    pub fn run(
        &self,
        state: State,
        scheduler: &mut Scheduler,
        steps: u64,
        a: Condition,
    ) -> Result<(State, Trace), AriaError> {
        let (state, trace, _) = self.run_monitored(state, scheduler, steps, a)?;
        Ok((state, trace))
    }

    /// Run the engine, additionally monitoring the optional Inv5–Inv11 gates.
    ///
    /// The monitor is a passive observer: it sees each completed step and can
    /// only report. Enabling a gate never changes which actions are taken, so
    /// the set of admissible behaviors is exactly the same as for [`run`].
    pub fn run_monitored(
        &self,
        mut state: State,
        scheduler: &mut Scheduler,
        steps: u64,
        a: Condition,
    ) -> Result<(State, Trace, GateReport), AriaError> {
        let mut trace = Trace::new(
            self.config.n_modes,
            self.config.latent_dim,
            self.config.eps,
            self.config.seed,
            &self.config.schedule,
            a,
            self.config.match_policy,
        );
        let mut monitor = GateMonitor::new(self.config.gates.clone());

        for _ in 0..steps {
            let action = scheduler.next_action_budgeted();
            let t_before = state.t;

            state = self.apply(state, action, a)?;

            let residual = self.compute_residual(&state, a);
            let energy = state.energy();
            trace.push(
                t_before,
                action,
                residual,
                energy,
                state.g.size(),
                &format!("{a:?}").to_lowercase(),
            );
            monitor.observe(action, &state, residual, self.config.eps);
        }

        Ok((state, trace, monitor.finish()))
    }

    /// Same loop as [`Self::run_monitored`], plus the post-step latent `z`.
    ///
    /// Used by the post-hoc `aria emit` surface to recover the z-sequence of
    /// a completed run without writing `z` into the JSONL — so the default
    /// trace stays byte-stable and emit cannot feed anything back into Φ.
    /// Collecting `z` is state observation, not a Φ operator.
    pub fn run_monitored_with_latents(
        &self,
        mut state: State,
        scheduler: &mut Scheduler,
        steps: u64,
        a: Condition,
    ) -> Result<MonitoredLatents, AriaError> {
        let mut trace = Trace::new(
            self.config.n_modes,
            self.config.latent_dim,
            self.config.eps,
            self.config.seed,
            &self.config.schedule,
            a,
            self.config.match_policy,
        );
        let mut monitor = GateMonitor::new(self.config.gates.clone());
        let mut latents = Vec::with_capacity(usize::try_from(steps).unwrap_or(0));

        for _ in 0..steps {
            let action = scheduler.next_action_budgeted();
            let t_before = state.t;

            state = self.apply(state, action, a)?;

            let residual = self.compute_residual(&state, a);
            let energy = state.energy();
            trace.push(
                t_before,
                action,
                residual,
                energy,
                state.g.size(),
                &format!("{a:?}").to_lowercase(),
            );
            monitor.observe(action, &state, residual, self.config.eps);
            latents.push(state.z.clone());
        }

        Ok((state, trace, monitor.finish(), latents))
    }

    /// Check all invariants on the current state without applying an action.
    pub fn check(&self, state: &State, a: Condition) -> InvariantReport {
        let residual = self.compute_residual(state, a);
        invariants::check_all(
            state,
            residual,
            self.config.eps,
            self.config.eps_energy,
            self.config.n_modes,
            self.config.latent_dim,
        )
    }

    /// JEPA residual Res(ψ, z, t) = d(z, P(I(ψ), a_t)).
    ///
    /// Public so the WS6 streaming verifier can audit Inv2 / X4 without
    /// retaining a full in-memory [`crate::trace::Trace`].
    pub fn residual(&self, state: &State, a: Condition) -> f64 {
        self.compute_residual(state, a)
    }

    /// Compute JEPA residual: Res(ψ, z, t) = d(z, P(I(ψ), a_t))
    fn compute_residual(&self, state: &State, a: Condition) -> f64 {
        let embedded = self.predictor.embed(&state.psi);
        let predicted = self.predictor.predict(&embedded, a);
        self.predictor.dist(&state.z, &predicted)
    }

    /// Access config.
    pub fn config(&self) -> &AriaConfig {
        &self.config
    }

    /// Access the graph backend.
    ///
    /// Read-only: the backend owns policy-layer state (the metric index) whose
    /// consistency with `G` is maintained through `commit_ops`/`revert_ops`, so
    /// callers may inspect it but never mutate it behind the engine's back.
    pub fn graph_backend(&self) -> &GB {
        &self.graph_backend
    }
}
