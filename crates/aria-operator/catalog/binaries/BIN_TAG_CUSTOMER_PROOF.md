# BIN.TAG.CUSTOMER_PROOF

**Operator:** `TAG.CUSTOMER_PROOF` · **layer:** RESIDUAL · **class:** TAG · **parent:** SIGNAL
**Crate:** `aria-res-tag-customer-proof` · **verify:** True · **result:** `residual.tag.customer_proof`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.CUSTOMER_PROOF` or `work --json` with `ops: ["BIN.TAG.CUSTOMER_PROOF"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: CUSTOMER_PROOF
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
