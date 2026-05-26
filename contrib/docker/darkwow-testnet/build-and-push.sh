#!/bin/bash
# Build and optionally push the DarkWow Testnet Docker image.
#
# Usage:
#   # Build only
#   ./contrib/docker/darkwow-testnet/build-and-push.sh
#
#   # Build and push to a registry
#   REGISTRY=docker.io/myuser/ IMAGE_NAME=darkwow-testnet ./contrib/docker/darkwow-testnet/build-and-push.sh
#
#   # Custom tag
#   TAG=experimental ./contrib/docker/darkwow-testnet/build-and-push.sh

set -e

# cd to repo root
cd "$(dirname "$0")/../../.."

IMAGE_NAME="${IMAGE_NAME:-darkwow-testnet}"
REGISTRY="${REGISTRY:-}"
TAG="${TAG:-latest}"
VERSION_TAG="${VERSION_TAG:-$(git describe --tags --always --dirty 2>/dev/null || echo "0.5.0")}"

# Map image name to Dockerfile
case "$IMAGE_NAME" in
    darkwow-wallet)
        DOCKERFILE="contrib/docker/darkwow-testnet/Dockerfile.wallet"
        ;;
    *)
        DOCKERFILE="contrib/docker/darkwow-testnet/Dockerfile"
        ;;
esac

echo "=== DarkWow Docker Build ==="
echo "  Image:   ${REGISTRY}${IMAGE_NAME}:${TAG}"
echo "  Version: ${REGISTRY}${IMAGE_NAME}:${VERSION_TAG}"
echo "  File:    ${DOCKERFILE}"
echo

docker build \
    -t "${REGISTRY}${IMAGE_NAME}:${TAG}" \
    -t "${REGISTRY}${IMAGE_NAME}:${VERSION_TAG}" \
    -f "$DOCKERFILE" \
    .

echo
echo "=== Build complete ==="
echo "  ${REGISTRY}${IMAGE_NAME}:${TAG}"
echo "  ${REGISTRY}${IMAGE_NAME}:${VERSION_TAG}"

if [ -n "$REGISTRY" ]; then
    echo
    echo "=== Pushing to registry ==="
    docker push "${REGISTRY}${IMAGE_NAME}:${TAG}"
    docker push "${REGISTRY}${IMAGE_NAME}:${VERSION_TAG}"
    echo "=== Push complete ==="
    echo
    echo "Pull on other machines:"
    echo "  docker pull ${REGISTRY}${IMAGE_NAME}:${TAG}"
fi
