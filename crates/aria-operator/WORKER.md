# Worker contract

A worker names one or more catalog operators and supplies the JSON it wants
organized. The node returns typed graph verticals. The worker prunes on its
side. Nothing is stored between calls; the node never chooses the next step.

## Input

Any of: a typed graph `{nodes:[{id,type,label,notes,tags,…}], edges:[{from,to,type}]}`,
a sheet `{rows:[…]}`, free text `{notes:[…]}`, or a previous `aria-work-v1`
callback (map mixers `BIN.REF.*` re-ingest it).

Free text is type-cast against a closed vocabulary of listed tokens
(`"founded"` → `PERSON_FOUNDER`). Unlisted values on designated fields come
back as `limitations: ["uncast_token: field=value"]`. No inference, no new nodes.

## Callback — `aria-work-v1`

```json
{"schema":"aria-work-v1","phi_once":true,"asked":3,"ops":2,
 "organize":{"tokens":9,"hits":["PERSON_FOUNDER"],"binaries":["BIN.TAG.PERSON_FOUNDER","BIN.PEOPLE"],"nodes":2,"edges":1,"kinds":["Company","Person"]},
 "results":[ …one envelope per operator that returned data… ]}
```

`asked ≠ ops` is the audit: operators with nothing to return are absent, never
empty. `organize` reports what the node found in the input and which operators
will structure it — every recommended operator returns data on that input.

## Envelope — `aria-operator-envelope-v1`

One key order for all 560 operators; optional members omit when empty.

`schema, binary_id, operator, schema_version, crate, plan_hash, requirement_id?,
subject_ids?, resultDefinitionRef, anchor_tags?, neo4j_hit?, nodes?,
relationships?, properties?, verify, coverage_state, no_finding_reason?,
limitations?, content_hash, graph?, telemetry?`

- `coverage_state ∈ {proposal, no-finding, limitation, truncation, failure}`
- `content_hash` = sha256 of canonical `{nodes, relationships, properties}`
- `graph` = catalog position `{class, layer, weight, height, shape, anchors[]}`,
  a fixed function of the catalog, never of the input

**Prune (same for every operator):** keep `binary_id, coverage_state, nodes,
relationships, properties, content_hash, graph`.

## Harness lane

`work --harness < request.json` — one JSON document on stdout, stderr always
empty, exit 0 for every bound result, exit 2 only for an unbound protocol error.

Request (`pcvc-aria-telemetry-request-v1`): `capability: "aria.telemetry.project"`,
`runId`, `planHash` (hex64), `attemptId`, `fencingToken ≥ 1`, `requirementId`,
`ops[]` (`["*"]` = all), `payload`, `steps` (0), `seed` (1),
`outputLimitBytes` (≤ 65536).

Result (`pcvc-aria-telemetry-result-v1`): the bindings echoed, `status ∈
{result, no-finding, truncation, limitation}`, and `callback` (above).
Over budget, the largest verticals are dropped first, deterministically
(`droppedVerticals`). `work --dispatch` emits the registry descriptor:
capability, executable sha256, and all 560 operators.

## Hosted shell

`work --serve ADDR`: fixed worker pool, bounded queue, `503 Retry-After` past
the queue, socket deadlines, no shared mutable state. `GET /health /commands
/dispatch` · `POST /work /harness`. Same bytes from any replica.
