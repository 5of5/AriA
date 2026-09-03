# BIN.REL.HAS_PERSONA

**Operator:** `REL.HAS_PERSONA` · **layer:** RESIDUAL · **class:** REL · **parent:** ACCOUNT
**Crate:** `aria-res-rel-has-persona` · **verify:** True · **result:** `residual.rel.has_persona`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Relationship residual. Returns only this rel type and the endpoints actually used.

## Use

`work --binary BIN.REL.HAS_PERSONA` or `work --json` with `ops: ["BIN.REL.HAS_PERSONA"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: HAS_PERSONA
- anchor tags: —
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
