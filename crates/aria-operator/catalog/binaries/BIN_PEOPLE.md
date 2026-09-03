# BIN.PEOPLE

**Operator:** `PEOPLE` · **layer:** ENTITY · **class:** ENTITY · **parent:** PORTFOLIO
**Crate:** `aria-telemetry-people` · **verify:** True · **result:** `entity.person`

## Why

Person-kind anchor. Independent calculation of person nodes and incident edges required by buyer / founder / influence / ICP maps. Does not inherit company scores.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.PEOPLE` or `work --json` with `ops: ["BIN.PEOPLE"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Person
- relationships: WORKS_AT, FOUNDED, PREVIOUSLY_AT, INFLUENCES, MEMBER_OF, BUYS
- anchor tags: PERSON, ROLE, BUYER_TAG
- property key: —

## Maps that consume this

- 04 Buyer Persona Relationship → BIN.REF.BUYER_PERSONA_RELATIONSHIP
- 07 Developer Ecosystem → BIN.REF.DEVELOPER_ECOSYSTEM
- 13 Influence Network → BIN.REF.INFLUENCE_NETWORK
- 16 ICP Expansion → BIN.REF.ICP_EXPANSION
- 18 Account Targeting → BIN.REF.ACCOUNT_TARGETING
- 20 Founder/Operator Lineage → BIN.REF.FOUNDER_OPERATOR_LINEAGE

## Sheet notes

PEOPLE = 25

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
