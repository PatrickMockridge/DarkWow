# DarkWow Bridge Node
#
# Single container image for running a cross-chain bridge relayer with
# capital endowment. Three modes at runtime:
#   MODE=full         — dwowd + contracts + universal_relayer (all-in-one)
#   MODE=relayer-only — universal_relayer only (external dwowd)
#   MODE=lilith       — standalone P2P seed node
#
# Build from repo root:
#   docker build -t darkwow-node/bridge . -f contrib/docker/bridge-node/Dockerfile
#
# Or use the build script:
#   contrib/docker/bridge-node/build-and-push.sh

# ============================================================================
# Stage 1: Rust Builder
# ============================================================================
FROM darkwow-base:24.04 AS builder

ENV CARGO_BUILD_JOBS=2
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /app

COPY . .

RUN cargo build -p universal_relayer --release -j 2

RUN cp target/release/universal_relayer /app/universal_relayer

# ============================================================================
# Stage 2: Runtime
# ============================================================================
FROM darkwow-base:24.04

WORKDIR /app

COPY --from=builder /app/universal_relayer /app/universal_relayer

COPY contrib/docker/bridge-node/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENV RUST_MIN_STACK=67108864

ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/universal_relayer"]
