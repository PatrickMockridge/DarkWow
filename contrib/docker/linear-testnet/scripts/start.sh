#!/bin/bash
# Start the 5-node linear-testnet Docker stack

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== DarkFi Linear-Testnet 5-Node Stack ==="
echo ""

# Check if docker is running
if ! docker info > /dev/null 2>&1; then
    echo "ERROR: Docker is not running or not accessible."
    exit 1
fi

# Check for darkfid binary (needed for Dockerfile-based build)
if [ ! -f "../../target/release/darkfid" ] && [ ! -f "../../target/debug/darkfid" ]; then
    echo "WARNING: darkfid binary not found in ../../target/release/debug"
    echo "You may need to build it first: cargo build -p darkfid"
fi

# Create volume network if it doesn't exist
docker network create darkfi-local 2>/dev/null || true

# Start the stack
echo "Starting darkfid nodes and xmrig miners..."
docker-compose up -d

echo ""
echo "=== Stack Status ==="
docker-compose ps

echo ""
echo "=== Node RPC Endpoints ==="
echo "Node0 RPC: http://localhost:28345"
echo "Node1 RPC: http://localhost:28346"
echo "Node2 RPC: http://localhost:28347"
echo "Node3 RPC: http://localhost:28348"
echo "Node4 RPC: http://localhost:28349"

echo ""
echo "=== Mining Management ==="
echo "To check logs: ./scripts/logs.sh [node0|node1|...]"
echo "To mine a block: ./scripts/mine.sh [node_index] [reward]"
echo "To stop: ./scripts/stop.sh"
echo ""
echo "Default xmrig wallet: DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"
echo "To use custom wallet: WALLET_ADDR=<address> docker-compose up -d"