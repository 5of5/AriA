# Measured behaviour — aria-json-telemetry 0.2.2

The node is stateless: request bytes in, callback bytes out. Φ is five
actions; Inv1–4 hold; readout stays outside Φ.

| Surface | Contract |
|---|---|
| Library | `run_binary` / `run_many` / `execute_work` — one ingest, N independent projectors |
| CLI | `work` — `--binary`, `--json`, `--commands`, `--harness`, `--dispatch`, `--serve` |
| Callback | `aria-work-v1`: `{schema, phi_once, asked, ops, organize, results[]}` — working verticals only |
| Harness | `--harness`: stdin → stdout, stderr empty, ≤ 64 KiB, bindings echoed |
| Container | `Dockerfile` target `work`: scratch, static (asserted with `ldd`), UID 65534, **2.04 MB** |
| Hosted shell | fixed pool 4× cores · bounded queue 1024 · `503 Retry-After` past the queue · 10 s socket deadlines · static routes cached · zero shared mutable state |

560 catalog identities (535 research/host + 25 `BIN.REF.*` mixers), all
served by one binary.

## Projector cost

1. One `GraphIndex` pass, O(N+E): kind, kind-like, tag (explicit ∪ cast),
   relationship, first-property, id→idx. Every projector is a lookup.
2. Type-cast is an n-gram scan over a 327-phrase closed lexicon — O(tokens · L),
   whole-word / phrase only. Unlisted tokens become `uncast_token`.
3. One serde key order for all 560 envelopes; empty members omit; the
   callback drops operators with no data.
4. Host identities never enter Φ.

Byte-identity referee (`scripts/dump_referee.py`) held on every file across
the index + lexicon rewrite. Identify timings (debug profile):

| Case | Before | After |
|---|---|---|
| 414-node graph, 560 ops | 1226 ms | 297 ms |
| 5k nodes / 10k edges | 14.5 s | 3.2 s |
| 400-tag node | 112 ms | 23 ms |

## Concurrency

| Measurement | Result |
|---|---|
| 64 workers, sequential vs one thread per core | byte-identical callbacks; 25 → 89 ops/s |
| 32 clients × 8 harness calls over TCP (`tests/serve_load.rs`) | 1 distinct body, 0 errors, ~950 ops/s (debug) |
| re-feeding a callback into the 25 mixers, twice | depth-2 output = depth-1 (closed grammar, no runaway) |

## Hostile input

| Input | Behaviour |
|---|---|
| 2000-deep nesting | `Err` (recursion limit), bound `limitation`, no panic |
| duplicate ids / dangling edges | `Err` with the offending position |
| 1.2 MB text cell · 5k×10k graph · 400-tag node · unicode/injection labels | OK, bounded |

Invariants on every run: no Trust/score keys on the wire, no skeleton in a
callback, garbage never mints a Person, host operators never carry graph data.

## Not in this cut

No sixth Φ action. No writes. No decoder in Φ. Operator crates stay
`publish = false`; the workspace stays below 0.3.0.
