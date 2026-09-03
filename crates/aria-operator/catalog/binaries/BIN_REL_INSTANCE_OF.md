# BIN.REL.INSTANCE_OF

**Operator:** `REL.INSTANCE_OF` · **layer:** RESIDUAL · **class:** REL · **parent:** MARKET_MAP
**Crate:** `aria-res-rel-instance-of` · **verify:** True · **result:** `residual.rel.instance_of`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.INSTANCE_OF` or `work --json` with `ops: ["BIN.REL.INSTANCE_OF"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: INSTANCE_OF
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
