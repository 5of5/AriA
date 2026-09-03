# BIN.TAG.IND_IDENTITY

**Operator:** `TAG.IND_IDENTITY` · **layer:** DEEP_TAG · **class:** TAG · **parent:** MARKET
**Crate:** `aria-res-tag-ind-identity` · **verify:** True · **result:** `residual.tag.ind_identity`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.IND_IDENTITY` or `work --json` with `ops: ["BIN.TAG.IND_IDENTITY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: IND_IDENTITY
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
