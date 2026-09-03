# BIN.PROP.RETRIEVED_AT

**Operator:** `PROP.RETRIEVED_AT` · **layer:** RESIDUAL · **class:** PROP · **parent:** SOURCE
**Crate:** `aria-res-prop-retrieved-at` · **verify:** True · **result:** `residual.prop.retrieved_at`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.RETRIEVED_AT` or `work --json` with `ops: ["BIN.PROP.RETRIEVED_AT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: retrieved_at

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
