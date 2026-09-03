# BIN.REL.COMPETES_WITH

**Operator:** `REL.COMPETES_WITH` · **layer:** RESIDUAL · **class:** REL · **parent:** COMPETITOR
**Crate:** `aria-res-rel-competes-with` · **verify:** True · **result:** `residual.rel.competes_with`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.COMPETES_WITH` or `work --json` with `ops: ["BIN.REL.COMPETES_WITH"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: COMPETES_WITH
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
