#!/bin/bash
# Deterministic Level 1 (Lightweight) test runner for universal_relayer
#
# Runs the full relayer unit test suite with fixed environment settings
# and verifies reproducibility — same result every run.
#
# Usage:
#   bash bin/universal_relayer/test_relayer_lightweight.sh
#
# No ZK proofs, no Docker, no network. < 5 seconds.

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

info "universal_relayer — Level 1 Lightweight (deterministic)"

# --- Prerequisites ---
cd "$REPO_ROOT"

[ -f "Cargo.toml" ]
check $? "Repository root found"

command -v cargo >/dev/null 2>&1
check $? "cargo is available"

[ -d "bin/universal_relayer/src" ]
check $? "universal_relayer source directory exists"

# --- Deterministic environment ---
export RAYON_NUM_THREADS=10
info "RAYON_NUM_THREADS=10 (fixed thread count)"

# Suppress incremental compilation variance — same code, same binary
export CARGO_BUILD_INCREMENTAL=false
info "CARGO_BUILD_INCREMENTAL=false (deterministic build)"

# --- First run ---
info "Run 1: cargo test -p universal_relayer"
RUN1_OUTPUT=$(mktemp)
cargo test -p universal_relayer 2>&1 | tee "$RUN1_OUTPUT"
RUN1_EXIT=${PIPESTATUS[0]}
check $RUN1_EXIT "Run 1: cargo test exits 0"

RUN1_PASSED=$(grep -c "ok" "$RUN1_OUTPUT" || true)
RUN1_FAILED=$(grep -c "FAILED" "$RUN1_OUTPUT" || true)
RUN1_TOTAL=$(grep -c "running [0-9]" "$RUN1_OUTPUT" || true)
info "Run 1: test results captured"

# --- Second run ---
info "Run 2: cargo test -p universal_relayer"
RUN2_OUTPUT=$(mktemp)
cargo test -p universal_relayer 2>&1 | tee "$RUN2_OUTPUT"
RUN2_EXIT=${PIPESTATUS[0]}
check $RUN2_EXIT "Run 2: cargo test exits 0"

RUN2_PASSED=$(grep -c "ok" "$RUN2_OUTPUT" || true)
RUN2_FAILED=$(grep -c "FAILED" "$RUN2_OUTPUT" || true)
info "Run 2: test results captured"

# --- Determinism check ---
[ "$RUN1_EXIT" -eq "$RUN2_EXIT" ]
check $? "Determinism: exit codes match (both $RUN1_EXIT)"

[ "$RUN1_FAILED" -eq 0 ] && [ "$RUN2_FAILED" -eq 0 ]
check $? "Determinism: zero failures in both runs"

info "Run 1: $RUN1_PASSED passing tests, $RUN1_FAILED failures"
info "Run 2: $RUN2_PASSED passing tests, $RUN2_FAILED failures"

rm -f "$RUN1_OUTPUT" "$RUN2_OUTPUT"
report
