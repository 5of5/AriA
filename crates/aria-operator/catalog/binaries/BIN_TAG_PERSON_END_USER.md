# BIN.TAG.PERSON_END_USER

**Operator:** `TAG.PERSON_END_USER` · **layer:** DEEP_TAG · **class:** TAG · **parent:** CUSTOMER
**Crate:** `aria-res-tag-person-end-user` · **verify:** True · **result:** `residual.tag.person_end_user`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.PERSON_END_USER` or `work --json` with `ops: ["BIN.TAG.PERSON_END_USER"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: PERSON_END_USER
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
