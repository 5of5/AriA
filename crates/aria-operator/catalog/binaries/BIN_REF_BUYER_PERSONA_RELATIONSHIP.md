# BIN.REF.BUYER_PERSONA_RELATIONSHIP

**Operator:** `REF.BUYER_PERSONA_RELATIONSHIP` · **layer:** REFINEMENT · **class:** REFINEMENT · **parent:** MARKET_MAP
**Crate:** `aria-ref-buyer-persona-relationship` · **verify:** True · **result:** `map.ref.buyer_persona_relationship`

## Why

Sealed map type Buyer Persona Relationship. One dump × 25 mixers is the viral coefficient: the same tagged telemetry fans out into 25 structured map JSON results without a second Trust write.

## Function

Map mixer. Ingests the same JSON any operator ingests (raw graph, or already-processed `aria-work-v1` callback). Returns ONLY the neighborhood this sealed map type is allowed to consume (declared kinds/rels/tags). Missing data is omitted. Source bytes are not rewritten. Downstream views consume this envelope; the node does not score it.

## Use

`work --binary BIN.REF.BUYER_PERSONA_RELATIONSHIP` or `work --json` with `ops: ["BIN.REF.BUYER_PERSONA_RELATIONSHIP"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Person, Company, Account, Claim
- relationships: WORKS_AT, BUYS, HAS_PERSONA
- anchor tags: PERSON, BUYER_TAG, COMPANY, CUSTOMER_PROOF, Buyer Persona Relationship, MAP_TYPE
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
