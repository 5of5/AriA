//! Closed operator JSON over the Aria telemetry transform.
//!
//! Each catalog binary is a distinct crate that includes its own `spec.json`
//! and calls [`run_spec`]. AriA is the underlying transformer in every
//! binary: Φ is not copied, it is linked.

#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

mod dispatch;
mod envelope;
mod harness;
mod index;
mod organize;
mod run;
#[cfg(feature = "cli")]
mod serve;
mod typecast;
mod work_api;
#[cfg(feature = "cli")]
mod work_cli;

pub use harness::{
    dispatch_json, execute_harness, harness_lane, HarnessError, HarnessRequest, HarnessResult,
    DEFAULT_OUTPUT_LIMIT, HARNESS_CAPABILITY, HARNESS_REQUEST_V1, HARNESS_RESULT_V1,
};
#[cfg(feature = "cli")]
pub use serve::{serve, serve_background};

pub use dispatch::{
    endpoint_by_binary_id, endpoint_by_operator, endpoint_by_package, WorkerEndpoint,
};
pub use envelope::{
    OperatorEnvelope, OperatorGraph, OperatorNode, OperatorRel, OperatorSpec, ENVELOPE_KEYS,
};
pub use run::{run_binary, run_many, run_spec, OperatorError, RunOpts};
pub use organize::{organize_slop, OrganizeReport};
pub use typecast::{cast_lexicon, cast_tags, tag_phrase, uncast_fields};
pub use work_api::{
    callback_results, commands_json, execute_work, looks_like_work_command, WorkRequest,
    WorkResponse, WORK_V1,
};
#[cfg(feature = "cli")]
pub use work_cli::work_main;

#[cfg(feature = "cli")]
use clap::Parser;
#[cfg(feature = "cli")]
use std::io::{self, Read, Write};
#[cfg(feature = "cli")]
use std::path::PathBuf;
#[cfg(feature = "cli")]
use std::process::ExitCode;
use std::sync::OnceLock;

/// Schema tag for the operator envelope.
pub const OPERATOR_ENVELOPE_V1: &str = "aria-operator-envelope-v1";
/// Closed envelope semver carried on every operator document.
pub const OPERATOR_SCHEMA_VERSION: &str = "1.0.0";
/// Frozen catalog shipped with this crate (560 rows: 535 research/host + 25 map mixers).
pub const CATALOG_JSON: &str = include_str!("../catalog/operators.json");
/// Slim spawn table: 560 rows, no graph block.
pub const DISPATCH_JSON: &str = include_str!("../catalog/dispatch.json");

static CATALOG: OnceLock<Vec<OperatorSpec>> = OnceLock::new();

/// Frozen catalog (560 rows). Parsed once per process.
#[must_use]
pub fn catalog() -> &'static [OperatorSpec] {
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(CATALOG_JSON)
                .expect("catalog/operators.json is generated valid JSON")
        })
        .as_slice()
}

/// Look up one catalog row by `BIN.*` (interned, O(1)).
#[must_use]
pub fn spec_by_id(binary_id: &str) -> Option<&'static OperatorSpec> {
    intern_maps()
        .by_id
        .get(binary_id)
        .copied()
}

/// Look up one catalog row by operator name (`PEOPLE`, `TAG.PERSON_FOUNDER`, …).
#[must_use]
pub fn spec_by_operator(operator: &str) -> Option<&'static OperatorSpec> {
    intern_maps()
        .by_operator
        .get(operator)
        .copied()
}

/// Look up one catalog row by cargo package.
#[must_use]
pub fn spec_by_package(package: &str) -> Option<&'static OperatorSpec> {
    intern_maps()
        .by_package
        .get(package)
        .copied()
}

/// Residual + DEEP_TAG specs whose `parent` is this family operator (E10).
#[must_use]
pub fn family_residuals(parent: &str) -> &'static [&'static OperatorSpec] {
    intern_maps()
        .family
        .get(parent)
        .map_or(&[], Vec::as_slice)
}

/// Research binaries that declare this exact anchor tag.
#[must_use]
pub fn specs_for_tag(tag: &str) -> &'static [&'static OperatorSpec] {
    intern_maps()
        .by_anchor
        .get(tag)
        .map_or(&[], Vec::as_slice)
}

struct InternMaps {
    by_id: std::collections::HashMap<&'static str, &'static OperatorSpec>,
    by_operator: std::collections::HashMap<&'static str, &'static OperatorSpec>,
    by_package: std::collections::HashMap<&'static str, &'static OperatorSpec>,
    family: std::collections::HashMap<&'static str, Vec<&'static OperatorSpec>>,
    by_anchor: std::collections::HashMap<&'static str, Vec<&'static OperatorSpec>>,
}

fn intern_maps() -> &'static InternMaps {
    static M: OnceLock<InternMaps> = OnceLock::new();
    M.get_or_init(|| {
        let cat = catalog();
        let mut by_id = std::collections::HashMap::with_capacity(cat.len());
        let mut by_operator = std::collections::HashMap::with_capacity(cat.len());
        let mut by_package = std::collections::HashMap::with_capacity(cat.len());
        let mut family: std::collections::HashMap<&str, Vec<&OperatorSpec>> =
            std::collections::HashMap::new();
        let mut by_anchor: std::collections::HashMap<&str, Vec<&OperatorSpec>> =
            std::collections::HashMap::new();
        for s in cat {
            by_id.insert(s.binary_id.as_str(), s);
            by_operator.entry(s.operator.as_str()).or_insert(s);
            by_package.entry(s.package.as_str()).or_insert(s);
            if !s.parent.is_empty()
                && (s.layer.eq_ignore_ascii_case("RESIDUAL")
                    || s.layer.eq_ignore_ascii_case("DEEP_TAG"))
            {
                family.entry(s.parent.as_str()).or_default().push(s);
            }
            if !s.layer.eq_ignore_ascii_case("HOST") {
                for t in &s.anchor_tags {
                    by_anchor.entry(t.as_str()).or_default().push(s);
                }
            }
        }
        InternMaps {
            by_id,
            by_operator,
            by_package,
            family,
            by_anchor,
        }
    })
}

/// Wave ladder height (catalog `wave`): A→1, B→2, C→3, D→4.
#[must_use]
pub fn wave_height(wave: Option<&str>) -> u8 {
    match wave.unwrap_or("") {
        "A" => 1,
        "B" => 2,
        "C" => 3,
        "D" => 4,
        _ => 0,
    }
}

/// Token → (category weight, max wave height) over the frozen catalog.
/// Weight = # of research binaries declaring the token in 02/03/04 terms
/// (anchor_tags, node/rel types, property keys); HOST declarations weigh
/// nothing (B6). Parsed once per process; fully deterministic.
fn token_stats() -> &'static std::collections::BTreeMap<String, (u32, u8)> {
    use std::collections::BTreeMap;
    static STATS: OnceLock<BTreeMap<String, (u32, u8)>> = OnceLock::new();
    STATS.get_or_init(|| {
        let mut m: BTreeMap<String, (u32, u8)> = BTreeMap::new();
        for s in catalog().iter().filter(|s| s.layer != "HOST") {
            let h = wave_height(s.wave.as_deref());
            let mut bump = |t: &str| {
                let e = m.entry(t.to_ascii_lowercase()).or_insert((0, 0));
                e.0 += 1;
                e.1 = e.1.max(h);
            };
            for t in s
                .anchor_tags
                .iter()
                .chain(s.node_types.iter())
                .chain(s.relationship_types.iter())
            {
                bump(t);
            }
            if let Some(k) = s.property_key.as_deref() {
                bump(k);
            }
        }
        m
    })
}

/// (weight, height) of one grammar token: kind, rel type, tag, property key.
#[must_use]
pub fn token_stat(token: &str) -> (u32, u8) {
    token_stats()
        .get(&token.to_ascii_lowercase())
        .copied()
        .unwrap_or((0, 0))
}

#[cfg(feature = "cli")]
#[derive(Parser, Debug)]
#[command(
    name = "aria-operator",
    about = "Run one catalog operator: Aria transform + closed operator JSON"
)]
struct Cli {
    /// Payload JSON. `-` or omitted reads stdin.
    #[arg(long)]
    r#in: Option<PathBuf>,
    /// Write the operator envelope here (stdout when omitted).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Φ steps.
    #[arg(long, default_value_t = 32)]
    steps: u64,
    /// Seed. Equal payload + spec + seed ⇒ equal bytes.
    #[arg(long)]
    seed: Option<u64>,
    /// Bind this Observation Plan hash (hex). Unbound runs hash the payload.
    #[arg(long)]
    plan_hash: Option<String>,
    /// Coverage key from the sealed plan.
    #[arg(long)]
    requirement_id: Option<String>,
    /// Embed the Aria telemetry spine (optional). Off by default.
    #[arg(long)]
    telemetry: bool,
}

/// CLI entry used by every generated `[[bin]]`. Returns a process exit code.
#[cfg(feature = "cli")]
#[must_use]
pub fn bin_main(spec_json: &str) -> i32 {
    match bin_main_inner(spec_json) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

#[cfg(feature = "cli")]
fn bin_main_inner(spec_json: &str) -> Result<(), i32> {
    let cli = Cli::parse();
    let payload = read_payload(cli.r#in.as_deref()).map_err(|e| {
        eprintln!("aria-operator: {e}");
        3
    })?;
    let opts = RunOpts {
        steps: cli.steps,
        seed: cli.seed,
        plan_hash: cli.plan_hash,
        requirement_id: cli.requirement_id,
        include_telemetry: cli.telemetry,
        ..RunOpts::default()
    };

    let envelope = run_spec(spec_json, &payload, &opts).map_err(|e| {
        eprintln!("aria-operator: {e}");
        e.exit_code()
    })?;
    let bytes = serde_json::to_vec(&envelope).map_err(|e| {
        eprintln!("aria-operator: serialize: {e}");
        2
    })?;
    write_sink(cli.out.as_deref(), &bytes).map_err(|e| {
        eprintln!("aria-operator: {e}");
        3
    })?;
    Ok(())
}

#[cfg(feature = "cli")]
fn read_payload(path: Option<&std::path::Path>) -> io::Result<Vec<u8>> {
    match path {
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            Ok(buf)
        }
        Some(p) if p.as_os_str() == "-" => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            Ok(buf)
        }
        Some(p) => std::fs::read(p),
    }
}

#[cfg(feature = "cli")]
fn write_sink(path: Option<&std::path::Path>, bytes: &[u8]) -> io::Result<()> {
    match path {
        None => {
            io::stdout().write_all(bytes)?;
            Ok(())
        }
        Some(p) => {
            std::fs::write(p, bytes)?;
            Ok(())
        }
    }
}

/// `ExitCode` wrapper for callers that prefer the std type.
#[cfg(feature = "cli")]
#[must_use]
pub fn bin_exit(spec_json: &str) -> ExitCode {
    ExitCode::from(u8::try_from(bin_main(spec_json)).unwrap_or(2))
}
