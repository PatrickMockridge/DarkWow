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

# BUILD_COMMIT: mandatory for deterministic builds from origin.
ARG BUILD_COMMIT
RUN test -n "$BUILD_COMMIT" || (echo "FATAL: BUILD_COMMIT is required" && exit 1)

ARG CARGO_BUILD_JOBS=1
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
ARG RAYON_NUM_THREADS=2
ENV RAYON_NUM_THREADS=${RAYON_NUM_THREADS}
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV RUST_MIN_STACK=67108864

WORKDIR /app

# The contract Makefiles expand $(HOME) for the remap flags and the recipe check
# canonicalizes it to the literal HOME. Pin it so the image build is a function of
# the source, not of the base image's default home.
ENV HOME=/root

# Source: clone from origin for deterministic, reproducible builds.
RUN git clone --single-branch --branch linear-master \
        https://github.com/PatrickMockridge/DarkWow.git /app && \
    cd /app && git reset --hard "$BUILD_COMMIT" && \
    ACTUAL=$(git rev-parse HEAD) && \
    if [ "$BUILD_COMMIT" != "$ACTUAL" ]; then \
        echo "FATAL: commit mismatch after clone! Expected $BUILD_COMMIT got $ACTUAL"; exit 1; \
    fi && \
    echo "Commit verified at clone: $ACTUAL"

# Build zkas from the pinned source and materialize the gitignored ./zkas binary that
# the per-contract Makefiles resolve (`ZKAS = $(abspath ../../../zkas)`).
RUN make -C /app zkas && echo "build: $BUILD_COMMIT"

# Install wasm32 target for WASM contract compilation
RUN rustup target add wasm32-unknown-unknown

# Reproducible contract builds: strip host paths out of the WASM.
#
# rustc embeds the absolute path of every crate it compiles into panic
# locations, and those paths are readable straight out of the artifact:
#
#     strings -a dwow_*_contract.wasm | grep /root
#
# They depend on the checkout path, CARGO_HOME and the rustc toolchain
# directory name — none of which are properties of the source. Genesis embeds
# these WASM bytes (bin/dwowd/src/lib.rs `build_genesis_deployment_txs`), so an
# unremapped path here changes the genesis hash, and the pin in
# bin/dwowd/genesis_hash.txt could then be satisfied inside this image *or* on a
# developer machine, never both.
#
# The contract build below does NOT use this variable: each contract Makefile
# invokes cargo with WASM_RUSTFLAGS from script/contract-build-recipe.mk, which
# carries the remaps, -C link-arg=--max-memory=268435456, -C strip=symbols and
# -Zlocation-detail=none for the wasm target. This ENV is what the host-binary
# builds further down this stage use; that is deliberate and inert: the remap only
# rewrites panic-location path strings, and nothing in the workspace hashes
# `file!()` or `module_path!()`.
ENV RUSTFLAGS="--remap-path-prefix=/root/.cargo=/cargo --remap-path-prefix=/root/.rustup=/rustup"

# Build ALL contract WASMs through the shared recipe, exactly as the host does.
# The per-contract Makefiles apply the recipe to every package, regenerate every
# .zk.bin, copy the WASM into src/contract/<name>/ for include_bytes!, and write
# .source_hash. There is no second build recipe here.
#
# Delete artifacts that arrived with the clone first: 7 WASM files are tracked and
# the nine genesis artifacts are untracked, so git-checkout mtimes cannot be trusted
# to force a rebuild. Deleting makes the image a function of the source.
RUN find /app/src/contract -name '*.wasm' -delete && \
    find /app/src/contract -name '*.zk.bin' -delete && \
    make -C /app contracts && \
    echo "build: $BUILD_COMMIT"

# Artifact gates that must pass on the artifacts this image will execute:
#   - every exported memory declares the recipe's maximum (4096 pages);
#   - the nine genesis artifacts satisfy the artifact-purity invariant;
#   - every artifact's .source_hash matches its sources and the build recipe.
RUN bash /app/contrib/wasm_artifact_check.sh --memory-max && \
    bash /app/contrib/wasm_artifact_check.sh --genesis && \
    bash /app/scripts/check-artifact-freshness.sh && \
    echo "artifact gates: $BUILD_COMMIT"

# Build dwowd — single RUN layer
RUN cargo build --locked -p dwowd --release --features fee-window -j ${CARGO_BUILD_JOBS} && \
    cp target/release/dwowd /app/dwowd && \
    echo "build: $BUILD_COMMIT"

# Build the dwowd lib test binary (--no-run: compile all #[test] fns, run none).
# Needed by the opt-in CAPABILITY_TESTS pipeline phase (L1 capability write-path).
# The test binary embeds the contract WASM/zkas via include_bytes!, which the
# contract build above has already placed under /app/src/contract/<name>/.
# Capture the harness executable path DETERMINISTICALLY from cargo's JSON output.
# The old `find target/release/deps -name 'dwowd-*'` also matched the daemon binary
# (cargo build -p dwowd also emits a dwowd-<hash> executable) and the last `cp` won
# non-deterministically — installing the clap daemon as dwowd_lib_tests.
RUN cargo test --locked --release -p dwowd --lib --no-run --features fee-window -j ${CARGO_BUILD_JOBS} \
      --message-format=json-render-diagnostics 2>/dev/null \
    | grep -o '"executable":"[^"]*dwowd-[^"]*"' | head -1 \
    | sed 's/"executable":"\([^"]*\)"/\1/' > /tmp/dwowd_test_bin_path && \
    cp "$(cat /tmp/dwowd_test_bin_path)" /app/dwowd_lib_tests && \
    echo "build-tests: $BUILD_COMMIT"

# Build the darkwow top-level launcher (node/wallet/account front-end)
RUN cargo build --locked -p darkwow --release -j ${CARGO_BUILD_JOBS} && \
    cp target/release/darkwow /app/darkwow && \
    echo "build: $BUILD_COMMIT"

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM darkwow-base:24.04

ENV RUST_MIN_STACK=67108864

WORKDIR /app

COPY --from=builder /app/dwowd ./
COPY --from=builder /app/darkwow ./
COPY --from=builder /app/dwowd_lib_tests ./
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
