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

# Build ALL WASM contracts from source (deterministic — git clone ensures clean state).
# Each contract that compiles to WASM must be listed here so compilation failures
# are caught at build time, not silently skipped.
RUN cargo build --target wasm32-unknown-unknown --release -p dwow_deployooor_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_native_token_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_money_v3_contract && \
    cargo build --target wasm32-unknown-unknown --release -p dwow_baccarat_contract && \
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
    cargo build --target wasm32-unknown-unknown --release -p dwow_tender_contract

# Copy WASM files to contract directories for include_bytes! macro.
# Only the contracts referenced by include_bytes! in dwowd/src/lib.rs are
# strictly required; the rest are built above for smoke-test coverage.
RUN cp target/wasm32-unknown-unknown/release/dwow_deployooor_contract.wasm /app/src/contract/deployooor/ && \
    cp target/wasm32-unknown-unknown/release/dwow_native_token_contract.wasm /app/src/contract/native_token/ && \
    cp target/wasm32-unknown-unknown/release/dwow_money_v3_contract.wasm /app/src/contract/money_v3/ && \
    cp target/wasm32-unknown-unknown/release/dwow_baccarat_contract.wasm /app/src/contract/baccarat/ && \
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

# Build dwowd with reduced parallelism to avoid memory issues
RUN cargo build -p dwowd --release -j 2

# Build lilith seed node
RUN cargo build -p lilith --release -j 2

RUN cp target/release/dwowd /app/dwowd && \
    cp target/release/lilith /app/lilith && \
    cp bin/lilith/lilith_config.toml /app/lilith_config.toml

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM darkwow-base:24.04

WORKDIR /app

COPY --from=builder /app/dwowd ./
COPY --from=builder /app/lilith ./
COPY --from=builder /app/lilith_config.toml ./
COPY --from=builder /app/contrib/docker/darkwow-testnet/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# Entrypoint handles config generation, CMD provides default binary args
ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/dwowd"]
