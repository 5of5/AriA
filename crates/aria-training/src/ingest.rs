//! UT-0 in-repo columnar ingest (Q-2026-08-17-1 fallback).
//!
//! The referee is [`aria_engine_backends::dataset_from_bytes`]: same windows,
//! same `encode_window` phasors, same flattened frames. This module writes
//! those frames as little-endian f64 columns instead of serde-JSON decimals,
//! which is the 10× wall-clock win against the JSON path.

use std::path::{Path, PathBuf};

use aria_engine_backends::data::{
    decode_columnar, encode_columnar, ingest_columnar as backends_ingest, FieldDataset,
};

use crate::sha256::sha256_hex;
use crate::TrainingError;

pub use aria_engine_backends::data::{ingest_columnar, COLUMNAR_MAGIC};

/// Binding live-docs corpus rule (smoke.rs / WS5-comparable): README.md,
/// ideas.md, every `docs/*.md` and `docs/*/*.md`, every `spec/*.md`, sorted
/// by relative path. Returns (relative paths, concatenated bytes).
pub fn live_docs_corpus(root: &Path) -> Result<(Vec<String>, Vec<u8>), TrainingError> {
    let mut files: Vec<String> = vec!["README.md".into(), "ideas.md".into()];
    let push_md = |dir: &Path, out: &mut Vec<String>| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().is_some_and(|e| e == "md") {
                    if let Ok(rel) = p.strip_prefix(root) {
                        out.push(rel.to_string_lossy().into_owned());
                    }
                }
            }
        }
    };
    push_md(&root.join("docs"), &mut files);
    if let Ok(entries) = std::fs::read_dir(root.join("docs")) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                push_md(&p, &mut files);
            }
        }
    }
    push_md(&root.join("spec"), &mut files);
    files.retain(|f| root.join(f).is_file());
    files.sort();
    files.dedup();
    let mut corpus = Vec::new();
    for f in &files {
        corpus.extend_from_slice(&std::fs::read(root.join(f))?);
    }
    if corpus.len() < 8 {
        return Err(TrainingError::Dataset(format!(
            "live-docs corpus too small ({} bytes) under {}",
            corpus.len(),
            root.display()
        )));
    }
    Ok((files, corpus))
}

/// Repo root two levels above this crate (crates/aria-training → repo).
pub fn crate_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate lives two levels under the repo root")
        .to_path_buf()
}

/// Ingest `bytes` via the shipped columnar path and return the referee
/// dataset, the columnar blob, and the blob's SHA-256.
pub fn ingest_with_hash(
    source: &str,
    bytes: &[u8],
    n_modes: usize,
    stride: usize,
) -> Result<(FieldDataset, Vec<u8>, String), TrainingError> {
    let (ds, blob) =
        backends_ingest(source, bytes, n_modes, stride).map_err(TrainingError::Dataset)?;
    let sha = sha256_hex(&blob);
    Ok((ds, blob, sha))
}

/// Reload a columnar blob and confirm frames match `ds` bit-for-bit.
pub fn assert_columnar_roundtrip(ds: &FieldDataset, blob: &[u8]) -> Result<(), TrainingError> {
    let back = decode_columnar(blob).map_err(TrainingError::Dataset)?;
    if back.n_modes != ds.n_modes || back.source_bytes != ds.source_bytes {
        return Err(TrainingError::Dataset(
            "columnar round-trip header mismatch".into(),
        ));
    }
    let a = ds.trajectories.first().map_or(&[][..], Vec::as_slice);
    let b = back.trajectories.first().map_or(&[][..], Vec::as_slice);
    if a.len() != b.len() {
        return Err(TrainingError::Dataset(format!(
            "columnar frame count {} != {}",
            b.len(),
            a.len()
        )));
    }
    for (i, (fa, fb)) in a.iter().zip(b).enumerate() {
        if fa.len() != fb.len() {
            return Err(TrainingError::Dataset(format!(
                "columnar frame {i} length mismatch"
            )));
        }
        for (x, y) in fa.iter().zip(fb) {
            if x.to_bits() != y.to_bits() {
                return Err(TrainingError::Dataset(format!(
                    "columnar frame {i} bit mismatch"
                )));
            }
        }
    }
    let _ = encode_columnar(ds);
    Ok(())
}
