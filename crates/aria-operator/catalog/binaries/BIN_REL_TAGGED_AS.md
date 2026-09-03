# BIN.REL.TAGGED_AS

**Operator:** `REL.TAGGED_AS` · **layer:** RESIDUAL · **class:** REL · **parent:** COMPETITOR
**Crate:** `aria-res-rel-tagged-as` · **verify:** True · **result:** `residual.rel.tagged_as`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.TAGGED_AS` or `work --json` with `ops: ["BIN.REL.TAGGED_AS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
