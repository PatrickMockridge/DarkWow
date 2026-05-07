#!/bin/bash
# DarkWow-Testnet Full Pipeline
# Three-container darkwow-testnet (lilith + 2 mining nodes)
# Validates entrypoint, builds, starts, verifies, runs contract tests

set -e

cd "$(dirname "$0")"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

echo "=== DarkWow-Testnet Full Pipeline ==="
echo ""

# ==============================================================================
# Phase 1: Clean (conservative — no system prune)
# ==============================================================================
info "[1/6] Cleaning previous deployment..."
docker-compose down -v 2>/dev/null || true

# Verify no stale containers
STALE=$(docker ps -a --format '{{.Names}}' | grep "dwow-" || true)
if [ -n "$STALE" ]; then
    warn "Stale containers found, removing..."
    echo "$STALE" | xargs docker rm -f 2>/dev/null || true
fi
info "Clean"

# ==============================================================================
# Phase 2: Validate
# ==============================================================================
info "[2/6] Validating prerequisites..."
[ -f entrypoint.sh ]       || error "entrypoint.sh missing"
[ -f docker-compose.yml ]  || error "docker-compose.yml missing"
[ -f Dockerfile ]          || error "Dockerfile missing"
info "All required files present"

# ==============================================================================
# Phase 3: Build
# ==============================================================================
info "[3/6] Building images..."
docker-compose build --no-cache 2>&1 | tail -20
info "Build complete"

# ==============================================================================
# Phase 4: Start
# ==============================================================================
info "[4/6] Starting containers..."
docker-compose up -d

# Wait for containers to initialize
sleep 10

# Verify no immediate exits
EXITED=$(docker-compose ps | grep "Exit" || true)
if [ -n "$EXITED" ]; then
    echo "$EXITED"
    error "Container exited immediately — check logs: docker-compose logs"
fi

info "All containers running"

# ==============================================================================
# Phase 5: Verify node health
# ==============================================================================
info "[5/6] Verifying node health..."

# Check node0 RPC
for i in $(seq 1 30); do
    if docker exec dwow-node0 curl -s --max-time 2 http://127.0.0.1:31345 >/dev/null 2>&1; then
        info "Node0 RPC is up (attempt $i)"
        break
    fi
    [ "$i" -eq 30 ] && error "Node0 RPC did not become healthy"
    sleep 2
done

# Check containers
echo ""
info "Container status:"
docker-compose ps

# Check xmrig connections
echo ""
info "Mining status:"
docker-compose logs xmrig0 2>&1 | grep -i "new job\|accepted\|error" | tail -5 || warn "No xmrig data yet"

echo ""
echo -e "${GREEN}=== Pipeline Complete ===${NC}"
echo ""
echo "Run contract tests:  ./test-contracts.sh"
echo "Check logs:          docker-compose logs -f"
echo "Tear down:           docker-compose down"
