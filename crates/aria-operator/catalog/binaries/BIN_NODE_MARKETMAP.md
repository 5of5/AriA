# BIN.NODE.MARKETMAP

**Operator:** `NODE.MARKETMAP` · **layer:** RESIDUAL · **class:** NODE · **parent:** MARKET_MAP
**Crate:** `aria-res-node-marketmap` · **verify:** True · **result:** `residual.node.marketmap`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Node residual. Returns only this kind. Empty is no-finding, not a guess.

## Use

`work --binary BIN.NODE.MARKETMAP` or `work --json` with `ops: ["BIN.NODE.MARKETMAP"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: MarketMap
- relationships: —
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
