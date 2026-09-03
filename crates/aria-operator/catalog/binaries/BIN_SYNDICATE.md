# BIN.SYNDICATE

**Operator:** `SYNDICATE` · **layer:** TAG · **class:** TAG · **parent:** VENTURE_CAPITAL
**Crate:** `aria-telemetry-syndicate` · **verify:** True · **result:** `tag.syndicate`

## Why

Composition operator. SYNDICATE = BUYERS OR SELLERS on the capital path (investors + co-investors + portfolio overlap). Calculates only membership and path edges. Does not recompute PEOPLE or COMPANY.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.SYNDICATE` or `work --json` with `ops: ["BIN.SYNDICATE"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Investor, Syndicate
- relationships: CO_INVESTS_WITH, MEMBER_OF, SYNDICATES_WITH
- anchor tags: SYNDICATE_TAG, BUYER_TAG, SELLER_TAG, INVESTOR
- property key: —

## Maps that consume this

- 05 Investor Syndicate → BIN.REF.INVESTOR_SYNDICATE

## Sheet notes

SYNDICATE = BUYERS OR SELLERS (duplicated on prior sheet; collapsed to one row).

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
