# BIN.TAG.PERSON_BUYER_CHAMPION

**Operator:** `TAG.PERSON_BUYER_CHAMPION` · **layer:** DEEP_TAG · **class:** TAG · **parent:** BUYER
**Crate:** `aria-res-tag-person-buyer-champion` · **verify:** True · **result:** `residual.tag.person_buyer_champion`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.PERSON_BUYER_CHAMPION` or `work --json` with `ops: ["BIN.TAG.PERSON_BUYER_CHAMPION"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: PERSON_BUYER_CHAMPION
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
