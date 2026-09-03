# BIN.PROP.PLAN_HASH

**Operator:** `PROP.PLAN_HASH` · **layer:** RESIDUAL · **class:** PROP · **parent:** all operators
**Crate:** `aria-res-prop-plan-hash` · **verify:** True · **result:** `residual.prop.plan_hash`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.PLAN_HASH` or `work --json` with `ops: ["BIN.PROP.PLAN_HASH"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: plan_hash

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
