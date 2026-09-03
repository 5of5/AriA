# BIN.REL.ATTENDED_BY

**Operator:** `REL.ATTENDED_BY` · **layer:** RESIDUAL · **class:** REL · **parent:** PEOPLE
**Crate:** `aria-res-rel-attended-by` · **verify:** True · **result:** `residual.rel.attended_by`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.ATTENDED_BY` or `work --json` with `ops: ["BIN.REL.ATTENDED_BY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: ATTENDED_BY
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
