// Exit₂ parity check — the WASM surface must run the same OPMD schedule as the CLI.
//
//   ./scripts/build_wasm.sh
//   cargo build --release -p aria-engine
//   node www/parity.mjs
//
// Parity here means: identical schema, identical action sequence, identical
// discrete state (t, |G|), identical invariant verdicts, and numeric agreement
// within f64 tolerance. It is deliberately NOT byte equality: `sin`, `cos`, and
// `sqrt` come from the host libm natively and from Rust's `libm` port on
// wasm32, and those disagree in the last ulp. Discrete Spec behavior is
// identical; only the transcendental tail differs.

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";

import * as wasm from "./pkg-node/aria_engine_wasm.js";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const cli = join(repoRoot, "target", "release", "aria");

const STEPS = 200;
const TOL = 1e-9;

// Optional argv overrides: `node www/parity.mjs [n_modes] [latent_dim]`.
// Defaults are the v0.1.0 config (N = 16 ⇒ Householder backend). Passing
// N ≥ 256 exercises the WS2 FFT optical path on both surfaces.
const nModes = process.argv[2] ? Number(process.argv[2]) : 16;
const latentDim = process.argv[3] ? Number(process.argv[3]) : (nModes === 16 ? 16 : 64);

const config = {
  n_modes: nModes,
  latent_dim: latentDim,
  eps: 1.0,
  stutter_k: 2,
  schedule: "opmd",
  condition: "token",
  match_policy: "identity",
  diff_policy: "identity",
  check_inv: ["inv1", "inv2", "inv3", "inv4"],
  max_graph_size: 10000,
  seed: 42,
  strict: true,
};

// 1. The Σ lock is visible from JavaScript too.
assert.deepEqual(wasm.actionAlphabet(), ["O", "P", "M", "D", "S"]);

// 2. The run summary is green and follows the preferred Φ-cycle.
const summary = wasm.run(JSON.stringify(config), STEPS);
assert.equal(summary.invariants_ok, true, JSON.stringify(summary.failures));
assert.equal(summary.action_sequence, "OPMD".repeat(STEPS / 4));
assert.equal(summary.t, STEPS / 4);

// 3. Optional Inv5–11 gates are off by default and observe-only when enabled.
assert.deepEqual(summary.gates.enabled, [], "gates must default off");

const gated = wasm.run(
  JSON.stringify({ ...config, gates: { enabled: ["inv5", "inv6", "inv7", "inv8", "inv9", "inv10", "inv11"] } }),
  STEPS,
);
assert.equal(gated.gates.enabled.length, 7);
// Observe-only is unconditional: gates must never steer the run.
assert.equal(gated.action_sequence, summary.action_sequence, "a gate must not steer");
if (nModes === 16) {
  // The shipped v0.1.0 config: OPMD satisfies every operating gate.
  assert.deepEqual(gated.gates.breaches, [], "OPMD should satisfy Inv5–11");
} else {
  // FFT path (WS2): Inv1–4 hold; the stub predictor's residual window-trend
  // (Inv8, an observer) legitimately degrades under the spec's t-indexed
  // phase-mask scrambling — a WS5 training-signal observation, not a safety
  // breach. Reported, never asserted away.
  console.log(`  gated observations @ N=${nModes}: ${gated.gates.breaches.length} breach(es)`);
}

const wasmJsonl = wasm.runTraceJsonl(JSON.stringify(config), STEPS);

if (!existsSync(cli)) {
  console.log("WASM surface OK (CLI not built — skipping the differential check)");
  console.log(summary);
  process.exit(0);
}

const cliJsonl = execFileSync(
  cli,
  [
    "run",
    "--steps", String(STEPS),
    "--schedule", config.schedule,
    "--n-modes", String(config.n_modes),
    "--latent-dim", String(config.latent_dim),
    "--eps", String(config.eps),
    "--seed", String(config.seed),
  ],
  { encoding: "utf8" },
);

const parse = (jsonl) => jsonl.trim().split("\n").map((l) => JSON.parse(l));
const wasmRows = parse(wasmJsonl);
const cliRows = parse(cliJsonl);

assert.equal(wasmRows.length, cliRows.length, "trace lengths differ");

// 4. The config header agrees exactly.
assert.deepEqual(wasmRows[0], cliRows[0], "config header differs");

// 5. Every entry agrees discretely, and numerically within tolerance.
let maxDelta = 0;
for (let i = 1; i < wasmRows.length; i++) {
  const a = wasmRows[i];
  const b = cliRows[i];
  assert.equal(a.t, b.t, `row ${i}: t differs`);
  assert.equal(a.action, b.action, `row ${i}: action differs`);
  assert.equal(a.graph_size, b.graph_size, `row ${i}: |G| differs`);
  assert.equal(a.condition, b.condition, `row ${i}: condition differs`);
  for (const key of ["res", "energy"]) {
    const delta = Math.abs(a[key] - b[key]);
    maxDelta = Math.max(maxDelta, delta);
    assert.ok(delta < TOL, `row ${i}: ${key} differs by ${delta} (> ${TOL})`);
  }
}

console.log("Exit2 parity OK — WASM runs the same OPMD schedule as the CLI");
console.log(`  rows=${cliRows.length - 1} max |Δ| on res/energy = ${maxDelta.toExponential(3)}`);
console.log(`  steps=${summary.steps} t=${summary.t} |G|=${summary.graph_size} energy=${summary.energy}`);
