//! Unique operator "BIN.TAG.ECO_AGENT_CALLABLE" ("TAG.ECO_AGENT_CALLABLE").
//!
//! Links the Aria transformer through `aria-operator` and emits closed
//! operator JSON (Binary Repository v1 / sheet 09) with the shared
//! `aria-telemetry-query-v1` spine under `telemetry`.

/// Catalog identity.
pub const BINARY_ID: &str = "BIN.TAG.ECO_AGENT_CALLABLE";
/// Operator name on the closed envelope.
pub const OPERATOR: &str = "TAG.ECO_AGENT_CALLABLE";
/// This crate's frozen spec (sheet row).
pub const SPEC: &str = include_str!("../spec.json");

/// Run this operator on a host JSON payload.
pub fn run(payload: &[u8]) -> Result<aria_operator::OperatorEnvelope, aria_operator::OperatorError> {
    aria_operator::run_spec(SPEC, payload, &aria_operator::RunOpts::default())
}
