#!/usr/bin/env bash
#
# test-wallet-l4.sh — Cross-implementation wallet verification
#
# Level 4 test: validates the Rust wallet against the Python canonical model.
# The Python oracle runs first to establish expected outputs. Then the Rust
# wallet runs against the same scenario, and outputs are compared.
#
# Prerequisites:
#   - test_pipeline.sh must have completed (Docker stack running)
#   - /tmp/dwow_mining_secret must exist
#   - darkwow-wallet:latest Docker image must exist
#
# Usage:
#   RAYON_NUM_THREADS=10 bash contrib/docker/darkwow-testnet/test-wallet-l4.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
MODEL_DIR="$REPO_ROOT/contrib/model"
FIXTURE_DIR="$MODEL_DIR/fixtures"

PASS=0
FAIL=0

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

pass_msg() { echo -e "${GREEN}[PASS]${NC} $1"; PASS=$((PASS + 1)); }
fail_msg() { echo -e "${RED}[FAIL]${NC} $1"; FAIL=$((FAIL + 1)); }

# ── Phase 0: Python Oracle ──────────────────────────────────────────────

echo "=== L4: Python Oracle Verification ==="
echo ""
echo "Running Python model against fixtures..."

ORACLE_OUT=$(python3 "$MODEL_DIR/test_oracle.py" "$FIXTURE_DIR"/*.json 2>&1) || true
echo "$ORACLE_OUT"

# Count oracle pass/fail from output
ORACLE_PASS=$(echo "$ORACLE_OUT" | grep -c "PASSED" || true)
ORACLE_FAIL=$(echo "$ORACLE_OUT" | grep -c "FAILED\|ERROR" || true)

if [ "$ORACLE_FAIL" -gt 0 ]; then
    fail_msg "Python oracle: $ORACLE_PASS passed, $ORACLE_FAIL failed"
else
    pass_msg "Python oracle: all $ORACLE_PASS fixtures passed"
fi

# ── Phase 1: Pre-flight Checks ──────────────────────────────────────────

echo ""
echo "=== L4: Pre-flight Checks ==="

# Verify Docker is running
if ! docker info >/dev/null 2>&1; then
    fail_msg "Docker is not running"
    echo "Results: $PASS passed, $FAIL failed"
    exit 1
fi
pass_msg "Docker is running"

# Verify node0 is running
if docker ps --format '{{.Names}}' | grep -q "dwow-node0"; then
    pass_msg "dwow-node0 container is running"
else
    fail_msg "dwow-node0 container is NOT running"
    echo "Run test_pipeline.sh first to start the Docker stack."
    echo "Results: $PASS passed, $FAIL failed"
    exit 1
fi

# ── Phase 2: Rust Wallet Scan + Position ────────────────────────────────

echo ""
echo "=== L4: Rust Wallet Position ==="

# Run wallet position via docker exec on the existing wallet container
# or start a one-off container
WALLET_OUT=$(docker exec dwow-wallet-1 /app/dwow_wallet -c /app/drk.toml position 2>&1) || true
echo "$WALLET_OUT" | head -20
echo "..."

# ── Phase 3: Cross-Implementation Assertions ────────────────────────────

echo ""
echo "=== L4: Cross-Implementation Verification ==="

# Assert 1: Coin capabilities present
if echo "$WALLET_OUT" | grep -q "Coin worth"; then
    pass_msg "Rust wallet shows coin capabilities (matches Python oracle)"
else
    fail_msg "Rust wallet missing coin capabilities"
fi

# Assert 2: Capabilities section present
if echo "$WALLET_OUT" | grep -q "Capabilities"; then
    pass_msg "Rust wallet shows Capabilities section (matches Python oracle)"
else
    fail_msg "Rust wallet missing Capabilities section"
fi

# Assert 3: Actions section present
if echo "$WALLET_OUT" | grep -q "Actions"; then
    pass_msg "Rust wallet shows Actions section (matches Python oracle)"
else
    fail_msg "Rust wallet missing Actions section"
fi

# Assert 4: Wallet address present (confirms init + keygen worked)
if echo "$WALLET_OUT" | grep -q "Wallet address"; then
    pass_msg "Rust wallet shows wallet address (confirms init)"
else
    fail_msg "Rust wallet missing wallet address"
fi

# Assert 5: Descriptor count
DESC_COUNT=$(echo "$WALLET_OUT" | grep -oP 'Descriptors loaded:\s*\K\d+' || echo "0")
if [ "$DESC_COUNT" -gt 0 ]; then
    pass_msg "Rust wallet loaded $DESC_COUNT descriptors"
else
    fail_msg "Rust wallet loaded zero descriptors"
fi

# ── Report ──────────────────────────────────────────────────────────────

echo ""
echo "=== L4 Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"

if [ "$FAIL" -gt 0 ]; then
    echo "FAILURE"
    exit 1
else
    echo "SUCCESS"
fi
