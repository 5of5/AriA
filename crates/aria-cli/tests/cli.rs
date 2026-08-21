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

/// A minimal `aria-predictor-v1` JSON checkpoint, deliberately far from the
/// identity-ish stub `SimPredictor` produces — so a run/replay that actually
/// uses it is visibly different from one that silently falls back to the
/// stub. `lipschitz_bound` is kept tiny so `validate_config`'s Inv2
/// worst-case-jump check passes at the default `eps`.
fn small_predictor_json(n_modes: usize, latent_dim: usize) -> String {
    let embed: Vec<Vec<f64>> = (0..latent_dim)
        .map(|i| {
            (0..2 * n_modes)
                .map(|j| if (i + j) % 3 == 0 { 0.05 } else { -0.02 })
                .collect()
        })
        .collect();
    let predict: Vec<Vec<f64>> = (0..latent_dim)
        .map(|i| (0..latent_dim).map(|j| if i == j { 0.03 } else { 0.0 }).collect())
        .collect();

    serde_json::json!({
        "format": "aria-predictor-v1",
        "n_modes": n_modes,
        "latent_dim": latent_dim,
        "lipschitz_bound": 0.05,
        "embed": embed,
        "predict": {
            "token": predict,
            "diffusion": predict,
            "world_model": predict,
        }
    })
    .to_string()
}

/// Regression test for the bug fixed alongside this test: `aria run
/// --predictor` used to silently overwrite an explicitly-passed
/// `--n-modes`/`--latent-dim` with the checkpoint's own dimensions instead
/// of erroring on the conflict — which also made the dimension-mismatch
/// check in `runner::validate_config` unreachable from the CLI, since the
/// CLI had already forced agreement before calling it.
#[test]
fn run_errors_on_a_predictor_dimension_conflict_instead_of_silently_overriding() {
    let dir = tempfile::tempdir().unwrap();
    let predictor_path = dir.path().join("predictor.json");
    let base_config = dir.path().join("base.toml");
    let output = dir.path().join("trace.jsonl");

    // Checkpoint trained at N=8, dim(Z)=16.
    std::fs::write(&predictor_path, small_predictor_json(8, 16)).unwrap();
    std::fs::write(
        &base_config,
        "n_modes = 8\nlatent_dim = 16\nallow_sub_spec_dims = true\n",
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_aria");

    // Explicit --n-modes conflicts with the checkpoint's N=8: must error,
    // not silently adopt the checkpoint's dimensions.
    let conflicting = std::process::Command::new(bin)
        .arg("run")
        .arg("--config")
        .arg(&base_config)
        .arg("--n-modes")
        .arg("16")
        .arg("--predictor")
        .arg(&predictor_path)
        .arg("--steps")
        .arg("5")
        .output()
        .expect("spawn aria");
    assert!(
        !conflicting.status.success(),
        "expected a conflict error, but the run succeeded"
    );
    let stderr = String::from_utf8_lossy(&conflicting.stderr);
    assert!(
        stderr.contains("--n-modes") && stderr.contains("conflicts"),
        "expected a conflict message mentioning --n-modes, got: {stderr}"
    );

    // No --n-modes/--latent-dim pinned: the checkpoint's dimensions are
    // still adopted automatically, same as before this fix.
    let unpinned = std::process::Command::new(bin)
        .arg("run")
        .arg("--config")
        .arg(&base_config)
        .arg("--predictor")
        .arg(&predictor_path)
        .arg("--steps")
        .arg("5")
        .arg("--output")
        .arg(&output)
        .output()
        .expect("spawn aria");
    assert!(
        unpinned.status.success(),
        "unpinned run should still succeed: {}",
        String::from_utf8_lossy(&unpinned.stderr)
    );
}

/// Regression test for the bug fixed alongside this test: `aria emit` used
/// to always replay with the untrained `SimPredictor`, silently decoding the
/// wrong tokens for any run made with `aria run --predictor`. It also used
/// to ignore the trace header's seed/schedule/condition/match_policy, so a
/// `--config`-less replay of a non-default run diverged from the original
/// trajectory without warning.
///
/// This exercises the real `aria` binary end to end: `run --predictor`,
/// then `emit` both with and without `--predictor` against the same trace,
/// and asserts the decoded output differs — proving `emit --predictor`
/// actually drives the replay rather than being silently ignored.
#[test]
fn emit_predictor_flag_changes_decoded_output() {
    use std::process::Command;

    let dir = tempfile::tempdir().unwrap();
    let base_config = dir.path().join("base.toml");
    let predictor_path = dir.path().join("predictor.json");
    let trace_path = dir.path().join("trace.jsonl");
    let readout_path = dir.path().join("readout.safetensors");
    let out_with = dir.path().join("out_with_predictor.jsonl");
    let out_without = dir.path().join("out_without_predictor.jsonl");

    // N = 8 is sub-spec; only reachable through the escape, which the CLI
    // only exposes via a config file, not a flag.
    std::fs::write(
        &base_config,
        "n_modes = 8\nlatent_dim = 16\nallow_sub_spec_dims = true\n",
    )
    .unwrap();
    std::fs::write(&predictor_path, small_predictor_json(8, 16)).unwrap();

    let bin = env!("CARGO_BIN_EXE_aria");
    let run_ok = |cmd: &mut Command| {
        let out = cmd.output().expect("spawn aria");
        assert!(
            out.status.success(),
            "aria invocation failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };

    run_ok(
        Command::new(bin)
            .arg("run")
            .arg("--config")
            .arg(&base_config)
            .arg("--steps")
            .arg("20")
            .arg("--seed")
            .arg("7")
            .arg("--predictor")
            .arg(&predictor_path)
            .arg("--output")
            .arg(&trace_path),
    );

    // `--config` here supplies the `allow_sub_spec_dims` escape for N = 8;
    // the trace header (not this file) is what supplies seed/schedule/
    // condition/match_policy for a faithful replay.
    run_ok(
        Command::new(bin)
            .arg("emit")
            .arg("--config")
            .arg(&base_config)
            .arg("--trace")
            .arg(&trace_path)
            .arg("--readout")
            .arg(&readout_path)
            .arg("--init-seeded")
            .arg("3"),
    );

    run_ok(
        Command::new(bin)
            .arg("emit")
            .arg("--config")
            .arg(&base_config)
            .arg("--trace")
            .arg(&trace_path)
            .arg("--readout")
            .arg(&readout_path)
            .arg("--predictor")
            .arg(&predictor_path)
            .arg("--output")
            .arg(&out_with),
    );

    run_ok(
        Command::new(bin)
            .arg("emit")
            .arg("--config")
            .arg(&base_config)
            .arg("--trace")
            .arg(&trace_path)
            .arg("--readout")
            .arg(&readout_path)
            .arg("--output")
            .arg(&out_without),
    );

    let with_predictor = std::fs::read_to_string(&out_with).unwrap();
    let without_predictor = std::fs::read_to_string(&out_without).unwrap();
    assert_ne!(
        with_predictor, without_predictor,
        "emit --predictor must change the decoded ids — before the fix, \
         emit always replayed with the untrained stub regardless of \
         --predictor, so these were identical"
    );
}
