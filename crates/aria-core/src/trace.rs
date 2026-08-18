use serde::{Deserialize, Serialize};

use crate::action::Action;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub config_n_modes: usize,
    pub config_latent_dim: usize,
    pub config_eps: f64,
    pub entries: Vec<TraceEntry>,
}

impl Trace {
    pub fn new(n_modes: usize, latent_dim: usize, eps: f64) -> Self {
        Trace {
            config_n_modes: n_modes,
            config_latent_dim: latent_dim,
            config_eps: eps,
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
