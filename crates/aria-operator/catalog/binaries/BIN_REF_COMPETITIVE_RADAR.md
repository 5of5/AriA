# BIN.REF.COMPETITIVE_RADAR

**Operator:** `REF.COMPETITIVE_RADAR` · **layer:** REFINEMENT · **class:** REFINEMENT · **parent:** MARKET_MAP
**Crate:** `aria-ref-competitive-radar` · **verify:** True · **result:** `map.ref.competitive_radar`

## Why

Sealed map type Competitive Radar. One dump × 25 mixers is the viral coefficient: the same tagged telemetry fans out into 25 structured map JSON results without a second Trust write.

## Function

Map mixer. Ingests the same JSON any operator ingests (raw graph, or already-processed `aria-work-v1` callback). Returns ONLY the neighborhood this sealed map type is allowed to consume (declared kinds/rels/tags). Missing data is omitted. Source bytes are not rewritten. Downstream views consume this envelope; the node does not score it.

## Use

`work --binary BIN.REF.COMPETITIVE_RADAR` or `work --json` with `ops: ["BIN.REF.COMPETITIVE_RADAR"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company, MarketSignal, Claim
- relationships: COMPETES_WITH
- anchor tags: COMPANY, COMPETITOR_TAG, SIGNAL, Competitive Radar, MAP_TYPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
