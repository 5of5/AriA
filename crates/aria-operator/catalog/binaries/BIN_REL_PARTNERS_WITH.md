# BIN.REL.PARTNERS_WITH

**Operator:** `REL.PARTNERS_WITH` · **layer:** RESIDUAL · **class:** REL · **parent:** PARTNER
**Crate:** `aria-res-rel-partners-with` · **verify:** True · **result:** `residual.rel.partners_with`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.PARTNERS_WITH` or `work --json` with `ops: ["BIN.REL.PARTNERS_WITH"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: PARTNERS_WITH
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
