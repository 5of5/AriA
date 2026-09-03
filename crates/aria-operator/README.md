# aria-json-telemetry

Stateless JSON telemetry node over the Aria transformer. **0.2.2.**

Structured or unstructured JSON in → closed, typed graph verticals out.
One binary carries the transform, a catalog of 560 operator identities, and a
closed type-cast vocabulary. Nothing is stored; nothing is judged.

```toml
aria-json-telemetry = "0.2.2"
```

```bash
cargo install aria-json-telemetry --bin work
```

## Use

```bash
# One operator, identify funnel (no Φ match): working vertical or nothing
printf '{"nodes":[{"id":1,"type":"Person","label":"Ada","notes":"founder"}]}' \
  | work --binary BIN.PEOPLE --steps 0

# Many operators, one ingest
echo '{"ops":["BIN.PEOPLE","BIN.COMPANY"],"in":{"nodes":[...]}}' | work --json

# Harness lane: stdin request → stdout result, stderr silent, ≤ 64 KiB
work --harness < request.json

# Catalog and registry descriptor
work --commands
work --dispatch

# Hosted shell
work --serve 0.0.0.0:8080     # GET /health /commands /dispatch · POST /work /harness
```

Callback shape (`aria-work-v1`): `{schema, phi_once, asked, ops, organize, results[]}`.
`results` holds only operators that returned data. Each result is one closed
envelope; keep `binary_id, coverage_state, nodes, relationships, properties,
content_hash, graph` and drop the rest — the same prune for every operator.

Container (scratch, static, non-root):

```bash
docker build --target work -t aria-work:0.2.2 .
docker run -i --network=none aria-work:0.2.2 --harness < request.json
docker run -p 8080:8080 aria-work:0.2.2
```

## Library

```rust
use aria_operator::{run_many, RunOpts};
let envs = run_many(&["BIN.PEOPLE".into()], payload_bytes, &RunOpts::default())?;
```

`run_binary` / `run_many` / `execute_work` — one ingest, N independent
projectors. Deterministic: equal input bytes ⇒ equal output bytes.

Contract details: [WORKER.md](WORKER.md). Measured behaviour:
[OPTIMIZATION.md](OPTIMIZATION.md). Identities: [catalog/INDEX.md](catalog/INDEX.md).
Map mixers: [maps/MAPS.md](maps/MAPS.md).
