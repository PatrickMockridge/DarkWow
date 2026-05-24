# DarkWow Testnet Docker Image
#
# Multi-stage build: clones from origin, builds inside Docker. Build from repo root:
#   docker build -t darkwow-testnet . -f contrib/docker/darkwow-testnet/Dockerfile
#
# Or use the build script:
#   contrib/docker/darkwow-testnet/build-and-push.sh

# ============================================================================
# Stage: Builder
# ============================================================================
FROM darkwow-base:24.04 AS builder

ENV CARGO_BUILD_JOBS=2
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /app

# Shallow clone (--depth 1) from origin on linear-master branch.
# Full clone is ~200MB and hits Codeberg CDN 504 timeouts; shallow clone
# transfers <10MB and completes in seconds.
# Retry on transient CDN errors (Codeberg sometimes returns 504).
RUN for i in 1 2 3 4 5; do \
        git clone --depth 1 --single-branch --branch linear-master \
            https://codeberg.org/PatrickM123/darkwow.git /app && break || sleep 10; \
    done

# Build zkas tool for host (needed to compile .zk source to .zk.bin)
RUN cargo build --release -p zkas

# Rebuild .zk.bin files from .zk sources (needed by contracts' include_bytes! macro)
RUN /app/target/release/zkas rebuild /app/src/contract/native_token/proof && \
    /app/target/release/zkas rebuild /app/src/contract/money_v3/proof && \
    /app/target/release/zkas rebuild /app/src/contract/baccarat/proof

# Install wasm32 target for WASM contract compilation
RUN rustup target add wasm32-unknown-unknown

# Build WASM contracts first (needed for dwowd binary)
RUN cargo build --target wasm32-unknown-unknown --release -p dwow_deployooor_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_native_token_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_money_v3_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_baccarat_contract

# Copy WASM files to contract directories for include_bytes! macro
RUN cp target/wasm32-unknown-unknown/release/dwow_deployooor_contract.wasm /app/src/contract/deployooor/ && \
    cp target/wasm32-unknown-unknown/release/dwow_native_token_contract.wasm /app/src/contract/native_token/ && \
    cp target/wasm32-unknown-unknown/release/dwow_money_v3_contract.wasm /app/src/contract/money_v3/ && \
    cp target/wasm32-unknown-unknown/release/dwow_baccarat_contract.wasm /app/src/contract/baccarat/

# Build dwowd with reduced parallelism to avoid memory issues
RUN cargo build -p dwowd --release -j 2

# Build lilith seed node
RUN cargo build -p lilith --release -j 2

# Build p2pool chain adaptor (presents dwowd as monerod to p2pool)
RUN cargo build -p dwow-p2pool-adaptor --release -j 2

RUN cp target/release/dwowd /app/dwowd && \
    cp target/release/lilith /app/lilith && \
    cp target/release/dwow-p2pool-adaptor /app/dwow-p2pool-adaptor && \
    cp bin/lilith/lilith_config.toml /app/lilith_config.toml

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM darkwow-base:24.04

# Use official xmrig binary (statically linked) to avoid glibc fortify buffer overflow
# in Ubuntu's packaged xmrig 6.21.1 during RandomX dataset init
RUN wget -q https://github.com/xmrig/xmrig/releases/download/v6.22.2/xmrig-6.22.2-linux-static-x64.tar.gz && \
    tar xf xmrig-6.22.2-linux-static-x64.tar.gz && \
    cp xmrig-6.22.2/xmrig /usr/local/bin/xmrig && \
    rm -rf xmrig-6.22.2*

WORKDIR /app

COPY --from=builder /app/dwowd ./
COPY --from=builder /app/lilith ./
COPY --from=builder /app/lilith_config.toml ./
COPY --from=builder /app/dwow-p2pool-adaptor ./
COPY --from=builder /app/contrib/docker/darkwow-testnet/entrypoint.sh /app/entrypoint.sh
COPY --from=builder /app/contrib/docker/darkwow-testnet/entrypoint-adaptor.sh /app/entrypoint-adaptor.sh
RUN chmod +x /app/entrypoint.sh /app/entrypoint-adaptor.sh

# Entrypoint handles config generation, CMD provides default binary args
ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/dwowd"]
