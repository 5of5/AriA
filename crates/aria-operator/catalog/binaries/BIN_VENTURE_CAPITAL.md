# BIN.VENTURE_CAPITAL

**Operator:** `VENTURE_CAPITAL` · **layer:** ENTITY · **class:** ENTITY · **parent:** PORTFOLIO
**Crate:** `aria-telemetry-vc` · **verify:** True · **result:** `entity.investor`

## Why

Investor-kind anchor. Tight default limit. Capital-path maps consume this operator; they do not compute investor identity inside COMPANY.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.VENTURE_CAPITAL` or `work --json` with `ops: ["BIN.VENTURE_CAPITAL"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Investor, Fund
- relationships: INVESTS_IN, CO_INVESTS_WITH, MEMBER_OF, LEADS
- anchor tags: INVESTOR, FUND, SYNDICATE_TAG
- property key: —

## Maps that consume this

- 05 Investor Syndicate → BIN.REF.INVESTOR_SYNDICATE
- 12 Funding Momentum → BIN.REF.FUNDING_MOMENTUM
- 13 Influence Network → BIN.REF.INFLUENCE_NETWORK

## Sheet notes

VENTURE_CAPITAL = 5

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
