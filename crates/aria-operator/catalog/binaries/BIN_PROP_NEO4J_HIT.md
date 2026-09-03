# BIN.PROP.NEO4J_HIT

**Operator:** `PROP.NEO4J_HIT` · **layer:** RESIDUAL · **class:** PROP · **parent:** all operators
**Crate:** `aria-res-prop-neo4j-hit` · **verify:** True · **result:** `residual.prop.neo4j_hit`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Property residual. Echoes one declared key when present. Never mints Trust.

## Use

`work --binary BIN.PROP.NEO4J_HIT` or `work --json` with `ops: ["BIN.PROP.NEO4J_HIT"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: —
- anchor tags: —
- property key: neo4j_hit

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
