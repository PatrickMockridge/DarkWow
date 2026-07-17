#!/bin/bash
# Build ZK circuit binaries for DarkWow contracts.
#
# Each contract's Makefile compiles .zk sources to .zk.bin via the zkas
# compiler. This script drives all contracts (or one specific contract).
# It replaces the validate_zk_bins.sh gate — binaries produced by a
# successful make invocation are valid by construction.
#
# Usage:
#   scripts/build-contract-zk.sh                # all contracts
#   scripts/build-contract-zk.sh native_token   # one contract
#   scripts/build-contract-zk.sh attestation auction baccarat  # several
#
# zkas must already be built (make zkas) before running this script.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASSED=0
FAILED=0
FAILED_NAMES=()

build_one() {
    local name="$1"
    local dir="src/contract/$name"
    if [ ! -d "$dir" ]; then
        echo -e "${RED}ERROR:${NC} '$name' is not a contract directory (expected $dir)" >&2
        return 1
    fi
    if [ ! -d "$dir/proof" ]; then
        # Some contracts have no ZK proofs — skip silently.
        return 0
    fi
    local count
    count=$(find "$dir/proof" -maxdepth 1 -name '*.zk' -type f 2>/dev/null | wc -l)
    if [ "$count" -eq 0 ]; then
        return 0
    fi

    echo -n "  ${name} (${count} circuit"
    [ "$count" -ne 1 ] && echo -n "s"
    echo -n ")... "

    # Force rebuild: remove stale .source_hash and old .zk.bin files so
    # make recompiles from source. Without this, a cargo clean leaves
    # old binaries on disk that fail validation against the current zkas.
    rm -f "$dir/.source_hash" "$dir/proof"/*.zk.bin 2>/dev/null || true
    if make -C "$dir" all >/dev/null 2>&1; then
        echo -e "${GREEN}OK${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAIL${NC}"
        FAILED=$((FAILED + 1))
        FAILED_NAMES+=("$name")
        return 1
    fi
}

if [ $# -ge 1 ]; then
    for name in "$@"; do
        build_one "$name" || exit 1
    done
else
    echo "Building ZK circuit binaries for all contracts..."
    for d in src/contract/*/; do
        name="$(basename "$d")"
        build_one "$name" || exit 1  # fail-fast on first error
    done
fi

echo ""
echo "========================================"
if [ "$FAILED" -eq 0 ]; then
    echo -e "ZK circuits: ${GREEN}${PASSED} built${NC}, ${RED}0 failed${NC}"
else
    echo -e "ZK circuits: ${GREEN}${PASSED} built${NC}, ${RED}${FAILED} failed${NC}"
    echo -e "Failed: ${RED}${FAILED_NAMES[*]}${NC}"
    exit 1
fi
echo "========================================"
