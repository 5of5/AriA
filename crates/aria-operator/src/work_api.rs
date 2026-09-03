//! JSON-CLI protocol for the work gateway.
//!
//! A worker (or Aria compiling a hosted command list) sends one JSON object.
//! The gateway never invents a `BIN.*`. Unknown ops fail closed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::catalog;
use crate::organize::{organize_slop, OrganizeReport};
use crate::run::{run_binary, run_many, OperatorError, RunOpts};

/// Schema tag for a compiled work response.
pub const WORK_V1: &str = "aria-work-v1";

/// JSON command the nervous-system gateway accepts.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WorkRequest {
    /// Dump the hosted command list and stop.
    #[serde(default)]
    pub commands: bool,
    /// One `BIN.*` (or operator name).
    #[serde(default)]
    pub work: Option<String>,
    /// Many `BIN.*` / operator names. One Φ, N verticals.
    #[serde(default)]
    pub ops: Vec<String>,
    /// Host payload. Required unless `commands` is true.
    #[serde(default, rename = "in")]
    pub payload: Option<Value>,
    /// Embed the Aria spine. Default off.
    #[serde(default)]
    pub telemetry: bool,
    /// Φ steps.
    #[serde(default)]
    pub steps: Option<u64>,
    /// Seed.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Plan bind.
    #[serde(default)]
    pub plan_hash: Option<String>,
    /// Coverage key.
    #[serde(default)]
    pub requirement_id: Option<String>,
}

/// Compiled results. Zero Trust: no Trust/Use/Goal keys.
///
/// Production contract: `results` holds **working verticals only**. A binary
/// that cannot tag the payload is omitted — not returned as an empty
/// skeleton. `asked` is how many identities were named; `ops` is how many
/// came back with nodes, relationships, or properties.
#[derive(Debug, Clone, Serialize)]
pub struct WorkResponse {
    /// Always [`WORK_V1`].
    pub schema: String,
    /// True when one transform served every op.
    pub phi_once: bool,
    /// How many catalog identities were named (compiled).
    pub asked: usize,
    /// How many working envelopes are in `results`.
    pub ops: usize,
    /// Tokens, listed hits, and binaries that will structure this slop.
    pub organize: OrganizeReport,
    /// Working verticals only. Missing data is absence, not an empty object.
    pub results: Vec<Value>,
}

/// Filter a projection set down to envelopes a worker
/// can consume. Dump scoring still uses the unfiltered `run_many` set.
#[must_use]
pub fn callback_results(envs: &[crate::OperatorEnvelope]) -> Vec<&crate::OperatorEnvelope> {
    envs.iter().filter(|e| e.has_working_data()).collect()
}

/// Hosted command list — what Aria compiles against.
#[must_use]
pub fn commands_json() -> Value {
    let mut rows: Vec<Value> = catalog()
        .iter()
        .map(|s| {
            json!({
                "binary_id": s.binary_id,
                "operator": s.operator,
                "package": s.package,
                "layer": s.layer,
                "class": s.class,
                "resultDefinitionRef": s.result_definition_ref,
                "verify": s.verify,
                "cli": format!("work --binary {}", s.binary_id),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        a["binary_id"]
            .as_str()
            .unwrap_or("")
            .cmp(b["binary_id"].as_str().unwrap_or(""))
    });
    json!({
        "schema": "aria-work-commands-v1",
        "count": rows.len(),
        "commands": rows,
    })
}

/// Execute a JSON work request.
pub fn execute_work(req: &WorkRequest, opts_overlay: &RunOpts) -> Result<Value, OperatorError> {
    if req.commands {
        return Ok(commands_json());
    }

    let mut opts = opts_overlay.clone();
    if let Some(s) = req.steps {
        opts.steps = s;
    }
    if req.seed.is_some() {
        opts.seed = req.seed;
    }
    if req.plan_hash.is_some() {
        opts.plan_hash.clone_from(&req.plan_hash);
    }
    if req.requirement_id.is_some() {
        opts.requirement_id.clone_from(&req.requirement_id);
    }
    opts.include_telemetry = req.telemetry;

    let payload = match &req.payload {
        Some(v) => serde_json::to_vec(v)?,
        None => {
            return Err(OperatorError::Spec(
                "work JSON requires 'in' payload unless commands=true".into(),
            ));
        }
    };

    let ids = resolve_ops(req)?;
    let asked = ids.len();
    let envs = if asked == 1 {
        vec![run_binary(&ids[0], &payload, &opts)?]
    } else {
        run_many(&ids, &payload, &opts)?
    };
    let results: Vec<Value> = callback_results(&envs)
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;
    Ok(serde_json::to_value(WorkResponse {
        schema: WORK_V1.into(),
        phi_once: true,
        asked,
        ops: results.len(),
        organize: organize_slop(&payload),
        results,
    })?)
}

fn resolve_ops(req: &WorkRequest) -> Result<Vec<String>, OperatorError> {
    let mut ids = Vec::new();
    if let Some(w) = &req.work {
        ids.push(resolve_one(w)?);
    }
    for op in &req.ops {
        ids.push(resolve_one(op)?);
    }
    if ids.is_empty() {
        return Err(OperatorError::Spec(
            "name work or ops[] from the hosted command list".into(),
        ));
    }
    Ok(ids)
}

fn resolve_one(name: &str) -> Result<String, OperatorError> {
    if let Some(s) = crate::spec_by_id(name) {
        return Ok(s.binary_id.clone());
    }
    if let Some(e) = crate::endpoint_by_operator(name) {
        return Ok(e.binary_id);
    }
    if let Some(e) = crate::endpoint_by_package(name) {
        return Ok(e.binary_id);
    }
    Err(OperatorError::UnknownBinary(name.to_string()))
}

/// True if a JSON value looks like a work command rather than a raw payload.
#[must_use]
pub fn looks_like_work_command(v: &Value) -> bool {
    v.get("commands")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || v.get("work").is_some()
        || v.get("ops").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
}


