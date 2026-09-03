# BIN.REL.INVESTS_IN

**Operator:** `REL.INVESTS_IN` · **layer:** RESIDUAL · **class:** REL · **parent:** VENTURE_CAPITAL
**Crate:** `aria-res-rel-invests-in` · **verify:** True · **result:** `residual.rel.invests_in`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.INVESTS_IN` or `work --json` with `ops: ["BIN.REL.INVESTS_IN"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: INVESTS_IN
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
