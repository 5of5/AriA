# BIN.PROP.STOP_REASON

**Operator:** `PROP.STOP_REASON` · **layer:** RESIDUAL · **class:** PROP · **parent:** all operators
**Crate:** `aria-res-prop-stop-reason` · **verify:** True · **result:** `residual.prop.stop_reason`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.STOP_REASON` or `work --json` with `ops: ["BIN.PROP.STOP_REASON"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: stop_reason

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
