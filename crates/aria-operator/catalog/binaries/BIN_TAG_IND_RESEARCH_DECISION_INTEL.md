# BIN.TAG.IND_RESEARCH_DECISION_INTEL

**Operator:** `TAG.IND_RESEARCH_DECISION_INTEL` · **layer:** DEEP_TAG · **class:** TAG · **parent:** MARKET
**Crate:** `aria-res-tag-ind-research-decision-intel` · **verify:** True · **result:** `residual.tag.ind_research_decision_intel`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.IND_RESEARCH_DECISION_INTEL` or `work --json` with `ops: ["BIN.TAG.IND_RESEARCH_DECISION_INTEL"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: IND_RESEARCH_DECISION_INTEL
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
