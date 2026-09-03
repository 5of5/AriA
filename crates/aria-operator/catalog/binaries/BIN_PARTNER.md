# BIN.PARTNER

**Operator:** `PARTNER` · **layer:** TAG · **class:** TAG · **parent:** COMPANY
**Crate:** `aria-telemetry-partner` · **verify:** True · **result:** `tag.partner`

## Why

Role-tag for partnership-path and ecosystem-gravity maps. Warm-path calculation is this operator's job, not COMPANY's.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.PARTNER` or `work --json` with `ops: ["BIN.PARTNER"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Company
- relationships: PARTNERS_WITH, PATH_TO, TAGGED_AS
- anchor tags: PARTNER_TAG, COMPANY, PATH
- property key: —

## Maps that consume this

- 07 Developer Ecosystem → BIN.REF.DEVELOPER_ECOSYSTEM
- 08 Partnership Path → BIN.REF.PARTNERSHIP_PATH
- 18 Account Targeting → BIN.REF.ACCOUNT_TARGETING
- 22 Market Entry → BIN.REF.MARKET_ENTRY
- 23 Ecosystem Gravity → BIN.REF.ECOSYSTEM_GRAVITY

## Sheet notes

Required by maps 08 and 23. Not on prior sheet.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
