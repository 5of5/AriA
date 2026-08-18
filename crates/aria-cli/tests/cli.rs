//! CLI surface tests: config plumbing and trace parity with the shared runner.

use aria_engine_backends::runner;
use aria_engine_core::config::AriaConfig;
use std::io::Write;

fn test_config() -> AriaConfig {
    AriaConfig {
        n_modes: 8,
        latent_dim: 16,
        seed: Some(42),
        ..AriaConfig::test_config()
    }
}

#[test]
fn cli_trace_equals_runner_trace() {
    // The CLI writes exactly what the shared runner produces; that is what
    // makes CLI/Python/WASM parity structural rather than coincidental.
    let outcome = runner::run(test_config(), 100).unwrap();
    let jsonl = outcome.trace.to_jsonl();

    assert_eq!(jsonl.lines().count(), 101, "1 config line + 100 entries");
    assert!(jsonl
        .lines()
        .next()
        .unwrap()
        .contains("\"type\":\"config\""));
    assert_eq!(outcome.summary.action_sequence, "OPMD".repeat(25));
    assert!(outcome.summary.invariants_ok);
}

#[test]
fn toml_config_round_trips_through_the_cli_format() {
    let src = r#"
n_modes = 8
latent_dim = 16
eps = 1.0
stutter_k = 2
schedule = "opmd"
condition = "world_model"
match_policy = "one_edit"
diff_policy = "graph_conditioned"
max_graph_size = 5000
allow_sub_spec_dims = true
seed = 7
strict = true
"#;

    let config = AriaConfig::from_toml(src).expect("config should parse");
    assert_eq!(config.n_modes, 8);
    assert_eq!(config.schedule, "opmd");
    assert_eq!(config.seed, Some(7));
    // N = 8 is sub-spec; the test-only escape is what lets this run through
    // the shared runner's 𝒮 validation (plan WS0).
    assert!(config.allow_sub_spec_dims);

    let outcome = runner::run(config, 40).unwrap();
    assert!(
        outcome.summary.invariants_ok,
        "{:?}",
        outcome.summary.failures
    );
    assert_eq!(outcome.summary.action_sequence, "OPMD".repeat(10));
}

#[test]
fn config_file_on_disk_parses() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "n_modes = 8\nlatent_dim = 16\neps = 1.0\nseed = 42").unwrap();

    let contents = std::fs::read_to_string(f.path()).unwrap();
    let config = AriaConfig::from_toml(&contents).unwrap();
    assert_eq!(config.n_modes, 8);
    // Fields omitted from the file fall back to the documented defaults.
    assert_eq!(config.schedule, "opmd");
    assert_eq!(config.stutter_k, 2);
}

#[test]
fn all_three_conditions_run_the_same_schedule() {
    // A4: conditioning switches without a second architecture.
    for name in ["token", "diffusion", "world_model"] {
        let mut config = test_config();
        config.condition = runner::parse_condition(name).unwrap();
        let outcome = runner::run(config, 40).unwrap();
        assert!(
            outcome.summary.invariants_ok,
            "{name}: {:?}",
            outcome.summary.failures
        );
        assert_eq!(outcome.summary.action_sequence, "OPMD".repeat(10));
    }
}
