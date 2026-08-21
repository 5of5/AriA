//! Aria CLI — Spec-faithful runtime runner.
//!
//! Usage:
//!   aria run --schedule opmd --steps 1000
//!   aria step --action OpticalStep --state state.json
//!   aria check --state state.json
//!
//! `run` delegates to `aria_engine_backends::runner::run`, the same code path
//! used by the Python extension and the WASM module (Phase 2 parity).

use aria_engine_backends::runner::{self, canonical_init, sim_engine, RefPredictor};
use aria_engine_backends::{
    fit_growth_exponent, log_checkpoints, HnswIndex, HnswParams, SimPredictor, TrainedPredictor,
    VectorIndex,
};
use aria_engine_core::action::Action;
use aria_engine_core::condition::Condition;
use aria_engine_core::config::AriaConfig;
use aria_engine_core::gates::{Gate, GateConfig};
use aria_engine_core::policy::MatchPolicy;
use aria_engine_core::scheduler::Scheduler;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "aria", version, about = "Aria — Ariadne Transformer runtime")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to TOML config file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the engine for N steps
    Run {
        /// Schedule string (default: "opmd")
        #[arg(long)]
        schedule: Option<String>,

        /// Number of steps
        #[arg(long, default_value = "100")]
        steps: u64,

        /// Epsilon tolerance
        #[arg(long)]
        eps: Option<f64>,

        /// Optical backend: "fft" (ℙ1) or "householder" (v0.1.0 reference).
        /// Default: fft for N ≥ 256, householder below (plan WS2).
        #[arg(long)]
        optical: Option<String>,

        /// Conditioning: token, diffusion, world_model
        #[arg(long)]
        condition: Option<String>,

        /// Number of optical modes
        #[arg(long)]
        n_modes: Option<usize>,

        /// Latent dimension
        #[arg(long)]
        latent_dim: Option<usize>,

        /// Stutter budget K
        #[arg(long)]
        stutter_k: Option<u64>,

        /// Seed for reproducibility
        #[arg(long)]
        seed: Option<u64>,

        /// Output trace file (JSONL)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Trained predictor weights (JSON from python/training/train_jepa.py).
        /// Omit to use the Phase 1 stub predictor.
        #[arg(long)]
        predictor: Option<PathBuf>,

        /// Optional operating gates Inv5–Inv11, e.g. "inv5,inv9" or "all".
        /// These are monitors, never Spec enlargement.
        #[arg(long)]
        gates: Option<String>,

        /// Exit non-zero if any enabled operating gate is breached
        #[arg(long)]
        strict_gates: bool,

        /// Disable strict invariant checking
        #[arg(long)]
        no_strict: bool,

        /// Seed G₀ from a JSON file (`aria-dev-seed-v1` or a serialized Graph).
        /// Init only — not a new action.
        #[arg(long)]
        seed_graph: Option<PathBuf>,

        /// Write the final graph JSON after the run (outside Φ).
        #[arg(long)]
        export_graph: Option<PathBuf>,

        /// Match policy: identity | one_edit | merge | rebuild_gstar
        #[arg(long)]
        match_policy: Option<String>,

        /// Merge distance τ (only with `--match-policy merge`)
        #[arg(long)]
        merge_tau: Option<f64>,
    },

    /// Apply a single step to a state
    Step {
        /// Action: OpticalStep, Predict, Match, Diffuse, Stutter
        #[arg(long)]
        action: String,

        /// Conditioning
        #[arg(long)]
        condition: Option<String>,

        /// Path to state JSON
        #[arg(long)]
        state: Option<PathBuf>,

        /// N modes
        #[arg(long)]
        n_modes: Option<usize>,

        /// Latent dimension
        #[arg(long)]
        latent_dim: Option<usize>,

        /// Epsilon
        #[arg(long)]
        eps: Option<f64>,
    },

    /// Check invariants on a state
    Check {
        /// Path to state JSON
        #[arg(long)]
        state: PathBuf,

        /// Conditioning
        #[arg(long)]
        condition: Option<String>,

        /// Latent dimension
        #[arg(long)]
        latent_dim: Option<usize>,

        /// Trained checkpoint: load it, print the σ_max audit, and check
        /// against the trained backend that produced the state (plan WS1)
        #[arg(long)]
        predictor: Option<PathBuf>,
    },

    /// Measure Φ-cycle throughput across sizes (Phase 4 performance notes)
    Bench {
        /// Comma-separated N values to sweep
        #[arg(long, default_value = "16,64,256")]
        n_modes: String,

        /// Latent dimension
        #[arg(long, default_value = "64")]
        latent_dim: usize,

        /// Steps per measurement
        #[arg(long, default_value = "1000")]
        steps: u64,

        /// Also measure with every operating gate enabled
        #[arg(long)]
        with_gates: bool,

        /// ℙ5 retrieval bench (plan WS3): comma-separated |V| values. Builds a
        /// metric index of that many points and reports p50/p99 nearest-
        /// neighbour latency. The Phase-3 gate is < 250 µs at |V| = 10⁶.
        /// Native only — memory-heavy at 10⁶.
        #[arg(long)]
        graph: Option<String>,

        /// 𝕃3 growth bench (plan WS3): run this many Φ-cycles under
        /// `match_policy = merge` and fit β in |V| = O(T^β) by OLS on
        /// (ln T, ln |V|). Spec §8 predicate 3 requires β ≤ 1.
        #[arg(long, default_value = "0")]
        beta_cycles: u64,
    },

    /// Export a training dataset for the Phase 3 JEPA loop.
    ///
    /// With `--input`, encodes a real corpus (text, code, any bytes) as optical
    /// fields — this is the production path. Without it, emits synthetic
    /// phase-ramp trajectories, which exist for smoke tests only and are not
    /// training data.
    Dataset {
        /// Real corpus file. Anything byte-readable: text, code, logs.
        #[arg(long)]
        input: Option<PathBuf>,

        /// Window stride in bytes (default: window size = non-overlapping)
        #[arg(long)]
        stride: Option<usize>,

        /// Number of synthetic trajectories (smoke-test path only)
        #[arg(long, default_value = "64")]
        trajectories: usize,

        /// Snapshots per synthetic trajectory (smoke-test path only)
        #[arg(long, default_value = "16")]
        length: usize,

        /// Number of optical modes (= bytes per window on the real-data path)
        #[arg(long)]
        n_modes: Option<usize>,

        /// Seed for the optical operator (synthetic path only)
        #[arg(long)]
        seed: Option<u64>,

        /// Output JSON file (stdout when omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Write the in-repo columnar fallback (`ARIACOL1`) instead of JSON.
        /// Frames are bit-identical to `dataset_from_bytes`; only the wrapper
        /// changes. UT-0 / Q-2026-08-17-1.
        #[arg(long)]
        columnar: bool,
    },

    /// Decode a completed run's z-sequence (𝔸5 / 𝕃5).
    ///
    /// Reads a JSONL trace and a readout weight file. Recovers `z` by
    /// replaying Φ from the trace header (seed, schedule, condition, match
    /// policy — `--config` only fills in what the header omits). Pass
    /// `--predictor` when the original `aria run` used one: without it, a
    /// trace produced by a trained predictor replays with the untrained stub
    /// and silently decodes the wrong tokens. Never writes back into the
    /// engine — emit is an I/O sink.
    Emit {
        /// JSONL trace from `aria run --output`
        #[arg(long)]
        trace: PathBuf,

        /// `aria-readout-v1` safetensors (discrete or continuous)
        #[arg(long)]
        readout: PathBuf,

        /// Optional `aria-tokenizer-v1` JSON — maps discrete ids to pieces
        #[arg(long)]
        tokenizer: Option<PathBuf>,

        /// Write JSONL here (stdout when omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Also dump the recovered z-sequence (WS5 ℒ_NLL input)
        #[arg(long)]
        dump_latents: Option<PathBuf>,

        /// Write a seeded discrete head to `--readout` and exit (no Φ touch).
        /// Seed is the MMIX LCG start; dim and |V_o| come from the trace /
        /// config. Used to mint an `aria-readout-v1` file before a trained
        /// head exists (WS5).
        #[arg(long)]
        init_seeded: Option<u64>,

        /// Trained predictor weights the original `aria run` used (JSON v1 or
        /// safetensors v2). Required to replay a trained run faithfully —
        /// omit only when the trace was produced with the Phase 1 stub.
        #[arg(long)]
        predictor: Option<PathBuf>,
    },

    /// Streaming long-horizon verification (spec §8 / WS6).
    ///
    /// Runs T steps with memory O(1) in T, audits X1–X5, fits β, and writes
    /// an `aria-verify-receipt-v1` JSON. Default policy is merge (𝕃3).
    Verify {
        /// Number of steps (winning condition is T ≥ 10⁵)
        #[arg(long, default_value = "100000")]
        steps: u64,

        /// Schedule string (default: "opmd")
        #[arg(long)]
        schedule: Option<String>,

        /// Conditioning: token, diffusion, world_model
        #[arg(long)]
        condition: Option<String>,

        /// Number of optical modes
        #[arg(long)]
        n_modes: Option<usize>,

        /// Latent dimension
        #[arg(long)]
        latent_dim: Option<usize>,

        /// Seed
        #[arg(long)]
        seed: Option<u64>,

        /// Trained predictor (JSON v1 or safetensors v2). Omit for SimPredictor.
        #[arg(long)]
        predictor: Option<PathBuf>,

        /// Optional operating gates, e.g. "all"
        #[arg(long)]
        gates: Option<String>,

        /// Match policy (default: merge)
        #[arg(long)]
        match_policy: Option<String>,

        /// Merge distance τ
        #[arg(long)]
        merge_tau: Option<f64>,

        /// Optional streaming JSONL trace (O(1) memory; write-through)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Receipt JSON path (default: docs/evidence/v0.2.0_longrun_receipt.json)
        #[arg(long)]
        receipt: Option<PathBuf>,

        /// Optical-starve window W (default 64)
        #[arg(long, default_value = "64")]
        window: usize,

        /// Uncapped-diffuse run cap (default 8)
        #[arg(long, default_value = "8")]
        d_cap: usize,
    },

}

fn main() -> ExitCode {
    env_logger::init();
    let cli = Cli::parse();

    match real_main(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

// One block per subcommand (run/step/check/bench/dataset); WS6 of
// plan_v0.2.0.md adds `verify` and is the right moment to split this into
// per-subcommand handlers.
#[allow(clippy::too_many_lines)]
fn real_main(cli: Cli) -> Result<(), String> {
    let base = match cli.config {
        Some(ref path) => {
            let contents = fs::read_to_string(path)
                .map_err(|e| format!("failed to read config {}: {}", path.display(), e))?;
            AriaConfig::from_toml(&contents).map_err(|e| format!("failed to parse config: {e}"))?
        }
        None => AriaConfig::default(),
    };

    match cli.command {
        Commands::Run {
            schedule,
            steps,
            eps,
            optical,
            condition,
            n_modes,
            latent_dim,
            stutter_k,
            seed,
            output,
            predictor,
            gates,
            strict_gates,
            no_strict,
            seed_graph,
            export_graph,
            match_policy,
            merge_tau,
        } => {
            let mut config = base;
            if let Some(v) = schedule {
                config.schedule = v;
            }
            if let Some(v) = optical {
                config.optical = Some(v);
            }
            if let Some(v) = eps {
                config.eps = v;
            }
            if let Some(v) = n_modes {
                config.n_modes = v;
            }
            if let Some(v) = latent_dim {
                config.latent_dim = v;
            }
            if let Some(v) = stutter_k {
                config.stutter_k = v;
            }
            if let Some(v) = seed {
                config.seed = Some(v);
            }
            if let Some(ref v) = condition {
                config.condition = runner::parse_condition(v).map_err(|e| e.to_string())?;
            }
            config.strict = !no_strict;
            if let Some(ref list) = gates {
                config.gates.enabled = GateConfig::parse_list(list)?;
                config.gates.stutter_k = config.stutter_k;
            }
            if let Some(ref p) = match_policy {
                config.match_policy = parse_match_policy(p)?;
            }
            if let Some(t) = merge_tau {
                config.merge_tau = t;
            }

            let predictor = match predictor {
                Some(ref path) => {
                    let trained = TrainedPredictor::from_file(path)
                        .map_err(|e| format!("failed to load {}: {}", path.display(), e))?;
                    check_predictor_dims(n_modes, latent_dim, &trained, path)?;
                    // The checkpoint fixes N and dim(Z); adopt them (unless
                    // the caller explicitly pinned different ones, checked
                    // above).
                    config.n_modes = trained.n_modes();
                    config.latent_dim = trained.latent_dim();
                    let lip = trained.measured_lipschitz().map_err(|e| e.to_string())?;
                    eprintln!(
                        "Predictor: trained weights from {} (Lip(P) = {:.4})",
                        path.display(),
                        lip
                    );
                    RefPredictor::Trained(trained)
                }
                None => RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim)),
            };

            eprintln!(
                "Aria run: {} steps, schedule={}, eps={}, N={}, dim(Z)={}, condition={:?}, match={:?}",
                steps, config.schedule, config.eps, config.n_modes, config.latent_dim, config.condition, config.match_policy
            );

            let g0 = match seed_graph {
                Some(ref path) => {
                    let g = aria_engine_backends::load_seed_graph(
                        path,
                        config.n_modes,
                        config.latent_dim,
                    )
                    .map_err(|e| format!("seed graph {}: {e}", path.display()))?;
                    eprintln!(
                        "Seed graph: {} ({} nodes, {} edges)",
                        path.display(),
                        g.node_count(),
                        g.edge_count()
                    );
                    g
                }
                None => aria_engine_core::graph::Graph::empty(),
            };

            let outcome =
                runner::run_with_graph(config, steps, predictor, g0).map_err(|e| e.to_string())?;
            let s = &outcome.summary;

            eprintln!("Completed {} steps successfully.", s.steps);
            eprintln!(
                "Final: t={}, |V|={}, |G|={}, energy={:.6}, residual={:.6}, invariants={}",
                s.t,
                s.node_count,
                s.graph_size,
                s.energy,
                s.residual,
                if s.invariants_ok { "OK" } else { "FAILED" }
            );

            // Phase 1 audit (plan WS1): σ_max per weight matrix, present iff
            // the run used the trained backend.
            if let Some(r) = &s.spectral_report {
                eprintln!(
                    "σ_max audit: token={:.9} diffusion={:.9} world_model={:.9} embed={:.9}",
                    r.token, r.diffusion, r.world_model, r.embed
                );
            }

            if !s.invariants_ok {
                for f in &s.failures {
                    eprintln!("  {f}");
                }
                return Err("invariant check failed on final state".into());
            }

            if !s.gates.enabled.is_empty() {
                eprintln!(
                    "Operating gates [{}]: {} breach(es)",
                    s.gates.enabled.join(", "),
                    s.gates.breaches.len()
                );
                for b in &s.gates.breaches {
                    eprintln!("  {} @ step {}: {}", b.gate, b.step, b.detail);
                }
                if strict_gates && !s.gates.all_ok() {
                    return Err("operating gate breached (--strict-gates)".into());
                }
            }

            let jsonl = outcome.trace.to_jsonl();
            match output {
                Some(path) => {
                    fs::write(&path, &jsonl)
                        .map_err(|e| format!("failed to write trace {}: {}", path.display(), e))?;
                    eprintln!("Trace written to {}", path.display());
                }
                None => print!("{jsonl}"),
            }
            if let Some(ref path) = export_graph {
                let json = serde_json::to_string_pretty(&outcome.state.g)
                    .map_err(|e| format!("serialize graph: {e}"))?;
                fs::write(path, json)
                    .map_err(|e| format!("failed to write graph {}: {e}", path.display()))?;
                eprintln!(
                    "Graph written to {} ({} nodes, {} edges)",
                    path.display(),
                    outcome.state.g.node_count(),
                    outcome.state.g.edge_count()
                );
            }
            Ok(())
        }

        Commands::Step {
            action,
            condition,
            state: state_path,
            n_modes,
            latent_dim,
            eps,
        } => {
            let mut config = base;
            if let Some(v) = n_modes {
                config.n_modes = v;
            }
            if let Some(v) = latent_dim {
                config.latent_dim = v;
            }
            if let Some(v) = eps {
                config.eps = v;
            }
            let cond = match condition {
                Some(ref v) => runner::parse_condition(v).map_err(|e| e.to_string())?,
                None => config.condition,
            };
            let action = parse_action(&action)?;

            // `aria step --state` never calls Engine::init, so the 𝒮 bounds
            // are enforced here for that path (plan WS0: every CLI entry).
            config.validate().map_err(|e| e.to_string())?;
            let engine = sim_engine(config);
            let state = match state_path {
                Some(p) => {
                    let s = fs::read_to_string(&p)
                        .map_err(|e| format!("failed to read state {}: {}", p.display(), e))?;
                    serde_json::from_str(&s).map_err(|e| format!("failed to parse state: {e}"))?
                }
                None => canonical_init(&engine, cond).map_err(|e| e.to_string())?,
            };

            let new_state = engine
                .apply(state, action, cond)
                .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&new_state)
                    .map_err(|e| format!("failed to serialize state: {e}"))?
            );
            Ok(())
        }

        Commands::Check {
            state,
            condition,
            latent_dim,
            predictor,
        } => {
            let mut config = base;
            if let Some(v) = latent_dim {
                config.latent_dim = v;
            }
            let cond = match condition {
                Some(ref v) => runner::parse_condition(v).map_err(|e| e.to_string())?,
                None => config.condition,
            };

            let contents = fs::read_to_string(&state)
                .map_err(|e| format!("failed to read state {}: {}", state.display(), e))?;
            let state: aria_engine_core::state::State = serde_json::from_str(&contents)
                .map_err(|e| format!("failed to parse state: {e}"))?;

            // `aria check` never calls Engine::init; enforce the 𝒮 bounds
            // here for the same reason as `step` (plan WS0).
            config.validate().map_err(|e| e.to_string())?;

            let engine = match predictor {
                Some(ref path) => {
                    let trained = TrainedPredictor::from_file(path)
                        .map_err(|e| format!("failed to load {}: {}", path.display(), e))?;
                    check_predictor_dims(None, latent_dim, &trained, path)?;
                    config.n_modes = trained.n_modes();
                    config.latent_dim = trained.latent_dim();
                    let report = trained.spectral_report().map_err(|e| e.to_string())?;
                    println!(
                        "σ_max audit ({}) — token={:.9} diffusion={:.9} world_model={:.9} embed={:.9}",
                        path.display(),
                        report.token,
                        report.diffusion,
                        report.world_model,
                        report.embed
                    );
                    runner::engine_with(config, RefPredictor::Trained(trained))
                }
                None => sim_engine(config),
            };
            let report = engine.check(&state, cond);
            if report.all_ok() {
                println!("All invariants hold: Inv1 ✓ Inv2 ✓ Inv3 ✓ Inv4 ✓");
                Ok(())
            } else {
                for failure in report.failures() {
                    println!("  {failure}");
                }
                Err("invariant violations".into())
            }
        }

        Commands::Bench {
            n_modes,
            latent_dim,
            steps,
            with_gates,
            graph,
            beta_cycles,
        } => {
            let sizes: Vec<usize> = n_modes
                .split(',')
                .map(|s| {
                    s.trim()
                        .parse::<usize>()
                        .map_err(|e| format!("bad --n-modes value '{}': {}", s.trim(), e))
                })
                .collect::<Result<_, _>>()?;

            println!(
                "{:>8}  {:>8}  {:>10}  {:>12}  {:>12}  {:>10}  {:>10}",
                "N", "dim(Z)", "steps", "setup (ms)", "run (ms)", "steps/s", "µs/step"
            );
            let mut run_ms_by_n: Vec<(usize, f64)> = Vec::new();
            for n in sizes {
                let mut config = base.clone();
                config.n_modes = n;
                config.latent_dim = latent_dim.min(2 * n);
                if with_gates {
                    config.gates.enabled = Gate::ALL.to_vec();
                }

                // Setup (the O(N³) unitary build) is timed separately from the
                // Φ-cycle loop; conflating them hides which one actually scales.
                let t_setup = std::time::Instant::now();
                let engine = runner::sim_engine(config.clone());
                let state =
                    runner::canonical_init(&engine, config.condition).map_err(|e| e.to_string())?;
                let setup_ms = t_setup.elapsed().as_secs_f64() * 1000.0;

                let mut scheduler = Scheduler::from_string(&config.schedule, config.stutter_k)?;

                let t_run = std::time::Instant::now();
                let (final_state, _, _) = engine
                    .run_monitored(state, &mut scheduler, steps, config.condition)
                    .map_err(|e| e.to_string())?;
                let run = t_run.elapsed();
                let run_ms = run.as_secs_f64() * 1000.0;
                run_ms_by_n.push((n, run_ms));

                let report = engine.check(&final_state, config.condition);
                if !report.all_ok() {
                    return Err(format!(
                        "N={}: invariants failed: {:?}",
                        n,
                        report.failures()
                    ));
                }

                println!(
                    "{:>8}  {:>8}  {:>10}  {:>12.1}  {:>12.1}  {:>10.0}  {:>10.1}",
                    n,
                    config.latent_dim,
                    steps,
                    setup_ms,
                    run_ms,
                    steps as f64 / run.as_secs_f64(),
                    run_ms * 1000.0 / steps as f64
                );
            }

            // The ℙ1 scaling evidence (plan WS2): 256→1024 should be ≈ 4–5×
            // (N log N), not ~16× (N²) — printed only when both were measured.
            if let (Some((_, t256)), Some((_, t1024))) = (
                run_ms_by_n.iter().find(|(n, _)| *n == 256),
                run_ms_by_n.iter().find(|(n, _)| *n == 1024),
            ) {
                println!(
                    "256→1024 scaling ratio: {:.2}× (N log N predicts 4.53×, N² predicts 16×)",
                    t1024 / t256
                );
            }

            if let Some(sizes) = graph {
                bench_retrieval(&sizes, latent_dim)?;
            }
            if beta_cycles > 0 {
                bench_growth(&base, beta_cycles)?;
            }
            Ok(())
        }

        Commands::Dataset {
            input,
            stride,
            trajectories,
            length,
            n_modes,
            seed,
            output,
            columnar,
        } => {
            let n_modes = n_modes.unwrap_or(base.n_modes);

            if let Some(ref path) = input {
                let stride = stride.unwrap_or(n_modes);
                let bytes = fs::read(path)
                    .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
                let source = path.display().to_string();
                if columnar {
                    let (dataset, blob) =
                        aria_engine_backends::ingest_columnar(&source, &bytes, n_modes, stride)?;
                    let summary = format!(
                        "{} frames from {} bytes of {} (columnar ARIACOL1, N={}, stride={})",
                        dataset.trajectories[0].len(),
                        dataset.source_bytes,
                        dataset.source,
                        n_modes,
                        stride
                    );
                    match output {
                        Some(out) => {
                            fs::write(&out, &blob).map_err(|e| {
                                format!("failed to write dataset {}: {}", out.display(), e)
                            })?;
                            eprintln!("Dataset written to {}: {summary}", out.display());
                        }
                        None => {
                            return Err(
                                "--columnar needs --output (binary blob, not stdout JSON)".into()
                            );
                        }
                    }
                } else {
                    let dataset =
                        aria_engine_backends::dataset_from_bytes(&source, &bytes, n_modes, stride)?;
                    let summary = format!(
                        "{} frames from {} bytes of {} (spectral-dft, N={}, stride={})",
                        dataset.trajectories[0].len(),
                        dataset.source_bytes,
                        dataset.source,
                        n_modes,
                        stride
                    );
                    let json = serde_json::to_string(&dataset)
                        .map_err(|e| format!("failed to serialize dataset: {e}"))?;
                    match output {
                        Some(out) => {
                            fs::write(&out, &json).map_err(|e| {
                                format!("failed to write dataset {}: {}", out.display(), e)
                            })?;
                            eprintln!("Dataset written to {}: {summary}", out.display());
                        }
                        None => println!("{json}"),
                    }
                }
            } else {
                eprintln!(
                    "note: no --input given — emitting synthetic phase-ramp data for smoke tests only"
                );
                let seed = seed.or(base.seed).unwrap_or(42);
                let dataset = runner::optical_dataset(n_modes, seed, trajectories, length);
                let summary = format!(
                    "{trajectories} synthetic trajectories × {length} snapshots (N={n_modes}, seed={seed})"
                );
                let json = serde_json::to_string(&dataset)
                    .map_err(|e| format!("failed to serialize dataset: {e}"))?;
                match output {
                    Some(out) => {
                        fs::write(&out, &json).map_err(|e| {
                            format!("failed to write dataset {}: {}", out.display(), e)
                        })?;
                        eprintln!("Dataset written to {}: {summary}", out.display());
                    }
                    None => println!("{json}"),
                }
            }
            Ok(())
        }

        Commands::Emit {
            trace,
            readout,
            tokenizer,
            output,
            dump_latents,
            init_seeded,
            predictor,
        } => emit_cmd(
            base,
            &trace,
            &readout,
            tokenizer.as_deref(),
            output.as_deref(),
            dump_latents.as_deref(),
            init_seeded,
            predictor.as_deref(),
        ),

        Commands::Verify {
            steps,
            schedule,
            condition,
            n_modes,
            latent_dim,
            seed,
            predictor,
            gates,
            match_policy,
            merge_tau,
            output,
            receipt,
            window,
            d_cap,
        } => verify_cmd(
            base,
            steps,
            schedule.as_deref(),
            condition.as_deref(),
            n_modes,
            latent_dim,
            seed,
            predictor.as_deref(),
            gates.as_deref(),
            match_policy.as_deref(),
            merge_tau,
            output.as_deref(),
            receipt.as_deref(),
            window,
            d_cap,
        ),
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn verify_cmd(
    mut config: AriaConfig,
    steps: u64,
    schedule: Option<&str>,
    condition: Option<&str>,
    n_modes: Option<usize>,
    latent_dim: Option<usize>,
    seed: Option<u64>,
    predictor: Option<&std::path::Path>,
    gates: Option<&str>,
    match_policy: Option<&str>,
    merge_tau: Option<f64>,
    output: Option<&std::path::Path>,
    receipt: Option<&std::path::Path>,
    window: usize,
    d_cap: usize,
) -> Result<(), String> {
    use aria_engine_backends::{verify, AuditConfig, VerifyOpts};

    if let Some(v) = schedule {
        config.schedule = v.to_string();
    }
    if let Some(v) = n_modes {
        config.n_modes = v;
    }
    if let Some(v) = latent_dim {
        config.latent_dim = v;
    }
    if let Some(v) = seed {
        config.seed = Some(v);
    }
    if let Some(v) = condition {
        config.condition = runner::parse_condition(v).map_err(|e| e.to_string())?;
    }
    if let Some(list) = gates {
        config.gates.enabled = GateConfig::parse_list(list)?;
        config.gates.stutter_k = config.stutter_k;
    }
    config.match_policy = match match_policy {
        Some(p) => parse_match_policy(p)?,
        None => MatchPolicy::Merge,
    };
    if let Some(t) = merge_tau {
        config.merge_tau = t;
    }
    // A 10⁵ identity run can exceed the default |G| cap; merge stays well
    // under it, but do not fail a long verify on a bookkeeping bound.
    if config.max_graph_size < 100_000 {
        config.max_graph_size = 100_000;
    }

    let predictor = match predictor {
        Some(path) => {
            let trained = TrainedPredictor::from_file(path)
                .map_err(|e| format!("failed to load {}: {}", path.display(), e))?;
            check_predictor_dims(n_modes, latent_dim, &trained, path)?;
            // The checkpoint fixes N and dim(Z); adopt them (unless the
            // caller explicitly pinned different ones, checked above).
            config.n_modes = trained.n_modes();
            config.latent_dim = trained.latent_dim();
            eprintln!(
                "Predictor: trained weights from {} (Lip(P) = {:.4})",
                path.display(),
                trained.measured_lipschitz().map_err(|e| e.to_string())?
            );
            RefPredictor::Trained(trained)
        }
        None => RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim)),
    };

    let receipt_path = receipt.map_or_else(
        || PathBuf::from("docs/evidence/v0.2.0_longrun_receipt.json"),
        PathBuf::from,
    );
    if let Some(parent) = receipt_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
    }

    let mut audit = AuditConfig::from_config(&config);
    audit.w = window;
    audit.d_cap = d_cap;

    eprintln!(
        "Aria verify: {steps} steps, schedule={}, N={}, dim(Z)={}, condition={:?}, match={:?}",
        config.schedule, config.n_modes, config.latent_dim, config.condition, config.match_policy
    );

    let r = verify(VerifyOpts {
        config,
        steps,
        predictor,
        g0: aria_engine_core::graph::Graph::empty(),
        audit,
        trace_path: output.map(PathBuf::from),
        receipt_path: Some(receipt_path.clone()),
    })
    .map_err(|e| e.to_string())?;

    eprintln!(
        "verify: steps={} inv1_max_drift={:.3e} inv2={} inv3={} inv4={} β={:.4} (R²={:.4}) family={} X1-5=({},{},{},{},{}) {:.1} steps/s",
        r.steps,
        r.inv1_max_drift,
        r.inv2_violations,
        r.inv3_violations,
        r.inv4_violations,
        r.graph.measured_beta,
        r.graph.beta_r2,
        r.trace_audit.family,
        r.trace_audit.x1,
        r.trace_audit.x2,
        r.trace_audit.x3,
        r.trace_audit.x4,
        r.trace_audit.x5,
        r.steps_per_s
    );
    if let Some(note) = &r.beta_note {
        eprintln!("β note: {note}");
    }
    eprintln!("receipt: {}", receipt_path.display());

    if !r.invariants_ok {
        return Err("verify: invariants not green".into());
    }
    if r.inv1_max_drift > 1e-7 {
        return Err(format!(
            "verify: Inv1 max drift {:.3e} exceeds 1e-7",
            r.inv1_max_drift
        ));
    }
    if r.graph.measured_beta.is_finite() && r.graph.measured_beta > 1.0 {
        return Err(format!("verify: β = {:.4} > 1", r.graph.measured_beta));
    }
    Ok(())
}

/// Post-hoc readout. Structurally incapable of mutating Φ.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn emit_cmd(
    mut config: AriaConfig,
    trace_path: &std::path::Path,
    readout_path: &std::path::Path,
    tokenizer_path: Option<&std::path::Path>,
    output: Option<&std::path::Path>,
    dump_latents: Option<&std::path::Path>,
    init_seeded: Option<u64>,
    predictor_path: Option<&std::path::Path>,
) -> Result<(), String> {
    use aria_engine_backends::runner::latents_with;
    use aria_engine_backends::{BpeTokenizer, DiscreteReadout, Readout};

    let jsonl = fs::read_to_string(trace_path)
        .map_err(|e| format!("failed to read trace {}: {e}", trace_path.display()))?;
    let (header, rows) = parse_trace(&jsonl)?;
    config.n_modes = header.n_modes;
    config.latent_dim = header.latent_dim;
    config.eps = header.eps;

    if let Some(seed) = init_seeded {
        let head = DiscreteReadout::seeded(header.latent_dim, config.vocab_size, 1.0, seed)
            .map_err(|e| e.to_string())?;
        head.to_file(readout_path).map_err(|e| e.to_string())?;
        eprintln!(
            "wrote seeded discrete head dim={} vocab={} seed={seed} → {}",
            header.latent_dim,
            config.vocab_size,
            readout_path.display()
        );
        return Ok(());
    }

    // The trace header is the source of truth for the trajectory-defining
    // fields; `--config` only fills in what the header omits. Overriding
    // these from `--config` would silently diverge the replay from the run
    // that produced the trace.
    if let Some(seed) = header.seed {
        config.seed = Some(seed);
    }
    config.schedule = header.schedule.clone();
    config.condition = header.condition;
    config.match_policy = header.match_policy;

    let predictor = match predictor_path {
        Some(path) => {
            let trained = TrainedPredictor::from_file(path)
                .map_err(|e| format!("failed to load {}: {}", path.display(), e))?;
            check_predictor_dims(None, None, &trained, path)?;
            RefPredictor::Trained(trained)
        }
        None => RefPredictor::Sim(SimPredictor::new(config.n_modes, config.latent_dim)),
    };

    let steps = u64::try_from(rows.len()).map_err(|_| "trace longer than u64::MAX".to_string())?;
    let zs = latents_with(config, steps, predictor).map_err(|e| e.to_string())?;
    if zs.len() != rows.len() {
        return Err(format!(
            "replay produced {} latents for {} trace rows — config does not match the run",
            zs.len(),
            rows.len()
        ));
    }

    if let Some(path) = dump_latents {
        let mut buf = String::new();
        for z in &zs {
            buf.push_str(&serde_json::to_string(z).map_err(|e| e.to_string())?);
            buf.push('\n');
        }
        fs::write(path, buf)
            .map_err(|e| format!("failed to write latents {}: {e}", path.display()))?;
    }

    let readout = Readout::from_file(readout_path)
        .map_err(|e| format!("failed to load readout {}: {e}", readout_path.display()))?;
    if readout.dim() != header.latent_dim {
        return Err(format!(
            "readout dim {} does not match trace latent_dim {}",
            readout.dim(),
            header.latent_dim
        ));
    }
    let tokenizer = match tokenizer_path {
        Some(p) => Some(
            BpeTokenizer::from_file(p)
                .map_err(|e| format!("failed to load tokenizer {}: {e}", p.display()))?,
        ),
        None => None,
    };

    let mut out = String::new();
    for ((t, action), z) in rows.iter().zip(&zs) {
        let line = match &readout {
            Readout::Discrete(h) => {
                let id = h.decode_id(z).map_err(|e| e.to_string())?;
                let mut v = serde_json::json!({ "t": t, "action": action, "id": id });
                if let Some(tok) = &tokenizer {
                    v["piece"] = serde_json::json!(tok.decode_one(id).map_err(|e| e.to_string())?);
                }
                serde_json::to_string(&v).map_err(|e| e.to_string())?
            }
            Readout::Continuous(h) => {
                let a = h.emit(z).map_err(|e| e.to_string())?;
                serde_json::to_string(&serde_json::json!({ "t": t, "action": action, "a": a }))
                    .map_err(|e| e.to_string())?
            }
        };
        out.push_str(&line);
        out.push('\n');
    }

    match output {
        Some(path) => {
            fs::write(path, &out)
                .map_err(|e| format!("failed to write emit {}: {e}", path.display()))?;
            eprintln!(
                "emit: {} rows → {} ({})",
                rows.len(),
                path.display(),
                match readout {
                    Readout::Discrete(_) => "discrete",
                    Readout::Continuous(_) => "continuous",
                }
            );
        }
        None => print!("{out}"),
    }
    Ok(())
}

type TraceRows = Vec<(u64, String)>;

struct TraceHeader {
    n_modes: usize,
    latent_dim: usize,
    eps: f64,
    seed: Option<u64>,
    schedule: String,
    condition: Condition,
    match_policy: MatchPolicy,
}

fn header_usize(header: &serde_json::Value, key: &str) -> Result<usize, String> {
    let n = header
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("header missing {key}"))?;
    usize::try_from(n).map_err(|_| format!("header {key} exceeds usize"))
}

fn header_field_or_warn<T: serde::de::DeserializeOwned>(
    header: &serde_json::Value,
    key: &str,
    default: T,
) -> Result<T, String> {
    if let Some(v) = header.get(key) {
        serde_json::from_value(v.clone()).map_err(|e| format!("trace header {key}: {e}"))
    } else {
        eprintln!(
            "aria emit: trace header has no '{key}' (pre-v0.2.1 trace format) — \
             assuming a default; replay may not match the original run exactly"
        );
        Ok(default)
    }
}

fn parse_trace(jsonl: &str) -> Result<(TraceHeader, TraceRows), String> {
    let mut lines = jsonl.lines();
    let header_line = lines
        .next()
        .ok_or_else(|| "trace is empty".to_string())?;
    let header: serde_json::Value =
        serde_json::from_str(header_line).map_err(|e| format!("trace header: {e}"))?;
    if header.get("type").and_then(serde_json::Value::as_str) != Some("config") {
        return Err("first trace line must be a config header".into());
    }
    let n_modes = header_usize(&header, "n_modes")?;
    let latent_dim = header_usize(&header, "latent_dim")?;
    let eps = header
        .get("eps")
        .and_then(serde_json::Value::as_f64)
        .ok_or("header missing eps")?;
    let seed = header.get("seed").and_then(serde_json::Value::as_u64);
    let schedule = header_field_or_warn(&header, "schedule", "opmd".to_string())?;
    let condition = header_field_or_warn(&header, "condition", Condition::Token)?;
    let match_policy = header_field_or_warn(&header, "match_policy", MatchPolicy::Identity)?;

    let mut rows = Vec::new();
    for (i, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("trace row {i}: {e}"))?;
        let t = v
            .get("t")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("trace row {i} missing t"))?;
        let action = v
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("trace row {i} missing action"))?
            .to_string();
        rows.push((t, action));
    }
    if rows.is_empty() {
        return Err("trace has no step rows".into());
    }
    Ok((
        TraceHeader {
            n_modes,
            latent_dim,
            eps,
            seed,
            schedule,
            condition,
            match_policy,
        },
        rows,
    ))
}

/// Errors if the caller explicitly pinned `--n-modes`/`--latent-dim` to
/// something other than what `trained` was learned for.
///
/// `None` means "not pinned" — the checkpoint's dimensions are then adopted
/// silently by the caller, which stays the convenient default (most runs
/// don't pass these flags alongside `--predictor` at all). A pinned-but-
/// conflicting value used to be silently overridden instead of surfaced,
/// which also made the dimension check in `runner::validate_config`
/// unreachable from the CLI (it never saw a mismatch, because the CLI had
/// already forced agreement before calling it).
fn check_predictor_dims(
    n_modes: Option<usize>,
    latent_dim: Option<usize>,
    trained: &TrainedPredictor,
    predictor_path: &std::path::Path,
) -> Result<(), String> {
    if let Some(v) = n_modes {
        if v != trained.n_modes() {
            return Err(format!(
                "--n-modes {v} conflicts with predictor {}: the checkpoint was trained at N={}. \
                 Omit --n-modes to use the checkpoint's dimensions, or pass the matching value.",
                predictor_path.display(),
                trained.n_modes()
            ));
        }
    }
    if let Some(v) = latent_dim {
        if v != trained.latent_dim() {
            return Err(format!(
                "--latent-dim {v} conflicts with predictor {}: the checkpoint was trained at dim(Z)={}. \
                 Omit --latent-dim to use the checkpoint's dimensions, or pass the matching value.",
                predictor_path.display(),
                trained.latent_dim()
            ));
        }
    }
    Ok(())
}

fn parse_match_policy(s: &str) -> Result<MatchPolicy, String> {
    match s.to_lowercase().as_str() {
        "identity" => Ok(MatchPolicy::Identity),
        "one_edit" | "one-edit" => Ok(MatchPolicy::OneEdit),
        "merge" => Ok(MatchPolicy::Merge),
        "rebuild_gstar" | "rebuild-gstar" | "rebuild" => Ok(MatchPolicy::RebuildGStar),
        other => Err(format!(
            "unknown match policy '{other}' (expected identity|one_edit|merge|rebuild_gstar)"
        )),
    }
}

fn parse_action(s: &str) -> Result<Action, String> {
    match s.to_lowercase().as_str() {
        "opticalstep" | "optical_step" | "o" => Ok(Action::OpticalStep),
        "predict" | "p" => Ok(Action::Predict),
        "match" | "m" => Ok(Action::Match),
        "diffuse" | "d" => Ok(Action::Diffuse),
        "stutter" | "s" => Ok(Action::Stutter),
        other => Err(format!(
            "unknown action '{other}' (expected OpticalStep|Predict|Match|Diffuse|Stutter)"
        )),
    }
}

/// ℙ5 retrieval bench (plan WS3): nearest-neighbour latency versus `|V|`.
///
/// Reports p50/p99 rather than a mean because the Phase-3 gate is a tail
/// guarantee (< 250 µs at `|V| = 10⁶`), and a mean hides the tail that matters.
/// Also prints distance evaluations per query — the algorithmic quantity behind
/// the `O(log |V|)` claim, which unlike wall-clock does not depend on the host.
fn bench_retrieval(sizes: &str, dim: usize) -> Result<(), String> {
    let sizes: Vec<usize> = sizes
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<usize>()
                .map_err(|e| format!("bad --graph value '{}': {}", s.trim(), e))
        })
        .collect::<Result<_, _>>()?;

    let params = HnswParams::default();
    println!(
        "\nℙ5 retrieval bench — dim(Z) = {dim}, M = {}, ef_add = {}, ef_search = {} (gate: p99 < 250 µs at |V| = 10⁶)",
        params.connectivity, params.expansion_add, params.expansion_search
    );
    println!(
        "{:>10}  {:>12}  {:>12}  {:>12}  {:>12}  {:>10}",
        "|V|", "build (s)", "p50 (µs)", "p99 (µs)", "max (µs)", "visited"
    );

    for n in sizes {
        // Points are streamed into the index: keeping a second copy of 10⁶ ×
        // dim f64 alongside the index would double an already heavy footprint.
        let mut lcg = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            lcg = lcg
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((lcg >> 11) as f64) / ((1u64 << 53) as f64)
        };

        let t_build = std::time::Instant::now();
        let mut index = HnswIndex::with_params(dim, params);
        for id in 0..n as u64 {
            let v: Vec<f64> = (0..dim).map(|_| next()).collect();
            index.add(id, &v);
        }
        let build_s = t_build.elapsed().as_secs_f64();

        let queries: Vec<Vec<f64>> = (0..1000)
            .map(|_| (0..dim).map(|_| next()).collect())
            .collect();

        // Fault in the query path (thread-local stamp, vector pages the
        // search will actually touch) so p50/p99 measure retrieval, not
        // first-touch page faults. The Phase-3 gate is a hot-query tail.
        for q in queries.iter().take(50) {
            std::hint::black_box(index.nearest_probed(q, 10));
        }

        let mut micros = Vec::with_capacity(queries.len());
        let mut visited_total = 0usize;
        for q in &queries {
            let t = std::time::Instant::now();
            let (results, stats) = index.nearest_probed(q, 10);
            micros.push(t.elapsed().as_secs_f64() * 1e6);
            visited_total += stats.visited;
            std::hint::black_box(&results);
        }
        micros.sort_by(f64::total_cmp);
        // Quantile lookup: `q ∈ [0, 1]` maps to a sorted-index. The clamp
        // guards both ends; the explicit `min` afterwards is belt-and-braces
        // against the empty-vector edge case (queries is empty only if the
        // caller passed `--graph ""`, which we already reject at parse time).
        let pick = |q: f64| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "q is clamped to [0,1] and micros.len() ≤ 1000, so the product is a non-negative finite value well below 2^53"
            )]
            let idx = (micros.len() as f64 * q.clamp(0.0, 1.0)) as usize;
            micros[idx.min(micros.len() - 1)]
        };

        println!(
            "{:>10}  {:>12.2}  {:>12.1}  {:>12.1}  {:>12.1}  {:>10}",
            n,
            build_s,
            pick(0.50),
            pick(0.99),
            micros[micros.len() - 1],
            visited_total / queries.len()
        );
    }
    Ok(())
}

/// 𝕃3 growth bench (plan WS3): fit β in `|V| = O(T^β)` under the merge policy.
///
/// Runs the real engine — not a synthetic point stream — because the claim is
/// about latents the Φ-cycle actually produces, and a bounded 𝒵 is exactly what
/// makes the sphere-packing argument bite.
fn bench_growth(base: &AriaConfig, cycles: u64) -> Result<(), String> {
    let mut config = base.clone();
    config.match_policy = MatchPolicy::Merge;
    config.validate().map_err(|e| e.to_string())?;

    let engine = sim_engine(config.clone());
    let mut state = canonical_init(&engine, config.condition).map_err(|e| e.to_string())?;

    let checkpoints = log_checkpoints(cycles);
    let mut samples: Vec<(u64, usize)> = Vec::new();
    let t_run = std::time::Instant::now();

    for cycle in 1..=cycles {
        state = engine
            .step_phi(state, config.condition)
            .map_err(|e| format!("Φ-cycle {cycle} failed: {e}"))?;
        if checkpoints.contains(&cycle) {
            samples.push((cycle, state.g.node_count()));
        }
    }
    let run_s = t_run.elapsed().as_secs_f64();

    println!(
        "\n𝕃3 growth bench — match_policy = merge, τ = {}, {cycles} Φ-cycles in {run_s:.2} s",
        config.merge_tau
    );
    println!("{:>10}  {:>10}", "T", "|V|");
    for (t, v) in &samples {
        println!("{t:>10}  {v:>10}");
    }

    let report = engine.check(&state, config.condition);
    if !report.all_ok() {
        return Err(format!(
            "growth bench ended with invariant failures: {:?}",
            report.failures()
        ));
    }

    match fit_growth_exponent(&samples) {
        Some(fit) => {
            println!(
                "β = {:.4} (R² = {:.4}, {} samples) — spec §8 predicate 3 requires β ≤ 1",
                fit.beta, fit.r_squared, fit.samples
            );
            if fit.beta > 1.0 {
                return Err(format!(
                    "measured β = {:.4} > 1: growth is not sub-linear",
                    fit.beta
                ));
            }
        }
        None => return Err("not enough checkpoints to fit β".into()),
    }
    Ok(())
}
