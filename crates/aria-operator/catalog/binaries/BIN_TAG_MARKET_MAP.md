# BIN.TAG.MARKET_MAP

**Operator:** `TAG.MARKET_MAP` · **layer:** RESIDUAL · **class:** TAG · **parent:** MARKET_MAP
**Crate:** `aria-res-tag-market-map` · **verify:** True · **result:** `residual.tag.market_map`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.MARKET_MAP` or `work --json` with `ops: ["BIN.TAG.MARKET_MAP"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: MARKET_MAP
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
