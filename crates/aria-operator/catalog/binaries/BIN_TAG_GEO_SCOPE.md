# BIN.TAG.GEO_SCOPE

**Operator:** `TAG.GEO_SCOPE` · **layer:** RESIDUAL · **class:** TAG · **parent:** MARKET
**Crate:** `aria-res-tag-geo-scope` · **verify:** True · **result:** `residual.tag.geo_scope`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.GEO_SCOPE` or `work --json` with `ops: ["BIN.TAG.GEO_SCOPE"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: GEO_SCOPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
