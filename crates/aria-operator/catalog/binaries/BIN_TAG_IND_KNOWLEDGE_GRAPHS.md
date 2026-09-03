# BIN.TAG.IND_KNOWLEDGE_GRAPHS

**Operator:** `TAG.IND_KNOWLEDGE_GRAPHS` · **layer:** DEEP_TAG · **class:** TAG · **parent:** MARKET
**Crate:** `aria-res-tag-ind-knowledge-graphs` · **verify:** True · **result:** `residual.tag.ind_knowledge_graphs`

## Why

Listed catalog identity. Independent calculation; no borrowed scores.

## Function

Tag operator. Does not name a new entity. Tags or reads an existing node. Role-tags (BUYER/COMPETITOR/…) require the specific tag on the record.

## Use

`work --binary BIN.TAG.IND_KNOWLEDGE_GRAPHS` or `work --json` with `ops: ["BIN.TAG.IND_KNOWLEDGE_GRAPHS"]`

Production callback: working vertical or nothing. Forget is not delete — original payload stays in `source`.

## Declared neighborhood

- node types: —
- relationships: TAGGED_AS
- anchor tags: IND_KNOWLEDGE_GRAPHS
- property key: —

## Maps that consume this

- (not a primary binary for any map, or is itself a map mixer)

## Important

- No Trust / Use / Goal fields.
- Does not rewrite other binaries' verticals.
- Empty declared types → omitted from `aria-work-v1` results.
