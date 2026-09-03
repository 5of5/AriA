# BIN.COMPANY

**Operator:** `COMPANY` · **layer:** ENTITY · **class:** ENTITY · **parent:** PORTFOLIO
**Crate:** `aria-telemetry-company` · **verify:** True · **result:** `entity.company`

## Why

Company-kind anchor. Highest-volume entity binary. Independent of competitor-tag scoring; COMPETITOR is a separate operator that may tag the same node.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.COMPANY` or `work --json` with `ops: ["BIN.COMPANY"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company
- relationships: COMPETES_WITH, OPERATES_IN, SELLS_TO, PARTNERS_WITH, RAISES_FROM
- anchor tags: COMPANY, CATEGORY, GEOGRAPHY
- property key: —

## Maps that consume this

- 01 Competitive Radar → BIN.REF.COMPETITIVE_RADAR
- 02 Category Galaxy → BIN.REF.CATEGORY_GALAXY
- 03 Market Heat Island → BIN.REF.MARKET_HEAT_ISLAND
- 04 Buyer Persona Relationship → BIN.REF.BUYER_PERSONA_RELATIONSHIP
- 05 Investor Syndicate → BIN.REF.INVESTOR_SYNDICATE
- 06 Customer Logos → BIN.REF.CUSTOMER_LOGOS
- 07 Developer Ecosystem → BIN.REF.DEVELOPER_ECOSYSTEM
- 08 Partnership Path → BIN.REF.PARTNERSHIP_PATH
- 09 Wedge-to-Platform → BIN.REF.WEDGE_TO_PLATFORM
- 10 Market Maturity → BIN.REF.MARKET_MATURITY
- 11 Competitor Battlecard → BIN.REF.COMPETITOR_BATTLECARD
- 12 Funding Momentum → BIN.REF.FUNDING_MOMENTUM
- 13 Influence Network → BIN.REF.INFLUENCE_NETWORK
- 14 Category Stack → BIN.REF.CATEGORY_STACK
- 15 Market Map Canvas → BIN.REF.MARKET_MAP_CANVAS
- 17 Market White Space → BIN.REF.MARKET_WHITE_SPACE
- 18 Account Targeting → BIN.REF.ACCOUNT_TARGETING
- 19 Narrative Positioning → BIN.REF.NARRATIVE_POSITIONING
- 20 Founder/Operator Lineage → BIN.REF.FOUNDER_OPERATOR_LINEAGE
- 21 Product Capability → BIN.REF.PRODUCT_CAPABILITY
- 22 Market Entry → BIN.REF.MARKET_ENTRY
- 23 Ecosystem Gravity → BIN.REF.ECOSYSTEM_GRAVITY
- 24 Category Creation Timeline → BIN.REF.CATEGORY_CREATION_TIMELINE
- 25 Market Intelligence Brief → BIN.REF.MARKET_INTELLIGENCE_BRIEF

## Sheet notes

COMPANY = 100

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
