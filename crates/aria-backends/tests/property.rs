//! Property-based invariant tests.
//!
//! Random legal action sequences must preserve Inv1–4 after every step.

use aria_engine_backends::{SimDiffuser, SimGraphBackend, SimOptical, SimPredictor};
use aria_engine_core::action::Action;
use aria_engine_core::condition::Condition;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::Engine;
use aria_engine_core::graph::Graph;
use num_complex::Complex64;
use proptest::prelude::*;

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

fn arb_action() -> impl Strategy<Value = Action> {
    prop_oneof![
        Just(Action::OpticalStep),
        Just(Action::Predict),
        Just(Action::Match),
        Just(Action::Diffuse),
        Just(Action::Stutter),
    ]
}

fn arb_condition() -> impl Strategy<Value = Condition> {
    prop_oneof![
        Just(Condition::Token),
        Just(Condition::Diffusion),
        Just(Condition::WorldModel),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_sequences_preserve_invariants(
        actions in prop::collection::vec(arb_action(), 1..50),
        cond in arb_condition(),
    ) {
        let config = AriaConfig::test_config();
        let optical = SimOptical::with_seed(config.n_modes, 42);
        let predictor = SimPredictor::new(config.n_modes, config.latent_dim);
        let graph_backend = SimGraphBackend::new(config.latent_dim);
        let diffuser = SimDiffuser::new(config.latent_dim);
        let engine = Engine::new(config.clone(), optical, predictor, graph_backend, diffuser);

        let mut state = engine
            .init(normalized_psi(config.n_modes), Graph::empty(), cond)
            .expect("init should succeed");

        for action in actions {
            state = engine.apply(state, action, cond).expect("apply should succeed");
            let report = engine.check(&state, cond);
            prop_assert!(
                report.all_ok(),
                "invariants violated after {:?}: {:?}",
                action,
                report.failures()
            );
        }
    }

    #[test]
    fn opmd_cycles_preserve_invariants(n in 1usize..20, cond in arb_condition()) {
        let config = AriaConfig::test_config();
        let optical = SimOptical::with_seed(config.n_modes, 42);
        let predictor = SimPredictor::new(config.n_modes, config.latent_dim);
        let graph_backend = SimGraphBackend::new(config.latent_dim);
        let diffuser = SimDiffuser::new(config.latent_dim);
        let engine = Engine::new(config.clone(), optical, predictor, graph_backend, diffuser);

        let mut state = engine
            .init(normalized_psi(config.n_modes), Graph::empty(), cond)
            .expect("init should succeed");

        for _ in 0..n {
            state = engine.step_phi(state, cond).expect("OPMD cycle should succeed");
            let report = engine.check(&state, cond);
            prop_assert!(report.all_ok(), "invariants violated: {:?}", report.failures());
        }
    }
}
