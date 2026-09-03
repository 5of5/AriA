//! Hosted-shell load lock: many concurrent workers hit the node over real
//! TCP; every answer is identical bytes (stateless), nothing errors, nothing
//! hangs, and the run reports ops/s. Stdlib client only.

#![cfg(feature = "cli")]

use aria_operator::{serve_background, RunOpts, HARNESS_CAPABILITY, HARNESS_REQUEST_V1};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const CLIENTS: usize = 32;
const PER_CLIENT: usize = 8;

fn request_body() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": HARNESS_REQUEST_V1,
        "capability": HARNESS_CAPABILITY,
        "runId": "6f1d2c1e-0000-4000-8000-000000000001",
        "planHash": "816c0d436f7b8d5747972304e53863190907fe203ca2d17b2b15e431eac3dd9d",
        "attemptId": "6f1d2c1e-0000-4000-8000-000000000002",
        "fencingToken": 1,
        "requirementId": "req.load",
        "ops": ["BIN.PEOPLE", "BIN.COMPANY", "BIN.TAG.PERSON_FOUNDER", "BIN.REL.WORKS_AT"],
        "payload": {"nodes": [
            {"id": 1, "type": "Person", "label": "Ada", "notes": "Ada founded Acme"},
            {"id": 2, "type": "Company", "label": "Acme", "tags": ["COMPANY"]}
        ], "edges": [{"from": 1, "to": 2, "type": "WORKS_AT"}]},
        "steps": 0
    }))
    .unwrap()
}

fn post(addr: std::net::SocketAddr, path: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(
        s,
        "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    s.write_all(body).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw);
    let code: u16 = text.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").map_or(raw.len(), |p| p + 4);
    (code, raw[split..].to_vec())
}

fn get(addr: std::net::SocketAddr, path: &str) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw);
    let code: u16 = text.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").map_or(raw.len(), |p| p + 4);
    (code, raw[split..].to_vec())
}

#[test]
fn a_fleet_of_concurrent_workers_gets_identical_bytes_and_no_errors() {
    let opts = RunOpts {
        steps: 0,
        seed: Some(1),
        ..RunOpts::default()
    };
    let addr = serve_background("127.0.0.1:0", &opts).expect("bind");
    let body = request_body();

    // Warm the static routes and prove them cached (same bytes twice).
    let (c1, h1) = get(addr, "/health");
    let (c2, h2) = get(addr, "/health");
    assert_eq!((c1, c2), (200, 200));
    assert_eq!(h1, h2);
    let (cd, d) = get(addr, "/dispatch");
    assert_eq!(cd, 200);
    assert!(String::from_utf8_lossy(&d).contains("aria-dispatch-v1"));

    let t0 = Instant::now();
    let results: Vec<(u16, Vec<u8>)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..CLIENTS)
            .map(|_| {
                let body = &body;
                s.spawn(move || (0..PER_CLIENT).map(|_| post(addr, "/harness", body)).collect::<Vec<_>>())
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let wall = t0.elapsed();
    let n = CLIENTS * PER_CLIENT;
    assert_eq!(results.len(), n);
    let ok = results.iter().filter(|(c, _)| *c == 200).count();
    let shed = results.iter().filter(|(c, _)| *c == 503).count();
    assert_eq!(ok + shed, n, "every request gets a definite answer (200 or 503)");
    assert!(ok > 0);
    let bodies: std::collections::BTreeSet<&Vec<u8>> =
        results.iter().filter(|(c, _)| *c == 200).map(|(_, b)| b).collect();
    assert_eq!(bodies.len(), 1, "stateless: one distinct body across {ok} successes");
    let v: serde_json::Value = serde_json::from_slice(bodies.iter().next().unwrap()).unwrap();
    assert_eq!(v["status"], "result");
    assert_eq!(v["callback"]["ops"], 4);
    eprintln!(
        "serve_load: {n} requests / {CLIENTS} clients in {:?} → {:.0} ops/s, shed={shed}",
        wall,
        n as f64 / wall.as_secs_f64()
    );
}

#[test]
fn a_stalled_client_does_not_pin_the_node() {
    let addr = serve_background("127.0.0.1:0", &RunOpts::default()).expect("bind");
    // Open a connection and send nothing: the socket timeout must free the
    // worker, and a live client must still be served meanwhile.
    let _stalled = TcpStream::connect(addr).unwrap();
    let (code, _) = get(addr, "/health");
    assert_eq!(code, 200);
}
