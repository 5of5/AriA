# BIN.REL.SUBSTITUTES

**Operator:** `REL.SUBSTITUTES` · **layer:** RESIDUAL · **class:** REL · **parent:** PRODUCT
**Crate:** `aria-res-rel-substitutes` · **verify:** True · **result:** `residual.rel.substitutes`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.SUBSTITUTES` or `work --json` with `ops: ["BIN.REL.SUBSTITUTES"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: SUBSTITUTES
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
