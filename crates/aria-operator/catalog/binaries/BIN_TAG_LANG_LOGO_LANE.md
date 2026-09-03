# BIN.TAG.LANG_LOGO_LANE

**Operator:** `TAG.LANG_LOGO_LANE` · **layer:** DEEP_TAG · **class:** TAG · **parent:** CUSTOMER
**Crate:** `aria-res-tag-lang-logo-lane` · **verify:** True · **result:** `residual.tag.lang_logo_lane`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.LANG_LOGO_LANE` or `work --json` with `ops: ["BIN.TAG.LANG_LOGO_LANE"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: LANG_LOGO_LANE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
