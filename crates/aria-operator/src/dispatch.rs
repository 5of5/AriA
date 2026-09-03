//! Worker dispatch: one catalog row is one host capability (Spawning S6).
//!
//! A worker is pointed at exactly one of these endpoints.
//! The worker does not choose the next binary. It execs this crate's bin,
//! passes unstructured JSON, and must receive a closed operator envelope.

use crate::envelope::OperatorSpec;

/// How a worker invokes one catalog binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerEndpoint {
    /// `BIN.*` — the work definition on the sealed plan.
    pub binary_id: String,
    /// Operator name on the closed envelope.
    pub operator: String,
    /// Cargo package (`cargo run -p`).
    pub package: String,
    /// Catalog crate field (underscored).
    pub crate_name: String,
    /// Plan `resultDefinitionRef` this binary is allowed to satisfy.
    pub result_definition_ref: String,
    /// Family / HOST / TRANSFORM / RESIDUAL / DEEP_TAG.
    pub layer: String,
}

impl WorkerEndpoint {
    /// `cargo run -p <package>` — the process a worker execs.
    #[must_use]
    pub fn cargo_invoke(&self) -> String {
        format!("cargo run -p {}", self.package)
    }
}

impl From<&OperatorSpec> for WorkerEndpoint {
    fn from(spec: &OperatorSpec) -> Self {
        Self {
            binary_id: spec.binary_id.clone(),
            operator: spec.operator.clone(),
            package: spec.package.clone(),
            crate_name: spec.crate_name.clone(),
            result_definition_ref: spec.result_definition_ref.clone(),
            layer: spec.layer.clone(),
        }
    }
}

/// Point a worker at a `BIN.*` work definition.
#[must_use]
pub fn endpoint_by_binary_id(binary_id: &str) -> Option<WorkerEndpoint> {
    crate::spec_by_id(binary_id).map(WorkerEndpoint::from)
}

/// Point a worker at a catalog operator name (`PEOPLE`, `TAG.PERSON_FOUNDER`, …).
#[must_use]
pub fn endpoint_by_operator(operator: &str) -> Option<WorkerEndpoint> {
    crate::spec_by_operator(operator).map(WorkerEndpoint::from)
}

/// Point a worker at a cargo package (`aria-telemetry-people`).
#[must_use]
pub fn endpoint_by_package(package: &str) -> Option<WorkerEndpoint> {
    crate::spec_by_package(package).map(WorkerEndpoint::from)
}
