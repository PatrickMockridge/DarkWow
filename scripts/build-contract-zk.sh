#!/bin/bash
# Build ZK circuit binaries for all DarkWow contracts.
# One command. Uses ./zkas directly — no make, no wasm, no cargo.
#
# Usage:
#   scripts/build-contract-zk.sh            # build all
#   scripts/build-contract-zk.sh -           # build all, show failures
#
# zkas must already be built (make zkas) before running this script.
# ./zkas <file> auto-outputs <file>.bin — no -o flag needed.

set -euo pipefail

cd "$(dirname "$0")/.."
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"

BUILT=0
FAILED=0
FAILED_LIST=()

for d in src/contract/*/proof/; do
    for zk in "$d"*.zk; do
        [ -f "$zk" ] || continue
        name="$(basename "$(dirname "$d")")/$(basename "$zk" .zk)"
        if ./zkas "$zk" 2>/dev/null; then
            echo "  $name OK"
            BUILT=$((BUILT + 1))
        else
            echo "  $name FAILED"
            FAILED=$((FAILED + 1))
            FAILED_LIST+=("$name")
        fi
    done
done

echo ""
echo "========================================"
echo -e "ZK circuits: $BUILT built, $FAILED failed"
if [ "$FAILED" -gt 0 ]; then
    echo -e "Failed: ${FAILED_LIST[*]}"
    exit 1
fi
echo "========================================"
