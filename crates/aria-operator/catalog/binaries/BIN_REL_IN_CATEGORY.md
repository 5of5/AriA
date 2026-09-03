# BIN.REL.IN_CATEGORY

**Operator:** `REL.IN_CATEGORY` · **layer:** RESIDUAL · **class:** REL · **parent:** MARKET
**Crate:** `aria-res-rel-in-category` · **verify:** True · **result:** `residual.rel.in_category`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.IN_CATEGORY` or `work --json` with `ops: ["BIN.REL.IN_CATEGORY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: IN_CATEGORY
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
