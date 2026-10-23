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
FROM darkwow-base:24.04 AS builder

ARG CARGO_BUILD_JOBS=2
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
ARG RAYON_NUM_THREADS=2
ENV RAYON_NUM_THREADS=${RAYON_NUM_THREADS}
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /app

# The contract Makefiles expand $(HOME); pin it so the image is a function of source.
ENV HOME=/root

COPY . /app

# Build zkas from source and materialize the ./zkas binary the contract Makefiles
# resolve. Remove any host-built binary that came with the copy context first.
RUN rm -f /app/zkas && make -C /app zkas

RUN rustup target add wasm32-unknown-unknown

# Build every contract through the shared recipe, exactly as the host does. Delete
# artifacts that arrived with the copy context so the image never ships a stale one.
RUN find /app/src/contract -name '*.wasm' -delete && \
    find /app/src/contract -name '*.zk.bin' -delete && \
    make -C /app contracts

# The artifact invariants: bounded memory, genesis purity, recipe/source attestation.
RUN bash /app/contrib/wasm_artifact_check.sh --memory-max && \
    bash /app/contrib/wasm_artifact_check.sh --genesis && \
    bash /app/scripts/check-artifact-freshness.sh

# Build dwowd (fullnode daemon) with reduced parallelism
RUN cargo build -p dwowd --release -j ${CARGO_BUILD_JOBS}

# Build lilith (P2P seed node)
RUN cargo build -p lilith --release -j ${CARGO_BUILD_JOBS}

RUN cp target/release/dwowd /app/dwowd && \
    cp target/release/lilith /app/lilith && \
    cp bin/lilith/lilith_config.toml /app/lilith_config.toml

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM darkwow-base:24.04

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
