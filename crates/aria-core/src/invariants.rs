use crate::action::Action;
use crate::error::InvViolation;
use crate::graph::Graph;
use crate::state::State;

/// Result of an invariant check.
// Four independent pass/fail flags deliberately mirror Inv1–Inv4; they are
// parallel verdicts, not an encoded state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct InvariantReport {
    pub inv1_ok: bool,
    pub inv2_ok: bool,
    pub inv3_ok: bool,
    pub inv4_ok: bool,
    pub inv1_detail: Option<String>,
    pub inv2_detail: Option<String>,
    pub inv3_detail: Option<String>,
    pub inv4_detail: Option<String>,
}

impl InvariantReport {
    pub fn all_ok(&self) -> bool {
        self.inv1_ok && self.inv2_ok && self.inv3_ok && self.inv4_ok
    }

    pub fn failures(&self) -> Vec<String> {
        let mut v = Vec::new();
        if !self.inv1_ok {
            v.push(format!(
                "Inv1 FAIL: {}",
                self.inv1_detail.as_deref().unwrap_or("")
            ));
        }
        if !self.inv2_ok {
            v.push(format!(
                "Inv2 FAIL: {}",
                self.inv2_detail.as_deref().unwrap_or("")
            ));
        }
        if !self.inv3_ok {
            v.push(format!(
                "Inv3 FAIL: {}",
                self.inv3_detail.as_deref().unwrap_or("")
            ));
        }
        if !self.inv4_ok {
            v.push(format!(
                "Inv4 FAIL: {}",
                self.inv4_detail.as_deref().unwrap_or("")
            ));
        }
        v
    }
}

/// Check Inv1: ‖ψ‖₂ = ‖ψ₀‖₂ — tolerance from `config.eps_energy` (spec §0.2).
pub fn check_inv1(state: &State, eps_energy: f64) -> (bool, Option<String>) {
    let energy = state.energy();
    let ok = (energy - state.energy_0).abs() < eps_energy;
    let detail = if ok {
        None
    } else {
        Some(format!(
            "energy {:.10} ≠ energy_0 {:.10}",
            energy, state.energy_0
        ))
    };
    (ok, detail)
}

/// Check Inv2: Res(ψ, z, t) ≤ prevRes + ε
pub fn check_inv2(state: &State, residual: f64, eps: f64) -> (bool, Option<String>) {
    let bound = state.prev_res + eps;
    // Tiny f64 tolerance so rounding errors near exact equality do not fail.
    let ok = residual <= bound + 1e-12;
    let detail = if ok {
        None
    } else {
        Some(format!(
            "Res {:.6} > prevRes + ε = {:.6} + {:.6} = {:.6}",
            residual, state.prev_res, eps, bound
        ))
    };
    (ok, detail)
}

/// Check Inv3: GraphOK(G)
pub fn check_inv3(g: &Graph, latent_dim: usize) -> (bool, Option<String>) {
    if g.ok(latent_dim) {
        (true, None)
    } else {
        (false, Some("GraphOK failed".into()))
    }
}

/// Check Inv4: TypeOK — all state variables are well-typed.
pub fn check_inv4(state: &State, n_modes: usize, latent_dim: usize) -> (bool, Option<String>) {
    // ψ must be in C^N: exactly n_modes components, all finite. A short field
    // would silently truncate the unitary mat-vec rather than fail loudly.
    if state.psi.len() != n_modes {
        return (
            false,
            Some(format!(
                "psi has {} modes ≠ n_modes {}",
                state.psi.len(),
                n_modes
            )),
        );
    }
    for (i, c) in state.psi.iter().enumerate() {
        if !c.re.is_finite() || !c.im.is_finite() {
            return (false, Some(format!("psi[{i}] is not finite")));
        }
    }
    // z must be in Z: all entries finite, correct dim
    if state.z.len() != latent_dim {
        return (
            false,
            Some(format!(
                "z has dim {} ≠ latent_dim {}",
                state.z.len(),
                latent_dim
            )),
        );
    }
    for (i, &v) in state.z.iter().enumerate() {
        if !v.is_finite() {
            return (false, Some(format!("z[{i}] is not finite")));
        }
    }
    // G must be a finite directed typed graph — already checked by Inv3
    // t must be in N
    // prev_res must be in N (finite, non-negative)
    if !state.prev_res.is_finite() || state.prev_res < 0.0 {
        return (
            false,
            Some(format!("prev_res = {} is not valid", state.prev_res)),
        );
    }
    (true, None)
}

/// Run all four invariant checks on the state after an action.
pub fn check_all(
    state: &State,
    residual: f64,
    eps: f64,
    eps_energy: f64,
    n_modes: usize,
    latent_dim: usize,
) -> InvariantReport {
    let (inv1_ok, inv1_detail) = check_inv1(state, eps_energy);
    let (inv2_ok, inv2_detail) = check_inv2(state, residual, eps);
    let (inv3_ok, inv3_detail) = check_inv3(&state.g, latent_dim);
    let (inv4_ok, inv4_detail) = check_inv4(state, n_modes, latent_dim);

    InvariantReport {
        inv1_ok,
        inv2_ok,
        inv3_ok,
        inv4_ok,
        inv1_detail,
        inv2_detail,
        inv3_detail,
        inv4_detail,
    }
}

/// Map an invariant failure to a structured violation for the given action.
pub fn violation_from_report(
    report: &InvariantReport,
    action: Action,
    energy: f64,
    energy_0: f64,
    residual: f64,
    prev_res: f64,
    eps: f64,
) -> Option<InvViolation> {
    if !report.inv1_ok {
        Some(InvViolation::inv1(action, energy, energy_0))
    } else if !report.inv2_ok {
        Some(InvViolation::inv2(action, residual, prev_res, eps))
    } else if !report.inv3_ok {
        Some(InvViolation::inv3(
            action,
            report.inv3_detail.as_deref().unwrap_or(""),
        ))
    } else if !report.inv4_ok {
        Some(InvViolation::inv4(
            action,
            report.inv4_detail.as_deref().unwrap_or(""),
        ))
    } else {
        None
    }
}
