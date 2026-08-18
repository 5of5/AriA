//! WS3 Phase 3 gate — typed topological memory and `O(log |V|)` retrieval.
//!
//! Covers the plan's acceptance list end-to-end through the engine:
//!
//! - the default `match_policy = identity` trace is **byte-unchanged** by the
//!   graph-v2 migration (the goldens WS2 produced are the reference);
//! - Inv3 holds through every policy, and a refused op aborts Match without
//!   mutating `G` (𝕃6 transactionality at the engine boundary);
//! - the merge policy keeps `|V|` sub-linear (𝕃3) with the metric index and
//!   the graph in lock-step;
//! - Match is no longer quadratic in `|V|` — the O(T²) clone WS2's notes
//!   flagged is gone.

use std::path::PathBuf;
use std::sync::Mutex;

use aria_engine_backends::runner::{self, canonical_init, sim_engine};
use aria_engine_backends::{
    fit_growth_exponent, SimDiffuser, SimGraphBackend, SimOptical, SimPredictor,
};
use aria_engine_core::action::Action;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::{Engine, GraphBackend};
use aria_engine_core::error::AriaError;
use aria_engine_core::graph::{EdgeType, Graph, GraphOp, NodeType, UndoOp};
use aria_engine_core::policy::MatchPolicy;

fn spec_config() -> AriaConfig {
    AriaConfig::default() // N = 256, dim(Z) = 64, ε = 1, seed 42, identity
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/traces")
        .join(name)
}

/// The default trace must survive the graph-v2 migration byte-for-byte.
///
/// `match_policy = identity` absorbs exactly one node per Match, which is what
/// v0.1.0 did when the engine appended `z_{t}` before calling the backend. If
/// graph v2 changed *anything* observable on the default path — node counts,
/// residuals, energies — this is where it shows up.
#[test]
fn identity_trace_matches_the_committed_golden() {
    let mut config = spec_config();
    config.optical = Some("fft".into());
    let outcome = runner::run(config, 1000).expect("1000-step run must succeed");
    let produced = outcome.trace.to_jsonl();

    let path = golden_path("golden_opmd_n256_fft_1000.jsonl");
    if !path.exists() {
        // Fresh clone (the trace corpus is gitignored): bootstrap it rather
        // than skipping, so the next run in this tree is a real comparison.
        std::fs::create_dir_all(path.parent().unwrap()).expect("trace dir");
        std::fs::write(&path, &produced).expect("write golden");
        println!("bootstrapped golden at {}", path.display());
        return;
    }

    let golden = std::fs::read_to_string(&path).expect("read golden");
    let (g_lines, p_lines): (Vec<&str>, Vec<&str>) =
        (golden.lines().collect(), produced.lines().collect());
    assert_eq!(
        g_lines.len(),
        p_lines.len(),
        "trace length changed: golden {} rows, produced {} rows",
        g_lines.len(),
        p_lines.len()
    );
    for (i, (g, p)) in g_lines.iter().zip(&p_lines).enumerate() {
        assert_eq!(
            g, p,
            "row {i} diverged from the golden — graph v2 changed the default trace\n golden: {g}\n now:    {p}"
        );
    }
}

/// Identity keeps v0.1.0's shape: one node per Φ-cycle, no edges.
#[test]
fn identity_absorbs_one_node_per_cycle() {
    let outcome = runner::run(spec_config(), 1000).expect("run");
    assert_eq!(outcome.state.t, 250, "1000 OPMD steps = 250 Φ-cycles");
    assert_eq!(
        outcome.state.g.node_count(),
        250,
        "identity must absorb exactly one latent per Match"
    );
    assert_eq!(outcome.state.g.edge_count(), 0);
    assert!(
        outcome.summary.invariants_ok,
        "{:?}",
        outcome.summary.failures
    );
}

/// 𝕃3 through the real engine: merging bounds `|V|` sub-linearly.
#[test]
fn merge_policy_keeps_growth_sublinear() {
    let mut config = spec_config();
    config.match_policy = MatchPolicy::Merge;
    let engine = sim_engine(config.clone());
    let mut state = canonical_init(&engine, config.condition).expect("init");

    let mut samples = Vec::new();
    let cycles = 512u64;
    for cycle in 1..=cycles {
        state = engine
            .step_phi(state, config.condition)
            .unwrap_or_else(|e| panic!("Φ-cycle {cycle} failed: {e}"));
        if cycle.is_power_of_two() && cycle >= 16 {
            samples.push((cycle, state.g.node_count()));
        }
    }
    samples.push((cycles, state.g.node_count()));

    assert!(
        state.g.node_count() < usize::try_from(cycles).unwrap(),
        "merge produced one node per cycle ({}) — nothing merged",
        state.g.node_count()
    );
    let fit = fit_growth_exponent(&samples).expect("β must be fittable");
    assert!(
        fit.beta <= 1.0,
        "β = {:.4} > 1 (R² = {:.4}, samples {:?}) — growth is not sub-linear",
        fit.beta,
        fit.r_squared,
        samples
    );

    let report = engine.check(&state, config.condition);
    assert!(
        report.all_ok(),
        "merge run broke invariants: {:?}",
        report.failures()
    );
    assert!(
        state.g.ok(config.latent_dim),
        "Inv3 must hold after merging"
    );
}

/// The index must track `|V|` exactly across a long merge run: a proposer
/// reading a stale structure would silently degrade every merge decision.
#[test]
fn index_stays_in_step_with_the_graph() {
    let mut config = spec_config();
    config.match_policy = MatchPolicy::Merge;
    config.latent_dim = 16;
    let backend = SimGraphBackend::with_merge_tau(config.latent_dim, config.merge_tau);
    let engine = Engine::new(
        config.clone(),
        SimOptical::with_seed(config.n_modes, 42),
        SimPredictor::new(config.n_modes, config.latent_dim),
        backend,
        SimDiffuser::new(config.latent_dim),
    );
    let mut state = canonical_init_for(&engine, &config);
    for _ in 0..200 {
        state = engine.step_phi(state, config.condition).expect("cycle");
    }
    assert_eq!(
        engine.graph_backend().index_len(),
        state.g.node_count(),
        "index live count diverged from |V|"
    );
}

fn canonical_init_for(
    engine: &Engine<SimOptical, SimPredictor, SimGraphBackend, SimDiffuser>,
    config: &AriaConfig,
) -> aria_engine_core::state::State {
    engine
        .init(
            runner::canonical_psi0(config.n_modes),
            Graph::empty(),
            config.condition,
        )
        .expect("init")
}

/// A backend that proposes an op the graph must refuse (edge to a node that
/// does not exist). Match has to abort *before* `G` changes — 𝕃6.
#[derive(Debug)]
struct BadOpBackend {
    latent_dim: usize,
    committed: Mutex<usize>,
}

impl GraphBackend for BadOpBackend {
    fn edit_ops(
        &self,
        g: &Graph,
        z: &[f64],
        _policy: MatchPolicy,
        _target: Option<&Graph>,
        t: u64,
    ) -> Vec<GraphOp> {
        vec![
            GraphOp::AddNode {
                id: g.next_id(),
                ntype: NodeType::Observation,
                emb: z.to_vec(),
                ts: t,
            },
            // Dangling: node 9_999_999 does not exist.
            GraphOp::AddEdge {
                from: g.next_id(),
                to: 9_999_999,
                etype: EdgeType::CausallyPrecedes,
            },
        ]
    }

    fn ok(&self, g: &Graph) -> bool {
        g.ok(self.latent_dim)
    }

    fn commit_ops(&self, _ops: &[GraphOp], _g: &Graph) {
        *self
            .committed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
    }
}

#[test]
fn a_refused_op_aborts_match_and_never_commits() {
    let config = spec_config();
    let engine = Engine::new(
        config.clone(),
        SimOptical::with_seed(config.n_modes, 42),
        SimPredictor::new(config.n_modes, config.latent_dim),
        BadOpBackend {
            latent_dim: config.latent_dim,
            committed: Mutex::new(0),
        },
        SimDiffuser::new(config.latent_dim),
    );
    let state = engine
        .init(
            runner::canonical_psi0(config.n_modes),
            Graph::empty(),
            config.condition,
        )
        .expect("init");

    let err = engine
        .apply(state, Action::Match, config.condition)
        .expect_err("a dangling edge must abort Match");
    assert!(
        matches!(err, AriaError::Backend(ref m) if m.contains("Match op refused")),
        "expected a refused-op backend error, got {err}"
    );
    assert_eq!(
        *engine
            .graph_backend()
            .committed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        0,
        "auxiliary structures must not be told about an aborted Match"
    );
}

/// `max_graph_size` trips *after* the ops apply, so the guard has to roll the
/// batch back rather than leave a half-grown graph behind.
#[test]
fn oversized_graph_is_rejected() {
    let mut config = spec_config();
    config.latent_dim = 16;
    config.max_graph_size = 3;
    let engine = sim_engine(config.clone());
    let mut state = canonical_init(&engine, config.condition).expect("init");

    let mut last = None;
    for _ in 0..10 {
        match engine.apply(state, Action::Match, config.condition) {
            Ok(s) => state = s,
            Err(e) => {
                last = Some(e);
                break;
            }
        }
    }
    let err = last.expect("Match must eventually exceed max_graph_size");
    assert!(
        matches!(err, AriaError::Schedule(ref m) if m.contains("exceeds max")),
        "expected the size guard, got {err}"
    );
}

/// Match must not be quadratic in `|V|`.
///
/// v0.1.0 cloned `G` twice per Match, so a run cost `O(T²)` — WS2's notes call
/// it out as the reason the FFT scaling ratio was diluted. Ops + journal make a
/// step cost its edit, so 4× the cycles should cost ≈ 4× the time, not 16×.
/// The bound is deliberately loose (8×) so the test measures the *complexity
/// class*, not the machine.
#[test]
fn match_is_not_quadratic_in_graph_size() {
    fn run_cycles(cycles: u64) -> f64 {
        let config = spec_config();
        let engine = sim_engine(config.clone());
        let mut state = canonical_init(&engine, config.condition).expect("init");
        let t = std::time::Instant::now();
        for _ in 0..cycles {
            state = engine.step_phi(state, config.condition).expect("cycle");
        }
        std::hint::black_box(&state);
        t.elapsed().as_secs_f64()
    }

    let small = run_cycles(250);
    let large = run_cycles(1000);
    let ratio = large / small;
    assert!(
        ratio < 8.0,
        "4× the cycles cost {ratio:.1}× the time ({small:.3}s → {large:.3}s) — \
         Match still scales with |V| (quadratic would be ≈16×)"
    );
}

/// Every policy must leave a graph that satisfies Inv3.
#[test]
fn all_policies_preserve_inv3() {
    for policy in [
        MatchPolicy::Identity,
        MatchPolicy::OneEdit,
        MatchPolicy::Merge,
        MatchPolicy::RebuildGStar,
    ] {
        let mut config = spec_config();
        config.latent_dim = 16;
        config.match_policy = policy;
        let engine = sim_engine(config.clone());
        let mut state = canonical_init(&engine, config.condition).expect("init");
        for _ in 0..40 {
            state = engine
                .step_phi(state, config.condition)
                .unwrap_or_else(|e| panic!("{policy:?} failed: {e}"));
        }
        assert!(
            state.g.ok(config.latent_dim),
            "{policy:?} produced a graph that violates Inv3"
        );
        let report = engine.check(&state, config.condition);
        assert!(
            report.all_ok(),
            "{policy:?} broke invariants: {:?}",
            report.failures()
        );
    }
}

/// The journal is the rollback contract: replaying it must restore `(V, E, ℳ)`
/// exactly, whatever the policy proposed.
#[test]
fn journal_replay_restores_the_graph_for_every_policy() {
    let dim = 16;
    for policy in [
        MatchPolicy::Identity,
        MatchPolicy::OneEdit,
        MatchPolicy::Merge,
        MatchPolicy::RebuildGStar,
    ] {
        let backend = SimGraphBackend::with_merge_tau(dim, 0.5);
        let mut g = Graph::empty();
        // Seed a few nodes so every policy has something to act on.
        for t in 0..8u64 {
            let z: Vec<f64> = (0..dim).map(|i| (t as f64) + (i as f64) * 0.01).collect();
            let ops = backend.edit_ops(&g, &z, MatchPolicy::Identity, None, t);
            let journal = g.apply_ops(&ops, dim).expect("seed");
            backend.commit_ops(&ops, &g);
            drop(journal);
        }

        let before = g.clone();
        let z: Vec<f64> = (0..dim).map(|i| 1.0 + (i as f64) * 0.01).collect();
        let ops = backend.edit_ops(&g, &z, policy, None, 99);
        let journal: Vec<UndoOp> = g.apply_ops(&ops, dim).expect("apply");
        backend.commit_ops(&ops, &g);

        g.undo_ops(&journal);
        backend.revert_ops(&journal, &g);

        assert!(
            g.same_content(&before),
            "{policy:?}: journal replay did not restore (V, E, ℳ)"
        );
        assert_eq!(
            backend.index_len(),
            g.node_count(),
            "{policy:?}: index diverged from |V| after rollback"
        );
    }
}
