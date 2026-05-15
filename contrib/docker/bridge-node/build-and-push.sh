#!/bin/bash
# Build and optionally push the DarkWow Bridge Node image.
#
# Produces a single image with dwowd, universal_relayer, dww, and all
# bridge-related contracts. Three runtime modes:
#   MODE=full         — dwowd + contracts + universal_relayer
#   MODE=relayer-only — universal_relayer only (external dwowd)
#   MODE=lilith       — standalone P2P seed node
#
# Usage:
#   # Build only
#   ./contrib/docker/bridge-node/build-and-push.sh
#
#   # Build and push to Docker Hub
#   REGISTRY=docker.io/darkwow-node/ ./contrib/docker/bridge-node/build-and-push.sh
#
#   # Custom version
#   VERSION=0.2.0 ./contrib/docker/bridge-node/build-and-push.sh

set -e

cd "$(dirname "$0")/../../.."

IMAGE_NAME="${IMAGE_NAME:-darkwow-node/bridge}"
REGISTRY="${REGISTRY:-}"
VERSION="${VERSION:-0.1.0}"
GIT_SHA=$(git rev-parse --short=8 HEAD 2>/dev/null || echo "unknown")
DOCKERFILE="contrib/docker/bridge-node/Dockerfile"

FULL_IMAGE="${REGISTRY}${IMAGE_NAME}"

echo "=== DarkWow Bridge Node Build ==="
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
    echo "  # Full bridge node"
    echo "  docker run --network=host \\"
    echo "    -e MODE=full \\"
    echo "    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \\"
    echo "    -v /path/to/mining_secret:/run/secrets/mining_secret:ro \\"
    echo "    -v /path/to/data:/root/.local/share/dwow/dwowd \\"
    echo "    ${FULL_IMAGE}:latest"
    echo
    echo "  # Relayer-only (external dwowd)"
    echo "  docker run --network=host \\"
    echo "    -e MODE=relayer-only \\"
    echo "    -e DARKFID_URL=tcp://127.0.0.1:31345 \\"
    echo "    ${FULL_IMAGE}:latest"
fi
