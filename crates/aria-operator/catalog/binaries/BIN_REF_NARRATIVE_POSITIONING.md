# BIN.REF.NARRATIVE_POSITIONING

**Operator:** `REF.NARRATIVE_POSITIONING` · **layer:** REFINEMENT · **class:** REFINEMENT · **parent:** MARKET_MAP
**Crate:** `aria-ref-narrative-positioning` · **verify:** True · **result:** `map.ref.narrative_positioning`

## Why

Sealed map type Narrative Positioning. One dump × 25 mixers is the viral coefficient: the same tagged telemetry fans out into 25 structured map JSON results without a second Trust write.

## Function

Map mixer. Ingests the same JSON any operator ingests (raw graph, or already-processed `aria-work-v1` callback). Returns ONLY the neighborhood this sealed map type is allowed to consume (declared kinds/rels/tags). Missing data is omitted. Source bytes are not rewritten. Downstream views consume this envelope; the node does not score it.

## Use

`work --binary BIN.REF.NARRATIVE_POSITIONING` or `work --json` with `ops: ["BIN.REF.NARRATIVE_POSITIONING"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company, Claim, Source, Content
- relationships: ASSERTS, CITES
- anchor tags: COMPANY, CONTENT, CLAIM, SOURCE, Narrative Positioning, MAP_TYPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
