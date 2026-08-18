# aria-engine-wasm

WebAssembly surface for the [Aria](https://github.com/aria-ai/aria) transformer runtime.

This crate is a thin façade over `aria_engine_backends::runner` — the same code
path the `aria` CLI and the Python extension use. It defines no transitions and
relaxes no invariant.

## Build

```bash
wasm-pack build crates/aria-wasm --target web --out-dir ../../www/pkg
```

## Use

```js
import init, { run, runTraceJsonl, defaultConfig, actionAlphabet } from "./pkg/aria_engine_wasm.js";

await init();

const config = JSON.stringify({ n_modes: 64, latent_dim: 32, eps: 1.0, schedule: "opmd", seed: 42 });
const summary = run(config, 1000);
// { steps: 1000, t: 250, graph_size: 250, energy: 1.0, invariants_ok: true, ... }

const jsonl = runTraceJsonl(config, 1000); // identical to `aria run --output trace.jsonl`
```

## API

| Export | Returns |
|---|---|
| `run(configJson, steps)` | `RunSummary` object |
| `runTraceJsonl(configJson, steps)` | JSONL trace string |
| `defaultConfig()` | the documented default config |
| `configFromToml(src)` | a config parsed from the CLI's TOML format |
| `actionAlphabet()` | `["O", "P", "M", "D", "S"]` |
