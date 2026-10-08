#!/bin/bash
# ZK binary freshness checker (RG-9)
#
# WHAT "FRESH" MEANS, and it is a question about BYTES. The contract embeds its circuits with
# `include_bytes!`, so the wasm is stale exactly when it does not carry the `.zk.bin` bytes that are
# on disk now. That is the test this script makes.
#
# **It used to compare mtimes (`-nt`) and it no longer does — `OBL-C142`.** A `.zk.bin` rewrite that
# produces *byte-identical* output moves the mtime and tripped the check, whose message then asserted
# a "VK mismatch" it had not established and whose remedy was `--fix`, i.e. rebuild. Measured
# 2026-09-25 on `box`: `take.zk.bin`'s 206 bytes are present verbatim in the 300162-byte wasm, so
# harness and wasm embed identical circuit bytes and no mismatch existed — and `box` is one of the
# nine genesis contracts, so the action it asked for can move `bin/dwowd/genesis_hash.txt`. A false
# positive whose suggested fix is a pin-moving rebuild is the worst shape a gate can have.
#
# A timestamp cannot answer "does this artifact contain these bytes"; `embeds` below can.
#
# Usage:
#   ./check_zkbin_freshness.sh            # report; exit 1 when a wasm does not embed a circuit
#   ./check_zkbin_freshness.sh --fix      # rebuild the contracts reported
#   ./check_zkbin_freshness.sh --self-test
# Exit 0 = all fresh, Exit 1 = stale WASM found, Exit 2 = error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
CONTRACT_DIR="$REPO_ROOT/src/contract"

# Does `$1` contain `$2`'s bytes verbatim? The whole verdict rests here, so it is one function with
# one job rather than a comparison spelled out at the call site.
embeds() {
    python3 - "$1" "$2" <<'PYEOF'
import sys
with open(sys.argv[1], "rb") as f:
    wasm = f.read()
with open(sys.argv[2], "rb") as f:
    zkbin = f.read()
sys.exit(0 if zkbin in wasm else 1)
PYEOF
}

# ── THE SELF-TEST (R8) ───────────────────────────────────────────────────────────────────────
#
# Three assertions, and the third is the row itself: a byte-identical rewrite with a NEWER mtime
# must decide nothing. Without it, this script could pass its own self-test while still being the
# timestamp check it replaced.
if [[ "${1:-}" == "--self-test" ]]; then
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    printf 'AAAABBBBCCCC' > "$tmp/wasm"
    printf 'BBBB' > "$tmp/embedded.zk.bin"
    printf 'ZZZZ' > "$tmp/absent.zk.bin"
    failed=0
    if ! embeds "$tmp/wasm" "$tmp/embedded.zk.bin"; then
        echo "SELF-TEST FAILED: a circuit whose bytes are in the wasm read as absent."
        failed=1
    fi
    if embeds "$tmp/wasm" "$tmp/absent.zk.bin"; then
        echo "SELF-TEST FAILED: a circuit whose bytes are NOT in the wasm read as present."
        failed=1
    fi
    # The regression this script exists to not have: same bytes, newer mtime.
    touch "$tmp/embedded.zk.bin"
    if ! embeds "$tmp/wasm" "$tmp/embedded.zk.bin"; then
        echo "SELF-TEST FAILED: a byte-identical rewrite decided the verdict — that is the mtime bug."
        failed=1
    fi
    if [[ "$failed" -ne 0 ]]; then exit 1; fi
    echo "SELF-TEST OK: embedded bytes read fresh, absent bytes read stale, and a newer mtime over"
    echo "              identical bytes decides nothing."
    exit 0
fi

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

    # Check each .zk.bin in the proof directory
    for zkbin in "$proof_dir"/*.zk.bin; do
        [[ -f "$zkbin" ]] || continue
        zkbin_name=$(basename "$zkbin")

        if ! embeds "$wasm_path" "$zkbin"; then
            STALE=$((STALE + 1))
            echo "[STALE] $contract: $wasm_name does not embed $zkbin_name's current bytes"
            echo "        The artifact was built before this circuit's bytes changed."
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
        echo "       A stale WASM embeds circuits other than those on disk, so the harness and the"
        echo "       artifact disagree about the circuit the proof is made against."
    fi
    exit 1
fi

echo "[OK] Every WASM embeds the current bytes of every circuit in its proof/ directory."
exit 0
