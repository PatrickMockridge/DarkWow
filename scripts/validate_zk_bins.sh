#!/bin/bash
# Validate all ZK binaries across all contracts
#
# Usage:
#   ./scripts/validate_zk_bins.sh           # Validate all
#   ./scripts/validate_zk_bins.sh rebuild    # Auto-rebuild corrupted
#   ./scripts/validate_zk_bins.sh contract   # Validate specific contract dir

set -e

ZKAS="${ZKAS:-target/release/zkas}"
CONTRACT_DIR="src/contract"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

OK=0
CORRUPTED=0
REBUILT=0

validate_bin() {
    local bin="$1"
    if "$ZKAS" validate "$bin" 2>/dev/null; then
        echo -e "${GREEN}OK:${NC} $bin"
        OK=$((OK + 1))
        return 0
    else
        echo -e "${RED}CORRUPTED:${NC} $bin"
        CORRUPTED=$((CORRUPTED + 1))
        return 1
    fi
}

rebuild_bin() {
    local bin="$1"
    local zk="${bin%.zk.bin}.zk"

    if [ ! -f "$zk" ]; then
        echo -e "${YELLOW}SKIP:${NC} No source for $bin"
        return 0
    fi

    echo -e "${YELLOW}REBUILDING:${NC} $bin from $zk"
    # Get directory of the file
    local dir=$(dirname "$bin")
    if "$ZKAS" rebuild "$dir" 2>/dev/null; then
        REBUILT=$((REBUILT + 1))
    fi
}

# Main validation loop
if [ "$1" == "rebuild" ]; then
    echo "=== Validating and auto-rebuilding corrupted binaries ==="
    echo ""

    for bin in "$CONTRACT_DIR"/*/proof/*.zk.bin; do
        if [ ! -f "$bin" ]; then
            continue
        fi

        if ! "$ZKAS" validate "$bin" 2>/dev/null; then
            rebuild_bin "$bin"
        else
            echo -e "${GREEN}OK:${NC} $bin"
            OK=$((OK + 1))
        fi
    done
elif [ -n "$1" ]; then
    echo "=== Validating ZK binaries in $1 ==="
    echo ""

    for bin in "$1"/*.zk.bin; do
        if [ -f "$bin" ]; then
            validate_bin "$bin" || true
        fi
    done
else
    echo "=== Validating all ZK binaries ==="
    echo ""

    for bin in "$CONTRACT_DIR"/*/proof/*.zk.bin; do
        if [ -f "$bin" ]; then
            validate_bin "$bin" || true
        fi
    done
fi

echo ""
echo "========================================"
echo -e "Summary: ${GREEN}$OK OK${NC}, ${RED}$CORRUPTED corrupted${NC}"
if [ "$1" == "rebuild" ]; then
    echo -e "         ${YELLOW}$REBUILT rebuilt${NC}"
fi
echo "========================================"

if [ $CORRUPTED -gt 0 ]; then
    exit 1
fi
exit 0