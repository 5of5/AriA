use serde::{Deserialize, Serialize};

/// The five named actions of Aria Spec.
///
/// LOCK (§1.1): Σ = {OpticalStep, Predict, Match, Diffuse, Stutter} exactly.
/// No sixth variant — enforced by G2 grep gate in CI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// ψ' = U_t(ψ); UNCHANGED ⟨z, G, t⟩ — §6.1
    OpticalStep,
    /// z' = P(I(ψ), a_t); UNCHANGED ⟨ψ, G, t⟩ — §6.2
    Predict,
    /// G' = ED(G ⊕ z, G*); UNCHANGED ⟨ψ, z, t⟩ — §6.3
    Match,
    /// z' = Diff_G(z); t' = t+1; UNCHANGED ⟨ψ, G⟩ — §6.4
    Diffuse,
    /// UNCHANGED all vars — TLA stuttering semantics
    Stutter,
}

impl Action {
    /// All five actions in a fixed array for iteration.
    pub const ALL: [Action; 5] = [
        Action::OpticalStep,
        Action::Predict,
        Action::Match,
        Action::Diffuse,
        Action::Stutter,
    ];

    /// The four productive (non-stutter) actions.
    pub const PRODUCTIVE: [Action; 4] = [
        Action::OpticalStep,
        Action::Predict,
        Action::Match,
        Action::Diffuse,
    ];

    /// Preferred Φ-cycle order: O → P → M → D (𝐂4)
    pub const PHI_CYCLE: [Action; 4] = [
        Action::OpticalStep,
        Action::Predict,
        Action::Match,
        Action::Diffuse,
    ];

    /// Human-readable symbol for trace output.
    pub fn symbol(&self) -> &'static str {
        match self {
            Action::OpticalStep => "O",
            Action::Predict => "P",
            Action::Match => "M",
            Action::Diffuse => "D",
            Action::Stutter => "S",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_has_exactly_five_variants() {
        // G2 contract: exactly five variants, no more no less
        assert_eq!(Action::ALL.len(), 5);
        assert_eq!(Action::PRODUCTIVE.len(), 4);
    }
}
