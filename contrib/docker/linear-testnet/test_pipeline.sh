#!/bin/bash
# Linear-Testnet Mining Pipeline
# Validates entrypoint, builds, starts, and verifies containers.
# Supports --mode native (dwowd + xmrig via stratum).

set -e

cd "$(dirname "$0")"

MODE="${1:-native}"
if [ "$MODE" = "--mode" ]; then
    MODE="${2:-native}"
fi

echo "=== Linear-Testnet Pipeline (mode=$MODE) ==="

# --- Phase 1: Clean ---
echo "[1/6] Cleaning..."
docker compose down -v 2>/dev/null || true
docker system prune -af --volumes 2>/dev/null || true

# --- Phase 2: Validate ---
echo "[2/6] Validating..."
[ -f entrypoint.sh ] || { echo "ERROR: entrypoint.sh missing"; exit 1; }
[ -f docker-compose.yml ] || { echo "ERROR: docker-compose.yml missing"; exit 1; }
[ -f Dockerfile ] || { echo "ERROR: Dockerfile missing"; exit 1; }

# --- Phase 3: Build ---
echo "[3/6] Building..."
docker compose build --no-cache

# --- Phase 4: Start ---
echo "[4/6] Starting..."
docker compose up -d

# Wait for containers to be running
echo "Waiting for containers..."
sleep 10

# --- Phase 5: Health check ---
echo "[5/6] Health check..."
docker compose ps | grep -E "Exit|unhealthy" && { echo "ERROR: container unhealthy"; docker compose logs; exit 1; }
echo "All containers running"

# --- Phase 6: Verify RPC ---
echo "[6/6] Verifying RPC..."
for port in 28345 28346; do
    echo "  Checking RPC port $port..."
    echo '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' | nc -w2 localhost $port || \
        { echo "WARNING: RPC on port $port not reachable"; }
done

echo ""
docker compose ps
echo ""
echo "=== DONE ==="
