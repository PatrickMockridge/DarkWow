#!/bin/bash
# The rule that makes `OBL-C198`'s node check possible: **a circuit's last two
# `constrain_instance` targets are `tx_binding` then `tx_nonce`.**
#
# Stage 4 of the tx_binding chain is one comparison — the node recomputes
# `poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` over the enclosing transaction and
# requires it to equal the `tx_binding` the proof published. To make that comparison it has to
# *find* the value, and nothing in `src/linear/` indexes a public-input vector: there is no
# per-circuit map, and adding one would mean a table kept in step with every circuit in the tree
# by hand. So the position is the interface, and the position is a shape rather than a name
# mapping — which is exactly why it can be gated and `check-circuit-metadata-alignment.sh`'s
# order comparison cannot. That check compares the circuit's variable names to the arm's Rust
# expressions and is a heuristic, so it warns. Here there is nothing to guess.
#
# THE CENSUS IS NOT WRITTEN HERE, ON PURPOSE. It was, as three counted lines, and within a day of
# the campaign starting all three were wrong — the conforming count, the list of contracts, and
# the claim that one circuit has no pair at all. A number in a header is a second home for a fact
# its own output states, and this file's is a file that moves every week. The live census is the
# run's own `COVERAGE: walked N of N` and `DECLARED:` lines, and the accounting is the header of
# `script/circuit_tx_pair_last_exceptions.txt`. What is stated here is only the shape:
#
#   * every circuit of every contract places `tx_binding` immediately before `tx_nonce`, both
#     last — the conforming state;
#   * or it does not, and it is declared in that file with the register row that schedules its
#     repair;
#   * or it has no pair at all, which is not a third category but the same one: a circuit that
#     cannot be covered by stage 4 until it carries the pair.
#
# THE CORPUS IS 166, and the boundary is worth stating because two sibling gates walk 178.
# `src/contract/*/proof/*.zk` is what the chain verifies. The other twelve are `proofs/core`
# (10) and `bin/darkirc/proof/` (2), and **nothing in the chain consumes them**: no Rust or TOML
# under `src/` or `bin/` names that path, and no contract `.zk` includes one. Measured
# 2026-10-05 — 0 references — so they are neither nested inside a contract circuit nor verified on
# their own. The plan for this campaign said to "confirm that before exempting them"; there is
# nothing to exempt, because nothing in the chain sees them. A reader who wonders why this gate
# walks 166 where the derivation gate walks 178 has the answer here.
#
# WHAT A PASS MEANS, and what it does not. That every circuit walked places the pair last, or is
# declared below. It is a structural check over source text. It does not know whether the
# derivation feeding `tx_binding` is the host's, whether the metadata arm pushes the same values
# in the same order, or whether the client's `to_vec` agrees — those are
# `check-circuit-metadata-alignment.sh`'s and the node's own stage 4's business.
#
# THIS GATE BLOCKS, and the reasoning is the opposite of the dead-values checker's. A gate whose
# findings are unadjudicated has no authority (`check-pubkey-binding.sh` was report-only from
# 2026-09-22 to 2026-09-23 for that reason). Here every finding HAS been adjudicated: there are
# 31, they are enumerated, and each carries the same disposition — move the pair to the end as
# part of that contract's migration. So a NEW finding is a circuit edited into a shape the node
# cannot read, and that is a blocked commit by construction, not a report. The register row is
# `OBL-C198`; the per-contract repairs belong to the campaign, not to this check.
#
# Usage:
#   scripts/check-circuit-tx-pair-last.sh                # all circuits
#   scripts/check-circuit-tx-pair-last.sh <files...>     # specific files
#   scripts/check-circuit-tx-pair-last.sh --self-test    # plant a defect, require a report
#   scripts/check-circuit-tx-pair-last.sh --report-only  # exit 0 even on findings
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
    # TWO circuits, and the control matters as much as the defect. `MiddlePairV2` is the shape
    # `mint.zk` shipped with before this gate existed — the pair at 8 and 9 with `total_pin`
    # after it — and it is one line of reordering away from being correct, which is what makes
    # it the defect worth planting. `LastPairV2` is the same circuit with the pair moved to the
    # end; a checker that reported it would be reporting all 136 conforming circuits in the
    # tree.
    #
    # The two calls share a line on purpose, as they do in most of the corpus: the first draft
    # of this checker was line-anchored and saw only the first `constrain_instance` on such a
    # line, so it read a conforming circuit's pair as a single element. The planted control
    # carries the shape that broke it.
    cat > "$TMPDIR_SELF/pair_last.zk" <<'ZKEOF'
k = 11; field = "pallas";
constant "SelfTest" { EcFixedPointBase NULLIFIER_K, }
witness "SelfTest" {
    Base total_pin, Base tx_commitment, Base tx_nonce, Base tx_binding,
}
circuit "MiddlePairV2" {
    constrain_instance(total_pin);
    tx_binding = poseidon_hash(witness_base(3), tx_commitment, tx_nonce);
    constrain_instance(tx_binding); constrain_instance(tx_nonce);
    constrain_instance(total_pin); constrain_instance(total_pin);
}
circuit "LastPairV2" {
    constrain_instance(total_pin); constrain_instance(total_pin);
    tx_binding = poseidon_hash(witness_base(3), tx_commitment, tx_nonce);
    constrain_instance(tx_binding); constrain_instance(tx_nonce);
}
ZKEOF
    # The self test asserts on the *content* of the report, not on an exit code. A traceback
    # also exits non-zero, so an exit-code assertion would pass while the checker was crashing —
    # AGENTS.md R8's own subject, written into the check that exists to catch a missing check.
    OUT="$(REPO_ROOT="$REPO_ROOT" python3 - "$SCRIPT_DIR/../script/circuit_tx_pair_last.py" "$TMPDIR_SELF/pair_last.zk" <<'PYEOF'
import importlib.util, os, sys
spec = importlib.util.spec_from_file_location("pairlast", sys.argv[1])
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
    if ! printf '%s\n' "$OUT" | grep -qF "PAIR-NOT-LAST MiddlePairV2"; then
        echo "SELF-TEST FAILED: the planted defect was not reported: PAIR-NOT-LAST MiddlePairV2"
        FAILED=1
    fi
    # And the negative half: the conforming circuit must not be reported. `LastPairV2` is
    # byte-for-byte the same body with the pair at the end.
    if printf '%s\n' "$OUT" | grep -qF "LastPairV2"; then
        echo "SELF-TEST FAILED: reported a conforming circuit (LastPairV2) as non-conforming."
        FAILED=1
    fi
    if [ "$FAILED" -ne 0 ]; then
        echo "$OUT" | sed 's/^/  /'
        exit 1
    fi
    echo "SELF-TEST OK: the middle-pair circuit reported, the last-pair circuit left alone."
    printf '%s\n' "$OUT" | sed 's/^/  /'
    exit 0
fi

if [ "${#TARGETS[@]}" -eq 0 ]; then
    while IFS= read -r f; do TARGETS+=("$f"); done < <(
        cd "$REPO_ROOT" && ls src/contract/*/proof/*.zk 2>/dev/null
    )
    ALL=1
else
    ALL=0
fi

REPO_ROOT="$REPO_ROOT" REPORT_ONLY="$REPORT_ONLY" ALL="$ALL" python3 - "${TARGETS[@]}" <<'PYEOF'
import os, sys, importlib.util

repo = os.environ["REPO_ROOT"]
spec = importlib.util.spec_from_file_location("pairlast", repo + "/script/circuit_tx_pair_last.py")
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

declared_path = os.path.join(repo, "script", "circuit_tx_pair_last_exceptions.txt")
declared = {}
if os.path.isfile(declared_path):
    for raw in open(declared_path, errors="replace").read().splitlines():
        entry = raw.split("#")[0].strip()
        if not entry:
            continue
        parts = [p.strip() for p in entry.split(" : ", 2)]
        if len(parts) != 3:
            print(f"ERROR: malformed line in script/circuit_tx_pair_last_exceptions.txt: {entry!r}")
            print("       expected: <rel path> : <circuit> : <reason citing a register ID>")
            sys.exit(2)
        declared.setdefault((parts[0], parts[1]), parts[2])

keys = {(f.path, f.name) for f in findings}
undeclared = [f for f in findings if (f.path, f.name) not in declared]
stale = [k for k in declared if k not in keys]

# Staleness is only decidable over the whole corpus. A run given specific files walks a subset,
# so every declaration whose site was not among them would look stale — 31 false notes on a
# one-file run, which is the kind of noise that gets a gate disabled.
if stale and os.environ["ALL"] == "1":
    print("NOTE: declared but no longer non-conforming — the pair moved last, so remove these lines:")
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

if undeclared:
    print("")
    print(f"FINDING: {len(undeclared)} circuit(s) whose last two instances are not")
    print("         `tx_binding` then `tx_nonce`. The node reads the pair from the end of the")
    print("         public-input vector; there is no per-circuit index map, so a proof whose")
    print("         pair sits elsewhere cannot be checked against its transaction at all:")
    for f in sorted(undeclared, key=lambda f: (f.path, f.name)):
        print(f"  {f.kind} {f.name}")
        print(f"    {f.path}:{f.line} — {f.detail}")
    print("")
    print("Repair by moving `constrain_instance(tx_binding); constrain_instance(tx_nonce);` to")
    print("the end of the circuit, then moving the same two entries to the end of the arm's")
    print("push vector and the client's `to_public_inputs` — all three or the proof stops")
    print("verifying. If the circuit cannot carry a pair, record why in")
    print("script/circuit_tx_pair_last_exceptions.txt.")
    if os.environ["REPORT_ONLY"] == "1":
        print("REPORT-ONLY: not failing.")
        sys.exit(0)
    sys.exit(1)

print("")
print(f"PASS: every circuit in the {len(walked)} file(s) walked places `tx_binding` immediately")
print("      before `tx_nonce`, both last, or is declared in")
print("      script/circuit_tx_pair_last_exceptions.txt.")
sys.exit(0)
PYEOF
