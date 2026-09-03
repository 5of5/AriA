//! Harness lane: stdin request → stdout result.
//!
//! The native-binary protocol a worker harness enforces: canonical JSON on stdin, one JSON document on stdout, **nothing**
//! on stderr, exit 0, bounded output, and every binding field echoed back so
//! the worker can reject a mismatched result. Statelessness is the contract:
//! `request bytes → result bytes`, deterministic under the seed.
//!
//! Capability: `aria.telemetry.project` — the worker names one or more
//! catalog binaries (`ops`) and supplies the payload it wants organized; it
//! receives the `aria-work-v1` callback (working verticals only) inside a
//! result that echoes its bindings.

use crate::{callback_results, run_many, OperatorError, RunOpts, WORK_V1};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Request schema tag (stable; additive changes bump the suffix).
pub const HARNESS_REQUEST_V1: &str = "pcvc-aria-telemetry-request-v1";
/// Result schema tag.
pub const HARNESS_RESULT_V1: &str = "pcvc-aria-telemetry-result-v1";
/// The one closed capability this node registers.
pub const HARNESS_CAPABILITY: &str = "aria.telemetry.project";
/// Default stdout budget (64 KiB).
pub const DEFAULT_OUTPUT_LIMIT: usize = 64 * 1024;

/// What a worker sends on stdin. camelCase field names; unknown fields are
/// rejected (closed model).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HarnessRequest {
    pub schema_version: String,
    pub capability: String,
    pub run_id: String,
    pub plan_hash: String,
    pub attempt_id: String,
    pub fencing_token: u64,
    pub requirement_id: String,
    /// Catalog identities to project (one or many). `["*"]` = every research op.
    pub ops: Vec<String>,
    /// The original anchor the worker wants organized (never rewritten).
    pub payload: Value,
    /// Φ steps; `0` is the identify funnel (default).
    #[serde(default)]
    pub steps: u64,
    /// Seed for byte-determinism (default 1).
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// Output budget in bytes (default 64 KiB); may only be tightened.
    #[serde(default = "default_limit")]
    pub output_limit_bytes: usize,
}

fn default_seed() -> u64 {
    1
}

fn default_limit() -> usize {
    DEFAULT_OUTPUT_LIMIT
}

/// Bindings a harness re-checks on the way back (`driver.py` field list).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessResult {
    pub schema_version: &'static str,
    pub capability: String,
    pub run_id: String,
    pub plan_hash: String,
    pub attempt_id: String,
    pub fencing_token: u64,
    pub requirement_id: String,
    /// `result` (≥1 working vertical) · `no-finding` (asked, nothing supports)
    /// · `truncation` (budget trimmed the callback) · `limitation` (engine
    /// rejected the payload; reason in `limitation`).
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limitation: Option<String>,
    /// The production callback (`aria-work-v1`). Absent only on `limitation`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback: Option<Value>,
    /// Bytes of the callback that were dropped to honour the budget.
    #[serde(skip_serializing_if = "is_zero")]
    pub dropped_verticals: usize,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip_serializing_if signature
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Validation failures a harness treats as protocol errors (non-zero exit).
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("request is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("schemaVersion must be {HARNESS_REQUEST_V1}")]
    Schema,
    #[error("capability must be {HARNESS_CAPABILITY}")]
    Capability,
    #[error("{0} is not a 64-hex sha256")]
    Hex(&'static str),
    #[error("{0} must be non-empty")]
    Empty(&'static str),
    #[error("fencingToken must be ≥ 1")]
    Fence,
    #[error("outputLimitBytes must be in 1..={DEFAULT_OUTPUT_LIMIT}")]
    Limit,
    #[error("unknown binary {0}")]
    UnknownBinary(String),
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl HarnessRequest {
    /// Parse + validate the closed request.
    pub fn parse(raw: &[u8]) -> Result<Self, HarnessError> {
        let req: Self = serde_json::from_slice(raw)?;
        if req.schema_version != HARNESS_REQUEST_V1 {
            return Err(HarnessError::Schema);
        }
        if req.capability != HARNESS_CAPABILITY {
            return Err(HarnessError::Capability);
        }
        if !is_hex64(&req.plan_hash) {
            return Err(HarnessError::Hex("planHash"));
        }
        for (name, v) in [
            ("runId", &req.run_id),
            ("attemptId", &req.attempt_id),
            ("requirementId", &req.requirement_id),
        ] {
            if v.trim().is_empty() {
                return Err(HarnessError::Empty(name));
            }
        }
        if req.fencing_token == 0 {
            return Err(HarnessError::Fence);
        }
        if req.output_limit_bytes == 0 || req.output_limit_bytes > DEFAULT_OUTPUT_LIMIT {
            return Err(HarnessError::Limit);
        }
        if req.ops.is_empty() {
            return Err(HarnessError::Empty("ops"));
        }
        for id in &req.ops {
            if id != "*" && crate::spec_by_id(id).is_none() {
                return Err(HarnessError::UnknownBinary(id.clone()));
            }
        }
        Ok(req)
    }

    fn resolved_ops(&self) -> Vec<String> {
        if self.ops.iter().any(|o| o == "*") {
            crate::catalog()
                .iter()
                .filter(|s| s.layer != "HOST")
                .map(|s| s.binary_id.clone())
                .collect()
        } else {
            self.ops.clone()
        }
    }

    fn bound(&self, status: &'static str) -> HarnessResult {
        HarnessResult {
            schema_version: HARNESS_RESULT_V1,
            capability: self.capability.clone(),
            run_id: self.run_id.clone(),
            plan_hash: self.plan_hash.clone(),
            attempt_id: self.attempt_id.clone(),
            fencing_token: self.fencing_token,
            requirement_id: self.requirement_id.clone(),
            status,
            limitation: None,
            callback: None,
            dropped_verticals: 0,
        }
    }
}

/// Execute one harness request. Never panics; engine rejections become a
/// `limitation` result (still bound, still exit 0) — the harness decides.
#[must_use]
pub fn execute_harness(req: &HarnessRequest) -> HarnessResult {
    let opts = RunOpts {
        steps: req.steps,
        seed: Some(req.seed),
        plan_hash: Some(req.plan_hash.clone()),
        requirement_id: Some(req.requirement_id.clone()),
        include_telemetry: false,
        ..RunOpts::default()
    };
    let payload = match serde_json::to_vec(&req.payload) {
        Ok(b) => b,
        Err(e) => {
            let mut r = req.bound("limitation");
            r.limitation = Some(format!("payload not serializable: {e}"));
            return r;
        }
    };
    let binaries = req.resolved_ops();
    let envs = match run_many(&binaries, &payload, &opts) {
        Ok(e) => e,
        Err(e) => {
            let mut r = req.bound("limitation");
            r.limitation = Some(match &e {
                OperatorError::UnknownBinary(id) => format!("unknown binary {id}"),
                other => other.to_string(),
            });
            return r;
        }
    };
    let asked = envs.len();
    let mut working: Vec<Value> = callback_results(&envs)
        .into_iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();

    // Budget: drop the largest verticals last-first until the result fits.
    // Deterministic (stable sort by size desc, then binary_id), reported.
    let mut dropped = 0usize;
    let organize = crate::organize_slop(&payload);
    let build = |results: &Vec<Value>| {
        json!({
            "schema": WORK_V1,
            "phi_once": true,
            "asked": asked,
            "ops": results.len(),
            "organize": organize,
            "results": results,
        })
    };
    let fits = |r: &HarnessResult| serde_json::to_vec(r).map_or(usize::MAX, |b| b.len()) <= req.output_limit_bytes;
    let mut result = req.bound(if working.is_empty() { "no-finding" } else { "result" });
    result.callback = Some(build(&working));
    if !fits(&result) {
        working.sort_by(|a, b| {
            let sa = serde_json::to_vec(a).map_or(0, |v| v.len());
            let sb = serde_json::to_vec(b).map_or(0, |v| v.len());
            sb.cmp(&sa).then_with(|| a["binary_id"].as_str().cmp(&b["binary_id"].as_str()))
        });
        while !working.is_empty() {
            working.remove(0);
            dropped += 1;
            result.callback = Some(build(&working));
            result.dropped_verticals = dropped;
            result.status = "truncation";
            if fits(&result) {
                break;
            }
        }
        if working.is_empty() {
            // Even the empty callback does not fit: report as limitation.
            result.status = "limitation";
            result.callback = None;
            result.limitation = Some(format!(
                "output budget {} B too small for a bound result",
                req.output_limit_bytes
            ));
        }
    }
    result
}

/// Full stdin → stdout lane. Returns `(exit code, stdout bytes)`; stderr is
/// never written. Protocol errors (unparseable/unbound request) exit 2 with
/// a JSON error object so the harness sees why without a stderr channel.
#[must_use]
pub fn harness_lane(raw: &[u8]) -> (i32, Vec<u8>) {
    match HarnessRequest::parse(raw) {
        Ok(req) => {
            let result = execute_harness(&req);
            (0, serde_json::to_vec(&result).unwrap_or_default())
        }
        Err(e) => (
            2,
            serde_json::to_vec(&json!({
                "schemaVersion": HARNESS_RESULT_V1,
                "status": "failure",
                "error": e.to_string(),
            }))
            .unwrap_or_default(),
        ),
    }
}

/// `aria-dispatch-v1`: the descriptor a registry pins.
/// Everything is a function of the frozen catalog plus this executable's
/// identity, so a harness can record exact executable hashes.
#[must_use]
pub fn dispatch_json() -> Value {
    // The executable's identity is fixed for the life of the process; hash it
    // once (a hosted node serves /dispatch to every registry probe).
    static EXE: std::sync::OnceLock<(Option<std::path::PathBuf>, Option<String>)> =
        std::sync::OnceLock::new();
    let (exe, exe_sha) = EXE.get_or_init(|| {
        let exe = std::env::current_exe().ok();
        let sha = exe
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| aria_engine_backends::ipo::sha256_hex(&b));
        (exe, sha)
    });
    let (exe, exe_sha) = (exe.clone(), exe_sha.clone());
    let binaries: Vec<Value> = crate::catalog()
        .iter()
        .map(|s| {
            json!({
                "binary_id": s.binary_id,
                "operator": s.operator,
                "package": s.package,
                "class": s.class,
                "layer": s.layer,
                "verify": s.verify,
                "resultDefinitionRef": s.result_definition_ref,
                "node_types": s.node_types,
                "relationship_types": s.relationship_types,
                "anchor_tags": s.anchor_tags,
                "default_limit": s.default_limit,
                "graph": crate::OperatorGraph::from_spec(s),
            })
        })
        .collect();
    json!({
        "schema": "aria-dispatch-v1",
        "crate": "aria-json-telemetry",
        "version": env!("CARGO_PKG_VERSION"),
        "capability": {
            "name": HARNESS_CAPABILITY,
            "version": "v1",
            "request_schema": HARNESS_REQUEST_V1,
            "result_schema": HARNESS_RESULT_V1,
            "callback_schema": WORK_V1,
            "stdin": "canonical JSON request",
            "stdout": "one JSON result; nothing else",
            "stderr": "always empty",
            "exit": {"0": "bound result (result | no-finding | truncation | limitation)", "2": "protocol error (unbound)"},
            "output_limit_bytes": DEFAULT_OUTPUT_LIMIT,
            "default_steps": 0,
            "deterministic": "equal request bytes ⇒ equal result bytes",
            "state": "none (stateless node)",
        },
        "executable": {
            "name": "work",
            "path": exe.map(|p| p.display().to_string()),
            "sha256": exe_sha,
            "system": std::env::consts::OS,
            "machine": std::env::consts::ARCH,
        },
        "catalog": binaries.len(),
        "binaries": binaries,
    })
}
