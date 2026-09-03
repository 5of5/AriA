# BIN.COMPETITOR

**Operator:** `COMPETITOR` · **layer:** TAG · **class:** TAG · **parent:** COMPANY
**Crate:** `aria-telemetry-competitor` · **verify:** True · **result:** `tag.competitor`

## Why

Role-tag operator. Does not name a new entity. Tags an existing COMPANY (or PRODUCT) with COMPETITOR_TAG under the active map. Category weight uses this tag versus other tags — it does not rewrite COMPANY.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.COMPETITOR` or `work --json` with `ops: ["BIN.COMPETITOR"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company
- relationships: COMPETES_WITH, TAGGED_AS
- anchor tags: COMPETITOR_TAG, COMPANY, MAP_TYPE
- property key: —

## Maps that consume this

- 01 Competitive Radar → BIN.REF.COMPETITIVE_RADAR
- 11 Competitor Battlecard → BIN.REF.COMPETITOR_BATTLECARD

## Sheet notes

COMPETITOR = MAP -> COMPETITOR_TAG

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
