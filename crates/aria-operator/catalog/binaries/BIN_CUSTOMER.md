# BIN.CUSTOMER

**Operator:** `CUSTOMER` · **layer:** ENTITY · **class:** ENTITY · **parent:** COMPANY
**Crate:** `aria-telemetry-customer` · **verify:** True · **result:** `entity.customer`

## Why

Customer-kind anchor for logos, ICP, account targeting. Proof strength is a property on CLAIM/SOURCE, not a customer-binary score leaked into COMPANY.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.CUSTOMER` or `work --json` with `ops: ["BIN.CUSTOMER"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Customer, Account
- relationships: CUSTOMER_OF, BUYS, USES
- anchor tags: CUSTOMER, ACCOUNT, PROOF
- property key: —

## Maps that consume this

- 06 Customer Logos → BIN.REF.CUSTOMER_LOGOS
- 16 ICP Expansion → BIN.REF.ICP_EXPANSION

## Sheet notes

Implied by prior BUYERS tag and Customer Logos.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
