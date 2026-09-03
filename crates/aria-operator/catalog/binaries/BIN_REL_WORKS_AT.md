# BIN.REL.WORKS_AT

**Operator:** `REL.WORKS_AT` · **layer:** RESIDUAL · **class:** REL · **parent:** PEOPLE
**Crate:** `aria-res-rel-works-at` · **verify:** True · **result:** `residual.rel.works_at`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.WORKS_AT` or `work --json` with `ops: ["BIN.REL.WORKS_AT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: WORKS_AT
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
