use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::condition::Condition;
use crate::graph::Graph;
use crate::policy::{DiffPolicy, MatchPolicy};

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
/// `latent_dim`, and `eps` alone are not enough. `seed`, `schedule`,
/// `condition`, and `match_policy` select the trajectory; `diff_policy`
/// (consumed by `Engine::apply`), `stutter_k` (consumed by the scheduler),
/// and `optical`/`merge_tau` (consumed by `engine_with` when the backends are
/// built) all change the replayed latents just as much. `initial_graph`
/// records `G₀` so a run seeded with a non-empty graph is self-describing
/// rather than silently replayed from `Graph::empty()`. Omitting any of these
/// makes replay silently diverge from the run that produced the trace (see
/// `aria emit`'s doc comment).
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
    /// Diffusion policy for `Diff_G(z)` — consumed by `Engine::apply`.
    pub config_diff_policy: DiffPolicy,
    /// Stutter budget K (𝐂5) — consumed by the scheduler.
    pub config_stutter_k: u64,
    /// Optical backend selection (`"fft"`/`"householder"`, `None` = automatic)
    /// — consumed by `engine_with` when the backend is built.
    pub config_optical: Option<String>,
    /// Graph merge distance threshold τ (spec §0.4) — consumed by
    /// `engine_with` via `SimGraphBackend::with_merge_tau`.
    pub config_merge_tau: f64,
    /// The graph the run started from (`G₀`). `None` iff the run started from
    /// `Graph::empty()`; recorded (not just a flag) so a seeded run stays
    /// self-describing. `aria emit` replays from an empty graph, so it must
    /// reject a trace whose `initial_graph` is non-empty rather than silently
    /// diverge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_graph: Option<Graph>,
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
        diff_policy: DiffPolicy,
        stutter_k: u64,
        optical: Option<String>,
        merge_tau: f64,
        initial_graph: Option<Graph>,
    ) -> Self {
        Trace {
            config_n_modes: n_modes,
            config_latent_dim: latent_dim,
            config_eps: eps,
            config_seed: seed,
            config_schedule: schedule.to_string(),
            config_condition: condition,
            config_match_policy: match_policy,
            config_diff_policy: diff_policy,
            config_stutter_k: stutter_k,
            config_optical: optical,
            config_merge_tau: merge_tau,
            initial_graph,
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
        // Header line with config. `initial_graph` is omitted (not null) when
        // the run started from `Graph::empty()`, mirroring the struct's
        // `skip_serializing_if` so empty-graph headers stay clean.
        let mut header = serde_json::json!({
            "type": "config",
            "n_modes": self.config_n_modes,
            "latent_dim": self.config_latent_dim,
            "eps": self.config_eps,
            "seed": self.config_seed,
            "schedule": self.config_schedule,
            "condition": self.config_condition,
            "match_policy": self.config_match_policy,
            "diff_policy": self.config_diff_policy,
            "stutter_k": self.config_stutter_k,
            "optical": self.config_optical,
            "merge_tau": self.config_merge_tau,
        });
        if let Some(ref g0) = self.initial_graph {
            header["initial_graph"] = serde_json::json!(g0);
        }
        out.push_str(&serde_json::to_string(&header).unwrap());
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
            DiffPolicy::GraphConditioned,
            3,
            Some("fft".to_string()),
            0.7,
            None,
        );
        let jsonl = trace.to_jsonl();
        let header: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();

        assert_eq!(header["seed"], serde_json::json!(7));
        assert_eq!(header["schedule"], serde_json::json!("opdms"));
        assert_eq!(header["condition"], serde_json::json!("world_model"));
        assert_eq!(header["match_policy"], serde_json::json!("merge"));
    }

    /// The header must also carry the inputs that change the replayed latents
    /// but were silently dropped before: `diff_policy` (consumed by
    /// `Engine::apply`), `stutter_k` (the scheduler), and `optical`/
    /// `merge_tau` (`engine_with`). Regression test for the review finding
    /// that `diff_policy = "graph_conditioned"` replayed as `Identity`.
    #[test]
    fn header_round_trips_diff_policy_stutter_k_optical_merge_tau() {
        let trace = Trace::new(
            256,
            64,
            1.0,
            Some(7),
            "opmd",
            Condition::Token,
            MatchPolicy::Merge,
            DiffPolicy::GraphConditioned,
            4,
            Some("householder".to_string()),
            0.25,
            None,
        );
        let jsonl = trace.to_jsonl();
        let header: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();

        assert_eq!(header["diff_policy"], serde_json::json!("graph_conditioned"));
        assert_eq!(header["stutter_k"], serde_json::json!(4));
        assert_eq!(header["optical"], serde_json::json!("householder"));
        assert_eq!(header["merge_tau"], serde_json::json!(0.25));
        // An empty G₀ is omitted from the header entirely, not stored as null.
        assert!(header.get("initial_graph").is_none());
    }

    /// A run seeded with a non-empty `G₀` records it so the trace is
    /// self-describing; `aria emit` must reject it rather than replay from
    /// `Graph::empty()`.
    #[test]
    fn header_records_a_non_empty_initial_graph() {
        let g0 = Graph::seed(crate::graph::GraphNode {
            id: 0,
            embedding: vec![0.0; 64],
            node_type: crate::graph::NodeType::Observation,
            timestamp: 0,
        });
        let trace = Trace::new(
            256,
            64,
            1.0,
            Some(7),
            "opmd",
            Condition::Token,
            MatchPolicy::Identity,
            DiffPolicy::Identity,
            2,
            None,
            0.5,
            Some(g0),
        );
        let jsonl = trace.to_jsonl();
        let header: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();

        let recorded = header
            .get("initial_graph")
            .expect("a non-empty G₀ must be recorded");
        let recorded: Graph = serde_json::from_value(recorded.clone()).unwrap();
        assert_eq!(recorded.node_count(), 1);
    }

    #[test]
    fn header_seed_is_null_when_run_had_none() {
        let trace = Trace::new(
            256,
            64,
            1.0,
            None,
            "opmd",
            Condition::Token,
            MatchPolicy::Identity,
            DiffPolicy::Identity,
            2,
            None,
            0.5,
            None,
        );
        let jsonl = trace.to_jsonl();
        let header: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
        assert!(header["seed"].is_null());
    }
}
