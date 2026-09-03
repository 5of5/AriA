# BIN.BUYER

**Operator:** `BUYER` · **layer:** TAG · **class:** TAG · **parent:** PEOPLE
**Crate:** `aria-telemetry-buyer` · **verify:** True · **result:** `tag.buyer`

## Why

Role-tag operator on PERSON / ACCOUNT. Buyer-persona maps consume BUYER_TAG independently of seller or syndicate calculations.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.BUYER` or `work --json` with `ops: ["BIN.BUYER"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Person, Account
- relationships: BUYS, HAS_PERSONA, TAGGED_AS
- anchor tags: BUYER_TAG, PERSON, ACCOUNT
- property key: —

## Maps that consume this

- 04 Buyer Persona Relationship → BIN.REF.BUYER_PERSONA_RELATIONSHIP
- 16 ICP Expansion → BIN.REF.ICP_EXPANSION

## Sheet notes

BUYERS = MAP -> BUYER_TAG

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
