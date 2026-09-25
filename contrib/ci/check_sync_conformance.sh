#!/bin/bash
# check_sync_conformance.sh — sync source declares its spec clause, and the
# command-registration matrix holds.
#
# AUTHORITY, corrected 2026-09-25. This header claimed to be a
# "sync-protocol.md §16 conformance gate" and its failure message cited "§16".
# **`sync-protocol.md` has no §16** — the section was deleted by `c7512b2269`
# ("collapse node sync to a wallet-shaped pull loop"), and the document's
# headings now run §14 straight to §17. The check passed because the headers
# happen to exist, not because any surviving clause required them.
#
# So its two halves have different, now-stated authorities:
#
#   * The `//! Spec:` header check enforces a **repository convention** — a
#     documentation discipline that keeps a file's clause mapping visible — and
#     NOT a spec clause. That makes it partition A under
#     doc/src/dev/testing/production-test-standard.md:339, where a convention
#     the compiler or a lint can enforce is not a test. It is kept, and it is
#     declared here for what it is, because the alternative (a rule-shaped
#     sentence citing a clause that does not exist) is the defect this
#     correction removes.
#   * The command-registration matrix below IS spec-backed, at
#     `sync-protocol.md §14.1` ("Command dispatch matrix + unknown-command
#     drain"). It previously cited §14.3, which does not exist — the section was
#     renumbered. See `scripts/check-authority-resolves.sh`, which found both.
#
# The full code↔clause mapping lives in doc/src/arch/sync-conformance.md.
# Zero output on success; explicit FAIL message on violation.
set -euo pipefail

if [ "${1:-}" = "--self-test" ]; then
    # Negative control: point the gate at an empty tree, where no sync source
    # file carries a header. It must FAIL. Before this control existed the gate
    # had no way to show it could report anything at all.
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    OUT="$(ROOT="$TMP" "$0" 2>&1)" && {
        echo "SELF-TEST FAIL: the header check did not notice an empty tree" >&2
        exit 1
    }
    if ! printf '%s' "$OUT" | grep -q 'missing .//! Spec: sync-protocol.md'; then
        echo "SELF-TEST FAIL: the gate failed, but not on the missing header" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    echo "SELF-TEST PASS: a missing '//! Spec:' header is reported"
    exit 0
fi

FAILED=0
# Overridable so the negative control above can point the checks at a temp tree.
ROOT="${ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"

echo "=== Sync conformance (header convention + §14.1 dispatch matrix) ==="

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
        echo "  Repository convention, not a spec clause (see this script's header):"
        echo "  a sync source file declares its clause in a '//! Spec:' module header."
        FAILED=1
    fi
done

# §14.1 command-registration matrix: the node registers the node-only push
# commands (`linearlblock`/`tx`); the wallet (pull-only) registers neither, so a
# peer that does not subscribe drains-and-ignores rather than desyncing.
# (Cited as §14.3 until 2026-09-25, when the section was found to be renumbered.)
echo ""
echo "=== Command registration matrix (sync-protocol.md §14.1) ==="

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
