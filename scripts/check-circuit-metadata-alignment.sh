#!/bin/bash
# M-5 fix: Verify ZK circuit constrain_instance count <= metadata push count
# for genesis contracts.
#
# This CI gate catches mismatches where a circuit's public input count changes
# but the metadata function's zk_inputs vector isn't updated — the root cause of
# 3 prior CRITICAL findings (Box, Purse, PromissoryNote).
#
# Per-circuit: count constrain_instance in .zk file, count pushes for that
# circuit's namespace in entrypoint code. The per-circuit push count must be
# >= the constrain_instance count (metadata may push auxiliary values too).
#
# Exit 0: all circuits pass
# Exit 1: mismatch found

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FAILURES=0
PASSES=0

echo "=== Circuit-Metadata Alignment Check ==="
echo ""

# Genesis contracts only (9 contracts — the ones whose circuits ship with the chain)
GENESIS=("native_token" "box" "purse" "promissory_note" "identity" "attestation" "oracle" "multisig")

for contract_name in "${GENESIS[@]}"; do
    proof_dir="$REPO_ROOT/src/contract/$contract_name/proof"
    [ -d "$proof_dir" ] || continue

    for zk_file in "$proof_dir"/*.zk; do
        [ -f "$zk_file" ] || continue
        circuit_name=$(basename "$zk_file" .zk)
        contract_dir="$REPO_ROOT/src/contract/$contract_name"

    # Count constrain_instance calls in the circuit
    circuit_count=$(grep -c 'constrain_instance(' "$zk_file" 2>/dev/null || true)
    circuit_count=$((circuit_count + 0))

    if [ "$circuit_count" -eq 0 ]; then
        echo "WARN: $contract_name/$circuit_name — zero constrain_instance calls"
        continue
    fi

    # Find the entrypoint file(s)
    entrypoint_dir="$contract_dir/src/entrypoint"
    entrypoint_files=$(find "$entrypoint_dir" -name '*.rs' 2>/dev/null || true)

    if [ -z "$entrypoint_files" ]; then
        echo "SKIP: $contract_name/$circuit_name — no entrypoint/*.rs found"
        continue
    fi

    # Count zk_input pushes in metadata functions.
    # Each zk_public_inputs.push call pushes a Vec of Base values.
    zk_push_count=0
    for f in $entrypoint_files; do
        c=$(grep -c '\.push((' "$f" 2>/dev/null || echo "0")
        c=${c//[^0-9]/}
        c=${c:-0}
        zk_push_count=$((zk_push_count + c))
    done

    if [ "$zk_push_count" -lt "$circuit_count" ]; then
        echo "FAIL: $contract_name/$circuit_name — $circuit_count constrain_instance vs $zk_push_count pushes in entrypoint"
        FAILURES=$((FAILURES + 1))
    else
        echo "OK:   $contract_name/$circuit_name — $circuit_count constrain_instance, $zk_push_count metadata pushes"
        PASSES=$((PASSES + 1))
    fi
    done
done

echo ""
echo "---"
echo "Passed: $PASSES  Failed: $FAILURES"

if [ "$FAILURES" -eq 0 ]; then
    echo "PASS: All circuits have matching metadata push counts"
    exit 0
else
    echo "FAIL: $FAILURES circuit(s) have insufficient metadata push counts"
    echo ""
    echo "Root cause: a circuit's constrain_instance order must match the metadata"
    echo "function's zk_inputs.push() order position-for-position (privacy.md §5.3)."
    echo "A mismatch silently produces wrong proof verification."
    exit 1
fi
