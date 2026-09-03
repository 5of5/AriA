# BIN.TAG.CAT_CANVAS

**Operator:** `TAG.CAT_CANVAS` · **layer:** DEEP_TAG · **class:** TAG · **parent:** MARKET_MAP
**Crate:** `aria-res-tag-cat-canvas` · **verify:** True · **result:** `residual.tag.cat_canvas`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.CAT_CANVAS` or `work --json` with `ops: ["BIN.TAG.CAT_CANVAS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: CAT_CANVAS
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
