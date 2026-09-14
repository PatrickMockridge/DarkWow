#!/usr/bin/env bash
#
# Per-file `#[test]` counts across the genesis-critical crates.
#
# The conversion rewrites every test body in scope. "No test was deleted, skipped or
# re-scoped" is otherwise only a promise, and deleting the test that fails is the
# cheapest way to make a suite green — so it gets a number. Run before and after and
# diff: the counts must be identical.
#
# Counts `#[test]` attributes rather than functions so the measure is unambiguous and
# reproducible by hand (`grep -c '#\[test\]' file`).
#
# Usage:
#   contrib/test_inventory.sh /tmp/inventory-before.txt
#   contrib/test_inventory.sh /tmp/inventory-after.txt
#   diff /tmp/inventory-before.txt /tmp/inventory-after.txt

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="${1:-/tmp/test-inventory.txt}"

ROOTS=(
    bin/dwowd/src
    bin/dwowd/tests
    src/linear/src
    src/sdk/src
    src/contract/deployooor
    src/contract/native_token
    src/contract/promissory_note
    src/contract/identity
    src/contract/oracle
    src/contract/attestation
    src/contract/purse
    src/contract/box
    src/contract/multisig
)

: > "$OUT"
for root in "${ROOTS[@]}"; do
    [ -d "$root" ] || continue
    while IFS= read -r file; do
        n="$(grep -c '^\s*#\[test\]' "$file")"
        [ "$n" -gt 0 ] && printf '%s\t%s\n' "$file" "$n"
    done < <(find "$root" -name '*.rs' | sort)
done >> "$OUT"

TOTAL="$(awk -F'\t' '{s+=$2} END{print s+0}' "$OUT")"
FILES="$(wc -l < "$OUT")"

echo "test-inventory: $TOTAL #[test] attributes across $FILES files"
echo "  written to $OUT"
