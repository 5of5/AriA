//! Trace-shape and fairness tests.
//!
//! These tests encode the positive/negative trace examples from the Spec:
//!   W1  (O P M D)^n        — accepted schedule
//!   X1  S^ω                — rejected by stutter budget (liveness/fairness)
//!
//! Safety alone cannot reject X1, so the scheduler enforces C5 by limiting
//! consecutive stutters to K.

use aria_engine_backends::{SimDiffuser, SimGraphBackend, SimOptical, SimPredictor};
use aria_engine_core::action::Action;
use aria_engine_core::condition::Condition;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::Engine;
use aria_engine_core::graph::Graph;
use aria_engine_core::scheduler::Scheduler;
use num_complex::Complex64;

fn normalized_psi(n: usize) -> Vec<Complex64> {
    let psi: Vec<Complex64> = (0..n)
        .map(|i| {
            let phase = (i as f64) * 0.12345;
            Complex64::new(phase.cos(), phase.sin())
        })
        .collect();
    let norm = psi
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt();
    psi.into_iter()
        .map(|c| c / Complex64::new(norm, 0.0))
        .collect()
}

fn make_engine() -> Engine<SimOptical, SimPredictor, SimGraphBackend, SimDiffuser> {
    let config = AriaConfig::test_config();
    let optical = SimOptical::with_seed(config.n_modes, 42);
    let predictor = SimPredictor::new(config.n_modes, config.latent_dim);
    let graph_backend = SimGraphBackend::new(config.latent_dim);
    let diffuser = SimDiffuser::new(config.latent_dim);
    Engine::new(config, optical, predictor, graph_backend, diffuser)
}

#[test]
fn w1_opmd_trace_accepts() {
    let engine = make_engine();
    let psi0 = normalized_psi(engine.config().n_modes);
    let state = engine
        .init(psi0, Graph::empty(), Condition::Token)
        .expect("init should succeed");

    let mut scheduler = Scheduler::from_string("opmd", 2).unwrap();
    let (final_state, trace) = engine
        .run(state, &mut scheduler, 40, Condition::Token)
        .unwrap();

    let seq = trace.action_sequence();
    assert_eq!(seq.len(), 40, "trace should contain one entry per step");

    for (i, chunk) in seq.as_bytes().chunks(4).enumerate() {
        assert_eq!(
            chunk,
            b"OPMD",
            "cycle {} did not match OPMD: got {:?}",
            i,
            std::str::from_utf8(chunk)
        );
    }

    assert_eq!(final_state.t, 10, "t should advance once per Diffuse");

    let report = engine.check(&final_state, Condition::Token);
    assert!(
        report.all_ok(),
        "invariants violated: {:?}",
        report.failures()
    );
}

#[test]
fn x1_pure_stutter_budget_enforced() {
    // X1: S^ω is not allowed because C5 limits consecutive stutters to K.
    let mut scheduler = Scheduler::from_string("ssssss", 2).unwrap();
    let mut consecutive = 0;

    for _ in 0..30 {
        let action = scheduler.next_action_budgeted();
        if action == Action::Stutter {
            consecutive += 1;
        } else {
            consecutive = 0;
        }
        assert!(
            consecutive <= 2,
            "stutter budget K=2 violated: {consecutive} consecutive stutters"
        );
    }
}

#[test]
fn w2_opmd_with_bounded_stutters() {
    // W2: an OPMD trace with up to K interleaved stutters is still accepted.
    let engine = make_engine();
    let psi0 = normalized_psi(engine.config().n_modes);
    let state = engine
        .init(psi0, Graph::empty(), Condition::Token)
        .expect("init should succeed");

    let mut scheduler = Scheduler::from_string("opsmd", 2).unwrap();
    let (final_state, trace) = engine
        .run(state, &mut scheduler, 5, Condition::Token)
        .unwrap();

    let report = engine.check(&final_state, Condition::Token);
    assert!(
        report.all_ok(),
        "invariants violated: {:?}",
        report.failures()
    );

    // The productive (non-stutter) sequence should still be O-P-M-D in order.
    let productive: String = trace
        .action_sequence()
        .chars()
        .filter(|c| *c != 'S')
        .collect();
    assert_eq!(productive, "OPMD");
}
