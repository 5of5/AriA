# aria-training

Native Rust training engine for the Aria transformer — milestone **M1** of the
program ladder (`docs/ARIA-TRAINING-PRD.tex` v7.3). Minimizes the 4-term hybrid
objective on the probability simplex Δ³ (ℙ6):

```
ℒ_total = λ_JEPA·ℒ_JEPA + λ_NLL·ℒ_NLL + λ_Spectral·ℒ_Spectral + λ_Graph·ℒ_Graph
```

with invariant preservation **by construction** (Inductive Safety Theorem):
after every AdamW step the parameters are hard-projected back into their
spectral balls — `σ_max(W_I) ≤ 1.0` (𝔸2) and `σ_max(W_P) ≤ ε/2` (ℙ2, Inv2) —
using the same seeded power iteration the runtime loader audits with
(`aria-backends::spectral`). Readout gradients never reach Φ parameters
(𝔸5, 𝕃5: `∂Φ/∂y = 0`, structural).

## Module map (PRD §crate architecture)

| Module | Role | Status |
|---|---|---|
| `lib.rs` | `TrainingConfig` / `TrainOutcome` / errors; Δ³ + 𝒮 validation delegated to `AriaConfig::validate` | WS-A1 |
| `dataset.rs` | serde `FieldDataset` loader; WS5-faithful batcher (trajectories of 8, time-split holdout 0.4); KG edge alignment | WS-A1 |
| `loss.rs` | 4-term ℒ_total with exact analytic f64 gradients; stop-gradient JEPA targets | WS-A1 |
| `optimizer.rs` | deterministic AdamW with Π_𝒮 = trivial-mode deflation ∘ spectral caps | WS-A1 |
| `train.rs` | seeded init, epoch loop, holdout + persistence, gates, decoupled readout pass | WS-A1 + WS-A2 |
| `collapse.rs` | RankMe via in-repo cyclic-Jacobi eigensolver; hard abort < 0.3·d | WS-A2 |
| `eval.rs` | paired Wilcoxon signed-rank + seeded bootstrap 99% CI | WS-A2 |
| `sha256.rs` | in-repo FIPS 180-4 SHA-256 (NIST-vector verified) | WS-A2 |
| `checkpoint.rs` | bit-exact v2 safetensors + canonical SHA-256 provenance; v1 debug path | WS-A2 |
| `linalg.rs` | deterministic f64 matrix/vector kernels shared by product modules | WS-A2 |

All Rust tests live under `tests/`; `src/` is production code only.

## Determinism

Every stochastic choice flows from `TrainingConfig::seed` through the repo's
LCG discipline (MMIX constants, no OS entropy). Gradients are analytic
(linear maps), transcendentals go through `libm`, and no hash-map iteration
order touches a result: two runs with equal config produce bit-identical
weights and outcomes.

## Fixtures (real data only)

- `fixtures/docs_excerpt_32k.txt` — the first 32,768 bytes of the live docs
  corpus (concatenation of `README.md`, `ideas.md`, `docs/**/*.md` (depth ≤ 2),
  `spec/*.md`, sorted by path; excerpted 2026-08-17, corpus sha256
  `46144f6395976f32245f54e5997ebd1e739c7f43348ec6cfcba1ff7c90a99ff9`, excerpt
  sha256 `03152a38bc81a745da8dd998492bcca5381f6a0cbb3e0615711bdd1fcb0db907`).
  Sole substrate for unit/property/integration tests — bit-for-bit stable, no
  test-time regeneration.
- `fixtures/conceptnet_edges_v1.json` — `aria-kg-edges-v1`: a real excerpt of
  the ConceptNet 5.7.0 assertion dump (see the provenance block embedded in
  the artifact: source URL, retrieval date, exact filter predicate, license
  CC BY-SA 4.0). Admitted per the corpus admission table amendment D3
  (PRD v7.3): ℒ_Graph edge substrate for smoke/regression runs and loss/Match
  unit tests only — never a quality-gate corpus, never map-quality claims.

## Pipeline receipt

`cargo run --release -p aria-training --example smoke` trains on the live docs
corpus with the canonical simplex (λ = 0.70 / 0 / 0.15 / 0.15) and archives
`docs/evidence/v0.3.0_wsa2_pipeline.json`. The production default is the
**gate-optimal 15-epoch protocol** measured on 2026-08-17: RankMe 13.78 vs the
9.6 gate, Wilcoxon p = 1.81e-28 with median improvement 8.59e-3, holdout
4.87e-3 vs persistence 1.39e-2 (2.87×), bit-deterministic v2 artifact with
SHA-256 provenance, and the decoupled readout pass. The older 600-epoch WS-A1
protocol predates RankMe instrumentation and is now rejected (RankMe 5.80):
longer training sheds effective rank without improving the relative predictor
quality enough to justify it.

The docs corpus remains smoke/regression only. The real-corpus quality gate and
non-collapsing `|V| > 1 @ T=10³` seam are WS-A3.
