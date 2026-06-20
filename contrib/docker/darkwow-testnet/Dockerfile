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

# BUILD_COMMIT: mandatory for deterministic builds. Passed via --build-arg.
# The build script resolves this from git rev-parse HEAD automatically.
ARG BUILD_COMMIT
RUN test -n "$BUILD_COMMIT" || (echo "FATAL: BUILD_COMMIT is required" && exit 1)

ENV CARGO_BUILD_JOBS=2
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV RUST_MIN_STACK=67108864

WORKDIR /app

# Pre-verify commit exists on remote via lightweight ls-remote.
# Exponential backoff handles transient rate limiting (Codeberg: ~250 req/30min).
RUN echo "Verifying commit $BUILD_COMMIT exists on remote..." && \
    for i in 1 2 3 4 5; do \
        if git ls-remote https://codeberg.org/PatrickM123/darkwow.git "refs/heads/linear-master" \
            | grep -q "$BUILD_COMMIT"; then \
            echo "Commit $BUILD_COMMIT confirmed on remote (attempt $i)"; break; \
        fi; \
        DELAY=$(( (1 << i) * 5 )); \
        echo "Rate-limited or commit not yet visible — retrying in ${DELAY}s (attempt $i/5)"; \
        sleep "$DELAY"; \
    done && \
    if ! git ls-remote https://codeberg.org/PatrickM123/darkwow.git "refs/heads/linear-master" \
        | grep -q "$BUILD_COMMIT"; then \
        echo "FATAL: $BUILD_COMMIT not found on remote after 5 attempts"; exit 1; \
    fi

# Clone from origin and checkout the exact commit.
RUN git clone --depth 100 --single-branch --branch linear-master \
        https://codeberg.org/PatrickM123/darkwow.git /app && \
    cd /app && git reset --hard "$BUILD_COMMIT" && \
    ACTUAL=$(git rev-parse HEAD) && \
    if [ "$BUILD_COMMIT" != "$ACTUAL" ]; then \
        echo "FATAL: commit mismatch after clone! Expected $BUILD_COMMIT got $ACTUAL"; exit 1; \
    fi && \
    echo "Commit verified at clone: $ACTUAL"

# Build zkas tool for host (needed to compile .zk source to .zk.bin)
RUN cargo build --release -p zkas && echo "build: $BUILD_COMMIT"

# Rebuild .zk.bin files from .zk sources (needed by contracts' include_bytes! macro).
# Every contract that uses include_bytes! must have its proof directory rebuilt here.
RUN /app/target/release/zkas rebuild /app/src/contract/native_token/proof && \
    /app/target/release/zkas rebuild /app/src/contract/promissory_note/proof && \
    /app/target/release/zkas rebuild /app/src/contract/purse/proof && \
    /app/target/release/zkas rebuild /app/src/contract/box/proof && \
    /app/target/release/zkas rebuild /app/src/contract/baccarat/proof && \
    /app/target/release/zkas rebuild /app/src/contract/bearer_bond/proof && \
    /app/target/release/zkas rebuild /app/src/contract/bridge/proof && \
    /app/target/release/zkas rebuild /app/src/contract/dao_escrow/proof && \
    /app/target/release/zkas rebuild /app/src/contract/dex/proof && \
    /app/target/release/zkas rebuild /app/src/contract/stablecoin/proof && \
    /app/target/release/zkas rebuild /app/src/contract/attestation/proof && \
    /app/target/release/zkas rebuild /app/src/contract/auction/proof && \
    /app/target/release/zkas rebuild /app/src/contract/betting_stake/proof && \
    /app/target/release/zkas rebuild /app/src/contract/darkbet_exchange/proof && \
    /app/target/release/zkas rebuild /app/src/contract/darktoshi_dice/proof && \
    /app/target/release/zkas rebuild /app/src/contract/drain_protection/proof && \
    /app/target/release/zkas rebuild /app/src/contract/escrow/proof && \
    /app/target/release/zkas rebuild /app/src/contract/game_room/proof && \
    /app/target/release/zkas rebuild /app/src/contract/identity/proof && \
    /app/target/release/zkas rebuild /app/src/contract/insurance_market/proof && \
    /app/target/release/zkas rebuild /app/src/contract/labor_market/proof && \
    /app/target/release/zkas rebuild /app/src/contract/lottery/proof && \
    /app/target/release/zkas rebuild /app/src/contract/oracle/proof && \
    /app/target/release/zkas rebuild /app/src/contract/otc_swap/proof && \
    /app/target/release/zkas rebuild /app/src/contract/pool_stake/proof && \
    /app/target/release/zkas rebuild /app/src/contract/relayer_endowment/proof && \
    /app/target/release/zkas rebuild /app/src/contract/roulette/proof && \
    /app/target/release/zkas rebuild /app/src/contract/slot/proof && \
    /app/target/release/zkas rebuild /app/src/contract/subscription/proof && \
    /app/target/release/zkas rebuild /app/src/contract/tender/proof && \
    echo "build: $BUILD_COMMIT"

# Install wasm32 target for WASM contract compilation
RUN rustup target add wasm32-unknown-unknown

# Build ALL WASM contracts from source (deterministic — git clone ensures clean state).
# Each contract that compiles to WASM must be listed here so compilation failures
# are caught at build time, not silently skipped.
RUN cargo build --target wasm32-unknown-unknown --release -p dwow_deployooor_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_native_token_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_promissory_note_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_purse_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_box_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_baccarat_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_bearer_bond_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_bridge_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_dao_escrow_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_dex_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_stablecoin_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_attestation_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_auction_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_betting_stake_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_darkbet_exchange_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_darktoshi_dice_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_drain_protection_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_escrow_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_game_room_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_identity_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_insurance_market_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_labor_market_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_lottery_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_oracle_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_otc_swap_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_pool_stake_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_relayer_endowment_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_roulette_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_slot_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_subscription_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_tender_contract && \
    echo "build: $BUILD_COMMIT"

# Copy WASM files to contract directories for include_bytes! macro.
# Only the contracts referenced by include_bytes! in dwowd/src/lib.rs are
# strictly required; the rest are built above for smoke-test coverage.
RUN cp target/wasm32-unknown-unknown/release/dwow_deployooor_contract.wasm /app/src/contract/deployooor/ && \
    cp target/wasm32-unknown-unknown/release/dwow_native_token_contract.wasm /app/src/contract/native_token/ && \
    cp target/wasm32-unknown-unknown/release/dwow_promissory_note_contract.wasm /app/src/contract/promissory_note/ && \
    cp target/wasm32-unknown-unknown/release/dwow_purse_contract.wasm /app/src/contract/purse/ && \
    cp target/wasm32-unknown-unknown/release/dwow_box_contract.wasm /app/src/contract/box/ && \
    cp target/wasm32-unknown-unknown/release/dwow_baccarat_contract.wasm /app/src/contract/baccarat/ && \
    cp target/wasm32-unknown-unknown/release/dwow_bearer_bond_contract.wasm /app/src/contract/bearer_bond/ && \
    cp target/wasm32-unknown-unknown/release/dwow_bridge_contract.wasm /app/src/contract/bridge/ && \
    cp target/wasm32-unknown-unknown/release/dwow_dao_escrow_contract.wasm /app/src/contract/dao_escrow/ && \
    cp target/wasm32-unknown-unknown/release/dwow_dex_contract.wasm /app/src/contract/dex/ && \
    cp target/wasm32-unknown-unknown/release/dwow_stablecoin_contract.wasm /app/src/contract/stablecoin/ && \
    cp target/wasm32-unknown-unknown/release/dwow_attestation_contract.wasm /app/src/contract/attestation/ && \
    cp target/wasm32-unknown-unknown/release/dwow_auction_contract.wasm /app/src/contract/auction/ && \
    cp target/wasm32-unknown-unknown/release/dwow_betting_stake_contract.wasm /app/src/contract/betting_stake/ && \
    cp target/wasm32-unknown-unknown/release/dwow_darkbet_exchange_contract.wasm /app/src/contract/darkbet_exchange/ && \
    cp target/wasm32-unknown-unknown/release/dwow_darktoshi_dice_contract.wasm /app/src/contract/darktoshi_dice/ && \
    cp target/wasm32-unknown-unknown/release/dwow_drain_protection_contract.wasm /app/src/contract/drain_protection/ && \
    cp target/wasm32-unknown-unknown/release/dwow_escrow_contract.wasm /app/src/contract/escrow/ && \
    cp target/wasm32-unknown-unknown/release/dwow_game_room_contract.wasm /app/src/contract/game_room/ && \
    cp target/wasm32-unknown-unknown/release/dwow_identity_contract.wasm /app/src/contract/identity/ && \
    cp target/wasm32-unknown-unknown/release/dwow_insurance_market_contract.wasm /app/src/contract/insurance_market/ && \
    cp target/wasm32-unknown-unknown/release/dwow_labor_market_contract.wasm /app/src/contract/labor_market/ && \
    cp target/wasm32-unknown-unknown/release/dwow_lottery_contract.wasm /app/src/contract/lottery/ && \
    cp target/wasm32-unknown-unknown/release/dwow_oracle_contract.wasm /app/src/contract/oracle/ && \
    cp target/wasm32-unknown-unknown/release/dwow_otc_swap_contract.wasm /app/src/contract/otc_swap/ && \
    cp target/wasm32-unknown-unknown/release/dwow_pool_stake_contract.wasm /app/src/contract/pool_stake/ && \
    cp target/wasm32-unknown-unknown/release/dwow_relayer_endowment_contract.wasm /app/src/contract/relayer_endowment/ && \
    cp target/wasm32-unknown-unknown/release/dwow_roulette_contract.wasm /app/src/contract/roulette/ && \
    cp target/wasm32-unknown-unknown/release/dwow_slot_contract.wasm /app/src/contract/slot/ && \
    cp target/wasm32-unknown-unknown/release/dwow_subscription_contract.wasm /app/src/contract/subscription/ && \
    cp target/wasm32-unknown-unknown/release/dwow_tender_contract.wasm /app/src/contract/tender/

# Build dwowd + lilith together — shared dependency tree, single RUN layer
RUN cargo build -p dwowd -p lilith --release -j 2 && \
    cp target/release/dwowd /app/dwowd && \
    cp target/release/lilith /app/lilith && \
    cp bin/lilith/lilith_config.toml /app/lilith_config.toml && \
    ACTUAL=$(/app/dwowd --version 2>&1 | grep -oP 'commit: \K[a-f0-9]+' | head -1) && \
    if [ "$BUILD_COMMIT" != "$ACTUAL" ]; then \
        echo "FATAL: dwowd commit mismatch! Expected $BUILD_COMMIT, got $ACTUAL"; exit 1; \
    fi && \
    echo "dwowd commit verified: $BUILD_COMMIT" && \
    echo "build: $BUILD_COMMIT"

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM darkwow-base:24.04

ENV RUST_MIN_STACK=67108864

WORKDIR /app

COPY --from=builder /app/dwowd ./
COPY --from=builder /app/lilith ./
COPY --from=builder /app/lilith_config.toml ./
COPY --from=builder /app/contrib/docker/darkwow-testnet/entrypoint.sh /app/entrypoint.sh
# monero-wallet-cli for container-native Monero wallet generation
# Same pinned release as Dockerfile.monero — sha256 verified
ARG MONERO_VERSION=v0.18.5.0
RUN curl -sL "https://downloads.getmonero.org/cli/monero-linux-x64-${MONERO_VERSION}.tar.bz2" -o /tmp/monero.tar.bz2 && \
    echo "166ad93036f95f5abeba24c8670061be022c9238dba2e6a7587611a1d759e294  /tmp/monero.tar.bz2" > /tmp/monero.sha256 && \
    sha256sum -c /tmp/monero.sha256 && \
    mkdir -p /tmp/monero-extract && \
    tar -xjf /tmp/monero.tar.bz2 --strip-components=1 -C /tmp/monero-extract/ && \
    cp /tmp/monero-extract/monero-wallet-cli /usr/local/bin/ && \
    chmod +x /usr/local/bin/monero-wallet-cli && \
    rm -rf /tmp/monero.tar.bz2 /tmp/monero.sha256 /tmp/monero-extract

RUN chmod +x /app/entrypoint.sh

# Download p2pool binary for merge mining sidecar (checksum-verified)
# v4.16 patches a network-layer exploit. Keep pinned and verified.
ARG P2POOL_VERSION=v4.16
RUN curl -sL "https://github.com/SChernykh/p2pool/releases/download/${P2POOL_VERSION}/p2pool-${P2POOL_VERSION}-linux-x64.tar.gz" -o /tmp/p2pool.tar.gz && \
    echo "1b03b2e4d4adfe488b2867eb85b57366fbaf8594a7f84cdbf13a3c8519159280  /tmp/p2pool.tar.gz" > /tmp/p2pool.sha256 && \
    sha256sum -c /tmp/p2pool.sha256 && \
    tar -xzf /tmp/p2pool.tar.gz --strip-components=1 -C /usr/local/bin/ && \
    rm /tmp/p2pool.tar.gz /tmp/p2pool.sha256 && \
    chmod +x /usr/local/bin/p2pool

# Entrypoint handles config generation, CMD provides default binary args
ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/dwowd"]
