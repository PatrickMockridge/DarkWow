#!/bin/bash
# The class the two existing circuit detectors cannot see: **a value the circuit computes that
# reaches no constraint.**
#
# `scripts/check-pubkey-binding.sh` adjudicates the *operands of a `constrain_equal_*`*, and
# `scripts/check-circuit-instance-derivation.sh` classifies each *`constrain_instance` target*.
# Both reason about values that appear in a constraint. A circuit that constrains nothing —
# or that computes a hash and drops it — has no operands and no instances to inspect, so it
# is invisible to both. That is not a hypothetical gap: it is the shape the attestation
# contract shipped with and the reason github issue #3 could forge an attestation.
#
# Measured on the attestation contract before its repair (`linear-master @ d775e37c6d`):
#
#   * `attester_secret`, `attester_pub_x` and `attester_pub_y` were witnessed and referenced
#     by **nothing** — no derivation, no equality, no instance — in six circuits
#     (`create_attestation`, `create_claim`, `attest_slash`, `commit_fee_schedule`,
#     `update_delegation`, and `delegate_attestation`'s `delegator_secret`), while the host
#     attributed a stored record to the caller-supplied public key. One party's own secret
#     could therefore attest in another party's name.
#   * `VerifyClaimV2` computed `evidence_hash`, `attestation_hash` and `leaf` and referenced
#     none of them again: three hashes calculated and discarded.
#
# THE RULE, in the shallowest form that catches the class — two halves, both source-level and
# mechanical, with no claim about cryptography:
#
#   H1  THE DEAD WITNESS       a `witness` name that appears in no derivation, no equality
#                              and no instance.
#   H2  THE DEAD DERIVATION    a value assigned in-circuit that is neither instanced nor
#                              reaches a `constrain_equal_*`, directly or through another
#                              assignment.
#
# Both are the same finding: a value the circuit computed that constrains nothing. H1 is the
# one that matters for authorization — a secret nothing is derived from authorizes nothing —
# and H2 is the one that matters for soundness, since a discarded hash is a check the reader
# believes was made.
#
# WHAT A PASS MEANS, and what it does not. That every witness and every assignment in the
# circuits walked reaches a constraint or is declared below. It is a structural check over
# source text: it does not know what a circuit *means*, and a value that reaches a constraint
# it does not belong in passes here (that is `check-pubkey-binding.sh`'s and
# `check-circuit-instance-derivation.sh`'s business).
#
# THE LIST IS A RATCHET, NOT AN AMNESTY. The first run over 32 contracts finds more than the
# attestation contract, and each entry is added by *reading the circuit and its host together*
# — the disposition that matters is whether the value has a host read to be bound to. A value
# whose host never reads it is usually removed rather than exposed: `AGENTS.md` R2, "remove the
# root cause; do not add a compensating guard". A value whose host *does* read it is exposed by
# `constrain_instance` and the host's own comparison, as `consume_claim.zk` already does.
#
# Wired as a REPORT until the list is adjudicated — see `hooks/pre-commit`, which is REPORT-ONLY
# for this checker for the same reason it was report-only for `check-pubkey-binding.sh` until
# 2026-09-23: a gate whose authority is not yet defensible is worse than a report. The register
# row is `OBL-Z23`.
#
# Usage:
#   scripts/check-circuit-dead-values.sh                # all circuits
#   scripts/check-circuit-dead-values.sh <files...>     # specific files
#   scripts/check-circuit-dead-values.sh --self-test    # plant a defect, require exit 1
#   scripts/check-circuit-dead-values.sh --report-only  # exit 0 even on findings
#
# Exit 0: no findings, or every finding declared. Exit 1: at least one undeclared finding.
# Exit 2: the declaration list is malformed. Exit 3: a file that should have been walked was
#         not — a check that silently checks nothing is worse than no check.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SELF_TEST=0
REPORT_ONLY=0
TARGETS=()
for arg in "$@"; do
    case "$arg" in
        --self-test) SELF_TEST=1 ;;
        --report-only) REPORT_ONLY=1 ;;
        *) TARGETS+=("$arg") ;;
    esac
done

if [ "$SELF_TEST" -eq 1 ]; then
    TMPDIR_SELF="$(mktemp -d)"
    trap 'rm -rf "$TMPDIR_SELF"' EXIT
    # TWO planted defects in an otherwise well-formed circuit, and they are the two shapes
    # github issue #3 shipped: a witness nothing derives from (the authorization hole —
    # `attester_secret` and its coordinates in six attestation circuits) and a hash computed
    # and discarded (`VerifyClaimV2`'s `evidence_hash`, `attestation_hash` and `leaf`).
    #
    # `orphan_secret` is referenced by nothing at all, which is what makes it dead: a secret
    # reached through a derivation is live even when no host reads it, and planting one of
    # those would test the wrong thing.
    cat > "$TMPDIR_SELF/dead_values.zk" <<'ZKEOF'
k = 11; field = "pallas";
constant "SelfTestV2" { EcFixedPointBase NULLIFIER_K, }
witness "SelfTestV2" {
    Base actor_secret, Base actor_pub_x, Base actor_pub_y, Base orphan_secret,
    Base tx_commitment, Base tx_nonce, Base tx_binding,
}
circuit "SelfTestV2" {
    # A comment before a statement, on purpose: the checker's first bug was that this glued
    # the comment to the assignment, the assignment was never parsed, and every witness
    # feeding it was reported dead — so the planted control carries the shape that broke it.
    actor_pub = ec_mul_base(actor_secret, NULLIFIER_K);
    constrain_equal_base(ec_get_x(actor_pub), actor_pub_x);
    constrain_equal_base(ec_get_y(actor_pub), actor_pub_y);
    constrain_instance(actor_pub_x); constrain_instance(actor_pub_y);
    tx_binding = poseidon_hash(witness_base(3), tx_commitment, tx_nonce);
    constrain_instance(tx_binding); constrain_instance(tx_nonce);
    discarded_hash = poseidon_hash(witness_base(4), actor_pub_x);
}
ZKEOF
    # The self test asserts on the *content* of the report, not on an exit code. An earlier
    # draft of this block passed while the checker was crashing, because a traceback also
    # exits non-zero — which is AGENTS.md R8's own subject, a check that cannot fail, written
    # into the check that exists to catch it.
    OUT="$(REPO_ROOT="$REPO_ROOT" python3 - "$SCRIPT_DIR/../script/circuit_dead_values.py" "$TMPDIR_SELF/dead_values.zk" <<'PYEOF'
import importlib.util, os, sys
spec = importlib.util.spec_from_file_location("deadval", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
for f in mod.check_file(sys.argv[2], os.environ["REPO_ROOT"]):
    print(f"{f.kind} {f.name}")
PYEOF
)"
    STATUS=$?
    if [ "$STATUS" -ne 0 ]; then
        echo "SELF-TEST FAILED: the checker exited $STATUS without producing a report:"
        echo "$OUT"
        exit 1
    fi
    FAILED=0
    for WANT in "DEAD-WITNESS orphan_secret" "DEAD-DERIVATION discarded_hash"; do
        if ! printf '%s\n' "$OUT" | grep -qF "$WANT"; then
            echo "SELF-TEST FAILED: the planted defect was not reported: $WANT"
            FAILED=1
        fi
    done
    # And the negative half: a value that *is* reached must not be reported. `actor_secret`
    # feeds `actor_pub`, which two equalities bind, so it is live — a checker that reported
    # it would be reporting every derive-and-expose circuit in the tree.
    if printf '%s\n' "$OUT" | grep -qF "actor_secret"; then
        echo "SELF-TEST FAILED: reported a live witness (actor_secret) as dead."
        FAILED=1
    fi
    if [ "$FAILED" -ne 0 ]; then
        echo "$OUT" | sed 's/^/  /'
        exit 1
    fi
    echo "SELF-TEST OK: both planted defects reported, and the live witness left alone."
    printf '%s\n' "$OUT" | sed 's/^/  /'
    exit 0
fi

if [ "${#TARGETS[@]}" -eq 0 ]; then
    while IFS= read -r f; do TARGETS+=("$f"); done < <(
        cd "$REPO_ROOT" && ls src/contract/*/proof/*.zk proofs/core/*.zk bin/darkirc/proof/*.zk 2>/dev/null
    )
    ALL=1
else
    ALL=0
fi

REPO_ROOT="$REPO_ROOT" REPORT_ONLY="$REPORT_ONLY" ALL="$ALL" python3 - "${TARGETS[@]}" <<'PYEOF'
import os, sys, importlib.util

repo = os.environ["REPO_ROOT"]
spec = importlib.util.spec_from_file_location("deadval", repo + "/script/circuit_dead_values.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

targets = list(sys.argv[1:])

# COVERAGE IS PRINTED, AND A FILE THAT WAS NOT WALKED IS A FAILURE. The recorded defect this
# designs against is `check-circuit-metadata-alignment.sh`'s OBL-C79: an allowlist called
# `GENESIS` that examined 11 of 32 contracts while printing PASS, so a gate whose green line
# covered a third of the tree read as covering all of it.
walked = []
unreadable = []
findings = []
for rel in targets:
    path = os.path.join(repo, rel)
    if not os.path.isfile(path):
        unreadable.append(rel)
        continue
    try:
        findings.extend(mod.check_file(path, repo))
    except Exception as e:  # a parse failure must not read as a pass
        unreadable.append(f"{rel} ({e})")
        continue
    walked.append(rel)

declared_path = os.path.join(repo, "script", "circuit_dead_value_exceptions.txt")
declared = {}
if os.path.isfile(declared_path):
    for raw in open(declared_path, errors="replace").read().splitlines():
        entry = raw.split("#")[0].strip()
        if not entry:
            continue
        parts = [p.strip() for p in entry.split(" : ", 2)]
        if len(parts) != 3:
            print(f"ERROR: malformed line in script/circuit_dead_value_exceptions.txt: {entry!r}")
            print("       expected: <rel path> : <name> : <reason citing a register ID or a host read>")
            sys.exit(2)
        declared.setdefault((parts[0], parts[1]), parts[2])

keys = {(f.path, f.name) for f in findings}
undeclared = [f for f in findings if (f.path, f.name) not in declared]
stale = [k for k in declared if k not in keys]

# DEAD-CONSTANT is informational: an alias for a constant (`ZERO = witness_base(0)`) computes
# nothing from a witness and can neither authorize nor check, so it is not the class this
# checker exists for. It is printed so the cleanup is visible, and it does not fail the run.
informational = [f for f in undeclared if f.kind == "DEAD-CONSTANT"]
undeclared = [f for f in undeclared if f.kind != "DEAD-CONSTANT"]

if stale:
    print("NOTE: declared but no longer a dead value — the repair landed, so remove these lines:")
    for p, n in sorted(stale):
        print(f"  {p} : {n}")
    print()

print(f"COVERAGE: walked {len(walked)} of {len(targets)} .zk file(s)"
      f"{' (whole corpus)' if os.environ['ALL'] == '1' else ''}.")

for f in findings:
    key = (f.path, f.name)
    if key in declared:
        print(f"DECLARED: {f.path} : {f.name} ({f.kind})")
        print(f"          {declared[key]}")

if unreadable:
    print("")
    print(f"FAIL: {len(unreadable)} file(s) that should have been walked were not:")
    for r in unreadable:
        print(f"  {r}")
    sys.exit(3)

if informational:
    print("")
    print(f"INFORMATIONAL: {len(informational)} unused constant alias(es) — `NAME = witness_base(N)`")
    print("               assigned and never referenced. Not this checker's class: a constant")
    print("               computes nothing from a witness. Deleting them is cleanup (AGENTS.md R2):")
    for f in sorted(informational, key=lambda f: (f.path, f.name))[:20]:
        print(f"  {f.name}  {f.path}:{f.line}")
    if len(informational) > 20:
        print(f"  ... and {len(informational) - 20} more")

if undeclared:
    print("")
    print(f"FINDING: {len(undeclared)} value(s) computed by a circuit that reach no constraint —")
    print("         a witness nothing derives from authorizes nothing, and a hash computed and")
    print("         discarded is a check a reader believes was made:")
    for f in sorted(undeclared, key=lambda f: (f.path, f.name)):
        print(f"  {f.kind} {f.name}")
        print(f"    {f.path}:{f.line}")
    print("")
    print("Repair by removing the value (AGENTS.md R2) or by exposing it and comparing it in the")
    print("host (the `consume_claim.zk` form), then record the disposition in")
    print("script/circuit_dead_value_exceptions.txt if it is sound as written.")
    if os.environ["REPORT_ONLY"] == "1":
        print("REPORT-ONLY: not failing. See hooks/pre-commit for why this checker is not yet")
        print("blocking.")
        sys.exit(0)
    sys.exit(1)

print("")
print(f"PASS: every witness and assignment in the {len(walked)} file(s) walked reaches a")
print("      constraint, or is declared in script/circuit_dead_value_exceptions.txt.")
sys.exit(0)
PYEOF
