# BIN.TAG.INTENT

**Operator:** `TAG.INTENT` · **layer:** RESIDUAL · **class:** TAG · **parent:** SIGNAL
**Crate:** `aria-res-tag-intent` · **verify:** True · **result:** `residual.tag.intent`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.INTENT` or `work --json` with `ops: ["BIN.TAG.INTENT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: INTENT
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
