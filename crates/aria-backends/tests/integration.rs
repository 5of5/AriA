//! Integration test: 1000-step OPMD run.
//!
//! This is the primary acceptance test for Phase 1:
//!   Exit₁ ≜ invariant suite green ∧ OPMD 1000-step run succeeds

use aria_engine_backends::{SimDiffuser, SimGraphBackend, SimOptical, SimPredictor};
use aria_engine_core::action::Action;
use aria_engine_core::condition::Condition;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::Engine;
use aria_engine_core::graph::Graph;
use aria_engine_core::scheduler::Scheduler;
use num_complex::Complex64;

#[test]
fn opmd_1000_step_run() {
    let config = AriaConfig::test_config();

    let optical = SimOptical::with_seed(config.n_modes, 42);
    let predictor = SimPredictor::new(config.n_modes, config.latent_dim);
    let graph_backend = SimGraphBackend::new(config.latent_dim);
    let diffuser = SimDiffuser::new(config.latent_dim);

    let engine = Engine::new(config.clone(), optical, predictor, graph_backend, diffuser);

    // Normalized initial field
    let psi0: Vec<Complex64> = (0..config.n_modes)
        .map(|i| {
            let phase = (i as f64) * 0.12345;
            Complex64::new(phase.cos(), phase.sin())
        })
        .collect();
    let norm: f64 = psi0
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt();
    let psi0: Vec<Complex64> = psi0
        .into_iter()
        .map(|c| c / Complex64::new(norm, 0.0))
        .collect();

    let g0 = Graph::empty();
    let state = engine
        .init(psi0, g0, Condition::Token)
        .expect("init should succeed");

    let mut scheduler = Scheduler::from_string("opmd", 2).unwrap();

    let result = engine.run(state.clone(), &mut scheduler, 1000, Condition::Token);

    match result {
        Ok((final_state, trace)) => {
            // Verify invariants at end
            let report = engine.check(&final_state, Condition::Token);
            assert!(
                report.all_ok(),
                "Invariants violated after 1000 steps: {:?}",
                report.failures()
            );

            // Check trace shape: should be mostly OPMD pattern
            let seq = trace.action_sequence();
            assert_eq!(seq.len(), 1000, "trace should have 1000 entries");

            // Each OPMD cycle produces "OPMD"
            for chunk_start in (0..seq.len()).step_by(4) {
                let end = (chunk_start + 4).min(seq.len());
                let chunk = &seq[chunk_start..end];
                // Each cycle should be O-P-M-D
                if chunk.len() == 4 {
                    assert_eq!(
                        chunk,
                        "OPMD",
                        "cycle at {} should be OPMD, got '{}'",
                        chunk_start / 4,
                        chunk
                    );
                }
            }

            // Final state should have advanced t (number of Diffuse steps = 250)
            assert_eq!(final_state.t, 250, "t should advance once per Diffuse");

            // Energy should be conserved (Inv1)
            assert!(
                (final_state.energy() - state.energy()).abs() < 1e-10,
                "Inv1: energy not conserved after 1000 steps"
            );

            // Graph should be finite and GraphOK
            assert!(final_state.g.ok(config.latent_dim), "Inv3: GraphOK failed");

            // Trace entries should be valid
            for entry in &trace.entries {
                assert!(entry.res.is_finite(), "residual must be finite");
                assert!(entry.energy.is_finite(), "energy must be finite");
            }

            eprintln!(
                "OPMD 1000-step: t={}, |G|={}, energy={:.6}, final_res={:.6}",
                final_state.t,
                final_state.g.size(),
                final_state.energy(),
                trace.entries.last().map_or(0.0, |e| e.res)
            );
        }
        Err(e) => {
            panic!("OPMD 1000-step run failed: {e}");
        }
    }
}

#[test]
fn step_phi_single_cycle() {
    let config = AriaConfig::test_config();
    let optical = SimOptical::new(config.n_modes);
    let predictor = SimPredictor::new(config.n_modes, config.latent_dim);
    let graph_backend = SimGraphBackend::new(config.latent_dim);
    let diffuser = SimDiffuser::new(config.latent_dim);
    let engine = Engine::new(config.clone(), optical, predictor, graph_backend, diffuser);

    let psi0: Vec<Complex64> = (0..config.n_modes)
        .map(|_| Complex64::new(1.0, 0.0))
        .collect();
    let norm = psi0
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt();
    let psi0: Vec<Complex64> = psi0
        .into_iter()
        .map(|c| c / Complex64::new(norm, 0.0))
        .collect();

    let state = engine
        .init(psi0, Graph::empty(), Condition::Token)
        .expect("init should succeed");
    let result = engine.step_phi(state, Condition::Token);

    match result {
        Ok(s) => {
            let report = engine.check(&s, Condition::Token);
            assert!(report.all_ok(), "Invariants violated after step_phi");
            assert_eq!(s.t, 1, "t should be 1 after one Diffuse");
        }
        Err(e) => panic!("step_phi failed: {e}"),
    }
}

#[test]
fn each_action_unchanged_clauses() {
    let config = AriaConfig::test_config();
    let optical = SimOptical::new(config.n_modes);
    let predictor = SimPredictor::new(config.n_modes, config.latent_dim);
    let graph_backend = SimGraphBackend::new(config.latent_dim);
    let diffuser = SimDiffuser::new(config.latent_dim);
    let engine = Engine::new(config.clone(), optical, predictor, graph_backend, diffuser);

    let psi0: Vec<Complex64> = (0..config.n_modes)
        .map(|i| Complex64::new((i as f64).cos(), (i as f64).sin()))
        .collect();
    let norm = psi0
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt();
    let psi0: Vec<Complex64> = psi0
        .into_iter()
        .map(|c| c / Complex64::new(norm, 0.0))
        .collect();

    let state = engine
        .init(psi0, Graph::empty(), Condition::Token)
        .expect("init should succeed");

    // OpticalStep: changes ψ, UNCHANGED z, G, t
    {
        let s1 = state.clone();
        let s2 = engine
            .apply(s1.clone(), Action::OpticalStep, Condition::Token)
            .unwrap();
        assert_ne!(s2.psi, s1.psi, "OpticalStep must change psi");
        assert_eq!(s2.z, s1.z, "OpticalStep: UNCHANGED z");
        assert_eq!(s2.t, s1.t, "OpticalStep: UNCHANGED t");
        assert_eq!(
            s2.g.node_count(),
            s1.g.node_count(),
            "OpticalStep: UNCHANGED G"
        );
    }

    // Predict: changes z, UNCHANGED ψ, G, t
    {
        let s1 = state.clone();
        let s2 = engine
            .apply(s1.clone(), Action::Predict, Condition::Token)
            .unwrap();
        assert_ne!(s2.z, s1.z, "Predict must change z");
        assert_eq!(s2.psi, s1.psi, "Predict: UNCHANGED psi");
        assert_eq!(s2.t, s1.t, "Predict: UNCHANGED t");
        assert_eq!(s2.g.node_count(), s1.g.node_count(), "Predict: UNCHANGED G");
    }

    // Match: changes G, UNCHANGED ψ, z, t
    {
        let s1 = state.clone();
        let s2 = engine
            .apply(s1.clone(), Action::Match, Condition::Token)
            .unwrap();
        assert_eq!(s2.psi, s1.psi, "Match: UNCHANGED psi");
        assert_eq!(s2.z, s1.z, "Match: UNCHANGED z");
        assert_eq!(s2.t, s1.t, "Match: UNCHANGED t");
    }

    // Diffuse: changes z, t, UNCHANGED ψ, G
    {
        let s1 = state.clone();
        let s2 = engine
            .apply(s1.clone(), Action::Diffuse, Condition::Token)
            .unwrap();
        // Diffuse: advances t, UNCHANGED ψ, G; z may change
        // (with identity policy, z stays same — both are valid per Spec)
        assert_eq!(s2.t, s1.t + 1, "Diffuse: t must advance");
        assert_eq!(s2.psi, s1.psi, "Diffuse: UNCHANGED psi");
        assert_eq!(s2.g.node_count(), s1.g.node_count(), "Diffuse: UNCHANGED G");
    }

    // Stutter: UNCHANGED all
    {
        let s1 = state.clone();
        let s2 = engine
            .apply(s1.clone(), Action::Stutter, Condition::Token)
            .unwrap();
        assert_eq!(s2.psi, s1.psi, "Stutter: UNCHANGED psi");
        assert_eq!(s2.z, s1.z, "Stutter: UNCHANGED z");
        assert_eq!(s2.t, s1.t, "Stutter: UNCHANGED t");
        assert_eq!(s2.g.node_count(), s1.g.node_count(), "Stutter: UNCHANGED G");
    }
}

#[test]
fn inv1_energy_conserved_across_steps() {
    let config = AriaConfig::test_config();
    let optical = SimOptical::new(config.n_modes);
    let predictor = SimPredictor::new(config.n_modes, config.latent_dim);
    let graph_backend = SimGraphBackend::new(config.latent_dim);
    let diffuser = SimDiffuser::new(config.latent_dim);
    let engine = Engine::new(config.clone(), optical, predictor, graph_backend, diffuser);

    let psi0: Vec<Complex64> = (0..config.n_modes)
        .map(|i| Complex64::new((i as f64).cos(), (i as f64).sin()))
        .collect();
    let norm = psi0
        .iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt();
    let psi0: Vec<Complex64> = psi0
        .into_iter()
        .map(|c| c / Complex64::new(norm, 0.0))
        .collect();

    let state = engine
        .init(psi0, Graph::empty(), Condition::Token)
        .expect("init should succeed");
    let e0 = state.energy_0;

    let mut s = state;
    for action in &[
        Action::OpticalStep,
        Action::Predict,
        Action::Match,
        Action::Diffuse,
    ] {
        s = engine.apply(s, *action, Condition::Token).unwrap();
        let report = engine.check(&s, Condition::Token);
        assert!(report.inv1_ok, "Inv1 violated after {action:?}");
        assert!(
            (s.energy() - e0).abs() < 1e-10,
            "energy changed after {:?}: {} != {}",
            action,
            s.energy(),
            e0
        );
    }
}
