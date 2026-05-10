# DarkWow Devnet Mining Node Docker Image
#
# Multi-stage build from local source. Build from repo root:
#   docker build -t dwow-devnet . -f contrib/docker/dwow-devnet/Dockerfile
#
# Or use the build script:
#   contrib/docker/dwow-devnet/build-and-push.sh

# ============================================================================
# Stage: Builder
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

COPY . /app

# Build zkas (ZK circuit compiler, needed to compile .zk proofs to .zk.bin)
RUN cargo build --release -p zkas

# Rebuild .zk.bin files from .zk sources
# native_token: required for genesis (NativeToken contract proofs)
# money_v3: DeFi token contract proofs
# baccarat: additional contract for testing
RUN /app/target/release/zkas rebuild /app/src/contract/native_token/proof && \
    /app/target/release/zkas rebuild /app/src/contract/money_v3/proof && \
    /app/target/release/zkas rebuild /app/src/contract/baccarat/proof

RUN rustup target add wasm32-unknown-unknown

# Build WASM contracts (embedded into dwowd via include_bytes!)
RUN cargo build --target wasm32-unknown-unknown --release -p dwow_deployooor_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_native_token_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_money_v3_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_baccarat_contract

# Copy WASM files to contract source dirs for include_bytes! macro
RUN cp target/wasm32-unknown-unknown/release/dwow_deployooor_contract.wasm /app/src/contract/deployooor/ && \
    cp target/wasm32-unknown-unknown/release/dwow_native_token_contract.wasm /app/src/contract/native_token/ && \
    cp target/wasm32-unknown-unknown/release/dwow_money_v3_contract.wasm /app/src/contract/money_v3/ && \
    cp target/wasm32-unknown-unknown/release/dwow_baccarat_contract.wasm /app/src/contract/baccarat/

# Build dwowd (fullnode daemon) with reduced parallelism
RUN cargo build -p dwowd --release -j 2

# Build lilith (P2P seed node)
RUN cargo build -p lilith --release -j 2

RUN cp target/release/dwowd /app/dwowd && \
    cp target/release/lilith /app/lilith && \
    cp bin/lilith/lilith_config.toml /app/lilith_config.toml

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    libssl3 libsqlite3-0 liblzma5 libstdc++6 curl bash wget

# xmrig static binary for RandomX PoW mining
# Static build avoids glibc fortify issues in Ubuntu's packaged xmrig 6.21.1
RUN wget -q https://github.com/xmrig/xmrig/releases/download/v6.22.2/xmrig-6.22.2-linux-static-x64.tar.gz && \
    tar xf xmrig-6.22.2-linux-static-x64.tar.gz && \
    cp xmrig-6.22.2/xmrig /usr/local/bin/xmrig && \
    rm -rf xmrig-6.22.2*

WORKDIR /app

COPY --from=builder /app/dwowd ./
COPY --from=builder /app/lilith ./
COPY --from=builder /app/lilith_config.toml ./
COPY contrib/docker/dwow-devnet/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/dwowd"]
