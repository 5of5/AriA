# BIN.TAG.IND_PROPTECH

**Operator:** `TAG.IND_PROPTECH` · **layer:** DEEP_TAG · **class:** TAG · **parent:** MARKET
**Crate:** `aria-res-tag-ind-proptech` · **verify:** True · **result:** `residual.tag.ind_proptech`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.IND_PROPTECH` or `work --json` with `ops: ["BIN.TAG.IND_PROPTECH"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: IND_PROPTECH
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
