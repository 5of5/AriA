# BIN.CLAIM

**Operator:** `CLAIM` · **layer:** FACT · **class:** FACT · **parent:** PORTFOLIO
**Crate:** `aria-telemetry-claim` · **verify:** True · **result:** `fact.claim`

## Why

Facts are claims pointing at sources, each with its own trust_level on the product side. This binary returns the claim payload only. It does not set Trust.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.CLAIM` or `work --json` with `ops: ["BIN.CLAIM"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Claim
- relationships: ASSERTS, CITES, CONFLICTS_WITH
- anchor tags: CLAIM, FIELD, TRUST_STATE
- property key: —

## Maps that consume this

- 01 Competitive Radar → BIN.REF.COMPETITIVE_RADAR
- 04 Buyer Persona Relationship → BIN.REF.BUYER_PERSONA_RELATIONSHIP
- 06 Customer Logos → BIN.REF.CUSTOMER_LOGOS
- 11 Competitor Battlecard → BIN.REF.COMPETITOR_BATTLECARD
- 19 Narrative Positioning → BIN.REF.NARRATIVE_POSITIONING
- 21 Product Capability → BIN.REF.PRODUCT_CAPABILITY
- 25 Market Intelligence Brief → BIN.REF.MARKET_INTELLIGENCE_BRIEF

## Sheet notes

Spine claim. Worked graph had 47 claims.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
