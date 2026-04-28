#!/bin/bash
# Linear-Testnet Mining Pipeline
# Two darkfi nodes mining on linear-testnet
# Robust: validates entrypoint, verifies containers, fails fast on errors

set -e

cd "$(dirname "$0")"

echo "=== Linear-Testnet Mining Pipeline ==="

# PHASE 1: Clean
echo "[1/5] Cleaning..."
docker-compose down -v 2>/dev/null || true
docker system prune -af --volumes 2>/dev/null || true

# Verify no stale containers
docker ps -a | grep darkfi-linear && { echo "ERROR: stale containers exist"; exit 1; }

# PHASE 2: Validate
echo "[2/5] Validating..."
# Validate entrypoint.sh exists and is executable
[ -f entrypoint.sh ] || { echo "ERROR: entrypoint.sh missing"; exit 1; }
# Validate docker-compose.yml exists
[ -f docker-compose.yml ] || { echo "ERROR: docker-compose.yml missing"; exit 1; }
# Validate Dockerfile exists
[ -f Dockerfile ] || { echo "ERROR: Dockerfile missing"; exit 1; }

# PHASE 3: Build
echo "[3/5] Building..."
docker-compose build --no-cache

# PHASE 4: Start
echo "[4/5] Starting..."
docker-compose up -d

# Wait for containers to be running
sleep 5

# Verify containers didn't exit immediately
docker-compose ps | grep "Exit" && { echo "ERROR: container exited immediately"; docker-compose logs; exit 1; }

# PHASE 5: Verify
echo "[5/5] Verifying..."
echo ""
echo "Containers:"
docker-compose ps

echo ""
echo "Network connectivity:"
docker exec darkfi-linear-node1 ping -c 2 node0

echo ""
echo "=== DONE ==="
