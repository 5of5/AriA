use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::condition::Condition;
use crate::policy::MatchPolicy;

/// A single trace entry for JSONL export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    /// Discrete step counter
    pub t: u64,
    /// Action taken
    pub action: String,
    /// Residual after step
    pub res: f64,
    /// Field energy after step
    pub energy: f64,
    /// Graph size |G| = |V| + |E|
    pub graph_size: usize,
    /// Conditioning
    pub condition: String,
}

/// Full trace: a sequence of entries.
///
/// The header (`config_*` fields) must carry everything `aria emit` needs to
/// replay Φ byte-for-byte without a matching `--config`: `n_modes`,
/// `latent_dim`, and `eps` alone are not enough — `seed`, `schedule`,
/// `condition`, and `match_policy` all affect the trajectory the trace
/// records. Omitting any of them makes replay silently diverge from the run
/// that produced the trace (see `aria emit`'s doc comment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub config_n_modes: usize,
    pub config_latent_dim: usize,
    pub config_eps: f64,
    /// Seed the run used — `None` only if the run itself used no fixed seed.
    pub config_seed: Option<u64>,
    /// Schedule string ("opmd" or a custom action-char sequence).
    pub config_schedule: String,
    /// Conditioning a_t the run used.
    pub config_condition: Condition,
    /// Match policy ℙ3 the run used.
    pub config_match_policy: MatchPolicy,
    pub entries: Vec<TraceEntry>,
}

impl Trace {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        n_modes: usize,
        latent_dim: usize,
        eps: f64,
        seed: Option<u64>,
        schedule: &str,
        condition: Condition,
        match_policy: MatchPolicy,
    ) -> Self {
        Trace {
            config_n_modes: n_modes,
            config_latent_dim: latent_dim,
            config_eps: eps,
            config_seed: seed,
            config_schedule: schedule.to_string(),
            config_condition: condition,
            config_match_policy: match_policy,
            entries: Vec::new(),
        }
    }

    pub fn push(
        &mut self,
        t: u64,
        action: Action,
        residual: f64,
        energy: f64,
        graph_size: usize,
        condition: &str,
    ) {
        self.entries.push(TraceEntry {
            t,
            action: action.symbol().to_string(),
            res: residual,
            energy,
            graph_size,
            condition: condition.to_string(),
        });
    }

    /// Export as JSONL string.
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        // Header line with config
        out.push_str(
            &serde_json::to_string(&serde_json::json!({
                "type": "config",
                "n_modes": self.config_n_modes,
                "latent_dim": self.config_latent_dim,
                "eps": self.config_eps,
                "seed": self.config_seed,
                "schedule": self.config_schedule,
                "condition": self.config_condition,
                "match_policy": self.config_match_policy,
            }))
            .unwrap(),
        );
        out.push('\n');
        for entry in &self.entries {
            out.push_str(&serde_json::to_string(entry).unwrap());
            out.push('\n');
        }
        out
    }

    /// Action symbol sequence for trace pattern matching.
    pub fn action_sequence(&self) -> String {
        self.entries
            .iter()
            .map(|e| e.action.as_str())
            .collect::<Vec<_>>()
            .join("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header must carry everything `aria emit` needs to replay Φ
    /// without a matching `--config` — a regression test for the bug where
    /// only n_modes/latent_dim/eps were recorded and seed/schedule/
    /// condition/match_policy silently fell back to defaults on replay.
    #[test]
    fn header_round_trips_seed_schedule_condition_match_policy() {
        let trace = Trace::new(
            256,
            64,
            1.0,
            Some(7),
            "opdms",
            Condition::WorldModel,
            MatchPolicy::Merge,
        );
        let jsonl = trace.to_jsonl();
        let header: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();

        assert_eq!(header["seed"], serde_json::json!(7));
        assert_eq!(header["schedule"], serde_json::json!("opdms"));
        assert_eq!(header["condition"], serde_json::json!("world_model"));
        assert_eq!(header["match_policy"], serde_json::json!("merge"));
    }

    #[test]
    fn header_seed_is_null_when_run_had_none() {
        let trace = Trace::new(256, 64, 1.0, None, "opmd", Condition::Token, MatchPolicy::Identity);
        let jsonl = trace.to_jsonl();
        let header: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
        assert!(header["seed"].is_null());
    }
}
