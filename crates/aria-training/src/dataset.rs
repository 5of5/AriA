//! Dataset loading, trajectory batching, and KG edge alignment (WS-A1 serde path).
//!
//! The batcher reproduces the WS5 statistical protocol exactly:
//! frames are cut into **non-overlapping trajectories of `trajectory_len`**
//! (WS5: 2,862 frames → 357 trajectories of length 8) and the holdout is a
//! **time split** — the last `holdout_frac` of trajectories in corpus order
//! (WS5: 0.4 → n = 143). Train order is shuffled per epoch with the seeded
//! LCG; the holdout is never shuffled and never trained on.
//!
//! ℒ_Graph substrate (amendment D3, ARIA-TRAINING-PRD v7.3 admission table):
//! typed edges from a real public knowledge-graph excerpt
//! (`aria-kg-edges-v1`), aligned to text windows by exact string match.
//! Alignment re-derives the window boundaries from the raw corpus bytes with
//! the same loop `aria dataset` used, and rejects on any frame-count mismatch
//! — a stride or corpus mismatch never silently misaligns edges.

use std::collections::BTreeMap;
use std::path::Path;

use aria_engine_backends::data::FieldDataset;
use serde::{Deserialize, Serialize};

use crate::sha256::sha256_hex;
use crate::{fnv1a64_hex, Lcg, TrainingConfig, TrainingError};

/// Format tag accepted for training (admission table: production text corpus).
pub const TEXT_DATASET_FORMAT: &str = "aria-text-dataset-v1";
/// Format tag refused for training (admission table: smoke tests only).
pub const OPTICAL_DATASET_FORMAT: &str = "aria-optical-dataset-v1";
/// Format tag of the KG edge-list artifact (amendment D3).
pub const KG_EDGES_FORMAT: &str = "aria-kg-edges-v1";

/// A dataset prepared for training: flat frame storage plus trajectory chunks
/// and the WS5 time split.
#[derive(Debug, Clone)]
pub struct PreparedDataset {
    /// 2·n_modes — the flattened frame dimension.
    pub frame_dim: usize,
    /// Global frame storage, corpus order (window index = global frame index).
    pub frames: Vec<Vec<f64>>,
    /// Trajectories: each is `trajectory_len` consecutive global frame indices.
    pub chunks: Vec<Vec<usize>>,
    /// `chunks[..n_train]` are the train split (corpus order); the rest is holdout.
    pub n_train: usize,
    /// Provenance carried into the receipt.
    pub format: String,
    pub source: String,
    pub source_bytes: usize,
    /// FNV-1a-64 fingerprint (continuity with the WS-A1 receipt).
    pub checksum: String,
    /// SHA-256 of the artifact bytes (cryptographic provenance, Gate 6; empty
    /// when the dataset was constructed in memory rather than loaded).
    pub checksum_sha256: String,
}

impl PreparedDataset {
    /// Load the artifact at `cfg.data_path` and prepare the WS5 split.
    pub fn load(cfg: &TrainingConfig) -> Result<Self, TrainingError> {
        let bytes = std::fs::read(&cfg.data_path)?;
        let checksum = fnv1a64_hex(&bytes);
        let sha = sha256_hex(&bytes);
        let ds: FieldDataset = if bytes.starts_with(aria_engine_backends::COLUMNAR_MAGIC) {
            aria_engine_backends::decode_columnar(&bytes).map_err(TrainingError::Dataset)?
        } else {
            serde_json::from_slice(&bytes)?
        };
        let mut prepared = Self::from_field_dataset(&ds, checksum, cfg)?;
        prepared.checksum_sha256 = sha;
        Ok(prepared)
    }

    /// Prepare from an in-memory [`FieldDataset`] (tests and WS-A2 callers).
    pub fn from_field_dataset(
        ds: &FieldDataset,
        checksum: String,
        cfg: &TrainingConfig,
    ) -> Result<Self, TrainingError> {
        if ds.format == OPTICAL_DATASET_FORMAT {
            return Err(TrainingError::Dataset(format!(
                "'{OPTICAL_DATASET_FORMAT}' is smoke-tests-only, never training data \
                 (corpus admission table, ARIA-TRAINING-PRD v7.3)"
            )));
        }
        if ds.format != TEXT_DATASET_FORMAT {
            return Err(TrainingError::Dataset(format!(
                "unsupported dataset format '{}' (expected '{TEXT_DATASET_FORMAT}')",
                ds.format
            )));
        }
        if ds.n_modes != cfg.n_modes {
            return Err(TrainingError::Dataset(format!(
                "dataset n_modes = {} does not match config n_modes = {}",
                ds.n_modes, cfg.n_modes
            )));
        }
        let frame_dim = 2 * ds.n_modes;

        let mut frames: Vec<Vec<f64>> = Vec::new();
        let mut chunks: Vec<Vec<usize>> = Vec::new();
        for (ti, traj) in ds.trajectories.iter().enumerate() {
            let base = frames.len();
            for (fi, frame) in traj.iter().enumerate() {
                if frame.len() != frame_dim {
                    return Err(TrainingError::Dataset(format!(
                        "trajectory {ti} frame {fi} has {} components, expected {frame_dim}",
                        frame.len()
                    )));
                }
                if frame.iter().any(|x| !x.is_finite()) {
                    return Err(TrainingError::Dataset(format!(
                        "trajectory {ti} frame {fi} contains a non-finite component"
                    )));
                }
                frames.push(frame.clone());
            }
            // Non-overlapping chunks of trajectory_len; the tail shorter than
            // a full trajectory is dropped (WS5: 2862 → 357 × 8).
            let n_frames = traj.len();
            let mut start = 0;
            while start + cfg.trajectory_len <= n_frames {
                chunks.push((base + start..base + start + cfg.trajectory_len).collect());
                start += cfg.trajectory_len;
            }
        }

        if chunks.is_empty() {
            return Err(TrainingError::Dataset(format!(
                "no full trajectory of length {} exists in the dataset ({} frames total)",
                cfg.trajectory_len,
                frames.len()
            )));
        }

        // Time split: floor(n·(1−h)) train from the front, the rest holdout.
        // WS5 check: 357 chunks, h = 0.4 → 214 train / 143 holdout.
        // Cast safety: holdout_frac ∈ (0, 1) is validated, so the product lies
        // in [0, n_chunks) — non-negative and in usize range by construction.
        let n_chunks = chunks.len();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n_train = ((n_chunks as f64) * (1.0 - cfg.holdout_frac)).floor() as usize;
        if n_train == 0 || n_train == n_chunks {
            return Err(TrainingError::Dataset(format!(
                "time split degenerate: {n_chunks} trajectories at holdout_frac = {} \
                 leave train = {n_train}, holdout = {} — both splits must be non-empty",
                cfg.holdout_frac,
                n_chunks - n_train
            )));
        }

        Ok(PreparedDataset {
            frame_dim,
            frames,
            chunks,
            n_train,
            format: ds.format.clone(),
            source: ds.source.clone(),
            source_bytes: ds.source_bytes,
            checksum,
            checksum_sha256: String::new(),
        })
    }

    /// Deterministic per-epoch order of train-trajectory indices: Fisher–Yates
    /// driven by the seeded LCG. The holdout tail is untouched by construction
    /// (only indices `0..n_train` are emitted).
    pub fn epoch_order(&self, seed: u64, epoch: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.n_train).collect();
        // Distinct stream per epoch: golden-ratio offset, same discipline as
        // the repo's other seeded streams.
        let mut rng = Lcg(seed ^ (epoch as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        for i in (1..order.len()).rev() {
            let j = rng.index(i + 1);
            order.swap(i, j);
        }
        order
    }

    /// Holdout trajectory indices (corpus order, never shuffled).
    pub fn holdout(&self) -> impl Iterator<Item = usize> + '_ {
        self.n_train..self.chunks.len()
    }
}

/// One typed edge of the KG excerpt: concept indices + relation index + weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgEdge {
    pub s: u32,
    pub e: u32,
    pub r: u32,
    pub w: f64,
}

/// The `aria-kg-edges-v1` artifact (amendment D3): a real public-KG excerpt
/// with full provenance. Never a quality-gate corpus; never map-quality claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgEdges {
    pub format: String,
    /// Where the edges come from (canonical dump URL).
    pub source: String,
    /// When the dump was retrieved (UTC date).
    pub retrieved: String,
    /// The exact filter predicate that produced the excerpt.
    pub filter: String,
    /// Upstream license notice.
    pub license: String,
    /// Concept string table (`/c/en/<term>` terms, bare).
    pub concepts: Vec<String>,
    /// Relation string table (`/r/<Relation>` names).
    pub relations: Vec<String>,
    /// Typed edges over the string tables.
    pub edges: Vec<KgEdge>,
}

impl KgEdges {
    /// Load and validate the artifact; returns (edges, fnv1a64, sha256).
    pub fn load(path: &Path) -> Result<(Self, String, String), TrainingError> {
        let bytes = std::fs::read(path)?;
        let checksum = fnv1a64_hex(&bytes);
        let sha = sha256_hex(&bytes);
        let kg: KgEdges = serde_json::from_slice(&bytes)?;
        kg.validate()?;
        Ok((kg, checksum, sha))
    }

    /// Structural validation: format tag, index ranges, no self-loops,
    /// finite positive weights.
    pub fn validate(&self) -> Result<(), TrainingError> {
        if self.format != KG_EDGES_FORMAT {
            return Err(TrainingError::Dataset(format!(
                "unsupported edge-list format '{}' (expected '{KG_EDGES_FORMAT}')",
                self.format
            )));
        }
        let nc = self.concepts.len();
        let nr = self.relations.len();
        for (i, edge) in self.edges.iter().enumerate() {
            if edge.s as usize >= nc || edge.e as usize >= nc {
                return Err(TrainingError::Dataset(format!(
                    "edge {i} references concept {} / {} outside the table (|concepts| = {nc})",
                    edge.s, edge.e
                )));
            }
            if edge.r as usize >= nr {
                return Err(TrainingError::Dataset(format!(
                    "edge {i} references relation {} outside the table (|relations| = {nr})",
                    edge.r
                )));
            }
            if edge.s == edge.e {
                return Err(TrainingError::Dataset(format!("edge {i} is a self-loop")));
            }
            if !edge.w.is_finite() || edge.w <= 0.0 {
                return Err(TrainingError::Dataset(format!(
                    "edge {i} weight {} must be finite and > 0",
                    edge.w
                )));
            }
        }
        Ok(())
    }
}

/// The window→concept alignment of a corpus against a KG excerpt.
#[derive(Debug, Clone)]
pub struct EdgeAlignment {
    /// Per global window index: sorted, deduplicated concept ids present.
    pub window_concepts: Vec<Vec<u32>>,
    /// Per concept id: sorted list of windows containing it (inverse map).
    pub concept_windows: BTreeMap<u32, Vec<usize>>,
    /// Fraction of windows with ≥ 1 aligned concept.
    pub coverage: f64,
    /// Edges whose BOTH endpoints occur somewhere in the corpus, deduplicated
    /// on (s, e) with the first relation kept, sorted — the ℒ_Graph pool.
    pub usable_edges: Vec<KgEdge>,
    /// FNV-1a-64 checksum of the raw corpus bytes.
    pub corpus_checksum: String,
}

/// Re-derive window boundaries exactly as `aria-backends::data::encode_corpus`
/// does (start += stride; window = [start, min(start+n_modes, len)); keep only
/// windows ≥ 8 bytes) and align each window's tokens to KG concepts by exact
/// string match.
///
/// `expected_windows` guards against stride/corpus mismatches: if the
/// re-derived window count differs from the dataset's frame count, the
/// alignment is rejected rather than silently shifted.
pub fn align_corpus(
    corpus: &[u8],
    n_modes: usize,
    stride: usize,
    expected_windows: usize,
    kg: &KgEdges,
) -> Result<EdgeAlignment, TrainingError> {
    let corpus_checksum = fnv1a64_hex(corpus);

    // Concept lookup table: term → id. BTreeMap for deterministic iteration.
    let mut concept_ids: BTreeMap<&str, u32> = BTreeMap::new();
    for (i, c) in kg.concepts.iter().enumerate() {
        let id = u32::try_from(i)
            .map_err(|_| TrainingError::Dataset("concept table exceeds the u32 id space".into()))?;
        concept_ids.insert(c.as_str(), id);
    }

    let mut window_concepts: Vec<Vec<u32>> = Vec::new();
    let mut start = 0usize;
    while start < corpus.len() {
        let end = (start + n_modes).min(corpus.len());
        if end - start < 8 {
            break;
        }
        window_concepts.push(window_to_concepts(&corpus[start..end], &concept_ids));
        start += stride;
    }

    if window_concepts.len() != expected_windows {
        return Err(TrainingError::Dataset(format!(
            "alignment mismatch: corpus yields {} windows at n_modes = {n_modes}, \
             stride = {stride}, but the dataset holds {expected_windows} frames — \
             corpus_path/stride do not match the dataset artifact",
            window_concepts.len()
        )));
    }

    let mut concept_windows: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (w, concepts) in window_concepts.iter().enumerate() {
        for &c in concepts {
            concept_windows.entry(c).or_default().push(w);
        }
    }

    let covered = window_concepts.iter().filter(|c| !c.is_empty()).count();
    let coverage = if window_concepts.is_empty() {
        0.0
    } else {
        covered as f64 / window_concepts.len() as f64
    };

    // Usable pool: both endpoints present somewhere; dedup on (s, e).
    let mut seen = std::collections::BTreeSet::new();
    let mut usable: Vec<KgEdge> = Vec::new();
    for edge in &kg.edges {
        if concept_windows.contains_key(&edge.s)
            && concept_windows.contains_key(&edge.e)
            && seen.insert((edge.s, edge.e))
        {
            usable.push(edge.clone());
        }
    }
    usable.sort_by_key(|e| (e.s, e.e));

    Ok(EdgeAlignment {
        window_concepts,
        concept_windows,
        coverage,
        usable_edges: usable,
        corpus_checksum,
    })
}

/// Tokenize a window's bytes (lowercased ASCII alphanumeric runs of length ≥ 3)
/// and return the sorted, deduplicated concept ids present in the table.
fn window_to_concepts(window: &[u8], concept_ids: &BTreeMap<&str, u32>) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut Vec<u32>| {
        if token.len() >= 3 {
            if let Some(&id) = concept_ids.get(token.as_str()) {
                out.push(id);
            }
        }
        token.clear();
    };
    for &b in window {
        match b {
            b'a'..=b'z' | b'0'..=b'9' => token.push(b as char),
            b'A'..=b'Z' => token.push((b + 32) as char),
            _ => flush(&mut token, &mut out),
        }
    }
    flush(&mut token, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}
