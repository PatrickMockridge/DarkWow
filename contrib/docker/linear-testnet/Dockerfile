# DarkFi Linear-Testnet Docker Image
#
# This image contains the darkfid binary for linear-testnet operation.
# It can be built in two modes:
#
# 1. Pre-built binary (default): Build darkfid on host, copy binary
#    docker build . -t darkfi:linear-testnet
#
# 2. Build from source: Pass --build-arg BUILD_FROM_SOURCE=1
#    docker build . -t darkfi:linear-testnet --build-arg BUILD_FROM_SOURCE=1

ARG ALPINE_VER=3.18

# Stage 1: Builder (only used when BUILD_FROM_SOURCE=1)
FROM alpine:${ALPINE_VER} as builder
RUN apk add --no-cache cmake jq wget clang curl gcc make llvm-dev \
  openssl-dev expat-dev freetype-dev libudev-zero-dev \
  libgudev-dev pkgconf clang-dev fontconfig-dev bash \
  ttf-opensans musl-dev alsa-lib-dev python3-dev perl sqlite-dev curl

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y --default-toolchain nightly-2025-04-10 --default-host x86_64-unknown-linux-musl
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup target add wasm32-unknown-unknown

WORKDIR /opt/darkfi
COPY . ./
RUN cargo build -p darkfid --release
RUN cp target/release/darkfid .

# Stage 2: Runtime
FROM alpine:${ALPINE_VER}
RUN apk add --no-cache openssl libgcc alsa-lib sqlite-libs ttf-opensans curl

WORKDIR /opt/darkfi

# Default: expect pre-built binary in build context
COPY --from=builder /opt/darkfi/darkfid ./

# For build from source mode, use multi-stage
ARG BUILD_FROM_SOURCE=0
FROM builder as source-builder
FROM alpine:${ALPINE_VER}
RUN apk add --no-cache openssl libgcc alsa-lib sqlite-libs ttf-opensans curl
WORKDIR /opt/darkfi
COPY --from=source-builder /opt/darkfi/darkfid ./

# Entry point
ENTRYPOINT ["./darkfid"]
CMD ["--help"]