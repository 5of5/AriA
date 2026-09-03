# BIN.REL.ASSERTS

**Operator:** `REL.ASSERTS` · **layer:** RESIDUAL · **class:** REL · **parent:** CLAIM
**Crate:** `aria-res-rel-asserts` · **verify:** True · **result:** `residual.rel.asserts`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.ASSERTS` or `work --json` with `ops: ["BIN.REL.ASSERTS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: ASSERTS
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
