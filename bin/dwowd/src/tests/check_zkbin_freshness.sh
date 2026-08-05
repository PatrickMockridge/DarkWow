#!/bin/bash
# ZK binary freshness checker (RG-9)
# Verifies every contract's WASM is not older than any .zk.bin it embedds.
# A stale WASM causes VK mismatch: harness embeds new zkbin, WASM embeds old.
# Usage: ./check_zkbin_freshness.sh [--fix]
# Exit 0 = all fresh, Exit 1 = stale WASM found, Exit 2 = error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
CONTRACT_DIR="$REPO_ROOT/src/contract"

STALE=0
FIX_MODE=false
if [[ "${1:-}" == "--fix" ]]; then
    FIX_MODE=true
fi

# Contracts with ZK circuits whose .zk.bin files are embedded in their WASM
# Format: contract_dir|wasm_filename
CONTRACTS=(
    "native_token|dwow_native_token_contract.wasm"
    "identity|dwow_identity_contract.wasm"
    "attestation|dwow_attestation_contract.wasm"
    "multisig|dwow_multisig_contract.wasm"
    "oracle|dwow_oracle_contract.wasm"
    "promissory_note|dwow_promissory_note_contract.wasm"
    "purse|dwow_purse_contract.wasm"
    "box|dwow_box_contract.wasm"
)

for entry in "${CONTRACTS[@]}"; do
    IFS='|' read -r contract wasm_name <<< "$entry"
    wasm_path="$CONTRACT_DIR/$contract/$wasm_name"
    proof_dir="$CONTRACT_DIR/$contract/proof"

    if [[ ! -f "$wasm_path" ]]; then
        echo "[WARN] $contract: WASM not found at $wasm_path"
        continue
    fi

    if [[ ! -d "$proof_dir" ]]; then
        # deployooor has no proof dir — skip
        continue
    fi

    wasm_mtime=$(stat -c %Y "$wasm_path" 2>/dev/null || stat -f %m "$wasm_path" 2>/dev/null)

    # Check each .zk.bin in the proof directory
    for zkbin in "$proof_dir"/*.zk.bin; do
        [[ -f "$zkbin" ]] || continue
        zkbin_name=$(basename "$zkbin")
        zkbin_mtime=$(stat -c %Y "$zkbin" 2>/dev/null || stat -f %m "$zkbin" 2>/dev/null)

        if [[ "$zkbin_mtime" -gt "$wasm_mtime" ]]; then
            STALE=$((STALE + 1))
            echo "[STALE] $contract: $zkbin_name ($zkbin_mtime) is newer than $wasm_name ($wasm_mtime)"
            echo "        Harness embeds new zkbin; WASM embeds old — VK mismatch will occur."
            if $FIX_MODE; then
                echo "        Rebuilding WASM..."
                make -C "$CONTRACT_DIR/$contract" all 2>&1 | sed 's/^/        /'
                echo "        [FIXED] $contract WASM rebuilt"
            fi
        fi
    done
done

if [[ "$STALE" -gt 0 ]]; then
    echo ""
    if $FIX_MODE; then
        echo "[FIXED] $STALE stale WASM(s) rebuilt."
    else
        echo "[FAIL] $STALE stale WASM(s) found. Run with --fix to rebuild."
        echo "       Stale WASMs cause VK mismatch: harness embeds new zkbin, WASM embeds old."
        echo "       Identity test failure (k=12 vs k=13) was caused by exactly this."
    fi
    exit 1
fi

echo "[OK] All WASM files are fresh (no zkbin newer than its WASM)."
exit 0
