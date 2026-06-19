#!/bin/bash
# Wallet Container Level 3 Integration Test
#
# Starts the wallet Docker container in test mode, waits for it to complete
# its automated scan→position→assert cycle, and verifies the output.
#
# Prerequisites:
#   - test_pipeline.sh must have completed successfully (Docker stack running)
#   - Mining must be producing blocks (wallet needs coinbase coins to discover)
#
# Usage:
#   ./test-wallet.sh
#
# The wallet container runs in test mode (WALLET_MODE=test):
#   1. Initializes wallet DB
#   2. Generates or imports keypair
#   3. Scans blockchain for coins
#   4. Runs position resolution
#   5. Asserts on output (coin capabilities, descriptors, actions)
#   6. Exits 0 on success, 1 on failure

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
export COMPOSE_PROJECT_NAME="darkwow-testnet"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; }

PASS=0
FAIL=0
pass() { echo -e "${GREEN}[PASS]${NC} $*"; PASS=$((PASS + 1)); }
fail() { echo -e "${RED}[FAIL]${NC} $*"; FAIL=$((FAIL + 1)); }

# ==============================================================================
# Phase 1: Pre-flight checks
# ==============================================================================
info "Phase 1: Pre-flight checks..."

# Verify the Docker stack is running
if docker ps --format '{{.Names}}' | grep -q "dwow-node0"; then
    pass "dwow-node0 container running"
else
    fail "dwow-node0 container not running (run test_pipeline.sh first)"
    exit 1
fi

# Verify mining secret exists (shared with nodes)
if ls /tmp/dwow_mining_secret_* >/dev/null 2>&1; then
    pass "mining secret present at /tmp/dwow_mining_secret"
else
    fail "mining secret missing (test_pipeline.sh should have created it)"
    exit 1
fi

# Verify wallet Docker image exists
WALLET_IMAGE="darkwow-wallet:latest"
if docker image inspect "$WALLET_IMAGE" &>/dev/null; then
    pass "wallet Docker image found"
else
    info "Building wallet Docker image..."
    docker build \
        -t "$WALLET_IMAGE" \
        -f "$SCRIPT_DIR/Dockerfile.wallet" \
        "$REPO_ROOT" 2>&1
    if docker image inspect "$WALLET_IMAGE" &>/dev/null; then
        pass "wallet image built"
    else
        fail "wallet image build failed"
        exit 1
    fi
fi

# ==============================================================================
# Phase 2: Start wallet container in test mode
# ==============================================================================
info "Phase 2: Starting wallet container in test mode..."

# Ensure any previous wallet container is removed
docker stop dwow-wallet 2>/dev/null || true
docker rm dwow-wallet 2>/dev/null || true

# Build and start the wallet container with test mode
WALLET_MODE=test docker compose -f "$COMPOSE_FILE" --profile wallet up -d wallet 2>&1

# Verify container started
sleep 3
if docker ps --format '{{.Names}}' | grep -q "dwow-wallet"; then
    pass "wallet container started"
else
    # Check if it already exited (fast test mode)
    if docker ps -a --format '{{.Names}}' | grep -q "dwow-wallet"; then
        pass "wallet container started (already exited — fast test mode)"
    else
        fail "wallet container failed to start"
        exit 1
    fi
fi

# ==============================================================================
# Phase 3: Wait for test completion
# ==============================================================================
info "Phase 3: Waiting for wallet test to complete..."

# Wait up to 120s for the container to exit (test mode auto-exits)
EXIT_CODE=-1
WAITED=0
while [ $WAITED -lt 120 ]; do
    if ! docker ps --format '{{.Names}}' | grep -q "dwow-wallet"; then
        EXIT_CODE=$(docker inspect dwow-wallet --format='{{.State.ExitCode}}' 2>/dev/null || echo "-1")
        info "Wallet container exited with code $EXIT_CODE after ${WAITED}s"
        break
    fi
    sleep 5
    WAITED=$((WAITED + 5))
done

if [ "$EXIT_CODE" = "-1" ]; then
    warn "Wallet container still running after 120s — killing"
    docker stop dwow-wallet 2>/dev/null || true
    EXIT_CODE=$(docker inspect dwow-wallet --format='{{.State.ExitCode}}' 2>/dev/null || echo "-1")
fi

# ==============================================================================
# Phase 4: Verify test output
# ==============================================================================
info "Phase 4: Verifying test output..."

LOGS=$(docker logs dwow-wallet 2>&1 || true)

echo "--- Wallet container logs ---"
echo "$LOGS"
echo "--- End logs ---"

# Check exit code
if [ "$EXIT_CODE" = "0" ]; then
    pass "wallet test mode exited 0"
else
    fail "wallet test mode exit code $EXIT_CODE (expected 0)"
fi

# Check for coin capabilities
if echo "$LOGS" | grep -q "Coin worth"; then
    pass "coin capabilities found in position output"
else
    fail "coin capabilities missing from position output"
fi

# Check for descriptors
if echo "$LOGS" | grep -q "Descriptors loaded"; then
    pass "descriptors count reported"
else
    fail "descriptors count missing"
fi

# Check for capabilities section
if echo "$LOGS" | grep -q "Capabilities"; then
    pass "Capabilities section present"
else
    fail "Capabilities section missing"
fi

# Check for wallet address (confirms init + keygen worked)
if echo "$LOGS" | grep -q "Wallet address"; then
    pass "wallet address displayed"
else
    fail "wallet address not displayed"
fi

# ==============================================================================
# Phase 5: Cleanup
# ==============================================================================
info "Phase 5: Cleanup..."

docker stop dwow-wallet 2>/dev/null || true
docker rm dwow-wallet 2>/dev/null || true

# ==============================================================================
# Report
# ==============================================================================
echo ""
echo "==========================================="
echo "  Wallet Container Test"
echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
echo "==========================================="
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Some checks failed${NC}"
    echo ""
    echo "Debug info:"
    echo "  docker compose -f $COMPOSE_FILE --profile wallet logs"
    echo "  docker logs dwow-wallet"
    exit 1
fi

echo -e "${GREEN}Wallet container test passed${NC}"
exit 0
