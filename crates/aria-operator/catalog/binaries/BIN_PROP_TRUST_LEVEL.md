# BIN.PROP.TRUST_LEVEL

**Operator:** `PROP.TRUST_LEVEL` · **layer:** RESIDUAL · **class:** PROP · **parent:** CLAIM
**Crate:** `aria-res-prop-trust-level` · **verify:** True · **result:** `residual.prop.trust_level`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.TRUST_LEVEL` or `work --json` with `ops: ["BIN.PROP.TRUST_LEVEL"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: trust_level

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
