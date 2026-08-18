use serde::{Deserialize, Serialize};

/// Conditioning variable a_t — 𝐂2
///
/// Changes the conditioning of P and Diff without changing architecture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// Discrete next-token prediction
    #[default]
    Token,
    /// Continuous diffusion score estimation
    Diffusion,
    /// Multi-step world-model roll-out
    WorldModel,
}
