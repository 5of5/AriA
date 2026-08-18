# aria-engine-core — Ariadne Transformer engine

<p align="center">
  <img src="https://raw.githubusercontent.com/aria-ai/aria/main/assets/aria-logo-transparent.png" alt="Aria" width="280" />
</p>

**Spec-faithful state machine for the Aria optical–JEPA–graph dynamical system.**

`aria-engine-core` is the reference runtime engine implementing the discrete formal specification of Aria — a transformer architecture based on optical mode amplitudes, pure JEPA latent prediction, and a first-class experience graph with jointly inductive safety invariants.

## Architecture

```
Φ = Diff ∘ Match ∘ P ∘ U
Spec ≜ Init ∧ □[Next]_vars
```

### Named actions (exactly five)

| Action | Mutates | Spec |
|---|---|---|
| **OpticalStep** | ψ (optical field) | ψ′ = Uₜ(ψ); UNCHANGED ⟨z,G,t⟩ |
| **Predict** | z (JEPA latent) | z′ = P(I(ψ), aₜ); UNCHANGED ⟨ψ,G,t⟩ |
| **Match** | G (experience graph) | G′ = ED(G⊕z, G★); UNCHANGED ⟨ψ,z,t⟩ |
| **Diffuse** | z, t | z′ = Diff_G(z); t′=t+1; UNCHANGED ⟨ψ,G⟩ |
| **Stutter** | — | UNCHANGED all (TLA stuttering) |

### Safety invariants (checked after every `apply`)

| Invariant | Statement |
|---|---|
| **Inv1** | ‖ψ‖₂ = ‖ψ₀‖₂ (optical energy conserved) |
| **Inv2** | Res(ψ,z,t) ≤ prevRes + ε (predictive contractivity) |
| **Inv3** | GraphOK(G) (typed nodes/edges, embeddings in Z) |
| **Inv4** | TypeOK (all state variables well-typed) |

## Quick start

```rust
use aria_core::action::Action;
use aria_core::condition::Condition;
use aria_core::config::AriaConfig;
use aria_core::engine::{Engine, OpticalBackend, Predictor, GraphBackend, Diffuser};
use aria_core::graph::Graph;
use aria_core::scheduler::Scheduler;

// Plug in your backends (or use aria-engine-backends for simulated ones)
let config = AriaConfig::default();
let engine = Engine::new(config, optical, predictor, graph_backend, diffuser);

// Initialize state
let state = engine.init(psi0, Graph::empty(), Condition::Token);

// Step one full Φ-cycle (OPMD)
let state = engine.step_phi(state, Condition::Token)?;

// Check invariants
let report = engine.check(&state, Condition::Token);
assert!(report.all_ok());
```

## Crate structure

| Module | Purpose |
|---|---|
| `action` | `Action` enum — exactly 5 variants (G2 gate) |
| `condition` | `Condition` — Token / Diffusion / WorldModel (𝐂2) |
| `config` | `AriaConfig` — TOML-serializable engine config (FR-10) |
| `engine` | `Engine` + backend traits — Init, apply, step_phi, run |
| `error` | `AriaError`, `InvViolation` — structured error types |
| `graph` | `Graph` — typed directed graph with latent embeddings (𝔸3, Inv3) |
| `invariants` | Inv1–4 checkers, `InvariantReport` |
| `policy` | `MatchPolicy`, `DiffPolicy` enums |
| `scheduler` | `Scheduler` — policy layer (OPMD default, 𝐂4/𝐂5) |
| `state` | `State` — full Spec variables ⟨ψ, z, G, t, prevRes⟩ |
| `trace` | `Trace`, `TraceEntry` — JSONL trace export |

## Formal foundations

- **Discrete Spec:** [`docs/FORMAL_SPEC.md`](https://github.com/aria-ai/aria/blob/main/docs/FORMAL_SPEC.md)
- **Safety:** [`docs/SAFETY.md`](https://github.com/aria-ai/aria/blob/main/docs/SAFETY.md)
- **TLA+ machine Spec:** [`spec/Aria.tla`](https://github.com/aria-ai/aria/blob/main/spec/Aria.tla)

## License

MIT OR Apache-2.0 — see the [repository](https://github.com/aria-ai/aria) for details.
