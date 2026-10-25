#!/bin/bash
# Quotient-bound gate (OBL-Z12).
#
# `proofs/lean/src/DarkFi/BaseDivGadget.lean:qr_unique` states that quotient and remainder are
# unique **over the integers**. A circuit that forms a quotient from field elements proves the
# same arithmetic over field residues unless it establishes that its operands are small integers.
# `range_check(N, operand)` is the on-chain establishment of that bound.
#
# The known quotient-remainder circuits are listed in `script/quotient_bounds.txt`; this gate
# requires every listed operand to carry a `range_check` in its file. A new quotient circuit is
# added to the manifest in the same change that introduces it; removing a check fails here.
#
# Exit 0: every listed operand is range-checked. Exit 1: one is not (named). Exit 2: malformed
# manifest or missing file.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="${QUOTIENT_BOUNDS_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"

if [ "${1:-}" = "--self-test" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    mkdir -p "$TMP/script" "$TMP/src/contract/demo/proof"
    CLEAN="$TMP/src/contract/demo/proof/clean.zk"
    BAD="$TMP/src/contract/demo/proof/bad.zk"
    cat > "$CLEAN" <<'FIXTURE'
circuit "clean" {
    range_check(64, numerator);
    range_check(64, denominator);
}
FIXTURE
    printf 'src/contract/demo/proof/clean.zk : numerator denominator\n' \
        > "$TMP/script/quotient_bounds.txt"
    if ! OUT="$(QUOTIENT_BOUNDS_ROOT="$TMP" "$0" 2>&1)"; then
        echo "SELF-TEST FAIL: the clean fixture does not pass" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi

    # Planted defect: the denominator loses its range check.
    cat > "$BAD" <<'FIXTURE'
circuit "bad" {
    range_check(64, numerator);
}
FIXTURE
    printf 'src/contract/demo/proof/bad.zk : numerator denominator\n' \
        > "$TMP/script/quotient_bounds.txt"
    # The gate's relative path root is the manifests' own root; `$0` is this script either way.
    if OUT="$(QUOTIENT_BOUNDS_ROOT="$TMP" "$SCRIPT_DIR/$(basename "$0")" 2>&1)"; then
        echo "SELF-TEST FAIL: a missing range check was not reported" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    ok=1
    printf '%s' "$OUT" | grep -q "bad.zk" || { echo "SELF-TEST FAIL: the file was not named" >&2; ok=0; }
    printf '%s' "$OUT" | grep -q "denominator" || { echo "SELF-TEST FAIL: the operand was not named" >&2; ok=0; }
    [ "$ok" -eq 1 ] || { printf '%s\n' "$OUT" | sed 's/^/  | /' >&2; exit 1; }

    echo "SELF-TEST PASS: the planted missing range check is named; the clean fixture passes"
    exit 0
fi

MANIFEST="$REPO_ROOT/script/quotient_bounds.txt"
if [ ! -f "$MANIFEST" ]; then
    echo "ERROR: missing $MANIFEST" >&2
    exit 2
fi

fail=0
while IFS= read -r raw; do
    entry="${raw%%#*}"
    entry="$(printf '%s' "$entry" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    [ -n "$entry" ] || continue
    if ! printf '%s' "$entry" | grep -q ' : '; then
        echo "ERROR: malformed line in quotient_bounds.txt: $entry" >&2
        exit 2
    fi
    rel="${entry%% : *}"
    vars="${entry#* : }"
    file="$REPO_ROOT/$rel"
    if [ ! -f "$file" ]; then
        echo "FAIL: $rel is listed in quotient_bounds.txt but does not exist" >&2
        fail=1
        continue
    fi
    for var in $vars; do
        if ! grep -Eq "range_check\([[:space:]]*[0-9]+[[:space:]]*,[[:space:]]*${var}[[:space:]]*\)" "$file"; then
            echo "FAIL: $rel: quotient operand '$var' has no range_check (OBL-Z12)" >&2
            fail=1
        fi
    done
done < "$MANIFEST"

if [ "$fail" -ne 0 ]; then
    echo "" >&2
    echo "Fix: establish the operand bound in-circuit, or remove the quotient from the circuit." >&2
    echo "The Lean theorem gives uniqueness over integers; the range check is what makes them integers." >&2
    exit 1
fi

echo "PASS: every listed quotient operand carries a range_check (script/quotient_bounds.txt)."
