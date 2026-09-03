# BIN.SELLER

**Operator:** `SELLER` · **layer:** TAG · **class:** TAG · **parent:** COMPANY
**Crate:** `aria-telemetry-seller` · **verify:** True · **result:** `tag.seller`

## Why

Role-tag operator on COMPANY / PRODUCT. Independent of BUYER. Required so syndicate = buyers OR sellers remains a composition, not a merged identity.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.SELLER` or `work --json` with `ops: ["BIN.SELLER"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company, Product
- relationships: SELLS_TO, OFFERED_BY, TAGGED_AS
- anchor tags: SELLER_TAG, COMPANY, PRODUCT
- property key: —

## Maps that consume this

- 06 Customer Logos → BIN.REF.CUSTOMER_LOGOS

## Sheet notes

SELLERS = MAP -> SELLER_TAG

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
