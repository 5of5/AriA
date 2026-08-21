//! UT-0: in-repo columnar ingest vs the serde-JSON `dataset_from_bytes` referee.

use std::time::Instant;

use aria_engine_backends::{dataset_from_bytes, decode_columnar};
use aria_training::{assert_columnar_roundtrip, ingest_columnar};

/// Fixture protocol: docs_excerpt_32k, N=64, stride=64 (existing encoder).
const INGEST_N: usize = 64;
const INGEST_STRIDE: usize = 64;

fn excerpt() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/docs_excerpt_32k.txt"
    ))
    .expect("fixture present")
}

fn frames_bits_eq(a: &[Vec<f64>], b: &[Vec<f64>]) {
    assert_eq!(a.len(), b.len());
    for (i, (fa, fb)) in a.iter().zip(b).enumerate() {
        assert_eq!(fa.len(), fb.len(), "frame {i}");
        for (x, y) in fa.iter().zip(fb) {
            assert_eq!(x.to_bits(), y.to_bits(), "frame {i} bit mismatch");
        }
    }
}

fn check_identity(bytes: &[u8], n_modes: usize, stride: usize) {
    let json_ds = dataset_from_bytes("docs_excerpt_32k.txt", bytes, n_modes, stride).unwrap();
    let (col_ds, blob) = ingest_columnar("docs_excerpt_32k.txt", bytes, n_modes, stride).unwrap();
    assert_eq!(json_ds.n_modes, col_ds.n_modes);
    assert_eq!(json_ds.source_bytes, col_ds.source_bytes);
    frames_bits_eq(&json_ds.trajectories[0], &col_ds.trajectories[0]);
    assert_columnar_roundtrip(&col_ds, &blob).unwrap();
    let decoded = decode_columnar(&blob).unwrap();
    frames_bits_eq(&json_ds.trajectories[0], &decoded.trajectories[0]);
    assert!(blob.starts_with(aria_training::COLUMNAR_MAGIC));
}

#[test]
fn columnar_frames_are_bit_identical_to_dataset_from_bytes() {
    let bytes = excerpt();
    check_identity(&bytes, INGEST_N, INGEST_STRIDE);
}

#[test]
fn ingest_columnar_is_at_least_ten_times_faster_than_json_path() {
    let bytes = excerpt();
    let _ = dataset_from_bytes("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE)
        .and_then(|ds| serde_json::to_string(&ds).map_err(|e| e.to_string()))
        .unwrap();
    let _ = ingest_columnar("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE).unwrap();

    let mut ratios = Vec::new();
    for pair in 0..2 {
        let t0 = Instant::now();
        let ds =
            dataset_from_bytes("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE).unwrap();
        let json = serde_json::to_string(&ds).unwrap();
        let json_ns = t0.elapsed().as_nanos().max(1);

        let t1 = Instant::now();
        let (col, blob) =
            ingest_columnar("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE).unwrap();
        let col_ns = t1.elapsed().as_nanos().max(1);

        frames_bits_eq(&ds.trajectories[0], &col.trajectories[0]);
        assert!(blob.starts_with(b"ARIACOL1"));
        let ratio = json_ns as f64 / col_ns as f64;
        ratios.push(ratio);
        eprintln!(
            "full-path N={INGEST_N} pair {pair}: json={json_ns} ns  col={col_ns} ns  ratio={ratio:.2}×  json_bytes={} col_bytes={}",
            json.len(),
            blob.len()
        );
    }
    for (i, r) in ratios.iter().enumerate() {
        assert!(
            *r >= 10.0,
            "pair {i}: ingest_columnar must be ≥ 10× vs dataset_from_bytes+JSON, got {r:.3}×"
        );
    }
}
