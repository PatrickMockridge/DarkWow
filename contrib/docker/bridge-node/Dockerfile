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
FROM ubuntu:24.04 AS builder

RUN apt-get update && apt-get install -y \
    build-essential gcc g++ make cmake curl git \
    libssl-dev pkg-config tcl-dev clang libsqlite3-dev libasound2-dev libsqlcipher-dev

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y \
    --default-toolchain nightly-2025-04-10

ENV PATH="/root/.cargo/bin:${PATH}"
ENV LIBCLANG_PATH="/usr/lib/x86_64-linux-gnu"
ENV RUST_MIN_STACK=67108864
ENV CARGO_BUILD_JOBS=2
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /app

COPY . .

RUN cargo build -p universal_relayer --release -j 2

RUN cp target/release/universal_relayer /app/universal_relayer

# ============================================================================
# Stage 2: Runtime
# ============================================================================
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    libssl3 libsqlite3-0 liblzma5 libstdc++6 curl bash openssl

WORKDIR /app

COPY --from=builder /app/universal_relayer /app/universal_relayer

COPY contrib/docker/bridge-node/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENV RUST_MIN_STACK=67108864

ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/universal_relayer"]
