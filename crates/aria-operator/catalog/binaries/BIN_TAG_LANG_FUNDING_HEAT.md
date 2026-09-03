# BIN.TAG.LANG_FUNDING_HEAT

**Operator:** `TAG.LANG_FUNDING_HEAT` · **layer:** DEEP_TAG · **class:** TAG · **parent:** SIGNAL
**Crate:** `aria-res-tag-lang-funding-heat` · **verify:** True · **result:** `residual.tag.lang_funding_heat`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.LANG_FUNDING_HEAT` or `work --json` with `ops: ["BIN.TAG.LANG_FUNDING_HEAT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: LANG_FUNDING_HEAT
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
