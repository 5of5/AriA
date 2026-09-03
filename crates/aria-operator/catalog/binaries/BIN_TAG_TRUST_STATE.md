# BIN.TAG.TRUST_STATE

**Operator:** `TAG.TRUST_STATE` · **layer:** RESIDUAL · **class:** TAG · **parent:** CLAIM
**Crate:** `aria-res-tag-trust-state` · **verify:** True · **result:** `residual.tag.trust_state`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.TRUST_STATE` or `work --json` with `ops: ["BIN.TAG.TRUST_STATE"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: TRUST_STATE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
