//! Non-collapsing merge seam: at a τ the contracted latent can actually
//! miss, Match appends a second node. Default τ = 0.5 stays put (P-ball
//! radius); the CLI already exposes `--merge-tau`.

use aria_engine_backends::runner::run;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::policy::MatchPolicy;

#[test]
fn merge_at_tau_005_grows_more_than_one_node_in_200_opmd_steps() {
    let cfg = AriaConfig {
        n_modes: 256,
        latent_dim: 32,
        match_policy: MatchPolicy::Merge,
        merge_tau: 0.05,
        schedule: "opmd".into(),
        seed: Some(42),
        ..AriaConfig::default()
    };
    let out = run(cfg, 200).expect("engine run");
    assert!(
        out.summary.invariants_ok,
        "failures: {:?}",
        out.summary.failures
    );
    assert!(
        out.summary.node_count > 1,
        "|V| = {} must grow under merge at τ = 0.05",
        out.summary.node_count
    );
}

#[test]
fn default_tau_merge_still_compiles_the_shipped_radius() {
    let cfg = AriaConfig::default();
    assert!((cfg.merge_tau - 0.5).abs() < 1e-15);
}
