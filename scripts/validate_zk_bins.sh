#!/bin/bash
# Validate all ZK binaries across all contracts
#
# Usage:
#   ./scripts/validate_zk_bins.sh           # Validate all
#   ./scripts/validate_zk_bins.sh rebuild    # Auto-rebuild corrupted
#   ./scripts/validate_zk_bins.sh contract   # Validate specific contract dir

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SELF="$SCRIPT_DIR/$(basename "$0")"
ROOT="${VALIDATE_ZK_BINS_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
ZKAS="${ZKAS:-$ROOT/zkas}"
[ -x "$ZKAS" ] || ZKAS="target/release/zkas"  # fallback for CI where only target/ exists
CONTRACT_DIR="src/contract"

# ── Negative control ───────────────────────────────────────────────────────────────────────────────
# A gate that cannot fail is not a gate. Plant a truncated `.zk.bin` beside a known-good one in a
# temp contract directory and require the validator to reject the former and accept the latter. It
# uses the built `zkas` compiler and a bin the tree already built; it never rebuilds anything.
if [ "${1:-}" = "--self-test" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    GOOD_SRC="$ROOT/proofs/core/mint.zk.bin"
    if [ ! -x "$ZKAS" ]; then
        echo "SELF-TEST FAIL: no zkas compiler at $ZKAS — the validator cannot be shown to fail" >&2
        exit 1
    fi
    if [ ! -f "$GOOD_SRC" ]; then
        echo "SELF-TEST FAIL: no built .zk.bin to use as the clean twin ($GOOD_SRC)" >&2
        echo "  This control never builds: run the circuit build first, then re-run the self-test." >&2
        exit 1
    fi
    mkdir -p "$TMP/contract/proof"
    cp "$GOOD_SRC" "$TMP/contract/proof/good.zk.bin"
    head -c 8 "$GOOD_SRC" > "$TMP/contract/proof/bad.zk.bin"
    if OUT="$("$SELF" "$TMP/contract/proof" 2>&1)"; then
        echo "SELF-TEST FAIL: the truncated .zk.bin was not rejected" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    ok=1
    printf '%s' "$OUT" | grep -q "CORRUPTED:.*bad.zk.bin" || {
        echo "SELF-TEST FAIL: the truncated bin was not named CORRUPTED" >&2; ok=0; }
    printf '%s' "$OUT" | grep -q "OK:.*good.zk.bin" || {
        echo "SELF-TEST FAIL: the clean bin was not accepted" >&2; ok=0; }
    [ "$ok" -eq 1 ] || { printf '%s\n' "$OUT" | sed 's/^/  | /' >&2; exit 1; }

    # Clean twin: only the good bin remains, and the validator must exit zero.
    rm -f "$TMP/contract/proof/bad.zk.bin"
    OUT="$("$SELF" "$TMP/contract/proof" 2>&1)" || {
        echo "SELF-TEST FAIL: the clean twin does not pass" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    }
    printf '%s' "$OUT" | grep -q "OK:.*good.zk.bin" || {
        echo "SELF-TEST FAIL: the clean bin was not accepted" >&2
        exit 1
    }
    echo "SELF-TEST PASS: a truncated .zk.bin is rejected CORRUPTED; a clean bin is accepted"
    exit 0
fi

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

# A check that validated nothing must not report success.
#
# `.zk.bin` is gitignored, so on a tree where the circuits have not been built
# this script printed "0 OK, 0 corrupted", noted that every source "has no binary
# here to check", and exited 0 — which is indistinguishable from a tree where
# every binary was validated and found well-formed. That is the tripwire defect:
# a guardrail that cannot see what it guards must not report success.
#
# `scripts/run-all-tests.sh` runs this immediately after `build-contract-zk.sh`,
# so inside the umbrella there is always something to check. Reaching this branch
# there means that ordering broke, which is itself the finding.
if [ $((OK + CORRUPTED)) -eq 0 ]; then
    echo -e "${RED}FAIL:${NC} no .zk.bin was validated — this check cannot see what it guards" >&2
    echo "  Build the circuits first: scripts/build-contract-zk.sh" >&2
    exit 2
fi
exit 0