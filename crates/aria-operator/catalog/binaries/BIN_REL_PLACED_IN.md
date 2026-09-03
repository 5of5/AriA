# BIN.REL.PLACED_IN

**Operator:** `REL.PLACED_IN` · **layer:** RESIDUAL · **class:** REL · **parent:** MARKET_MAP
**Crate:** `aria-res-rel-placed-in` · **verify:** True · **result:** `residual.rel.placed_in`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.PLACED_IN` or `work --json` with `ops: ["BIN.REL.PLACED_IN"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: PLACED_IN
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
