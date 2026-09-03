# BIN.PROP.INVESTOR_QUALITY

**Operator:** `PROP.INVESTOR_QUALITY` · **layer:** RESIDUAL · **class:** PROP · **parent:** VC
**Crate:** `aria-res-prop-investor-quality` · **verify:** True · **result:** `residual.prop.investor_quality`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.INVESTOR_QUALITY` or `work --json` with `ops: ["BIN.PROP.INVESTOR_QUALITY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: investor_quality

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
