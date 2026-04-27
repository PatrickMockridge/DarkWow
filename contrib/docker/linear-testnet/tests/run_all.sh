#!/bin/bash
# Run All Contract Tests
# Executes all test scripts in sequence with reporting
#
# Usage: ./run_all.sh [options]
#   --skip-setup    Skip setup phase
#   --skip-dao      Skip DAO-Escrow tests
#   --skip-stable   Skip Stablecoin tests
#   --skip-dex      Skip DEX tests
#   --skip-cross    Skip Cross-contract tests
#   --help          Show this help

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
TESTNET_DIR="$SCRIPT_DIR/.."
TESTS_DIR="$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test results
PASSED=0
FAILED=0
SKIPPED=0

# ============================================================
# Parse Arguments
# ============================================================

SKIP_SETUP=false
SKIP_DAO=false
SKIP_STABLE=false
SKIP_DEX=false
SKIP_CROSS=false

for arg in "$@"; do
    case $arg in
        --skip-setup) SKIP_SETUP=true ;;
        --skip-dao) SKIP_DAO=true ;;
        --skip-stable) SKIP_STABLE=true ;;
        --skip-dex) SKIP_DEX=true ;;
        --skip-cross) SKIP_CROSS=true ;;
        --help)
            echo "Usage: $0 [options]"
            echo "  --skip-setup    Skip setup phase"
            echo "  --skip-dao      Skip DAO-Escrow tests"
            echo "  --skip-stable   Skip Stablecoin tests"
            echo "  --skip-dex      Skip DEX tests"
            echo "  --skip-cross    Skip Cross-contract tests"
            echo "  --help          Show this help"
            exit 0
            ;;
    esac
done

# ============================================================
# Helper Functions
# ============================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((PASSED++))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((FAILED++))
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $1"
    ((SKIPPED++))
}

log_section() {
    echo ""
    echo "========================================"
    echo -e "${BLUE}$1${NC}"
    echo "========================================"
}

check_node() {
    local port=$1
    curl -s -f -X POST "http://localhost:$port" -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' > /dev/null 2>&1
}

# ============================================================
# Pre-flight Checks
# ============================================================

log_section "Pre-flight Checks"

# Check if darkfid binary exists
if [ ! -f "$ROOT_DIR/target/debug/drk" ]; then
    echo "WARNING: drk binary not found at $ROOT_DIR/target/debug/drk"
    echo "  Build with: cargo build -p drk"
fi

# Check if darkfid is running
log_info "Checking if linear-testnet is running..."
NODE_RUNNING=false
for port in 28345 28346 28347 28348 28349; do
    if check_node $port; then
        echo "  Node found on port $port"
        NODE_RUNNING=true
        break
    fi
done

if [ "$NODE_RUNNING" = false ]; then
    log_fail "No darkfid nodes are running!"
    echo ""
    echo "Start the testnet first:"
    echo "  cd $TESTNET_DIR"
    echo "  ./scripts/start.sh"
    echo ""
    echo "Then run tests again."
    exit 1
fi

log_success "Linear-testnet is running"

# ============================================================
# Phase 1: Setup (Contract Deployment)
# ============================================================

if [ "$SKIP_SETUP" = false ]; then
    log_section "Phase 1: Contract Deployment"
    log_info "Deploying contracts via Deployooor..."

    if [ -f "$TESTS_DIR/setup.sh" ]; then
        chmod +x "$TESTS_DIR/setup.sh"
        if bash "$TESTS_DIR/setup.sh"; then
            log_success "Contract deployment completed"
        else
            log_fail "Contract deployment failed"
            exit 1
        fi
    else
        log_fail "setup.sh not found!"
        exit 1
    fi
else
    log_skip "Skipped setup (--skip-setup)"
fi

# ============================================================
# Phase 2: DAO-Escrow Tests
# ============================================================

if [ "$SKIP_DAO" = false ]; then
    log_section "Phase 2: DAO-Escrow Contract Tests"
    log_info "Testing InitializeV1, PayPremiumV1, WithdrawV1, etc..."

    if [ -f "$TESTS_DIR/test_dao_escrow.sh" ]; then
        chmod +x "$TESTS_DIR/test_dao_escrow.sh"
        if bash "$TESTS_DIR/test_dao_escrow.sh" 0; then
            log_success "DAO-Escrow tests passed"
        else
            log_fail "DAO-Escrow tests failed"
        fi
    else
        log_skip "test_dao_escrow.sh not found"
    fi
else
    log_skip "Skipped DAO-Escrow tests (--skip-dao)"
fi

# ============================================================
# Phase 3: Stablecoin Tests
# ============================================================

if [ "$SKIP_STABLE" = false ]; then
    log_section "Phase 3: Stablecoin Contract Tests"
    log_info "Testing OpenPositionV1, MintStableV1, RepayV1, LiquidateV1..."

    if [ -f "$TESTS_DIR/test_stablecoin.sh" ]; then
        chmod +x "$TESTS_DIR/test_stablecoin.sh"
        if bash "$TESTS_DIR/test_stablecoin.sh" 1; then
            log_success "Stablecoin tests passed"
        else
            log_fail "Stablecoin tests failed"
        fi
    else
        log_skip "test_stablecoin.sh not found"
    fi
else
    log_skip "Skipped Stablecoin tests (--skip-stable)"
fi

# ============================================================
# Phase 4: DEX Tests
# ============================================================

if [ "$SKIP_DEX" = false ]; then
    log_section "Phase 4: DEX Contract Tests"
    log_info "Testing CreateSwapV1, AcceptSwapV1, ExecuteSwapV1, CancelSwapV1..."

    if [ -f "$TESTS_DIR/test_dex.sh" ]; then
        chmod +x "$TESTS_DIR/test_dex.sh"
        if bash "$TESTS_DIR/test_dex.sh" 2; then
            log_success "DEX tests passed"
        else
            log_fail "DEX tests failed"
        fi
    else
        log_skip "test_dex.sh not found"
    fi
else
    log_skip "Skipped DEX tests (--skip-dex)"
fi

# ============================================================
# Phase 5: Cross-Contract Tests
# ============================================================

if [ "$SKIP_CROSS" = false ]; then
    log_section "Phase 5: Cross-Contract Integration Tests"
    log_info "Testing dao_escrow <-> stablecoin <-> dex interactions..."

    if [ -f "$TESTS_DIR/test_cross.sh" ]; then
        chmod +x "$TESTS_DIR/test_cross.sh"
        if bash "$TESTS_DIR/test_cross.sh" 3; then
            log_success "Cross-contract tests passed"
        else
            log_fail "Cross-contract tests failed"
        fi
    else
        log_skip "test_cross.sh not found"
    fi
else
    log_skip "Skipped Cross-contract tests (--skip-cross)"
fi

# ============================================================
# Summary Report
# ============================================================

echo ""
echo "========================================"
echo "=== TEST SUMMARY ==="
echo "========================================"
echo ""
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests completed successfully!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed. Check output above for details.${NC}"
    exit 1
fi