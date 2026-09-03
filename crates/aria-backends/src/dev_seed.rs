//! DEV-ONLY seed I/O — not a Φ operator, not WS8 ingest.
//!
//! Loads an `aria-dev-seed-v1` document (text nodes + typed edges), embeds
//! each node's text with the same spectral encoder + `SimPredictor::embed`
//! the engine uses, and returns an Inv3-valid [`Graph`]. Used by the
//! Seed probe so Aria sees *real* entity text as points of 𝒵.

use aria_engine_core::engine::Predictor;
use aria_engine_core::error::AriaError;
use aria_engine_core::graph::{EdgeType, Graph, GraphOp, NodeType};
use serde::{Deserialize, Serialize};

use crate::data::encode_window;
use crate::predictor::SimPredictor;

/// On-disk tag. Anything else is refused.
pub const DEV_SEED_FORMAT: &str = "aria-dev-seed-v1";

/// A probe seed: human-readable nodes, embeddings computed at load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevSeed {
    /// Must be [`DEV_SEED_FORMAT`].
    pub format: String,
    /// Entity nodes.
    pub nodes: Vec<DevSeedNode>,
    /// Typed edges by node id.
    #[serde(default)]
    pub edges: Vec<DevSeedEdge>,
}

/// One entity to embed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevSeedNode {
    /// Arena id (must be unique, monotone preferred).
    pub id: u64,
    /// Display label (note title).
    pub label: String,
    /// Wire node type (`observation`, `custom:company`, …).
    pub ntype: String,
    /// Text that becomes I(encode_window(·)).
    pub text: String,
}

/// One typed edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevSeedEdge {
    /// Source id.
    pub from: u64,
    /// Target id.
    pub to: u64,
    /// Wire edge type.
    pub rel: String,
}

/// Embed every node and assemble an Inv3-valid graph.
pub fn graph_from_dev_seed(
    seed: &DevSeed,
    n_modes: usize,
    latent_dim: usize,
) -> Result<Graph, AriaError> {
    if seed.format != DEV_SEED_FORMAT {
        return Err(AriaError::Config(format!(
            "dev seed format '{}' (expected '{DEV_SEED_FORMAT}')",
            seed.format
        )));
    }
    if n_modes == 0 || latent_dim == 0 {
        return Err(AriaError::Config(
            "n_modes and latent_dim must be > 0".into(),
        ));
    }
    let predictor = SimPredictor::new(n_modes, latent_dim);
    let mut g = Graph::empty();
    for n in &seed.nodes {
        let window = pad_window(n.text.as_bytes(), n_modes);
        let psi = encode_window(&window, n_modes);
        let emb = predictor.embed(&psi);
        let ntype = NodeType::from_wire(&n.ntype);
        g.apply(
            &GraphOp::AddNode {
                id: n.id,
                ntype,
                emb,
                ts: 0,
            },
            latent_dim,
        )
        .map_err(|e| AriaError::Config(e.to_string()))?;
    }
    for e in &seed.edges {
        g.apply(
            &GraphOp::AddEdge {
                from: e.from,
                to: e.to,
                etype: EdgeType::from_wire(&e.rel),
            },
            latent_dim,
        )
        .map_err(|e| AriaError::Config(e.to_string()))?;
    }
    if !g.ok(latent_dim) {
        return Err(AriaError::Config(
            "dev seed produced a graph that fails GraphOK".into(),
        ));
    }
    Ok(g)
}

/// Load [`DevSeed`] or a raw [`Graph`] JSON (the latter must already be embedded).
pub fn load_seed_graph(
    path: &std::path::Path,
    n_modes: usize,
    latent_dim: usize,
) -> Result<Graph, AriaError> {
    let src = std::fs::read_to_string(path).map_err(|e| AriaError::Config(e.to_string()))?;
    if let Ok(seed) = serde_json::from_str::<DevSeed>(&src) {
        if seed.format == DEV_SEED_FORMAT {
            return graph_from_dev_seed(&seed, n_modes, latent_dim);
        }
    }
    serde_json::from_str::<Graph>(&src).map_err(|e| {
        AriaError::Config(format!(
            "seed graph is neither {DEV_SEED_FORMAT} nor a Graph: {e}"
        ))
    })
}

fn pad_window(bytes: &[u8], n_modes: usize) -> Vec<u8> {
    if bytes.len() >= n_modes {
        return bytes[..n_modes].to_vec();
    }
    let mut w = bytes.to_vec();
    w.resize(n_modes, 0);
    w
}
