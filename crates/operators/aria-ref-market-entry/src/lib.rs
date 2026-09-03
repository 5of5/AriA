//! Unique operator "BIN.REF.MARKET_ENTRY" ("REF.MARKET_ENTRY").
//!
//! Map mixer: slices already-tagged JSON telemetry into this sealed
//! market-map type. Source bytes are never rewritten.

/// Catalog identity.
pub const BINARY_ID: &str = "BIN.REF.MARKET_ENTRY";
/// Operator name on the closed envelope.
pub const OPERATOR: &str = "REF.MARKET_ENTRY";
/// This crate's frozen spec (sheet row).
pub const SPEC: &str = include_str!("../spec.json");

/// Run this operator on a host JSON payload.
pub fn run(payload: &[u8]) -> Result<aria_operator::OperatorEnvelope, aria_operator::OperatorError> {
    aria_operator::run_spec(SPEC, payload, &aria_operator::RunOpts::default())
}
