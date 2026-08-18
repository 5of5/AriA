// Aria-core: Spec-faithful state machine engine
//
// Implements the discrete Spec from docs/FORMAL_SPEC.md:
//   Spec ≜ Init ∧ □[Next]_vars
// with exactly five actions: OpticalStep, Predict, Match, Diffuse, Stutter.

pub mod action;
pub mod condition;
pub mod config;
pub mod engine;
pub mod error;
pub mod gates;
pub mod graph;
pub mod invariants;
pub mod policy;
pub mod prelude;
pub mod scheduler;
pub mod state;
pub mod trace;

pub use action::Action;
pub use condition::Condition;
pub use config::AriaConfig;
pub use engine::Engine;
pub use error::{AriaError, InvViolation};
pub use gates::{Gate, GateConfig, GateReport};
pub use graph::{
    EdgeType, Graph, GraphEdge, GraphNode, GraphOp, GraphOpError, NodeId, NodeType, UndoOp,
};
pub use invariants::InvariantReport;
pub use policy::{DiffPolicy, MatchPolicy};
pub use state::State;
pub use trace::TraceEntry;
