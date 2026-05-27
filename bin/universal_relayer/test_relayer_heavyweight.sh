#!/bin/bash
# Deterministic Level 2 (Heavyweight) test runner for relayer lifecycle
#
# Runs the bridge deposit→withdraw ZK proof lifecycle test with fixed
# environment settings. Requires --release for halo2 proof generation.
#
# Usage:
#   bash bin/universal_relayer/test_relayer_heavyweight.sh
#
# Requires: release build of dwowd. ZK proofs, no Docker. 3-4 minutes.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0; FAIL=0
pass() { echo -e "${GREEN}[PASS]${NC} $*"; PASS=$((PASS + 1)); }
fail() { echo -e "${RED}[FAIL]${NC} $*"; FAIL=$((FAIL + 1)); }
check() {
    if [ "$1" -eq 0 ]; then pass "$2"; else fail "$2"; fi
}
info()  { echo -e "${YELLOW}[INFO]${NC}  $*"; }

report() {
    echo ""
    echo "==========================================="
    echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
    echo "==========================================="
    if [ "$FAIL" -gt 0 ]; then
        echo -e "${RED}Some tests failed${NC}"
        exit 1
    fi
    echo -e "${GREEN}All tests passed${NC}"
}

info "relayer lifecycle — Level 2 Heavyweight (deterministic)"

cd "$REPO_ROOT"

# --- Prerequisites ---
[ -f "Cargo.toml" ]
check $? "Repository root found"

command -v cargo >/dev/null 2>&1
check $? "cargo is available"

[ -f "dwow_bridge_contract.wasm" ]
check $? "Bridge WASM binary present (dwow_bridge_contract.wasm)"

# --- Deterministic environment ---
export RAYON_NUM_THREADS=10
info "RAYON_NUM_THREADS=10 (heavyweight thread count)"

export CARGO_BUILD_INCREMENTAL=false
info "CARGO_BUILD_INCREMENTAL=false (deterministic build)"

# halo2 proof generation needs stack space in --release
export RUST_MIN_STACK=16777216
info "RUST_MIN_STACK=16777216 (ZK proof stack)"

# --- Build check ---
info "Checking release build..."
cargo build --release -p dwowd 2>&1 | tail -3
check ${PIPESTATUS[0]} "dwowd release build"

# --- Run heavyweight test ---
info "Running test_relayer_lifecycle_heavyweight..."
START_TIME=$(date +%s)

TEST_OUTPUT=$(mktemp)
cargo test --release -p dwowd test_relayer_lifecycle_heavyweight 2>&1 | tee "$TEST_OUTPUT"
TEST_EXIT=${PIPESTATUS[0]}

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# --- Verify ---
check $TEST_EXIT "test_relayer_lifecycle_heavyweight exits 0"

grep -q "test_relayer_lifecycle_heavyweight.*ok" "$TEST_OUTPUT"
check $? "Test output contains 'ok' result"

grep -q "0 failed" "$TEST_OUTPUT"
check $? "Zero test failures"

# Verify ZK proofs ran (check for lifecycle test output)
grep -qE "deposit executed OK|withdraw executed OK|double-spend correctly rejected|initialize \+ deploy_capital executed OK" "$TEST_OUTPUT"
check $? "Bridge lifecycle execution detected in output"

info "Test duration: ${DURATION}s"
rm -f "$TEST_OUTPUT"
report
