//! `aria node` — the binary contract a host depends on.
//!
//! Two things are being pinned here that library tests cannot reach:
//!
//! 1. **Exit codes are branchable.** A coordinator must be able to tell a
//!    malformed payload (2) from an inadmissible Φ state (1) from a disk
//!    problem (3) without parsing stderr.
//! 2. **stdout is the document and nothing else.** Diagnostics go to stderr,
//!    and on *any* failure the primary sink is left untouched — a partial
//!    envelope must never be mistakable for a result (L8).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const OK: i32 = 0;
const INVARIANT: i32 = 1;
const CONFIG: i32 = 2;
const IO: i32 = 3;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../aria-backends/fixtures")
        .join(name)
}

fn node() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_aria"));
    c.arg("node")
        .arg("--n-modes")
        .arg("64")
        .arg("--latent-dim")
        .arg("32")
        .arg("--steps")
        .arg("16")
        .arg("--seed")
        .arg("1");
    c
}

struct Run {
    code: i32,
    stdout: Vec<u8>,
    stderr: String,
}

fn run(cmd: &mut Command) -> Run {
    let out = cmd.output().expect("spawn aria");
    Run {
        code: out.status.code().expect("exit code"),
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn run_sheet() -> Run {
    run(node().arg("--in").arg(fixture("tabular_market_sheet.json")))
}

// ---------------------------------------------------------------------------
// The happy path
// ---------------------------------------------------------------------------

#[test]
fn a_spreadsheet_exits_zero_with_a_valid_document_on_stdout() {
    let r = run_sheet();
    assert_eq!(r.code, OK, "stderr: {}", r.stderr);

    let doc: Value = serde_json::from_slice(&r.stdout).expect("stdout must be one JSON document");
    assert_eq!(doc["schema"], "aria-telemetry-query-v1");
    assert_eq!(doc["version"], 1);
    assert_eq!(doc["receipt"]["invariants_ok"], Value::Bool(true));
}

/// stdout carries the document and nothing else — no banner, no progress, no
/// trailing summary. A host pipes it straight into a parser.
#[test]
fn stdout_is_json_only_and_diagnostics_go_to_stderr() {
    let r = run_sheet();
    let text = String::from_utf8(r.stdout).unwrap();
    assert!(text.starts_with('{'), "stdout must begin the document");
    assert_eq!(
        text.trim_end().lines().count(),
        1,
        "compact output is a single line"
    );
    serde_json::from_str::<Value>(&text).expect("parses whole");
}

#[test]
fn the_payload_can_arrive_on_stdin() {
    use std::io::Write;
    use std::process::Stdio;

    let payload = std::fs::read(fixture("tabular_market_sheet.json")).unwrap();
    let mut child = node()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(&payload)
        .expect("write payload");
    let out = child.wait_with_output().expect("wait");

    assert_eq!(out.status.code(), Some(OK));
    let doc: Value = serde_json::from_slice(&out.stdout).expect("valid document from stdin");
    assert_eq!(doc["schema"], "aria-telemetry-query-v1");
}

#[test]
fn dash_means_stdin() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = node()
        .arg("--in")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"notes":["alpha","beta"]}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(OK));
}

#[test]
fn an_explicit_graph_payload_also_works() {
    let r = run(node().arg("--in").arg(fixture("two_cluster_market.json")));
    assert_eq!(r.code, OK, "stderr: {}", r.stderr);
    let doc: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(doc["receipt"]["input_node_count"], 6);
}

// ---------------------------------------------------------------------------
// Determinism across processes
// ---------------------------------------------------------------------------

/// Two separate processes, same inputs, identical bytes. This is the property
/// a coordinator relies on to cache or to compare two workers' returns.
#[test]
fn two_invocations_produce_identical_bytes() {
    let a = run_sheet();
    let b = run_sheet();
    assert_eq!(a.code, OK);
    assert_eq!(a.stdout, b.stdout, "the node must be byte-deterministic");
}

// ---------------------------------------------------------------------------
// Exit-code vocabulary
// ---------------------------------------------------------------------------

#[test]
fn malformed_json_exits_config_and_writes_nothing_to_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, b"{ not json at all").unwrap();

    let r = run(node().arg("--in").arg(&bad));
    assert_eq!(r.code, CONFIG, "stderr: {}", r.stderr);
    assert!(r.stdout.is_empty(), "no partial document on failure");
    assert!(r.stderr.contains("aria node:"));
}

#[test]
fn a_dangling_edge_exits_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dangling.json");
    std::fs::write(
        &path,
        br#"{"nodes":[{"id":1,"label":"A"}],"edges":[{"from":1,"to":999}]}"#,
    )
    .unwrap();

    let r = run(node().arg("--in").arg(&path));
    assert_eq!(r.code, CONFIG);
    assert!(r.stdout.is_empty());
    assert!(r.stderr.contains("dangling"), "stderr: {}", r.stderr);
}

#[test]
fn an_unreadable_input_path_exits_io() {
    let r = run(node().arg("--in").arg("/nonexistent/definitely/not/here.json"));
    assert_eq!(r.code, IO, "stderr: {}", r.stderr);
    assert!(r.stdout.is_empty());
}

#[test]
fn an_unwritable_output_path_exits_io_and_leaves_stdout_clean() {
    let r = run(
        node()
            .arg("--in")
            .arg(fixture("tabular_market_sheet.json"))
            .arg("--out")
            .arg("/nonexistent/dir/out.json"),
    );
    assert_eq!(r.code, IO, "stderr: {}", r.stderr);
    assert!(
        r.stdout.is_empty(),
        "a failed write must not also dump the document to stdout"
    );
}

#[test]
fn a_resource_ceiling_breach_exits_config() {
    for (flag, value) in [
        ("--max-input-bytes", "8"),
        ("--max-nodes", "2"),
        ("--max-edges", "1"),
        ("--max-steps", "1"),
    ] {
        let r = run(
            node()
                .arg("--in")
                .arg(fixture("tabular_market_sheet.json"))
                .arg(flag)
                .arg(value),
        );
        assert_eq!(r.code, CONFIG, "{flag}: stderr {}", r.stderr);
        assert!(r.stdout.is_empty(), "{flag} leaked a document");
    }
}

#[test]
fn an_unknown_match_policy_exits_config() {
    let r = run(
        node()
            .arg("--in")
            .arg(fixture("tabular_market_sheet.json"))
            .arg("--match-policy")
            .arg("telepathy"),
    );
    assert_eq!(r.code, CONFIG);
    assert!(r.stdout.is_empty());
}

/// The invariant code exists and is distinct from the others. A seeded fixture
/// must not trip it — that is the point of exit 1 being rare.
#[test]
fn the_invariant_exit_code_is_reserved_and_not_tripped_by_a_good_run() {
    let r = run_sheet();
    assert_ne!(r.code, INVARIANT, "a seeded fixture must stay admissible");
    assert_eq!(r.code, OK);
}

// ---------------------------------------------------------------------------
// Sinks
// ---------------------------------------------------------------------------

#[test]
fn out_writes_the_document_and_keeps_stdout_empty() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("envelope.json");

    let r = run(
        node()
            .arg("--in")
            .arg(fixture("tabular_market_sheet.json"))
            .arg("--out")
            .arg(&out),
    );
    assert_eq!(r.code, OK, "stderr: {}", r.stderr);
    assert!(
        r.stdout.is_empty(),
        "with --out the document goes to the file, not both places"
    );

    let doc: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(doc["schema"], "aria-telemetry-query-v1");
    // The human-readable summary belongs on stderr.
    assert!(r.stderr.contains("nodes"), "stderr: {}", r.stderr);
}

#[test]
fn the_splits_round_trip_to_the_nested_objects() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("all.json");
    let graph = dir.path().join("graph.json");
    let ledger = dir.path().join("ledger.json");
    let receipt = dir.path().join("receipt.json");

    let r = run(
        node()
            .arg("--in")
            .arg(fixture("tabular_market_sheet.json"))
            .arg("--out")
            .arg(&out)
            .arg("--graph-out")
            .arg(&graph)
            .arg("--ledger-out")
            .arg(&ledger)
            .arg("--receipt-out")
            .arg(&receipt),
    );
    assert_eq!(r.code, OK, "stderr: {}", r.stderr);

    let full: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    let g: Value = serde_json::from_slice(&std::fs::read(&graph).unwrap()).unwrap();
    let l: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    let rc: Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();

    assert_eq!(g, full["graph"], "the split must equal the nested object");
    assert_eq!(rc, full["receipt"]);
    assert_eq!(l, full["ledger"]);
    assert_eq!(g["schema"], "aria-graph-ipo-v1");
}

/// Asking for the ledger file is asking for the ledger; requiring `--observe`
/// as well would silently write `null`.
#[test]
fn ledger_out_implies_observe() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("ledger.json");
    let r = run(
        node()
            .arg("--in")
            .arg(fixture("tabular_market_sheet.json"))
            .arg("--ledger-out")
            .arg(&ledger),
    );
    assert_eq!(r.code, OK, "stderr: {}", r.stderr);
    let l: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    assert!(l["steps"].is_number(), "the ledger must be populated");
}

#[test]
fn observe_attaches_a_ledger_to_the_document() {
    let plain = run_sheet();
    let observed = run(
        node()
            .arg("--in")
            .arg(fixture("tabular_market_sheet.json"))
            .arg("--observe"),
    );
    assert_eq!(observed.code, OK);

    let a: Value = serde_json::from_slice(&plain.stdout).unwrap();
    let b: Value = serde_json::from_slice(&observed.stdout).unwrap();
    assert!(a.get("ledger").is_none(), "off by default");
    assert!(b["ledger"]["steps"].is_number());

    // ℂ2 across the process boundary: the run itself is unchanged.
    assert_eq!(a["graph"], b["graph"]);
    assert_eq!(a["receipt"]["node_count"], b["receipt"]["node_count"]);
    assert_eq!(a["receipt"]["residual"], b["receipt"]["residual"]);
}

#[test]
fn pretty_is_opt_in_so_the_default_stays_byte_stable() {
    let compact = run_sheet();
    let pretty = run(
        node()
            .arg("--in")
            .arg(fixture("tabular_market_sheet.json"))
            .arg("--pretty"),
    );
    assert_eq!(pretty.code, OK);
    assert!(pretty.stdout.len() > compact.stdout.len());

    // Same document either way.
    let a: Value = serde_json::from_slice(&compact.stdout).unwrap();
    let b: Value = serde_json::from_slice(&pretty.stdout).unwrap();
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// The transform is not a judge
// ---------------------------------------------------------------------------

#[test]
fn the_emitted_document_contains_no_authority_field() {
    let r = run_sheet();
    let text = String::from_utf8(r.stdout).unwrap().to_lowercase();
    for forbidden in ["\"trust\"", "\"goal_complete\"", "\"verdict\"", "\"recommendation\""] {
        assert!(!text.contains(forbidden), "found {forbidden}");
    }
}

/// Other subcommands keep their historical exit behavior: the richer
/// vocabulary is scoped to `node` so committed goldens and scripts are safe.
#[test]
fn other_subcommands_keep_their_single_failure_code() {
    let out = Command::new(env!("CARGO_BIN_EXE_aria"))
        .args(["run", "--steps", "8", "--n-modes", "64", "--latent-dim", "32"])
        .output()
        .expect("spawn");
    assert!(out.status.success(), "a plain run must still succeed");
}
