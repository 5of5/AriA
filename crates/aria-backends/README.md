# aria-engine-backends — simulated operator implementations

<p align="center">
  <img src="https://raw.githubusercontent.com/aria-ai/aria/main/assets/aria-logo-transparent.png" alt="Aria" width="240" />
</p>

**Pluggable simulated backends for the `aria-engine-core` engine.**

This crate provides reference (electronic simulation) implementations of the four operator traits defined by [`aria-engine-core`](https://crates.io/crates/aria-engine-core): OpticalBackend, Predictor, GraphBackend, and Diffuser. All backends are trait objects — swap in GPU or photonic implementations later without changing the Spec runner.

## Backends

### `SimOptical` — ideal unitary evolution

Implements **𝔸4**: every admissible optical transformation is (near-)unitary. Uses a product of Householder reflections with deterministic phase rotations, seeded by the discrete clock `t`. Energy is exactly conserved (Inv1 holds ideally).

```rust
use aria_backends::SimOptical;
use aria_core::engine::OpticalBackend;

let optical = SimOptical::new(256); // N modes
let psi_prime = optical.unitary_step(t, &psi);
assert!((optical.energy(&psi_prime) - optical.energy(&psi)).abs() < 1e-10);
```

### `SimPredictor` — JEPA predictor

Implements **𝔸2** (isometry I: H → Z) and **ℙ2** (contractive predictor P). Uses random linear isometries and contractive matrices per conditioning.

```rust
use aria_backends::SimPredictor;
use aria_core::engine::Predictor;
use aria_core::condition::Condition;

let predictor = SimPredictor::new(256, 64); // N modes, latent dim
let z = predictor.embed(&psi);
let z_next = predictor.predict(&z, Condition::Token);
let d = predictor.dist(&z, &z_next);
```

### `SimGraphBackend` — graph editor

Implements **ℙ3**: graph edit distance via finite elementary operations (add/delete/relabel node, add/delete edge, rebuild to G★). Supports `Identity`, `OneEdit`, and `RebuildGStar` match policies.

```rust
use aria_backends::SimGraphBackend;
use aria_core::engine::GraphBackend;
use aria_core::policy::MatchPolicy;

let graph_backend = SimGraphBackend::new(64);
let g_prime = graph_backend.edit(&g, &z, MatchPolicy::OneEdit, None);
assert!(graph_backend.ok(&g_prime)); // Inv3: GraphOK
```

### `SimDiffuser` — latent diffusion

Implements one **atomic sample** of a continuous diffusion process conditioned on G (per `CONTINUOUS_REFINEMENT.md` §2.4). Supports `Identity`, `Flip`, and `GraphConditioned` policies.

```rust
use aria_backends::SimDiffuser;
use aria_core::engine::Diffuser;
use aria_core::policy::DiffPolicy;

let diffuser = SimDiffuser::new(64);
let z_prime = diffuser.diffuse(&g, &z, DiffPolicy::GraphConditioned);
```

## Quick start

```rust
use aria_backends::{SimOptical, SimPredictor, SimGraphBackend, SimDiffuser};
use aria_core::config::AriaConfig;
use aria_core::engine::Engine;
use aria_core::graph::Graph;
use aria_core::condition::Condition;
use num_complex::Complex64;

let config = AriaConfig::default();
let engine = Engine::new(
    config.clone(),
    SimOptical::new(config.n_modes),
    SimPredictor::new(config.n_modes, config.latent_dim),
    SimGraphBackend::new(config.latent_dim),
    SimDiffuser::new(config.latent_dim),
);

let psi0: Vec<Complex64> = vec![Complex64::new(1.0, 0.0); config.n_modes];
let state = engine.init(psi0, Graph::empty(), Condition::Token);

// Run 100 OPMD steps
use aria_core::scheduler::Scheduler;
let mut sched = Scheduler::from_string("opmd", 2).unwrap();
let (final_state, trace) = engine.run(state, &mut sched, 100, Condition::Token)?;

// Export trace
println!("{}", trace.to_jsonl());
```

## Safety

All backends are designed to preserve the four primary invariants checked by `aria-engine-core`:

| Invariant | Property | Backend responsible |
|---|---|---|
| Inv1 | Energy conserved | `SimOptical` (exact unitarity) |
| Inv2 | Residual ≤ prevRes + ε | `SimPredictor` (contractive), `SimDiffuser` |
| Inv3 | GraphOK | `SimGraphBackend` (well-typed edits) |
| Inv4 | TypeOK | all (type-safe outputs) |

## License

MIT OR Apache-2.0 — see the [repository](https://github.com/aria-ai/aria) for details.
