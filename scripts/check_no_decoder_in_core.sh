#!/usr/bin/env bash
# 𝔸5 — readout decoupling lock (plan WS0, CI gate).
#
# No readout/decoder surface may live inside the core Φ loop: readouts map D
# operate strictly outside Φ and gradients from readouts never touch Φ's
# safety guarantees (𝕃5). This gate fails if any aria-core source line
# matches the decoder pattern set, unless the line is covered by an entry in
# the escape list below.
#
# Deliberately NOT in the pattern set: `token` (matches the legitimate
# Condition::Token variant) and generic math words (`linear`, `norm`).
set -euo pipefail

cd "$(dirname "$0")/.."

CORE_SRC="crates/aria-core/src"
PATTERNS='readout|decoder|tokeniz|vocab|bpe|softmax|logits'

hits=$(grep -rn -i -E "$PATTERNS" "$CORE_SRC" || true)

# ── Escape list ──────────────────────────────────────────────────────────────
# Each entry below is a `path:line` regex matched against the grep output and
# MUST be preceded by an `# aria-decoder-gate: allow` justification comment.
# Additions require a CHANGELOG entry (plan WS0). An entry may only be as
# broad as its justification.
#
# aria-decoder-gate: allow
# config.rs carries the spec §0.1 discrete-readout vocabulary bound |V_o| —
# plan WS0 table row 8 mandates AriaConfig::validate() enforce
# 256 ≤ |V_o| ≤ 128000, and AriaConfig lives in aria-core. A configuration
# bound is not a decoder surface: no readout/decoder code exists in
# aria-core (𝔸5, 𝕃5 intact). Logged: CHANGELOG 2026-08-13.
ALLOWED='^crates/aria-core/src/config\.rs:[0-9]+:.*(vocab_size|vocabulary)'

remaining=$(printf '%s\n' "$hits" | grep -v -E "$ALLOWED" || true)

if [[ -n "$remaining" ]]; then
  echo "DECODER GATE FAILED: readout/decoder symbols found in aria-core." >&2
  echo "The core Φ loop must stay decoder-free (𝔸5, 𝕃5)." >&2
  echo "$remaining" >&2
  exit 1
fi

echo "Decoder gate OK: no readout/decoder symbols in aria-core"
