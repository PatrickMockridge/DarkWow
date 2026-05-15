#!/bin/bash
# Build and optionally push the DarkWow Public Testnet Node image.
#
# Produces a single image with all binaries needed to join the public
# DarkWow testnet as a mining node. Two runtime modes:
#   MODE=native — solo RandomX mining (dwowd + xmrig)
#   MODE=merge  — Monero merge mining (monerod + dwowd + p2pool + xmrig)
#
# Usage:
#   # Build only
#   ./contrib/docker/testnet-node/build-and-push.sh
#
#   # Build and push to Docker Hub
#   REGISTRY=docker.io/darkwow-node/ ./contrib/docker/testnet-node/build-and-push.sh
#
#   # Custom version
#   VERSION=0.2.0 ./contrib/docker/testnet-node/build-and-push.sh

set -e

cd "$(dirname "$0")/../../.."

IMAGE_NAME="${IMAGE_NAME:-darkwow-node/testnet}"
REGISTRY="${REGISTRY:-}"
VERSION="${VERSION:-0.1.0}"
GIT_SHA=$(git rev-parse --short=8 HEAD 2>/dev/null || echo "unknown")
DOCKERFILE="contrib/docker/testnet-node/Dockerfile"

FULL_IMAGE="${REGISTRY}${IMAGE_NAME}"

echo "=== DarkWow Public Testnet Node Build ==="
echo "  Image:     ${FULL_IMAGE}"
echo "  Version:   ${VERSION}"
echo "  Git SHA:   ${GIT_SHA}"
echo "  Registry:  ${REGISTRY:-<local only>}"
echo "  Dockerfile: ${DOCKERFILE}"
echo

docker build \
    -t "${FULL_IMAGE}:latest" \
    -t "${FULL_IMAGE}:${VERSION}" \
    -t "${FULL_IMAGE}:${GIT_SHA}" \
    -f "$DOCKERFILE" \
    .

echo
echo "=== Build complete ==="
echo "  ${FULL_IMAGE}:latest"
echo "  ${FULL_IMAGE}:${VERSION}"
echo "  ${FULL_IMAGE}:${GIT_SHA}"

if [ -n "$REGISTRY" ]; then
    echo
    echo "=== Pushing to registry ==="
    docker push "${FULL_IMAGE}:latest"
    docker push "${FULL_IMAGE}:${VERSION}"
    docker push "${FULL_IMAGE}:${GIT_SHA}"
    echo "=== Push complete ==="
    echo
    echo "To pull and run on another machine:"
    echo "  docker pull ${FULL_IMAGE}:latest"
    echo
    echo "  # Native mining"
    echo "  docker run --network=host \\"
    echo "    -e MODE=native \\"
    echo "    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \\"
    echo "    -v /path/to/mining_secret:/run/secrets/mining_secret:ro \\"
    echo "    -v /path/to/data:/root/.local/share/dwow/dwowd \\"
    echo "    ${FULL_IMAGE}:latest"
    echo
    echo "  # Merge mining"
    echo "  docker run --network=host \\"
    echo "    -e MODE=merge \\"
    echo "    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \\"
    echo "    -v /path/to/mining_secret:/run/secrets/mining_secret:ro \\"
    echo "    -v /path/to/data:/root/.local/share/dwow/dwowd \\"
    echo "    ${FULL_IMAGE}:latest"
fi
