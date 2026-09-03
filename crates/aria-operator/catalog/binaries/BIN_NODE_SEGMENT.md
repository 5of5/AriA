# BIN.NODE.SEGMENT

**Operator:** `NODE.SEGMENT` · **layer:** RESIDUAL · **class:** NODE · **parent:** MARKET_MAP
**Crate:** `aria-res-node-segment` · **verify:** True · **result:** `residual.node.segment`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Node residual. Returns only this kind. Empty is no-finding, not a guess.

## Use

`work --binary BIN.NODE.SEGMENT` or `work --json` with `ops: ["BIN.NODE.SEGMENT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: Segment
- relationships: —
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
