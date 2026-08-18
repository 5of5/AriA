use thiserror::Error;

use crate::action::Action;

/// Structured invariant violation.
#[derive(Debug, Clone, Error)]
pub enum AriaError {
    /// An invariant was violated during apply.
    #[error("invariant violation: {0}")]
    InvariantViolation(InvViolation),

    /// A backend operation failed.
    #[error("backend error: {0}")]
    Backend(String),

    /// Configuration error.
    #[error("config error: {0}")]
    Config(String),

    /// I/O error.
    #[error("io error: {0}")]
    Io(String),

    /// Invalid action sequence.
    #[error("invalid schedule: {0}")]
    Schedule(String),
}

/// Which invariant was violated, with before/after snapshots.
#[derive(Debug, Clone)]
pub struct InvViolation {
    /// Which invariant (1-4)
    pub invariant: u8,
    /// Human-readable message
    pub message: String,
    /// The action that triggered the violation
    pub action: Action,
    /// State before the action (if available)
    pub before: Option<String>,
    /// State after the action (if available)
    pub after: Option<String>,
}

impl std::fmt::Display for InvViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl InvViolation {
    pub fn new(invariant: u8, message: impl Into<String>, action: Action) -> Self {
        InvViolation {
            invariant,
            message: message.into(),
            action,
            before: None,
            after: None,
        }
    }

    pub fn inv1(action: Action, energy: f64, energy_0: f64) -> Self {
        InvViolation {
            invariant: 1,
            message: format!("Inv1 violated: energy {energy:.6} ≠ initial energy {energy_0:.6}"),
            action,
            before: None,
            after: None,
        }
    }

    pub fn inv2(action: Action, residual: f64, prev_res: f64, eps: f64) -> Self {
        InvViolation {
            invariant: 2,
            message: format!(
                "Inv2 violated: residual {:.6} > prevRes + ε = {:.6} + {:.6} = {:.6}",
                residual,
                prev_res,
                eps,
                prev_res + eps
            ),
            action,
            before: None,
            after: None,
        }
    }

    pub fn inv3(action: Action, reason: impl Into<String>) -> Self {
        InvViolation {
            invariant: 3,
            message: format!("Inv3 violated: GraphOK failed — {}", reason.into()),
            action,
            before: None,
            after: None,
        }
    }

    pub fn inv4(action: Action, reason: impl Into<String>) -> Self {
        InvViolation {
            invariant: 4,
            message: format!("Inv4 violated: TypeOK failed — {}", reason.into()),
            action,
            before: None,
            after: None,
        }
    }
}
