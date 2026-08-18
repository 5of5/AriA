//! Aria Python surface — PyO3 façade over the reference runtime.
//!
//! This module defines no transitions and relaxes no invariant. Every run goes
//! through `aria_engine_backends::runner`, the same code path used by the CLI
//! and the WASM module, so notebook results match CLI results exactly.

// `#[pymethods]` / `#[pyfunction]` expand to wrappers that call `.into()` on a
// value that is already a `PyErr`. The lint fires on the generated code, not on
// anything written here, so it is silenced at the crate level.
#![allow(clippy::useless_conversion)]

use aria_engine_backends::runner::{self, canonical_init, sim_engine, SimEngine};
use aria_engine_backends::DEFAULT_ITERATIONS;
use aria_engine_core::action::Action;
use aria_engine_core::condition::Condition;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::gates::GateConfig;
use aria_engine_core::state::State;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Aria engine configuration.
#[pyclass(name = "Config")]
#[derive(Clone)]
pub struct PyConfig {
    inner: AriaConfig,
}

#[pymethods]
impl PyConfig {
    /// Build a config, overriding any subset of the defaults.
    #[new]
    #[pyo3(signature = (
        n_modes = None,
        latent_dim = None,
        eps = None,
        stutter_k = None,
        schedule = None,
        condition = None,
        seed = None,
        strict = None,
        max_graph_size = None,
        gates = None,
        allow_sub_spec_dims = None,
    ))]
    // PyO3 extracts owned values from Python arguments; &str/&[T] parameters
    // would force extra borrow plumbing for no gain in an FFI constructor.
    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    fn new(
        n_modes: Option<usize>,
        latent_dim: Option<usize>,
        eps: Option<f64>,
        stutter_k: Option<u64>,
        schedule: Option<String>,
        condition: Option<String>,
        seed: Option<u64>,
        strict: Option<bool>,
        max_graph_size: Option<usize>,
        gates: Option<String>,
        allow_sub_spec_dims: Option<bool>,
    ) -> PyResult<Self> {
        let mut inner = AriaConfig::default();
        if let Some(v) = n_modes {
            inner.n_modes = v;
        }
        if let Some(v) = latent_dim {
            inner.latent_dim = v;
        }
        if let Some(v) = eps {
            inner.eps = v;
        }
        if let Some(v) = stutter_k {
            inner.stutter_k = v;
        }
        if let Some(v) = schedule {
            inner.schedule = v;
        }
        if let Some(ref v) = condition {
            inner.condition = runner::parse_condition(v).map_err(value_err)?;
        }
        if let Some(v) = seed {
            inner.seed = Some(v);
        }
        if let Some(v) = strict {
            inner.strict = v;
        }
        if let Some(v) = max_graph_size {
            inner.max_graph_size = v;
        }
        if let Some(ref v) = gates {
            // Optional Inv5–Inv11 operating gates; off unless asked for.
            inner.gates.enabled = GateConfig::parse_list(v).map_err(value_err)?;
            inner.gates.stutter_k = inner.stutter_k;
        }
        if let Some(v) = allow_sub_spec_dims {
            inner.allow_sub_spec_dims = v;
        }
        Ok(PyConfig { inner })
    }

    /// Parse a config from a TOML string — same format the CLI accepts.
    #[staticmethod]
    fn from_toml(src: &str) -> PyResult<Self> {
        let inner = AriaConfig::from_toml(src).map_err(value_err)?;
        Ok(PyConfig { inner })
    }

    #[getter]
    fn n_modes(&self) -> usize {
        self.inner.n_modes
    }

    #[getter]
    fn latent_dim(&self) -> usize {
        self.inner.latent_dim
    }

    #[getter]
    fn eps(&self) -> f64 {
        self.inner.eps
    }

    #[getter]
    fn schedule(&self) -> String {
        self.inner.schedule.clone()
    }

    #[getter]
    fn condition(&self) -> String {
        condition_name(self.inner.condition).to_string()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner).map_err(runtime_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "Config(n_modes={}, latent_dim={}, eps={}, schedule='{}', condition='{}')",
            self.inner.n_modes,
            self.inner.latent_dim,
            self.inner.eps,
            self.inner.schedule,
            condition_name(self.inner.condition),
        )
    }
}

/// Result of an invariant check: Inv1–4.
// Four independent pass/fail flags deliberately mirror Inv1–Inv4 (see
// aria_engine_core::invariants::InvariantReport).
#[allow(clippy::struct_excessive_bools)]
#[pyclass(name = "InvariantReport")]
#[derive(Clone)]
pub struct PyInvariantReport {
    #[pyo3(get)]
    inv1: bool,
    #[pyo3(get)]
    inv2: bool,
    #[pyo3(get)]
    inv3: bool,
    #[pyo3(get)]
    inv4: bool,
    #[pyo3(get)]
    failures: Vec<String>,
}

#[pymethods]
impl PyInvariantReport {
    #[getter]
    fn all_ok(&self) -> bool {
        self.inv1 && self.inv2 && self.inv3 && self.inv4
    }

    fn __repr__(&self) -> String {
        format!(
            "InvariantReport(inv1={}, inv2={}, inv3={}, inv4={})",
            self.inv1, self.inv2, self.inv3, self.inv4
        )
    }
}

/// A snapshot of the Spec state ⟨ψ, z, G, t⟩ plus the prevRes history variable.
#[pyclass(name = "State")]
#[derive(Clone)]
pub struct PyState {
    inner: State,
}

#[pymethods]
impl PyState {
    #[getter]
    fn t(&self) -> u64 {
        self.inner.t
    }

    #[getter]
    fn energy(&self) -> f64 {
        self.inner.energy()
    }

    #[getter]
    fn prev_res(&self) -> f64 {
        self.inner.prev_res
    }

    #[getter]
    fn graph_size(&self) -> usize {
        self.inner.g.size()
    }

    /// The JEPA latent z as a plain Python list.
    #[getter]
    fn z(&self) -> Vec<f64> {
        self.inner.z.clone()
    }

    /// ψ as a list of (re, im) pairs.
    #[getter]
    fn psi(&self) -> Vec<(f64, f64)> {
        self.inner.psi.iter().map(|c| (c.re, c.im)).collect()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner).map_err(runtime_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "State(t={}, energy={:.6}, |G|={})",
            self.inner.t,
            self.inner.energy(),
            self.inner.g.size()
        )
    }
}

/// The Aria engine: the Spec runner bound to the simulated operator backends.
#[pyclass(name = "AriaEngine")]
pub struct PyAriaEngine {
    engine: SimEngine,
    condition: Condition,
}

#[pymethods]
impl PyAriaEngine {
    /// Build an engine. Pass a `Config`, or omit for the documented defaults.
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<PyConfig>) -> Self {
        let cfg = config.map(|c| c.inner).unwrap_or_default();
        let condition = cfg.condition;
        PyAriaEngine {
            engine: sim_engine(cfg),
            condition,
        }
    }

    /// The canonical initial state for this engine's config.
    fn init(&self) -> PyResult<PyState> {
        Ok(PyState {
            inner: canonical_init(&self.engine, self.condition).map_err(value_err)?,
        })
    }

    /// Apply one named action. Raises on an invariant violation.
    fn apply(&self, state: &PyState, action: &str) -> PyResult<PyState> {
        let action = parse_action(action)?;
        let next = self
            .engine
            .apply(state.inner.clone(), action, self.condition)
            .map_err(runtime_err)?;
        Ok(PyState { inner: next })
    }

    /// Apply one full Φ-cycle: OpticalStep → Predict → Match → Diffuse (𝐂4).
    fn step_phi(&self, state: &PyState) -> PyResult<PyState> {
        let next = self
            .engine
            .step_phi(state.inner.clone(), self.condition)
            .map_err(runtime_err)?;
        Ok(PyState { inner: next })
    }

    /// Check Inv1–4 on a state without applying an action.
    fn check(&self, state: &PyState) -> PyInvariantReport {
        let report = self.engine.check(&state.inner, self.condition);
        PyInvariantReport {
            inv1: report.inv1_ok,
            inv2: report.inv2_ok,
            inv3: report.inv3_ok,
            inv4: report.inv4_ok,
            failures: report.failures(),
        }
    }

    fn __repr__(&self) -> String {
        let c = self.engine.config();
        format!(
            "AriaEngine(n_modes={}, latent_dim={}, eps={}, condition='{}')",
            c.n_modes,
            c.latent_dim,
            c.eps,
            condition_name(self.condition)
        )
    }
}

/// Run the reference schedule and return a summary dict.
///
/// Identical in every field to `aria run` and to the WASM `run()`.
#[pyfunction]
#[pyo3(signature = (steps = 1000, config = None))]
fn run(py: Python<'_>, steps: u64, config: Option<PyConfig>) -> PyResult<Py<PyDict>> {
    let cfg = config.map(|c| c.inner).unwrap_or_default();
    // Release the GIL for the duration of the run — a 1M-step run must not
    // freeze a notebook kernel's other threads.
    let outcome = py
        .allow_threads(|| runner::run(cfg, steps))
        .map_err(runtime_err)?;
    let s = outcome.summary;

    let d = PyDict::new_bound(py);
    d.set_item("steps", s.steps)?;
    d.set_item("t", s.t)?;
    d.set_item("graph_size", s.graph_size)?;
    d.set_item("energy", s.energy)?;
    d.set_item("residual", s.residual)?;
    d.set_item("action_sequence", s.action_sequence)?;
    d.set_item("invariants_ok", s.invariants_ok)?;
    d.set_item("failures", s.failures)?;

    // Optional Inv5–Inv11 gates; `enabled` is empty unless the config asked.
    let gates = PyDict::new_bound(py);
    gates.set_item("ok", s.gates.all_ok())?;
    gates.set_item("enabled", s.gates.enabled)?;
    let breaches: Vec<Py<PyDict>> = s
        .gates
        .breaches
        .into_iter()
        .map(|b| {
            let e = PyDict::new_bound(py);
            e.set_item("gate", b.gate)?;
            e.set_item("step", b.step)?;
            e.set_item("detail", b.detail)?;
            Ok::<_, PyErr>(e.into())
        })
        .collect::<PyResult<_>>()?;
    gates.set_item("breaches", breaches)?;
    d.set_item("gates", gates)?;

    Ok(d.into())
}

/// Run the reference schedule and return the trace as a JSONL string.
#[pyfunction]
#[pyo3(signature = (steps = 1000, config = None))]
fn run_trace_jsonl(py: Python<'_>, steps: u64, config: Option<PyConfig>) -> PyResult<String> {
    let cfg = config.map(|c| c.inner).unwrap_or_default();
    let outcome = py
        .allow_threads(|| runner::run(cfg, steps))
        .map_err(runtime_err)?;
    Ok(outcome.trace.to_jsonl())
}

/// The five named actions: Σ = {OpticalStep, Predict, Match, Diffuse, Stutter}.
#[pyfunction]
fn actions() -> Vec<String> {
    Action::ALL.iter().map(|a| format!("{a:?}")).collect()
}

/// σ_max(W) by the same seeded power iteration the trained-backend loader
/// uses to enforce ℙ2 (plan WS1, 𝕋4). `r` defaults to the same iteration
/// count as the Rust loader — the cross-language agreement tests call this.
#[pyfunction]
#[pyo3(signature = (matrix, r = None))]
// PyO3 extracts owned values from Python arguments; a &Vec<Vec<f64>> would
// force extra borrow plumbing for no gain in an FFI boundary.
#[allow(clippy::needless_pass_by_value)]
fn power_iteration(matrix: Vec<Vec<f64>>, r: Option<usize>) -> PyResult<f64> {
    let r = r.unwrap_or(DEFAULT_ITERATIONS);
    aria_engine_backends::spectral::power_iteration(&matrix, r).map_err(value_err)
}

#[pymodule]
fn aria(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyConfig>()?;
    m.add_class::<PyState>()?;
    m.add_class::<PyInvariantReport>()?;
    m.add_class::<PyAriaEngine>()?;
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(run_trace_jsonl, m)?)?;
    m.add_function(wrap_pyfunction!(actions, m)?)?;
    m.add_function(wrap_pyfunction!(power_iteration, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

fn parse_action(s: &str) -> PyResult<Action> {
    match s.to_lowercase().as_str() {
        "opticalstep" | "optical_step" | "o" => Ok(Action::OpticalStep),
        "predict" | "p" => Ok(Action::Predict),
        "match" | "m" => Ok(Action::Match),
        "diffuse" | "d" => Ok(Action::Diffuse),
        "stutter" | "s" => Ok(Action::Stutter),
        other => Err(PyValueError::new_err(format!(
            "unknown action '{other}' (expected OpticalStep|Predict|Match|Diffuse|Stutter)"
        ))),
    }
}

fn condition_name(c: Condition) -> &'static str {
    match c {
        Condition::Token => "token",
        Condition::Diffusion => "diffusion",
        Condition::WorldModel => "world_model",
    }
}

fn value_err<E: std::fmt::Display>(e: E) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn runtime_err<E: std::fmt::Display>(e: E) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}
