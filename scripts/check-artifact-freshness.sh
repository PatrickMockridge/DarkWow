#!/bin/bash
# Contract artifact freshness: the committed .source_hash must match the sources,
# and every live circuit namespace constant must name a circuit that exists.
#
# Why this is its own gate rather than left inside `make test`. Both checks already
# existed — every contract's Makefile has a `check-source-hash` target, and `make test`
# runs it via its `contracts` prerequisite. What was missing is that a failure there is
# a *bare* failure: `make test` stops at the first stale contract with
#
#     WARNING: dwow_dex_contract.wasm is stale.  ... make[1]: *** Error 1
#
# and nothing says how many others are stale or which. On 2026-09-22 nine contracts were
# stale — every one of them invalidated by a single `src/sdk/**` edit, because each
# contract's SOURCE_MANIFEST covers all of src/sdk — and the only way to find them was to
# run the per-contract target 32 times by hand. This names them in one command.
#
# Check 2 is `circuit-versioning.md` rule 5, which has had no gate since it was written:
# a Rust namespace constant must reproduce the .zk circuit declaration character for
# character. A constant naming a circuit that does not exist is dead code that disguises
# which circuit is actually in use. Note the distinction this draws, deliberately:
#   - a dead *_V2* constant is a BUG (a live lookup key that resolves to nothing) -> fails
#   - a dead legacy constant is HYGIENE (misleading, no runtime effect)      -> reported
# The second is not a reason to hold a build; pretending it is would make this gate the
# kind that gets weakened. It is counted and printed so it cannot be forgotten.
#
# Exit 0: every artifact is fresh and every live namespace constant resolves.
# Exit 1: at least one stale artifact or dead *_V2* constant, listed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

FURNITURE_UNSTALE=0
NEW_GENERATED=0

echo "=== 1. contract artifact freshness (.source_hash vs sources) ==="
STALE=()
FRESH=0
for d in src/contract/*/; do
    name="$(basename "$d")"
    [ -f "$d/Makefile" ] || continue
    [ -f "$d/.source_hash" ] || { echo "  NO .source_hash: $name"; STALE+=("$name"); continue; }
    if make -C "$d" check-source-hash >/dev/null 2>&1; then
        FRESH=$((FRESH + 1))
    else
        STALE+=("$name")
    fi
done

if [ "${#STALE[@]}" -eq 0 ]; then
    echo "  all $FRESH artifacts fresh"
else
    echo "  STALE (${#STALE[@]} of $((FRESH + ${#STALE[@]}))):"
    for c in "${STALE[@]}"; do echo "    $c"; done
    echo "  A stale artifact means its sources moved and nobody rebuilt it. Note that"
    echo "  src/sdk/** is in EVERY contract's SOURCE_MANIFEST, so one sdk edit makes all"
    echo "  32 stale at once — rebuild them in the same commit as the sdk change:"
    echo "    make -C src/contract/<name> clean all"
fi

echo ""
echo "=== 2. namespace constants resolve to declared circuits ==="
DEAD_V2=()
DEAD_LEGACY=0
for d in src/contract/*/; do
    name="$(basename "$d")"
    [ -d "$d/proof" ] || continue
    declared="$(grep -h 'circuit "' "$d"/proof/*.zk 2>/dev/null | sed 's/.*circuit "\([^"]*\)".*/\1/' | sort -u)"
    [ -n "$declared" ] || continue
    while IFS= read -r line; do
        const="${line%%:*}"
        circ="${line##*:}"
        [ -n "$circ" ] || continue
        if ! printf '%s\n' "$declared" | grep -qx "$circ"; then
            case "$const" in
                *NS_V2|*_NS_V2) DEAD_V2+=("$name  $const = \"$circ\"") ;;
                *)              DEAD_LEGACY=$((DEAD_LEGACY + 1)) ;;
            esac
        fi
    done < <(grep -ho 'ZKAS_[A-Z0-9_]*: &str = "[^"]*"' "$d/src/lib.rs" 2>/dev/null \
             | sed 's/^\(ZKAS_[A-Z0-9_]*\): &str = "\([^"]*\)"$/\1:\2/')
done

if [ "${#DEAD_V2[@]}" -eq 0 ]; then
    echo "  every live (_V2) namespace constant resolves"
else
    echo "  DEAD _V2 CONSTANTS (${#DEAD_V2[@]}) — these are live lookup keys resolving to nothing:"
    for x in "${DEAD_V2[@]}"; do echo "    $x"; done
fi
if [ "$DEAD_LEGACY" -gt 0 ]; then
    echo "  ($DEAD_LEGACY dead legacy constant(s) — dead code that misleads about which"
    echo "   circuit is in use; no runtime effect. Cleanup, not a blocker.)"
fi

echo ""
echo "========================================"
if [ "${#STALE[@]}" -eq 0 ] && [ "${#DEAD_V2[@]}" -eq 0 ]; then
    echo "PASS: $FRESH artifacts fresh, all live namespace constants resolve"
    exit 0
fi
echo "FAIL: ${#STALE[@]} stale artifact(s), ${#DEAD_V2[@]} dead _V2 constant(s)"
echo "========================================"
exit 1
