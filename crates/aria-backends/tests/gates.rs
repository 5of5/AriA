//! Phase 4 — the optional Inv5–Inv11 gates must observe without enlarging Spec.
//!
//! The load-bearing property is negative: turning a gate on must not change
//! which behaviors the engine produces. A gate can report; it can never steer.

use aria_engine_backends::runner;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::gates::{Gate, GateConfig};

fn config(gates: &[Gate]) -> AriaConfig {
    AriaConfig {
        gates: GateConfig {
            enabled: gates.to_vec(),
            window: 4,
            horizon: 8,
            ..GateConfig::default()
        },
        ..AriaConfig::test_config()
    }
}

#[test]
fn gates_are_off_by_default() {
    let out = runner::run(AriaConfig::test_config(), 400).unwrap();
    assert!(
        out.summary.gates.enabled.is_empty(),
        "no gate may default on"
    );
    assert!(out.summary.gates.all_ok());
}

#[test]
fn enabling_every_gate_does_not_change_behavior() {
    // Same config, gates off vs all on: identical trace, identical state.
    let off = runner::run(config(&[]), 400).unwrap();
    let on = runner::run(config(&Gate::ALL), 400).unwrap();

    assert_eq!(
        off.trace.to_jsonl(),
        on.trace.to_jsonl(),
        "gates changed the trace"
    );
    assert_eq!(off.summary.t, on.summary.t);
    assert_eq!(off.summary.graph_size, on.summary.graph_size);
    assert_eq!(off.summary.action_sequence, on.summary.action_sequence);
    assert_eq!(on.summary.gates.enabled.len(), 7);
}

#[test]
fn the_reference_opmd_run_passes_every_gate() {
    let out = runner::run(config(&Gate::ALL), 1000).unwrap();
    assert!(out.summary.invariants_ok, "{:?}", out.summary.failures);
    assert!(
        out.summary.gates.all_ok(),
        "OPMD should satisfy Inv5–11: {:?}",
        out.summary.gates.breaches
    );
}

#[test]
fn x1_perpetual_stutter_breaches_inv5_and_inv11() {
    // X1 is safety-admissible but must not read as success under the gates.
    // 𝒮 caps the scheduler's stutter budget at K = 4 (spec §0.4), so an
    // all-stutter schedule degrades to S⁴ O S⁴ O … under the 𝐂7 fallback;
    // the operating gates still reject the stream: Inv5 sees S-runs of 4
    // (budget 2) and Inv11 sees windows with zero productive actions.
    let mut cfg = config(&[Gate::Inv5StutterBudget, Gate::Inv11FairProductivity]);
    cfg.schedule = "sssssssssssssssss".into();
    cfg.stutter_k = 4;
    cfg.gates.stutter_k = 2;

    let out = runner::run(cfg, 60).unwrap();

    // Inv1–4 still hold: stuttering is Spec-legal.
    assert!(out.summary.invariants_ok, "{:?}", out.summary.failures);
    // But the operating gates reject it.
    assert!(out.summary.gates.breaches.iter().any(|b| b.gate == "inv5"));
    assert!(out.summary.gates.breaches.iter().any(|b| b.gate == "inv11"));
}

#[test]
fn a_bounded_stutter_schedule_keeps_inv5() {
    let mut cfg = config(&[Gate::Inv5StutterBudget]);
    cfg.schedule = "opssmd".into();
    let out = runner::run(cfg, 60).unwrap();
    assert!(
        out.summary.gates.all_ok(),
        "K = 2 stutters must be allowed: {:?}",
        out.summary.gates.breaches
    );
}

#[test]
fn one_edit_match_keeps_inv7_and_inv10() {
    let mut cfg = config(&[Gate::Inv7MergeAcyclicity, Gate::Inv10MatchWellTyped]);
    cfg.match_policy = aria_engine_core::policy::MatchPolicy::OneEdit;
    let out = runner::run(cfg, 400).unwrap();
    assert!(out.summary.invariants_ok, "{:?}", out.summary.failures);
    assert!(
        out.summary.gates.all_ok(),
        "one_edit must keep the spine acyclic and well typed: {:?}",
        out.summary.gates.breaches
    );
}

#[test]
fn gate_config_round_trips_through_toml() {
    let src = r#"
n_modes = 8
latent_dim = 16
eps = 1.0
allow_sub_spec_dims = true

[gates]
enabled = ["inv5", "inv9"]
window = 6
horizon = 16
"#;
    let cfg = AriaConfig::from_toml(src).expect("gates should parse from TOML");
    assert!(cfg.allow_sub_spec_dims, "N = 8 needs the test-only escape");
    assert_eq!(
        cfg.gates.enabled,
        vec![Gate::Inv5StutterBudget, Gate::Inv9EnergyEveryAction]
    );
    assert_eq!(cfg.gates.window, 6);
    assert_eq!(cfg.gates.horizon, 16);

    let out = runner::run(cfg, 200).unwrap();
    assert!(
        out.summary.gates.all_ok(),
        "{:?}",
        out.summary.gates.breaches
    );
}
