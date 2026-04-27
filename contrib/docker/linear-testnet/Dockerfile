# DarkFi Linear-Testnet Docker Image
#
# Multi-stage build: compiles darkfid inside Docker
# Build from repo root:
#   docker build -t darkfi:linear-testnet . -f contrib/docker/linear-testnet/Dockerfile

# ============================================================================
# Stage: Builder
# ============================================================================
FROM ubuntu:24.04 AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential gcc g++ make cmake curl git \
    libssl-dev pkg-config tcl-dev clang libsqlite3-dev libasound2-dev libsqlcipher-dev

# Install Rust with default toolchain
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y \
    --default-toolchain nightly-2025-04-10

ENV PATH="/root/.cargo/bin:${PATH}"
ENV LIBCLANG_PATH="/usr/lib/x86_64-linux-gnu"
ENV RUST_MIN_STACK=67108864
ENV CARGO_BUILD_JOBS=2
# Use git CLI for fetching - avoids SSL issues with cargo's built-in git
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /app

# Copy entire repo (from build context at repo root)
# .dockerignore excludes .git so cargo uses cached deps
COPY . ./

# Build darkfid with reduced parallelism to avoid memory issues
RUN cargo build -p darkfid --release -j 2

RUN cp target/release/darkfid /app/darkfid

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    libssl3 libsqlite3-0 liblzma5 libstdc++6 curl bash

WORKDIR /app

COPY --from=builder /app/darkfid ./
COPY contrib/docker/linear-testnet/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# Entrypoint handles config generation, CMD provides default binary args
ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/darkfid"]