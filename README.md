# AriA

A transformer architecture that unifies next-token prediction, continuous diffusion, and latent world modeling inside a single state machine.
It replaces conventional electronic attention with optical interference, performs pure joint-embedding predictive modeling in latent space (no reconstruction in the core loop), and treats experience as a first-class typed graph that supports one-shot structural correction.
The design is grounded in an explicit discrete-state specification with inductive safety invariants for optical energy conservation, predictive contractivity, and graph integrity.

## Crates

| Crate | Binary | What |
|---|---|---|
| [`aria-json-telemetry`](crates/aria-operator/README.md) | `work` | Stateless JSON telemetry node: 560 operator identities, harness lane, hosted shell |
| `aria-engine` | `aria` | Engine CLI: run / verify / bench / emit |
| `aria-engine-core` · `aria-engine-backends` | — | The state machine and its backends |
| `aria-engine-wasm` · `python/aria-py` | — | WASM and Python bindings |

```bash
cargo install aria-json-telemetry --bin work
docker build --target work -t aria-work .      # 2 MB static image
```

## License

MIT OR Apache-2.0.

---

<p align="center">
  <sub>Build directive: Spec fidelity over trend-chasing. Four actions + Stutter. No decoder in the core loop. No fifth named action.</sub>
</p>
