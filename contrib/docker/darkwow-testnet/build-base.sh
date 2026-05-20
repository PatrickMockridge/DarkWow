#!/bin/bash
# Build and optionally push the DarkWow base image.
#
# The base image contains pre-installed apt packages + Rust toolchain so
# the per-commit Docker build doesn't reinstall them every time.
#
# Usage:
#   # Build only (for local use)
#   ./contrib/docker/darkwow-testnet/build-base.sh
#
#   # Build and push to registry
#   REGISTRY=codeberg.org/darkrenaissance/darkfi/ ./contrib/docker/darkwow-testnet/build-base.sh
#
#   # Custom tag
#   TAG=24.04 ./contrib/docker/darkwow-testnet/build-base.sh

set -e

cd "$(dirname "$0")/../../.."

IMAGE_NAME="${IMAGE_NAME:-darkwow-base}"
REGISTRY="${REGISTRY:-}"
TAG="${TAG:-24.04}"
DOCKERFILE="contrib/docker/darkwow-testnet/Dockerfile.base"

echo "=== DarkWow Base Image Build ==="
echo "  Image:   ${REGISTRY}${IMAGE_NAME}:${TAG}"
echo "  File:    ${DOCKERFILE}"
echo

docker build \
    -t "${REGISTRY}${IMAGE_NAME}:${TAG}" \
    -f "$DOCKERFILE" \
    .

echo
echo "=== Build complete ==="
echo "  ${REGISTRY}${IMAGE_NAME}:${TAG}"

if [ -n "$REGISTRY" ]; then
    echo
    echo "=== Pushing to registry ==="
    docker push "${REGISTRY}${IMAGE_NAME}:${TAG}"
    echo "=== Push complete ==="
    echo
    echo "Pull on other machines:"
    echo "  docker pull ${REGISTRY}${IMAGE_NAME}:${TAG}"
fi
