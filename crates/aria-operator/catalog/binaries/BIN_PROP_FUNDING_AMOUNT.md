# BIN.PROP.FUNDING_AMOUNT

**Operator:** `PROP.FUNDING_AMOUNT` · **layer:** RESIDUAL · **class:** PROP · **parent:** SIGNAL
**Crate:** `aria-res-prop-funding-amount` · **verify:** True · **result:** `residual.prop.funding_amount`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.FUNDING_AMOUNT` or `work --json` with `ops: ["BIN.PROP.FUNDING_AMOUNT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: funding_amount

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
