#!/usr/bin/env bash
#
# run-all.sh — L4 Contract Test Suite Orchestrator
#
# Runs all per-contract wallet verification tests against the Docker testnet.
# Requires test_pipeline.sh to have completed successfully.
#
# Usage:
#   RAYON_NUM_THREADS=10 bash contrib/docker/darkwow-testnet/contract-tests/run-all.sh
#   RAYON_NUM_THREADS=10 bash contrib/docker/darkwow-testnet/contract-tests/run-all.sh --contract escrow
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Parse args
RUN_ALL=true
TARGET_CONTRACT=""
if [ "${1:-}" = "--contract" ]; then
    RUN_ALL=false
    TARGET_CONTRACT="${2:-}"
fi

source ./common.sh

PASS=0
FAIL=0
SKIP=0

PASS_TESTS=()
FAIL_TESTS=()

run_test() {
    local script="$1"
    local contract_name
    contract_name=$(basename "$script" .sh | sed 's/^test-//')

    if [ "$RUN_ALL" = false ] && [ "$contract_name" != "$TARGET_CONTRACT" ]; then
        return
    fi

    echo ""
    echo "══════════════════════════════════════════════════════════════"
    echo "  L4 Contract Test: $contract_name"
    echo "══════════════════════════════════════════════════════════════"

    if [ ! -x "$script" ]; then
        chmod +x "$script"
    fi

    if bash "$script" 2>&1; then
        PASS=$((PASS + 1))
        PASS_TESTS+=("$contract_name")
        echo "  [$contract_name] PASSED"
    else
        FAIL=$((FAIL + 1))
        FAIL_TESTS+=("$contract_name")
        echo "  [$contract_name] FAILED"
    fi
}

# Pre-flight
check_prerequisites 1

TOTAL=0

# Run each test script
for script in "$SCRIPT_DIR"/test-*.sh; do
    if [ ! -f "$script" ]; then
        continue
    fi
    TOTAL=$((TOTAL + 1))
    run_test "$script"
done

# ── Report ──────────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════════════════════════"
echo "  L4 Contract Test Suite — Results"
echo "══════════════════════════════════════════════════════════════"
echo "  Total:  $TOTAL"
echo "  Passed: $PASS"
echo "  Failed: $FAIL"

if [ ${#PASS_TESTS[@]} -gt 0 ]; then
    echo "  PASSED: ${PASS_TESTS[*]}"
fi
if [ ${#FAIL_TESTS[@]} -gt 0 ]; then
    echo "  FAILED: ${FAIL_TESTS[*]}"
fi

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "FAILURE — $FAIL contract test(s) failed"
    exit 1
else
    echo ""
    echo "SUCCESS — all $PASS contract tests passed"
fi
