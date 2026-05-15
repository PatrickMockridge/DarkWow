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
    libssl-dev pkg-config tcl-dev clang libsqlite3-dev libasound2-dev libsqlcipher3-dev

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y \
    --default-toolchain nightly-2025-04-10

ENV PATH="/root/.cargo/bin:${PATH}"
ENV LIBCLANG_PATH="/usr/lib/x86_64-linux-gnu"
ENV RUST_MIN_STACK=67108864
ENV CARGO_BUILD_JOBS=2
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /app

RUN git clone https://codeberg.org/PatrickM123/darkwow.git /app && \
    git -C /app checkout linear-master

RUN cargo build --release -p zkas

# Rebuild ZK proofs for bridge + relayer_endowment + money_v3
RUN /app/target/release/zkas rebuild /app/src/contract/money_v3/proof && \
    /app/target/release/zkas rebuild /app/src/contract/bridge/proof && \
    /app/target/release/zkas rebuild /app/src/contract/relayer_endowment/proof

RUN rustup target add wasm32-unknown-unknown

RUN cargo build --target wasm32-unknown-unknown --release -p dwow_deployooor_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_money_v3_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_bridge_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_relayer_endowment_contract

RUN cp target/wasm32-unknown-unknown/release/dwow_deployooor_contract.wasm /app/src/contract/deployooor/ && \
    cp target/wasm32-unknown-unknown/release/dwow_money_v3_contract.wasm /app/src/contract/money_v3/ && \
    cp target/wasm32-unknown-unknown/release/dwow_bridge_contract.wasm /app/src/contract/bridge/ && \
    cp target/wasm32-unknown-unknown/release/dwow_relayer_endowment_contract.wasm /app/src/contract/relayer_endowment/

RUN cargo build -p dwowd --release -j 2
RUN cargo build -p lilith --release -j 2
RUN cargo build -p universal_relayer --release -j 2
RUN cargo build -p dww --release -j 2

RUN cp target/release/dwowd /app/dwowd && \
    cp target/release/lilith /app/lilith && \
    cp target/release/universal_relayer /app/universal_relayer && \
    cp target/release/dww /app/dww && \
    cp bin/lilith/lilith_config.toml /app/lilith_config.toml

# ============================================================================
# Stage 2: Runtime
# ============================================================================
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    libssl3 libsqlite3-0 liblzma5 libstdc++6 curl bash openssl

WORKDIR /app

COPY --from=builder /app/dwowd /app/dwowd
COPY --from=builder /app/lilith /app/lilith
COPY --from=builder /app/universal_relayer /app/universal_relayer
COPY --from=builder /app/dww /app/dww
COPY --from=builder /app/lilith_config.toml /app/lilith_config.toml

# Copy WASM contracts for contract deployment
COPY --from=builder /app/src/contract/deployooor/dwow_deployooor_contract.wasm /app/wasm/deployooor.wasm
COPY --from=builder /app/src/contract/money_v3/dwow_money_v3_contract.wasm /app/wasm/money_v3.wasm
COPY --from=builder /app/src/contract/bridge/dwow_bridge_contract.wasm /app/wasm/bridge.wasm
COPY --from=builder /app/src/contract/relayer_endowment/dwow_relayer_endowment_contract.wasm /app/wasm/relayer_endowment.wasm

COPY contrib/docker/bridge-node/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENV RUST_MIN_STACK=67108864

ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/dwowd"]
