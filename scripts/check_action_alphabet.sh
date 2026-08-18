#!/usr/bin/env bash
# G2 — action alphabet lock.
#
# Sigma = {OpticalStep, Predict, Match, Diffuse, Stutter}, by equality.
# This gate fails if the `Action` enum ever gains or loses a variant.
set -euo pipefail

cd "$(dirname "$0")/.."

ACTION_RS="crates/aria-core/src/action.rs"

variants=$(
  awk '/^pub enum Action \{/,/^\}/' "$ACTION_RS" \
    | grep -E '^\s+[A-Z][A-Za-z]*,$' \
    | tr -d ' ,'
)

expected=$'OpticalStep\nPredict\nMatch\nDiffuse\nStutter'

if [[ "$variants" != "$expected" ]]; then
  echo "G2 FAILED: Action enum is not exactly Sigma." >&2
  echo "expected:" >&2
  echo "$expected" >&2
  echo "found:" >&2
  echo "$variants" >&2
  exit 1
fi

echo "G2 OK: Action = {OpticalStep, Predict, Match, Diffuse, Stutter}"
