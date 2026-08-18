# aria-engine — Aria transformer command line

<p align="center">
  <img src="https://raw.githubusercontent.com/aria-ai/aria/main/assets/aria-logo-transparent.png" alt="Aria" width="240" />
</p>

**Run Spec-faithful Aria Φ-cycles from the terminal.**

`aria-engine` is the native binary for the Aria transformer runtime. It executes the discrete Spec transitions with invariant checking, trace export, and configurable schedules — all from the command line.

## Installation

```bash
cargo install aria-engine
```

## Usage

### Run an OPMD schedule

```bash
# 1000-step OPMD cycle with default config
aria run --steps 1000 --schedule opmd

# Custom dimensions
aria run --steps 500 --n-modes 256 --latent-dim 64 --eps 1.0

# Export trace to JSONL
aria run --steps 100 --output trace.jsonl

# Load trained predictor weights (JSON v1 or safetensors v2)
aria run --steps 1000 --predictor weights.safetensors

# Non-strict mode (log violations, don't abort)
aria run --steps 1000 --no-strict
```

### Long-horizon verify (v0.2.0)

Streaming 10⁵-step run with O(1) memory, Inv1–4 checks, action-shape audit, and a JSON receipt:

```bash
aria verify --steps 100000 --match-policy merge --receipt receipt.json
```

### Decode a finished latent sequence

```bash
aria emit --trace run.jsonl --readout readout.safetensors
```

### Step a single action

```bash
# Apply OpticalStep to a state
aria step --action OpticalStep --n-modes 8 --latent-dim 16

# With a saved state
aria step --action Predict --state state.json --n-modes 8 --latent-dim 16
```

### Check invariants

```bash
aria check --state state.json --latent-dim 16
```

### Configuration via TOML

```toml
# aria.toml
n_modes = 256
latent_dim = 64
eps = 1.0
stutter_k = 2
schedule = "opmd"
match_policy = "identity"
diff_policy = "identity"
condition = "token"
check_inv = ["inv1", "inv2", "inv3", "inv4"]
```

```bash
aria --config aria.toml run --steps 1000
```

## Trace format (JSONL)

Each line is a JSON object:

```json
{"type":"config","n_modes":8,"latent_dim":16,"eps":1.0}
{"t":0,"action":"O","res":0.63,"energy":1.0,"graph_size":0,"condition":"token"}
{"t":0,"action":"P","res":0.0,"energy":1.0,"graph_size":0,"condition":"token"}
{"t":0,"action":"M","res":0.0,"energy":1.0,"graph_size":1,"condition":"token"}
{"t":0,"action":"D","res":0.0,"energy":1.0,"graph_size":1,"condition":"token"}
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | All steps completed, invariants held |
| 1 | Invariant violation or schedule error |

## Related crates

- [`aria-engine-core`](https://crates.io/crates/aria-engine-core) — engine, state, invariants, traits
- [`aria-engine-backends`](https://crates.io/crates/aria-engine-backends) — simulated operator implementations

## License

MIT OR Apache-2.0 — see the [repository](https://github.com/aria-ai/aria) for details.
