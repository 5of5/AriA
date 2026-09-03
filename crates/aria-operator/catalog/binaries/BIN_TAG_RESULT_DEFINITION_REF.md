# BIN.TAG.RESULT_DEFINITION_REF

**Operator:** `TAG.RESULT_DEFINITION_REF` · **layer:** RESIDUAL · **class:** TAG · **parent:** schema-lookup
**Crate:** `aria-res-tag-result-definition-ref` · **verify:** True · **result:** `residual.tag.result_definition_ref`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.RESULT_DEFINITION_REF` or `work --json` with `ops: ["BIN.TAG.RESULT_DEFINITION_REF"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: RESULT_DEFINITION_REF
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
