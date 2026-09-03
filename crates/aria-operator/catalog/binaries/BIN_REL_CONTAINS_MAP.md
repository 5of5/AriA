# BIN.REL.CONTAINS_MAP

**Operator:** `REL.CONTAINS_MAP` · **layer:** RESIDUAL · **class:** REL · **parent:** PORTFOLIO
**Crate:** `aria-res-rel-contains-map` · **verify:** True · **result:** `residual.rel.contains_map`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.CONTAINS_MAP` or `work --json` with `ops: ["BIN.REL.CONTAINS_MAP"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: CONTAINS_MAP
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
