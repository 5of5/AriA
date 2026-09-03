# BIN.SOURCE

**Operator:** `SOURCE` · **layer:** FACT · **class:** FACT · **parent:** CLAIM
**Crate:** `aria-telemetry-source` · **verify:** True · **result:** `fact.source`

## Why

Provenance trail. Required by P1 evidence gating and by the white-space finding that per-datum sources are unclaimed. Independent of claim wording.

## Function

Closed operator. Returns only its declared kinds as structured JSON. Independent of other binaries' scores (B2).

## Use

`work --binary BIN.SOURCE` or `work --json` with `ops: ["BIN.SOURCE"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Source
- relationships: SUPPORTS, RETRIEVED_FROM
- anchor tags: SOURCE, URL, RETRIEVED_AT, METHOD
- property key: —

## Maps that consume this

- 19 Narrative Positioning → BIN.REF.NARRATIVE_POSITIONING
- 25 Market Intelligence Brief → BIN.REF.MARKET_INTELLIGENCE_BRIEF

## Sheet notes

Spine source. Worked graph had 52 sources.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
