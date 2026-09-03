# BIN.MARKET

**Operator:** `MARKET` · **layer:** ENTITY · **class:** ENTITY · **parent:** PORTFOLIO
**Crate:** `aria-telemetry-market` · **verify:** True · **result:** `entity.market`

## Why

Market / category-kind anchor. Galaxy, heat, white space, canvas, maturity, brief all require a market node that is not a company.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.MARKET` or `work --json` with `ops: ["BIN.MARKET"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Market, Category
- relationships: IN_CATEGORY, ADJACENT_TO, CONTAINS_SEGMENT
- anchor tags: MARKET, CATEGORY, SEGMENT
- property key: —

## Maps that consume this

- 02 Category Galaxy → BIN.REF.CATEGORY_GALAXY
- 03 Market Heat Island → BIN.REF.MARKET_HEAT_ISLAND
- 10 Market Maturity → BIN.REF.MARKET_MATURITY
- 14 Category Stack → BIN.REF.CATEGORY_STACK
- 15 Market Map Canvas → BIN.REF.MARKET_MAP_CANVAS
- 17 Market White Space → BIN.REF.MARKET_WHITE_SPACE
- 22 Market Entry → BIN.REF.MARKET_ENTRY
- 25 Market Intelligence Brief → BIN.REF.MARKET_INTELLIGENCE_BRIEF

## Sheet notes

Implied by MARKET_MAP = ANY. Entity kind from spine (13 kinds).

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
