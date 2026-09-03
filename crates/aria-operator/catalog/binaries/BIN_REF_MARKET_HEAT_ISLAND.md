# BIN.REF.MARKET_HEAT_ISLAND

**Operator:** `REF.MARKET_HEAT_ISLAND` · **layer:** REFINEMENT · **class:** REFINEMENT · **parent:** MARKET_MAP
**Crate:** `aria-ref-market-heat-island` · **verify:** True · **result:** `map.ref.market_heat_island`

## Why

Sealed map type Market Heat Island. One dump × 25 mixers is the viral coefficient: the same tagged telemetry fans out into 25 structured map JSON results without a second Trust write.

## Function

Map mixer. Ingests the same JSON any operator ingests (raw graph, or already-processed `aria-work-v1` callback). Returns ONLY the neighborhood this sealed map type is allowed to consume (declared kinds/rels/tags). Missing data is omitted. Source bytes are not rewritten. Downstream views consume this envelope; the node does not score it.

## Use

`work --binary BIN.REF.MARKET_HEAT_ISLAND` or `work --json` with `ops: ["BIN.REF.MARKET_HEAT_ISLAND"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company, MarketSignal, Event, Market, Category
- relationships: SIGNALS, OPERATES_IN
- anchor tags: COMPANY, SIGNAL, FUNDING, HIRING, LAUNCH, Market Heat Island, MAP_TYPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
