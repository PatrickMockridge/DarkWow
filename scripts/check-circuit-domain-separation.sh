#!/bin/bash
# HAZOP RC-D: ZK circuits separate their poseidon_hash calls by domain.
#
# V2 circuits prepend a DOMAIN_* constant (a `witness_base(n)`) to every hash so that two
# derivations in one circuit cannot be confused for each other. This script checks three
# things, in increasing strength. What it *is not* is a global table check, and knowing why is
# the difference between a gate and a nuisance — see below.
#
#   1. PRESENCE. Every poseidon_hash call's argument list carries DOMAIN_ or witness_base.
#      This is the original check, unchanged. It is deliberately syntactic: a call that
#      reaches the domain through a named constant is fine, and a call with a bare hash is not.
#
#   2. COLLISION, within one circuit. Two *differently-named* domains must not share a value.
#      This is the property RC-D is actually about, and it is local rather than global: a
#      domain constant is baked into one circuit's zkas and never meets another circuit's, so
#      only a collision inside a circuit can defeat the separation. Measured 2026-09-23, the
#      tree uses `DOMAIN_NULLIFIER` with a dozen different values (1, 2, 3, 8, 9, 10, 11, 12,
#      13, 14), and that is fine; what is not fine is one circuit giving the same value to two
#      names, which is how `betting_stake`'s nullifier and tx binding came to share one, and
#      how `push_value_commitment`'s operator commitment and value commitment did.
#
#   3. ROLE. A derivation's domain must match the purpose its name declares — a nullifier is
#      hashed in the nullifier domain, a tx binding in the tx-binding domain, and so on.
#      Compared by *value*, so a circuit's own alias for the right domain passes.
#
# A call that carries the domain inline — `poseidon_hash(witness_base(9), ...)` — is left to the
# presence check rather than guessed at: it has no name to compare against, and inventing one is
# how a gate cries wolf. That residue is real and is recorded in the register rather than hidden.
#
# Exit 0: clean, or every finding adjudicated in `script/circuit_domain_exceptions.txt`.
# Exit 1: at least one unexcepted finding.
# Exit 2: the reviewed list is malformed — a broken list must not read as a pass.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SELF="$SCRIPT_DIR/$(basename "$0")"
REPO_ROOT="${DOMAIN_SEPARATION_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"

# ── Negative control ───────────────────────────────────────────────────────────────────────────────
# A gate that cannot fail is not a gate. Three halves, all on a temp tree so the real corpus is not
# touched: (1) a circuit with a bare `poseidon_hash` must fail and name the file and the missing
# domain; (2) a declared exception that matches no finding must fail and name the dead entry — the
# same control `scripts/run-all-tests.sh` runs inline; (3) the clean twin must pass.
if [ "${1:-}" = "--self-test" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    mkdir -p "$TMP/src/contract/demo/proof"
    CLEAN="$TMP/src/contract/demo/proof/clean.zk"
    BAD="$TMP/src/contract/demo/proof/bad.zk"
    cat > "$CLEAN" <<'FIXTURE'
DOMAIN_NULLIFIER = witness_base(1)

circuit "clean" {
    n = poseidon_hash(DOMAIN_NULLIFIER, secret);
    constrain_instance(n);
}
FIXTURE

    # Clean twin first: no findings and no exception list must be exit 0.
    if ! OUT="$(DOMAIN_SEPARATION_ROOT="$TMP" DOMAIN_EXCEPTIONS="$TMP/no-exceptions" \
               "$SELF" 2>&1)"; then
        echo "SELF-TEST FAIL: the clean fixture does not pass" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi

    # Planted defect: a poseidon_hash call with no domain.
    cat > "$BAD" <<'FIXTURE'
circuit "bad" {
    h = poseidon_hash(secret, nonce);
}
FIXTURE
    if OUT="$(DOMAIN_SEPARATION_ROOT="$TMP" DOMAIN_EXCEPTIONS="$TMP/no-exceptions" "$SELF" 2>&1)"; then
        echo "SELF-TEST FAIL: the planted bare poseidon_hash was not reported" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    ok=1
    printf '%s' "$OUT" | grep -q "bad.zk" || {
        echo "SELF-TEST FAIL: the offending file was not named" >&2; ok=0; }
    printf '%s' "$OUT" | grep -q "without DOMAIN_/witness_base" || {
        echo "SELF-TEST FAIL: the missing domain was not named" >&2; ok=0; }
    [ "$ok" -eq 1 ] || { printf '%s\n' "$OUT" | sed 's/^/  | /' >&2; exit 1; }

    # Stale-exception defect: the same check the umbrella runs inline. The planted path is assembled
    # at run time in the umbrella's idiom.
    rm -f "$BAD"
    P="src/contract/""nowhere/proof/""absent.zk"
    printf '%s :: collision :: 3 : planted stale entry\n' "$P" > "$TMP/exceptions"
    if OUT="$(DOMAIN_SEPARATION_ROOT="$TMP" DOMAIN_EXCEPTIONS="$TMP/exceptions" "$SELF" 2>&1)"; then
        echo "SELF-TEST FAIL: the planted stale exception was not reported" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    ok=1
    printf '%s' "$OUT" | grep -q "$P" || {
        echo "SELF-TEST FAIL: the stale exception's path was not named" >&2; ok=0; }
    printf '%s' "$OUT" | grep -q "match no finding" || {
        echo "SELF-TEST FAIL: the stale exception was not classified as covering nothing" >&2; ok=0; }
    [ "$ok" -eq 1 ] || { printf '%s\n' "$OUT" | sed 's/^/  | /' >&2; exit 1; }

    # Role defect (OBL-Z3): a derivation whose name declares one purpose is hashed in another's
    # domain. `commitment` is a commitment; `DOMAIN_NULLIFIER` is a nullifier domain.
    cat > "$BAD" <<'FIXTURE'
DOMAIN_NULLIFIER = witness_base(1)
circuit "role" {
    commitment = poseidon_hash(DOMAIN_NULLIFIER, secret);
    constrain_instance(commitment);
}
FIXTURE
    if OUT="$(DOMAIN_SEPARATION_ROOT="$TMP" DOMAIN_EXCEPTIONS="$TMP/no-exceptions" "$SELF" 2>&1)"; then
        echo "SELF-TEST FAIL: the planted role mismatch was not reported" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    if ! printf '%s' "$OUT" | grep -q "does not carry"; then
        echo "SELF-TEST FAIL: the role mismatch was not named" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi

    echo "SELF-TEST PASS: a bare poseidon_hash, a role mismatch and a stale exception are each named; the clean fixture passes"
    exit 0
fi

REPO_ROOT="$REPO_ROOT" python3 "$SCRIPT_DIR/../script/circuit_domain_separation.py"
