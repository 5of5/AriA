# BIN.EVENT

**Operator:** `EVENT` · **layer:** ENTITY · **class:** ENTITY · **parent:** PORTFOLIO
**Crate:** `aria-telemetry-event` · **verify:** True · **result:** `entity.event`

## Why

Event-kind + dated market_signal carrier. Timeline and heat maps require this independent time-indexed anchor so snapshot maps cannot invent dates.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.EVENT` or `work --json` with `ops: ["BIN.EVENT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Event, MarketSignal
- relationships: OCCURRED_AT, SIGNALS, ATTENDED_BY, ANNOUNCED
- anchor tags: EVENT, SIGNAL, TIMESTAMP
- property key: —

## Maps that consume this

- 03 Market Heat Island → BIN.REF.MARKET_HEAT_ISLAND
- 10 Market Maturity → BIN.REF.MARKET_MATURITY
- 12 Funding Momentum → BIN.REF.FUNDING_MOMENTUM
- 24 Category Creation Timeline → BIN.REF.CATEGORY_CREATION_TIMELINE

## Sheet notes

EVENT = 25

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
