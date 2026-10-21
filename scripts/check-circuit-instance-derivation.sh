#!/bin/bash
# Gate: the Orchard-class rule over every `.zk` source (OBL-Z1).
#
# Every `constrain_instance(X)` must be derived in-circuit, bound by a preceding
# `constrain_equal_*`, redundant with respect to another exposed determination, or named in
# `script/circuit_free_instances.txt` with a host-side justification. Anything else is a public
# input the prover sets freely, which is the Orchard-class shape.
#
# WHY THIS IS A GATE AND NOT A SCRIPT IN THE OTHER THREE. `check-circuit-metadata-alignment.sh`
# compares *counts* of `constrain_instance` against metadata pushes; `check-circuit-domain-
# separation.sh` checks that a `poseidon_hash` carries *some* domain prefix; `hooks/pre-commit`
# pattern-matches one binding shape on staged files. None of them asks whether the value exposed
# is determined, so before this gate nothing in the repository did.
#
# WHAT A PASS MEANS. That the checker's model of the derivation held for every instance of every
# circuit it walked. It is a structural check over source text, not a soundness proof — see the
# module docstring of `script/circuit_instance_derivation.py` for the assumptions that carry the
# step from "determined in-circuit" to "means what the contract intends".
#
# Exit 0: every instance classified.  Exit 1: at least one is not — the checker names it.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SELF="$SCRIPT_DIR/$(basename "$0")"
REPO_ROOT="${CIRCUIT_INSTANCE_DERIVATION_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"

# ── Negative control ───────────────────────────────────────────────────────────────────────────────
# A gate that cannot fail is not a gate. Plant one witness exposed with no derivation — the
# Orchard-class shape this checker exists to catch — and require a non-zero verdict that names the
# file and the witness. Then replace it with a derived-then-exposed value and require zero. The
# worker derives its tree from its own location, so the control runs it as a module with that root
# reassigned rather than copying it; the opcode vocabulary is read from the real `src/zkas`.
if [ "${1:-}" = "--self-test" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    mkdir -p "$TMP/src/contract/demo/proof" "$TMP/script"
    BAD="$TMP/src/contract/demo/proof/bad.zk"
    cat > "$BAD" <<'FIXTURE'
witness "w" {
    Base secret
}

circuit "bad" {
    constrain_instance(secret);
}
FIXTURE
    if OUT="$(CIRCUIT_INSTANCE_DERIVATION_ROOT="$TMP" "$SELF" 2>&1)"; then
        echo "SELF-TEST FAIL: the planted free witness expose was not reported" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    ok=1
    printf '%s' "$OUT" | grep -q "bad.zk" || {
        echo "SELF-TEST FAIL: the offending file was not named" >&2; ok=0; }
    printf '%s' "$OUT" | grep -q "no derivation" || {
        echo "SELF-TEST FAIL: the Orchard-class shape was not named" >&2; ok=0; }
    [ "$ok" -eq 1 ] || { printf '%s\n' "$OUT" | sed 's/^/  | /' >&2; exit 1; }

    # Clean twin: the same expose, now derived in-circuit, must pass.
    cat > "$BAD" <<'FIXTURE'
constant "c" {
    Base ONE
}

witness "w" {
    Base secret
}

circuit "clean" {
    derived = poseidon_hash(ONE, secret);
    constrain_instance(derived);
}
FIXTURE
    CIRCUIT_INSTANCE_DERIVATION_ROOT="$TMP" "$SELF" >/dev/null || {
        echo "SELF-TEST FAIL: the clean fixture does not pass" >&2
        CIRCUIT_INSTANCE_DERIVATION_ROOT="$TMP" "$SELF" 2>&1 | sed 's/^/  | /' >&2
        exit 1
    }
    echo "SELF-TEST PASS: a free witness expose is named Orchard-class; the derived twin passes"
    exit 0
fi

# The worker hard-codes its tree at `dirname(dirname(__file__))`. When a root override is in force,
# load it as a module and reassign the paths it derives from that tree; the checker itself is run
# from its real location, not copied.
if [ "$REPO_ROOT" = "$(cd "$SCRIPT_DIR/.." && pwd)" ]; then
    exec python3 "$REPO_ROOT/script/circuit_instance_derivation.py" "$@"
fi
exec python3 - "$REPO_ROOT" "$SCRIPT_DIR/../script/circuit_instance_derivation.py" \
    "$SCRIPT_DIR/../src/zkas/opcode.rs" "$@" <<'PYEOF'
import importlib.util, os, sys

root, gate, opcodes = sys.argv[1], sys.argv[2], sys.argv[3]
sys.argv = [gate] + sys.argv[4:]
spec = importlib.util.spec_from_file_location("circuit_instance_derivation", gate)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
mod.REPO_ROOT = root
mod.MANIFEST = os.path.join(root, "script", "circuit_free_instances.txt")
mod.COMPARISON_MANIFEST = os.path.join(root, "script", "circuit_comparison_exceptions.txt")
mod.OPCODES_RS = opcodes
sys.exit(mod.main())
PYEOF
