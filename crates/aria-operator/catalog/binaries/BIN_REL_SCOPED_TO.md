# BIN.REL.SCOPED_TO

**Operator:** `REL.SCOPED_TO` · **layer:** RESIDUAL · **class:** REL · **parent:** PORTFOLIO
**Crate:** `aria-res-rel-scoped-to` · **verify:** True · **result:** `residual.rel.scoped_to`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.SCOPED_TO` or `work --json` with `ops: ["BIN.REL.SCOPED_TO"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: SCOPED_TO
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
