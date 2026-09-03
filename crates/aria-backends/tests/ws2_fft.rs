//! WS2 Phase 2 gate — Inv1 drift over long runs at spec scale (plan WS2).
//!
//! The acceptance is: drift ≤ 1e-7 for N ≥ 256 over 10⁴ steps, measured on
//! BOTH backends (FFT is the new default there; Householder is the reference).
//!
//! The 10⁴ measurement runs on the optical operators directly: only the
//! OpticalStep touches ψ, so the energy-drift quantity is per-operator — and
//! the full runner loop at 10⁴ steps is dominated by the known O(T²) graph
//! clone (WS3's fix), not by the optical kernel. The runner integration is
//! checked separately at 10³ steps with invariants green.

use aria_engine_backends::runner;
use aria_engine_backends::{FftOptical, RefOptical, SimOptical};
use aria_engine_core::config::AriaConfig;
use aria_engine_core::engine::OpticalBackend;
use aria_engine_core::state::field_energy;
use num_complex::Complex64;

fn unit_psi(n: usize) -> Vec<Complex64> {
    let psi: Vec<Complex64> = (0..n)
        .map(|i| {
            let phase = (i as f64) * 0.12345;
            Complex64::new(phase.cos(), phase.sin())
        })
        .collect();
    let norm = field_energy(&psi);
    psi.into_iter()
        .map(|c| c / Complex64::new(norm, 0.0))
        .collect()
}

fn spec_scale_config(optical: &str) -> AriaConfig {
    // N = 256, d = 64, seed 42 — spec-valid defaults.
    AriaConfig {
        optical: Some(optical.to_string()),
        ..AriaConfig::default()
    }
}

/// Max |‖ψ‖₂ − 1| over 10⁴ optical applications (energy_0 = 1).
fn measure_drift_over_10k<O: OpticalBackend>(backend: &O) -> f64 {
    let psi0 = unit_psi(256);
    let mut psi = psi0;
    let mut worst = 0.0f64;
    for t in 0..10_000 {
        psi = backend.unitary_step(t, &psi);
        worst = worst.max((field_energy(&psi) - 1.0).abs());
    }
    worst
}

#[test]
fn inv1_drift_over_10k_steps_fft_backend() {
    let drift = measure_drift_over_10k(&FftOptical::with_seed(256, 42));
    assert!(
        drift <= 1e-7,
        "FFT energy drift {drift:e} exceeds the 1e-7 winning bound over 10⁴ steps"
    );
}

#[test]
fn inv1_drift_over_10k_steps_householder_backend() {
    let drift = measure_drift_over_10k(&SimOptical::with_seed(256, 42));
    assert!(
        drift <= 1e-7,
        "Householder energy drift {drift:e} exceeds the 1e-7 winning bound over 10⁴ steps"
    );
}

#[test]
fn fft_runner_integration_1000_steps_stays_green() {
    // The full Φ-loop with the FFT backend: invariants green end-to-end.
    let outcome = runner::run(spec_scale_config("fft"), 1000).unwrap();
    assert!(
        outcome.summary.invariants_ok,
        "{:?}",
        outcome.summary.failures
    );
    assert_eq!(outcome.summary.t, 250);
}

#[test]
fn fft_backend_runs_are_byte_deterministic() {
    // The committed byte-stability guard for the N ≥ 256 path: two identical
    // runs must produce byte-identical traces (seeded FFT, fixed fp order).
    let a = runner::run(spec_scale_config("fft"), 400).unwrap();
    let b = runner::run(spec_scale_config("fft"), 400).unwrap();
    assert_eq!(a.trace.to_jsonl(), b.trace.to_jsonl());
}

#[test]
fn default_selection_at_spec_scale_uses_the_fft_path() {
    // N = 256 with `optical = None` must behave exactly like an explicit
    // "fft" selection — proving the automatic default flips to FFT at N ≥ 256.
    let auto = AriaConfig {
        seed: Some(7),
        ..AriaConfig::default()
    };
    let mut forced = auto.clone();
    forced.optical = Some("fft".into());
    let a = runner::run(auto, 400).unwrap();
    let b = runner::run(forced, 400).unwrap();

    // The trajectories (every step row) must be byte-identical: both configs
    // resolve to the same FFT backend. The headers differ only in the
    // recorded `optical` field — `null` (auto) vs `"fft"` (explicit) — which
    // is exactly what a faithful, self-describing trace should preserve.
    let (a_jsonl, b_jsonl) = (a.trace.to_jsonl(), b.trace.to_jsonl());
    let entries = |jsonl: &str| {
        jsonl.lines()
            .skip(1)
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(entries(&a_jsonl), entries(&b_jsonl));

    let header_of = |jsonl: &str| {
        serde_json::from_str::<serde_json::Value>(jsonl.lines().next().unwrap()).unwrap()
    };
    let (ha, hb) = (header_of(&a_jsonl), header_of(&b_jsonl));
    assert_eq!(ha["optical"], serde_json::Value::Null);
    assert_eq!(hb["optical"], serde_json::json!("fft"));
    // Apart from `optical`, the headers agree — the resolved run is the same.
    let strip = |mut h: serde_json::Value| {
        h.as_object_mut().unwrap().remove("optical");
        h
    };
    assert_eq!(strip(ha), strip(hb));
}

#[test]
fn both_backends_preserve_energy_at_small_n() {
    for backend in [
        RefOptical::Fft(FftOptical::with_seed(8, 11)),
        RefOptical::Sim(SimOptical::with_seed(8, 11)),
    ] {
        let psi0 = unit_psi(8);
        let e0 = field_energy(&psi0);
        for t in 0..4 {
            let e1 = field_energy(&backend.unitary_step(t, &psi0));
            assert!((e1 - e0).abs() < 1e-12);
        }
    }
}
