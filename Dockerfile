# AriA — public deployment image (two single-binary targets, one build).
#
# Laws in image form: scratch base (zero layers/CVE surface), static MUSL,
# non-root, --locked (the committed Cargo.lock is the audited dependency
# set; drift fails the build). The tree is pure Rust + musl-dev only.
#
#   # telemetry node (public deployment): the whole 560-identity JSON
#   # telemetry catalog, harness lane, hosted HTTP shell
#   docker build --target work -t aria-work .
#   # CMD defaults to the hosted shell; pass --harness for the stdin lane
#   # (a harness passes argv ("--harness",) — never rely on auto-detect
#   # when the image's default command is --serve).
#   docker run -i --network=none aria-work --harness < request.json
#   docker run -p 8080:8080 aria-work                         # HTTP on :8080
#
#   # transformer engine CLI (verify/bench/emit — see crates/aria-cli)
#   docker build --target aria -t aria-engine .
#   docker run -i --network=none aria-engine node --steps 0 < sheet.json
#
# `--dispatch` (or /dispatch) returns each image's self-sha256 for the
# registry manifest.

FROM rust:1.97-alpine AS builder
RUN apk add --no-cache musl-dev binutils
WORKDIR /aria
COPY . .
# The alpine default target is *-unknown-linux-musl; both binaries must be
# fully static so the scratch stages have nothing to load — asserted below.
RUN cargo build --release --locked \
      -p aria-engine --bin aria \
      -p aria-json-telemetry --bin work \
 && strip target/release/aria target/release/work \
 && sha256sum target/release/work | tee /aria/work.sha256 \
 && for b in aria work; do \
      ldd target/release/$b 2>&1 | tee /tmp/ldd.out | grep -qiE 'not a (valid )?dynamic|statically linked' \
        || ! grep -q '=>' /tmp/ldd.out || exit 1; \
    done

# ── telemetry node (default for public deployment) ─────────────────
FROM scratch AS work
COPY --from=builder /aria/target/release/work /work
COPY --from=builder /aria/work.sha256 /work.sha256
USER 65534:65534
EXPOSE 8080
ENTRYPOINT ["/work"]
CMD ["--serve", "0.0.0.0:8080", "--steps", "0"]

# ── transformer engine CLI ─────────────────────────────────────────
FROM scratch AS aria
COPY --from=builder /aria/target/release/aria /aria
USER 65534:65534
ENTRYPOINT ["/aria"]
