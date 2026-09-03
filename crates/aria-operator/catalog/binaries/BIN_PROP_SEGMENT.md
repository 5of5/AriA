# BIN.PROP.SEGMENT

**Operator:** `PROP.SEGMENT` · **layer:** RESIDUAL · **class:** PROP · **parent:** MARKET_MAP
**Crate:** `aria-res-prop-segment` · **verify:** True · **result:** `residual.prop.segment`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.SEGMENT` or `work --json` with `ops: ["BIN.PROP.SEGMENT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: segment

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
