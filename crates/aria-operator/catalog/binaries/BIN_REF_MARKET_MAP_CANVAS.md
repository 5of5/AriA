# BIN.REF.MARKET_MAP_CANVAS

**Operator:** `REF.MARKET_MAP_CANVAS` · **layer:** REFINEMENT · **class:** REFINEMENT · **parent:** MARKET_MAP
**Crate:** `aria-ref-market-map-canvas` · **verify:** True · **result:** `map.ref.market_map_canvas`

## Why

Sealed map type Market Map Canvas. One dump × 25 mixers is the viral coefficient: the same tagged telemetry fans out into 25 structured map JSON results without a second Trust write.

## Function

Map mixer. Ingests the same JSON any operator ingests (raw graph, or already-processed `aria-work-v1` callback). Returns ONLY the neighborhood this sealed map type is allowed to consume (declared kinds/rels/tags). Missing data is omitted. Source bytes are not rewritten. Downstream views consume this envelope; the node does not score it.

## Use

`work --binary BIN.REF.MARKET_MAP_CANVAS` or `work --json` with `ops: ["BIN.REF.MARKET_MAP_CANVAS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company, Market, Category, Portfolio, MarketMap, Workspace
- relationships: IN_CATEGORY, PLACED_IN
- anchor tags: COMPANY, MARKET, MARKET_MAP, SEGMENT, Market Map Canvas, MAP_TYPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
