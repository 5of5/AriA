//! Timed UT-0 ingest: `dataset_from_bytes` + serde-JSON vs shipped
//! `ingest_columnar` (fused encode + columnar write). Two pairs, both ≥ 10×.

use std::time::Instant;

use aria_engine_backends::dataset_from_bytes;
use aria_training::ingest_columnar;

const INGEST_N: usize = 64;
const INGEST_STRIDE: usize = 64;

fn main() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/docs_excerpt_32k.txt");
    let bytes = std::fs::read(path).expect("fixture");
    let _ = dataset_from_bytes("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE)
        .and_then(|ds| serde_json::to_string(&ds).map_err(|e| e.to_string()))
        .unwrap();
    let _ = ingest_columnar("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE).unwrap();

    for pair in 1..=2 {
        let t0 = Instant::now();
        let ds =
            dataset_from_bytes("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE).unwrap();
        let enc_json_us = t0.elapsed().as_secs_f64() * 1e6;
        let t0b = Instant::now();
        let json = serde_json::to_string(&ds).unwrap();
        let ser_us = t0b.elapsed().as_secs_f64() * 1e6;
        let json_us = enc_json_us + ser_us;

        let t1 = Instant::now();
        let (col, blob) =
            ingest_columnar("docs_excerpt_32k.txt", &bytes, INGEST_N, INGEST_STRIDE).unwrap();
        let col_us = t1.elapsed().as_secs_f64() * 1e6;
        let ratio = json_us / col_us.max(1e-9);

        let same = ds.trajectories[0]
            .iter()
            .zip(&col.trajectories[0])
            .all(|(a, b)| a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits()));
        println!(
            "pair={pair} json_us={json_us:.1} (encode={enc_json_us:.1} ser={ser_us:.1}) col_us={col_us:.1} ratio={ratio:.2} identical={same} json_bytes={} col_bytes={}",
            json.len(),
            blob.len()
        );
        if !same {
            eprintln!("FAIL: pair {pair} frames not bit-identical");
            std::process::exit(1);
        }
        if ratio < 10.0 {
            eprintln!("FAIL: pair {pair} full-path ratio {ratio:.3}× < 10");
            std::process::exit(1);
        }
    }
}
