# BIN.TAG.PERSON_ANALYST_INDUSTRY

**Operator:** `TAG.PERSON_ANALYST_INDUSTRY` · **layer:** DEEP_TAG · **class:** TAG · **parent:** PEOPLE
**Crate:** `aria-res-tag-person-analyst-industry` · **verify:** True · **result:** `residual.tag.person_analyst_industry`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.PERSON_ANALYST_INDUSTRY` or `work --json` with `ops: ["BIN.TAG.PERSON_ANALYST_INDUSTRY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: PERSON_ANALYST_INDUSTRY
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
