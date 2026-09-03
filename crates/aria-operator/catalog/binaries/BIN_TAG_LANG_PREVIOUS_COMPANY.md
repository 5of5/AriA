# BIN.TAG.LANG_PREVIOUS_COMPANY

**Operator:** `TAG.LANG_PREVIOUS_COMPANY` · **layer:** DEEP_TAG · **class:** TAG · **parent:** COMPANY
**Crate:** `aria-res-tag-lang-previous-company` · **verify:** True · **result:** `residual.tag.lang_previous_company`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.LANG_PREVIOUS_COMPANY` or `work --json` with `ops: ["BIN.TAG.LANG_PREVIOUS_COMPANY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: LANG_PREVIOUS_COMPANY
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
