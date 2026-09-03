# BIN.MARKET_MAP

**Operator:** `MARKET_MAP` · **layer:** ENVELOPE · **class:** ENVELOPE · **parent:** PORTFOLIO
**Crate:** `aria-telemetry-market-map` · **verify:** True · **result:** `market_map.instance`

## Why

Named map instance bound to one sealed map_type from the 25-row registry. Returns operator=MARKET_MAP only. Module fields stay in module tables; this binary returns placement + readiness inputs, not Views.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.MARKET_MAP` or `work --json` with `ops: ["BIN.MARKET_MAP"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: MarketMap, MapNode, Segment
- relationships: INSTANCE_OF, PLACED_IN, REQUIRES_EDGE, REQUIRES_SIGNAL
- anchor tags: MARKET_MAP, MAP_TYPE, SEGMENT, READINESS
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Sheet notes

MARKET_MAP = ANY

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
