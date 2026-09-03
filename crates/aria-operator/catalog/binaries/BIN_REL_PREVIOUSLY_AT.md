# BIN.REL.PREVIOUSLY_AT

**Operator:** `REL.PREVIOUSLY_AT` · **layer:** RESIDUAL · **class:** REL · **parent:** PEOPLE
**Crate:** `aria-res-rel-previously-at` · **verify:** True · **result:** `residual.rel.previously_at`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.PREVIOUSLY_AT` or `work --json` with `ops: ["BIN.REL.PREVIOUSLY_AT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: PREVIOUSLY_AT
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
