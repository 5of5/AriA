# BIN.REF.CUSTOMER_LOGOS

**Operator:** `REF.CUSTOMER_LOGOS` · **layer:** REFINEMENT · **class:** REFINEMENT · **parent:** MARKET_MAP
**Crate:** `aria-ref-customer-logos` · **verify:** True · **result:** `map.ref.customer_logos`

## Why

Sealed map type Customer Logos. One dump × 25 mixers is the viral coefficient: the same tagged telemetry fans out into 25 structured map JSON results without a second Trust write.

## Function

Map mixer. Ingests the same JSON any operator ingests (raw graph, or already-processed `aria-work-v1` callback). Returns ONLY the neighborhood this sealed map type is allowed to consume (declared kinds/rels/tags). Missing data is omitted. Source bytes are not rewritten. Downstream views consume this envelope; the node does not score it.

## Use

`work --binary BIN.REF.CUSTOMER_LOGOS` or `work --json` with `ops: ["BIN.REF.CUSTOMER_LOGOS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Customer, Account, Company, Product, Claim
- relationships: CUSTOMER_OF, SELLS_TO
- anchor tags: CUSTOMER, SELLER_TAG, CUSTOMER_PROOF, Customer Logos, MAP_TYPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
