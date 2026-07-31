#!/bin/bash
# HAZOP RC-D: Verify all ZK circuits have domain-separated poseidon_hash calls.
# V2 circuits prepend DOMAIN_* constants (witness_base(1..7)) to every hash.
# V1 circuits use bare poseidon_hash(inputs...) — this script catches them.
#
# Exit 0: all circuits have domain separation
# Exit 1: found undifferentiated poseidon_hash calls

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

UNDIFFERENTIATED=$(grep -rn 'poseidon_hash(' "$REPO_ROOT"/src/contract/*/proof/*.zk \
    | grep -v 'DOMAIN_\|witness_base\|//.*allowed\|#.*allowed' \
    || true)

if [ -z "$UNDIFFERENTIATED" ]; then
    echo "PASS: All contract circuits have domain-separated poseidon_hash calls"
    exit 0
else
    echo "FAIL: Found undifferentiated poseidon_hash calls (missing DOMAIN_ prefix):"
    echo "$UNDIFFERENTIATED"
    echo ""
    echo "Fix: prepend the appropriate DOMAIN_ constant to each poseidon_hash call."
    echo "  DOMAIN_NULLIFIER    = witness_base(1)"
    echo "  DOMAIN_TOKEN_COMMIT = witness_base(2)"
    echo "  DOMAIN_TX_BINDING   = witness_base(3)"
    echo "  DOMAIN_COIN_COMMIT  = witness_base(4)"
    echo "  DOMAIN_USER_DATA_ENC = witness_base(6)"
    echo "  DOMAIN_SIGNATURE_SECRET = witness_base(7)"
    exit 1
fi
