# BIN.SIGNAL

**Operator:** `SIGNAL` · **layer:** FACT · **class:** FACT · **parent:** EVENT
**Crate:** `aria-telemetry-signal` · **verify:** True · **result:** `fact.market_signal`

## Why

Dated, typed, strength-scored market_signal rows. Refreshability of every map depends on this independent fact operator. Strength is not Trust.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.SIGNAL` or `work --json` with `ops: ["BIN.SIGNAL"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: MarketSignal
- relationships: SIGNALS, ABOUT, SOURCED_FROM
- anchor tags: SIGNAL, FUNDING, HIRING, LAUNCH, MEDIA, INTENT, CUSTOMER_PROOF
- property key: —

## Maps that consume this

- 01 Competitive Radar → BIN.REF.COMPETITIVE_RADAR
- 02 Category Galaxy → BIN.REF.CATEGORY_GALAXY
- 03 Market Heat Island → BIN.REF.MARKET_HEAT_ISLAND
- 09 Wedge-to-Platform → BIN.REF.WEDGE_TO_PLATFORM
- 10 Market Maturity → BIN.REF.MARKET_MATURITY
- 12 Funding Momentum → BIN.REF.FUNDING_MOMENTUM
- 17 Market White Space → BIN.REF.MARKET_WHITE_SPACE
- 24 Category Creation Timeline → BIN.REF.CATEGORY_CREATION_TIMELINE

## Sheet notes

Spine market_signal. Worked graph had 81 signals.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
