# BIN.REL.RAISES_FROM

**Operator:** `REL.RAISES_FROM` · **layer:** RESIDUAL · **class:** REL · **parent:** COMPANY
**Crate:** `aria-res-rel-raises-from` · **verify:** True · **result:** `residual.rel.raises_from`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.RAISES_FROM` or `work --json` with `ops: ["BIN.REL.RAISES_FROM"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: RAISES_FROM
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
