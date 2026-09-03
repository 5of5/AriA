# BIN.TAG.ELIGIBLE_SOURCE_CLASS

**Operator:** `TAG.ELIGIBLE_SOURCE_CLASS` · **layer:** RESIDUAL · **class:** TAG · **parent:** feed.by-name
**Crate:** `aria-res-tag-eligible-source-class` · **verify:** True · **result:** `residual.tag.eligible_source_class`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.ELIGIBLE_SOURCE_CLASS` or `work --json` with `ops: ["BIN.TAG.ELIGIBLE_SOURCE_CLASS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: ELIGIBLE_SOURCE_CLASS
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
