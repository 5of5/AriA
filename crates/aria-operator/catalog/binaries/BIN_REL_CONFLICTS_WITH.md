# BIN.REL.CONFLICTS_WITH

**Operator:** `REL.CONFLICTS_WITH` · **layer:** RESIDUAL · **class:** REL · **parent:** CLAIM
**Crate:** `aria-res-rel-conflicts-with` · **verify:** True · **result:** `residual.rel.conflicts_with`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.CONFLICTS_WITH` or `work --json` with `ops: ["BIN.REL.CONFLICTS_WITH"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: CONFLICTS_WITH
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
