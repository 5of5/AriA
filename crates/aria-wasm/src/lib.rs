//! Aria WASM surface — the browser binding for the reference runtime.
//!
//! This crate is a thin façade over `aria_engine_backends::runner`, the same
//! code path the CLI and the Python extension use. It defines no transitions
//! and relaxes no invariant; it only marshals config in and traces out.

use aria_engine_backends::runner;
use aria_engine_core::action::Action;
use aria_engine_core::config::AriaConfig;
use wasm_bindgen::prelude::*;

/// Install a panic hook that forwards Rust panics to `console.error`.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// The five named actions, as symbols — Σ = {O, P, M, D, S}.
#[wasm_bindgen(js_name = actionAlphabet)]
pub fn action_alphabet() -> Vec<JsValue> {
    Action::ALL
        .iter()
        .map(|a| JsValue::from_str(a.symbol()))
        .collect()
}

/// The default config as a JS object.
#[wasm_bindgen(js_name = defaultConfig)]
pub fn default_config() -> Result<JsValue, JsValue> {
    to_js(&AriaConfig::default())
}

/// Run the reference OPMD schedule and return a `RunSummary` object.
///
/// `config_json` is an Aria config as JSON; pass `null`/`undefined` for defaults.
#[wasm_bindgen(js_name = run)]
pub fn run(config_json: Option<String>, steps: u32) -> Result<JsValue, JsValue> {
    let config = parse_config(config_json)?;
    let outcome = runner::run(config, u64::from(steps)).map_err(err)?;
    to_js(&outcome.summary)
}

/// Run the reference schedule and return the trace as a JSONL string.
///
/// Byte-identical to `aria run --output trace.jsonl` for the same config.
#[wasm_bindgen(js_name = runTraceJsonl)]
pub fn run_trace_jsonl(config_json: Option<String>, steps: u32) -> Result<String, JsValue> {
    let config = parse_config(config_json)?;
    let outcome = runner::run(config, u64::from(steps)).map_err(err)?;
    Ok(outcome.trace.to_jsonl())
}

/// Decode a completed run to discrete token ids (𝔸5 / 𝕃5).
///
/// Replays Φ from `config_json` (defaults if omitted) and applies the
/// `aria-readout-v1` weights in `readout`. Returns one id per step. The
/// readout cannot write back into the engine — this export is an I/O sink.
#[wasm_bindgen(js_name = emitIds)]
pub fn emit_ids(
    config_json: Option<String>,
    steps: u32,
    readout: &[u8],
) -> Result<Vec<u32>, JsValue> {
    use aria_engine_backends::{latents_of, Readout};
    let config = parse_config(config_json)?;
    let zs = latents_of(config, u64::from(steps)).map_err(err)?;
    let Readout::Discrete(head) = Readout::from_bytes(readout).map_err(err)? else {
        return Err(err("emitIds requires a discrete readout"));
    };
    zs.iter().map(|z| head.decode_id(z).map_err(err)).collect()
}

/// Parse a TOML config into the JSON shape accepted by [`run`].
#[wasm_bindgen(js_name = configFromToml)]
pub fn config_from_toml(toml_src: &str) -> Result<JsValue, JsValue> {
    let config = AriaConfig::from_toml(toml_src).map_err(err)?;
    to_js(&config)
}

fn parse_config(config_json: Option<String>) -> Result<AriaConfig, JsValue> {
    match config_json {
        Some(s) if !s.trim().is_empty() => serde_json::from_str(&s).map_err(err),
        _ => Ok(AriaConfig::default()),
    }
}

fn to_js<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(err)
}

fn err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // Exact comparison on purpose: the default ε must round-trip as the
    // literal 1.0 for byte-stable parity across surfaces.
    #[allow(clippy::float_cmp)]
    fn parse_config_defaults_on_empty() {
        let c = parse_config(None).unwrap();
        assert_eq!(c.schedule, "opmd");
        assert_eq!(c.eps, 1.0);
    }

    #[test]
    fn parse_config_reads_json() {
        let json = r#"{"n_modes":8,"latent_dim":16,"steps":0}"#;
        let c = parse_config(Some(json.into())).unwrap();
        assert_eq!(c.n_modes, 8);
        assert_eq!(c.latent_dim, 16);
    }
}
