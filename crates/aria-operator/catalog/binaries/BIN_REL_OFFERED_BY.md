# BIN.REL.OFFERED_BY

**Operator:** `REL.OFFERED_BY` · **layer:** RESIDUAL · **class:** REL · **parent:** PRODUCT
**Crate:** `aria-res-rel-offered-by` · **verify:** True · **result:** `residual.rel.offered_by`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.OFFERED_BY` or `work --json` with `ops: ["BIN.REL.OFFERED_BY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: OFFERED_BY
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
