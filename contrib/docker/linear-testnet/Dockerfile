# DarkFi Linear-Testnet Docker Image
#
# This image builds darkfid from source inside Docker for Debian
#
# Usage:
#   Build from source:
#     docker build -t darkfi:linear-testnet . -f contrib/docker/linear-testnet/Dockerfile --build-arg BUILD_FROM_SOURCE=1
#   (must be run from repo root, not from linear-testnet directory)

# ============================================================================
# Stage: Builder
# ============================================================================
FROM debian:bookworm-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential gcc g++ make cmake curl git \
    libssl-dev pkg-config tcl-dev clang libsqlite3-dev

# Install Rust with default toolchain
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y \
    --default-toolchain nightly-2025-04-10

ENV PATH="/root/.cargo/bin:${PATH}"
ENV LIBCLANG_PATH="/usr/lib/x86_64-linux-gnu"
ENV RUST_MIN_STACK=16777216

WORKDIR /app

# Copy entire repo (from build context at repo root)
COPY . ./

# Build darkfid
RUN cargo build -p darkfid --release

RUN cp target/release/darkfid /app/darkfid

# ============================================================================
# Stage: Runtime
# ============================================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl3 libsqlite3-0 liblzma5 libstdc++6 fonts-dejavu curl

WORKDIR /app

COPY --from=builder /app/darkfid ./

ENTRYPOINT ["./darkfid"]
CMD ["--help"]