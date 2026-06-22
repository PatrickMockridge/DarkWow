# DarkWow Public Testnet Node
#
# Single container image for joining the public DarkWow testnet as a mining node.
# Two modes at runtime: MODE=native (solo mining) and MODE=merge (Monero merge mining).
#
# Build from repo root:
#   docker build -t darkwow-node/testnet . -f contrib/docker/testnet-node/Dockerfile
#
# Or use the build script:
#   contrib/docker/testnet-node/build-and-push.sh

# ============================================================================
# Stage 1: Rust Builder
# ============================================================================
FROM darkwow-base:24.04 AS builder

ARG CARGO_BUILD_JOBS=2
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
ARG RAYON_NUM_THREADS=2
ENV RAYON_NUM_THREADS=${RAYON_NUM_THREADS}
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /app

RUN git clone https://codeberg.org/PatrickM123/darkwow.git /app && \
    git -C /app checkout linear-master

RUN cargo build --release -p zkas

RUN /app/target/release/zkas rebuild /app/src/contract/native_token/proof && \
    /app/target/release/zkas rebuild /app/src/contract/promissory_note/proof && \
    /app/target/release/zkas rebuild /app/src/contract/baccarat/proof

RUN rustup target add wasm32-unknown-unknown

RUN cargo build --target wasm32-unknown-unknown --release -p dwow_deployooor_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_native_token_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_promissory_note_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_baccarat_contract

RUN cp target/wasm32-unknown-unknown/release/dwow_deployooor_contract.wasm /app/src/contract/deployooor/ && \
    cp target/wasm32-unknown-unknown/release/dwow_native_token_contract.wasm /app/src/contract/native_token/ && \
    cp target/wasm32-unknown-unknown/release/dwow_promissory_note_contract.wasm /app/src/contract/promissory_note/ && \
    cp target/wasm32-unknown-unknown/release/dwow_baccarat_contract.wasm /app/src/contract/baccarat/

RUN cargo build -p dwowd --release -j ${CARGO_BUILD_JOBS}
RUN cargo build -p lilith --release -j ${CARGO_BUILD_JOBS}

RUN cp target/release/dwowd /app/dwowd && \
    cp target/release/lilith /app/lilith && \
    cp bin/lilith/lilith_config.toml /app/lilith_config.toml

# ============================================================================
# Stage 2: Binary Downloader
# ============================================================================
FROM darkwow-base:24.04 AS downloader

# monerod — pre-built Monero daemon for merge mining
RUN wget -q https://downloads.getmonero.org/cli/linux64 -O monero-cli.tar.bz2 && \
    tar xf monero-cli.tar.bz2 && \
    cp monero-x86_64-linux-gnu-*/monerod /usr/local/bin/monerod && \
    cp monero-x86_64-linux-gnu-*/monero-wallet-cli /usr/local/bin/monero-wallet-cli && \
    rm -rf monero-*

# p2pool v4.16 — merge mining sidechain (checksum-verified)
# v4.16 patches a network-layer exploit.
RUN wget -q https://github.com/SChernykh/p2pool/releases/download/v4.16/p2pool-v4.16-linux-x64.tar.gz && \
    echo "1b03b2e4d4adfe488b2867eb85b57366fbaf8594a7f84cdbf13a3c8519159280  p2pool-v4.16-linux-x64.tar.gz" > /tmp/p2pool.sha256 && \
    sha256sum -c /tmp/p2pool.sha256 && \
    tar xf p2pool-v4.16-linux-x64.tar.gz && \
    cp p2pool-v4.16-linux-x64/p2pool /usr/local/bin/p2pool && \
    rm -rf p2pool-v4.16* /tmp/p2pool.sha256

# xmrig v6.22.2 — static RandomX miner (glibc-safe)
RUN wget -q https://github.com/xmrig/xmrig/releases/download/v6.22.2/xmrig-6.22.2-linux-static-x64.tar.gz && \
    tar xf xmrig-6.22.2-linux-static-x64.tar.gz && \
    cp xmrig-6.22.2/xmrig /usr/local/bin/xmrig && \
    rm -rf xmrig-6.22.2*

# ============================================================================
# Stage 3: Runtime
# ============================================================================
FROM darkwow-base:24.04

WORKDIR /app

COPY --from=builder /app/dwowd /app/dwowd
COPY --from=builder /app/lilith /app/lilith
COPY --from=builder /app/lilith_config.toml /app/lilith_config.toml
COPY --from=downloader /usr/local/bin/monerod /usr/local/bin/monerod
COPY --from=downloader /usr/local/bin/monero-wallet-cli /usr/local/bin/monero-wallet-cli
COPY --from=downloader /usr/local/bin/p2pool /usr/local/bin/p2pool
COPY --from=downloader /usr/local/bin/xmrig /usr/local/bin/xmrig

COPY contrib/docker/testnet-node/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENV RUST_MIN_STACK=67108864

ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/dwowd"]
