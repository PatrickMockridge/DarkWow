#!/bin/bash
# check_sync_conformance.sh — sync-protocol.md §16 conformance gate
# Runs on every push. Enforces that every sync source file carries a
# module-level `//! Spec: sync-protocol.md §N` header, so a file whose header
# drifts from the spec fails the build.
#
# The full code↔clause mapping lives in doc/src/arch/sync-conformance.md.
# Zero output on success; explicit FAIL message on violation.
set -euo pipefail

FAILED=0
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

echo "=== Sync Conformance (sync-protocol.md §16) ==="

# The sync source files (one per ρ-process role + its supporting types).
# Each carries a `//! Spec:` header mapping it to its clause(s).
SYNC_FILES=(
    "src/linear/src/sync_types.rs"
    "src/linear/src/sync_boundary.rs"
    "src/linear/src/sync_connection.rs"
    "bin/dwowd/src/task/consensus_linear.rs"
    "bin/dwowd/src/proto/linear_sync_client.rs"
    "bin/dwowd/src/proto/linear_broadcast.rs"
    "bin/dwowd/src/proto/mod.rs"
    "bin/dww/src/sync_task.rs"
    "bin/dww/src/p2p_wallet.rs"
)

for f in "${SYNC_FILES[@]}"; do
    echo -n "[conformance] $f ... "
    if grep -q "Spec: sync-protocol.md" "$ROOT/$f" 2>/dev/null; then
        echo "PASS"
    else
        echo "FAIL"
        echo "  missing '//! Spec: sync-protocol.md §N' module header"
        echo "  sync-protocol.md §16: every sync source file declares its spec clause."
        FAILED=1
    fi
done

# §14.3 command-registration matrix: the node registers the node-only push
# commands (`linearlblock`/`tx`); the wallet (pull-only) registers neither, so a
# peer that does not subscribe drains-and-ignores rather than desyncing.
echo ""
echo "=== Command registration matrix (sync-protocol.md §14.3) ==="

echo -n "[dispatch] node registers 'linearlblock' ... "
if grep -rq '"linearlblock"' "$ROOT/bin/dwowd/src/proto/" 2>/dev/null; then
    echo "PASS"
else
    echo "FAIL  (node must register the block-broadcast command)"
    FAILED=1
fi

echo -n "[dispatch] node registers 'tx' ... "
if grep -rq 'ProtocolTx' "$ROOT/bin/dwowd/src/proto/" 2>/dev/null; then
    echo "PASS"
else
    echo "FAIL  (node must register the transaction relay command)"
    FAILED=1
fi

echo -n "[dispatch] wallet does not register 'linearlblock' ... "
if grep -rq 'linearlblock\|BlockBroadcast' "$ROOT/bin/dww/src/" 2>/dev/null; then
    echo "FAIL  (wallet must NOT register the block-broadcast command)"
    FAILED=1
else
    echo "PASS"
fi

echo -n "[dispatch] wallet does not register 'tx' ... "
if grep -rq 'ProtocolTx' "$ROOT/bin/dww/src/" 2>/dev/null; then
    echo "FAIL  (wallet must NOT register the transaction relay command)"
    FAILED=1
else
    echo "PASS"
fi

echo ""
if [ "$FAILED" -eq 1 ]; then
    echo "=== SYNC CONFORMANCE FAILED ==="
    exit 1
else
    echo "=== SYNC CONFORMANCE PASSED ==="
    exit 0
fi
