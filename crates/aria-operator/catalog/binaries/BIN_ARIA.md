# BIN.ARIA

**Operator:** `AriA` · **layer:** TRANSFORM · **class:** TRANSFORM · **parent:** —
**Crate:** `aria-optical-jepa-graph` · **verify:** True · **result:** `transform.aria`

## Why

Dedicated transformer. Consumes independent binary JSON telemetries. Prunes / filters / trajectory features. Never JUDGE. Never Trust. Not a research-operator binary and not a host tool that writes proposals.

## Function

AriA transformer pass-through of the ingested graph. Pass-through only; no scoring.

## Use

`work --binary BIN.ARIA` or `work --json` with `ops: ["BIN.ARIA"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: TELEMETRY_SPINE, TRAJECTORY
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Sheet notes

https://github.com/5of5/json-telemetry — fork root for every operator crate.

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
