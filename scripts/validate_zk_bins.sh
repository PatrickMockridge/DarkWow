#!/bin/bash
# Validate all ZK binaries across all contracts
#
# Usage:
#   ./scripts/validate_zk_bins.sh           # Validate all
#   ./scripts/validate_zk_bins.sh rebuild    # Auto-rebuild corrupted
#   ./scripts/validate_zk_bins.sh contract   # Validate specific contract dir

set -e

ZKAS="${ZKAS:-$(cd "$(dirname "$0")/.." && pwd)/zkas}"
[ -x "$ZKAS" ] || ZKAS="target/release/zkas"  # fallback for CI where only target/ exists
CONTRACT_DIR="src/contract"

# Which sources the binaries below are supposed to come from, so the summary can say how many it did
# NOT check. This loop walks `.zk.bin` files, and `.zk.bin` is gitignored (`.gitignore:9`): on a tree
# whose binaries are not built it checks exactly nothing and exits 0. Measured 2026-09-24 by running
# this script against an empty directory — `Summary: 0 OK, 0 corrupted`, exit 0 — which is the class
# `OBL-C122` names. In `scripts/run-all-tests.sh` that is mitigated by gate order, since `build
# contract ZK circuits` runs immediately before this; the coverage line below is what makes it visible
# everywhere else. The check that is immune by construction is `scripts/check-circuit-fidelity.py`,
# which walks the sources.
# The coverage below is the *stated* intent, not a widening: this gate is named "ZK binaries
# well-formed", and the runner's comment beside it says it "runs the compiler's own `validate` over
# every one of them". It ran over `src/contract/*/proof/` alone, which is 166 of the 178 circuits the
# layer analyses — `proofs/core/` (10) and `bin/darkirc/proof/` (2) were silently outside it, while
# their binaries are built and, measured 2026-09-24, all twelve pass `zkas validate`. So they are in.
# The values are quoted whole and expanded unquoted at the loops below: `VAR=a/*.zk b/*.zk` is parsed
# as a *command* with an assignment in front of it, so the second glob is executed as a program —
# measured, it printed `proofs/core/arithmetic.zk: Permission denied`. Quoting keeps the globs inside
# the value, and `for bin in $BIN_GLOB` still word-splits and expands them.
case "$1" in
    ""|rebuild) SRC_GLOB="$CONTRACT_DIR/*/proof/*.zk proofs/core/*.zk bin/darkirc/proof/*.zk"
                BIN_GLOB="$CONTRACT_DIR/*/proof/*.zk.bin proofs/core/*.zk.bin bin/darkirc/proof/*.zk.bin" ;;
    *)          SRC_GLOB="$1/*.zk"
                BIN_GLOB="$1/*.zk.bin" ;;
esac

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

    for bin in $BIN_GLOB; do
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

    for bin in $BIN_GLOB; do
        if [ -f "$bin" ]; then
            validate_bin "$bin" || true
        fi
    done
else
    echo "=== Validating all ZK binaries ==="
    echo ""

    for bin in $BIN_GLOB; do
        if [ -f "$bin" ]; then
            validate_bin "$bin" || true
        fi
    done
fi

echo ""
echo "========================================"
echo -e "Summary: ${GREEN}$OK OK${NC}, ${RED}$CORRUPTED corrupted${NC}"
SOURCES=$(ls $SRC_GLOB 2>/dev/null | wc -l)
echo -e "         ${YELLOW}$((SOURCES - OK - CORRUPTED)) of $SOURCES source(s) have no binary here to check${NC}"
if [ "$1" == "rebuild" ]; then
    echo -e "         ${YELLOW}$REBUILT rebuilt${NC}"
fi
echo "========================================"

if [ $CORRUPTED -gt 0 ]; then
    exit 1
fi
exit 0