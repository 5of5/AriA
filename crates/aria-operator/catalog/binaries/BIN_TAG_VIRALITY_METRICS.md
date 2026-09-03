# BIN.TAG.VIRALITY_METRICS

**Operator:** `TAG.VIRALITY_METRICS` · **layer:** RESIDUAL · **class:** TAG · **parent:** PORTFOLIO
**Crate:** `aria-res-tag-virality-metrics` · **verify:** True · **result:** `residual.tag.virality_metrics`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.VIRALITY_METRICS` or `work --json` with `ops: ["BIN.TAG.VIRALITY_METRICS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: VIRALITY_METRICS
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
