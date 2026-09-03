# BIN.TAG.CONTENT_HASH

**Operator:** `TAG.CONTENT_HASH` · **layer:** RESIDUAL · **class:** TAG · **parent:** hash-stamp
**Crate:** `aria-res-tag-content-hash` · **verify:** True · **result:** `residual.tag.content_hash`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.CONTENT_HASH` or `work --json` with `ops: ["BIN.TAG.CONTENT_HASH"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: CONTENT_HASH
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
