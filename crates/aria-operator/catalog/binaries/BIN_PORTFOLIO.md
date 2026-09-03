# BIN.PORTFOLIO

**Operator:** `PORTFOLIO` · **layer:** ENVELOPE · **class:** ENVELOPE · **parent:** —
**Crate:** `aria-telemetry-portfolio` · **verify:** True · **result:** `portfolio.envelope`

## Why

Envelope of all market-map instances under one Workspace Current Context Version. Independent completeness of the envelope is the MAX graphical frame AriA is allowed to transport — not a score borrowed from child maps.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.PORTFOLIO` or `work --json` with `ops: ["BIN.PORTFOLIO"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Portfolio, MarketMap, Workspace
- relationships: CONTAINS_MAP, SCOPED_TO, VERSION_OF
- anchor tags: PORTFOLIO, MARKET_MAP, CURRENT_CONTEXT_VERSION
- property key: —

## Maps that consume this

- 15 Market Map Canvas → BIN.REF.MARKET_MAP_CANVAS
- 25 Market Intelligence Brief → BIN.REF.MARKET_INTELLIGENCE_BRIEF

## Sheet notes

PORTFOLIO = MARKET_MAP (prior sheet). Encapsulates everything under portfolio management.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
