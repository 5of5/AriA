# BIN.REF.DEVELOPER_ECOSYSTEM

**Operator:** `REF.DEVELOPER_ECOSYSTEM` · **layer:** REFINEMENT · **class:** REFINEMENT · **parent:** MARKET_MAP
**Crate:** `aria-ref-developer-ecosystem` · **verify:** True · **result:** `map.ref.developer_ecosystem`

## Why

Sealed map type Developer Ecosystem. One dump × 25 mixers is the viral coefficient: the same tagged telemetry fans out into 25 structured map JSON results without a second Trust write.

## Function

Map mixer. Ingests the same JSON any operator ingests (raw graph, or already-processed `aria-work-v1` callback). Returns ONLY the neighborhood this sealed map type is allowed to consume (declared kinds/rels/tags). Missing data is omitted. Source bytes are not rewritten. Downstream views consume this envelope; the node does not score it.

## Use

`work --binary BIN.REF.DEVELOPER_ECOSYSTEM` or `work --json` with `ops: ["BIN.REF.DEVELOPER_ECOSYSTEM"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company, Product, Person
- relationships: INTEGRATES_WITH, OFFERED_BY
- anchor tags: COMPANY, PRODUCT, PARTNER_TAG, Developer Ecosystem, MAP_TYPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
