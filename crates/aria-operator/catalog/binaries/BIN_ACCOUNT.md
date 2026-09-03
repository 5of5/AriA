# BIN.ACCOUNT

**Operator:** `ACCOUNT` · **layer:** ENTITY · **class:** ENTITY · **parent:** CUSTOMER
**Crate:** `aria-telemetry-account` · **verify:** True · **result:** `entity.account`

## Why

Named account anchor for targeting and ICP expansion. Separate from generic CUSTOMER so warm-path calculations stay requirement-bound.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.ACCOUNT` or `work --json` with `ops: ["BIN.ACCOUNT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Account
- relationships: TARGETED_BY, HAS_PERSONA, PATH_TO
- anchor tags: ACCOUNT, ICP, WARM_PATH
- property key: —

## Maps that consume this

- 08 Partnership Path → BIN.REF.PARTNERSHIP_PATH
- 16 ICP Expansion → BIN.REF.ICP_EXPANSION
- 18 Account Targeting → BIN.REF.ACCOUNT_TARGETING

## Sheet notes

Required by maps 16 and 18.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
