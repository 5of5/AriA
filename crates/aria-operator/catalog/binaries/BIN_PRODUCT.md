# BIN.PRODUCT

**Operator:** `PRODUCT` · **layer:** ENTITY · **class:** ENTITY · **parent:** COMPANY
**Crate:** `aria-telemetry-product` · **verify:** True · **result:** `entity.product`

## Why

Product-kind anchor for capability, stack, wedge-to-platform, and developer-ecosystem maps. Independent of company narrative.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.PRODUCT` or `work --json` with `ops: ["BIN.PRODUCT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Product
- relationships: OFFERED_BY, INTEGRATES_WITH, SUBSTITUTES, HAS_CAPABILITY
- anchor tags: PRODUCT, CAPABILITY, LAYER
- property key: —

## Maps that consume this

- 07 Developer Ecosystem → BIN.REF.DEVELOPER_ECOSYSTEM
- 09 Wedge-to-Platform → BIN.REF.WEDGE_TO_PLATFORM
- 11 Competitor Battlecard → BIN.REF.COMPETITOR_BATTLECARD
- 14 Category Stack → BIN.REF.CATEGORY_STACK
- 21 Product Capability → BIN.REF.PRODUCT_CAPABILITY
- 23 Ecosystem Gravity → BIN.REF.ECOSYSTEM_GRAVITY

## Sheet notes

Added: required by maps 07, 09, 11, 14, 21. Not on prior sheet.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
