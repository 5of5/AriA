# BIN.REL.SYNDICATES_WITH

**Operator:** `REL.SYNDICATES_WITH` · **layer:** RESIDUAL · **class:** REL · **parent:** SYNDICATE
**Crate:** `aria-res-rel-syndicates-with` · **verify:** True · **result:** `residual.rel.syndicates_with`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.SYNDICATES_WITH` or `work --json` with `ops: ["BIN.REL.SYNDICATES_WITH"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: SYNDICATES_WITH
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
