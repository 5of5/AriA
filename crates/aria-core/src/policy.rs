use serde::{Deserialize, Serialize};

/// Match policy for ED (graph edit distance) — ℙ3
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPolicy {
    /// Leave G unchanged
    #[default]
    Identity,
    /// Apply one elementary edit (add/delete/relabel node or edge)
    OneEdit,
    /// Rebuild G to match target G*
    RebuildGStar,
    /// Absorb z into the nearest node within τ instead of appending (𝕃3).
    ///
    /// The sub-linear-growth policy: a latent closer than `merge_tau` to an
    /// existing node is merged into it (EMA embedding update, timestamp
    /// refresh — spec §5.3), so `|V| = O(T^β)` with `β ≤ 1` instead of one
    /// node per Match. Requires a metric index to stay `O(log |V|)` (ℙ5).
    Merge,
}

/// Diffusion policy for Diff_G(z)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffPolicy {
    /// Leave z unchanged
    #[default]
    Identity,
    /// Flip/perturb latent
    Flip,
    /// Graph-conditioned diffusion step
    GraphConditioned,
}
