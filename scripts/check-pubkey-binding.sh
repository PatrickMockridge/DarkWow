#!/bin/bash
# A `constrain_equal_base(A, B)` where the prover chooses *both* operands proves nothing:
# the equality holds for any secret, so the verifier learns nothing from it. This is
# OBL-Z9's defect, and it is the reason the oracle circuits carry the old form as a
# comment annotated `-- witness == witness` (src/contract/oracle/proof/push_value.zk:53).
#
# The rule, which is easy to get backwards:
#
#   SOUND    derive-and-expose — `pub = ec_mul_base(secret, K); constrain_instance(ec_get_x(pub))`.
#            The exposed value is computed *from* a witness, so publishing it proves the
#            prover knows a secret under which it is that value. The host compares it to
#            the stored record and the comparison means something.
#
#   SOUND    witness-vs-computed — `constrain_equal_base(derived, ONE)`, or any equality
#            where one side is a literal or a named constant: the witness is pinned.
#
#   UNSOUND  witness-vs-witness — `constrain_equal_base(ec_get_x(ec_mul_base(s, K)), pub_x)`
#            where `pub_x` is a free witness and neither side is ever exposed. Both operands
#            are the prover's to choose, so the constraint is satisfied by any `s`, and the
#            entrypoint has no exposed value to check against chain state.
#
# **What this replaced, and why the previous check was worse than useless.** `hooks/pre-commit`
# carried an awk that enforced the opposite: "a value derived by ec_get_x must be bound by
# constrain_equal_base before it is exposed". That flags derive-and-expose — the form the
# OBL-Z9 remedy *adopted* — so a commit touching `proofs/core/{burn,opcodes}.zk` would have
# been rejected for being correct. It never fired only because those files were never edited
# since the hook was written. The awk was also line-anchored, single-assignment, matched
# pending names by substring, and scanned only staged files.
#
# Usage:
#   scripts/check-pubkey-binding.sh                 # all circuits
#   scripts/check-pubkey-binding.sh <files...>      # specific files (the hook passes staged)
#
# Exit 0: no vacuous bindings.
# Exit 1: at least one, reported with file:line.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ "$#" -eq 0 ] || [ "$1" = "--all" ]; then
    TARGETS=()
    while IFS= read -r f; do TARGETS+=("$f"); done < <(cd "$REPO_ROOT" && ls src/contract/*/proof/*.zk proofs/core/*.zk bin/darkirc/proof/*.zk 2>/dev/null)
else
    TARGETS=("$@")
fi

REPO_ROOT="$REPO_ROOT" python3 - "${TARGETS[@]}" <<'PYEOF'
import os, re, sys

repo = os.environ["REPO_ROOT"]

ASSIGN = re.compile(r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?);?\s*$')
EQ = re.compile(r'\bconstrain_equal_base\s*\(([^)]*)\)')
INST = re.compile(r'\bconstrain_instance\s*\(([^)]*)\)')
IDS = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')

def ids(expr):
    return set(IDS.findall(expr))

def is_anchored_operand(expr, rhs_of, exposed, seen=None):
    """True if this operand's value is already something the verifier can see.

    Anchoring follows **direct aliasing only** — `x = y` where y is exposed — and never
    descends through a function call. That distinction is the whole check: `ec_get_x(pub)`
    mentions the constant NULLIFIER_K somewhere inside `pub`'s own definition, and a
    version that recursed through calls called that "anchored", which made the check pass
    the exact form it exists to catch. A call is a computation, not an alias; the value it
    produces is the prover's unless the result is itself exposed.
    """
    if seen is None:
        seen = set()
    expr = expr.strip()
    if re.fullmatch(r'\d+', expr):          # a literal
        return True
    if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', expr):
        return False                        # a call or compound expression — a computation
    if expr.isupper():                      # a named circuit constant (ONE, ZERO, NULLIFIER_K)
        return True
    if expr in exposed:
        return True
    if expr in seen:
        return False
    seen.add(expr)
    rhs = rhs_of.get(expr)
    if rhs is not None and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', rhs.strip()):
        return is_anchored_operand(rhs, rhs_of, exposed, seen)
    return False

def scan(rel, text):
    lines = []
    for raw in text.split("\n"):
        lines.append("" if raw.lstrip().startswith("#") else raw.split("//")[0])

    rhs_of = {}
    for ln in lines:
        m = ASSIGN.match(ln)
        if m and not m.group(2).startswith(("constrain_", "range_check", "less_than")):
            rhs_of.setdefault(m.group(1), m.group(2))

    exposed = set()
    for ln in lines:
        for args in INST.findall(ln):
            exposed |= ids(args)

    findings = []
    for i, ln in enumerate(lines, 1):
        for args in EQ.findall(ln):
            parts = [p.strip() for p in args.split(",")]
            if len(parts) != 2:
                continue
            a, b = parts
            if not is_anchored_operand(a, rhs_of, exposed) and \
               not is_anchored_operand(b, rhs_of, exposed):
                findings.append((i, f"constrain_equal_base({a}, {b}) binds two "
                                    f"prover-chosen values; neither is exposed, so the "
                                    f"equality holds for any witness"))
    return findings

total = 0
for rel in sys.argv[1:]:
    path = os.path.join(repo, rel)
    if not os.path.isfile(path):
        continue
    try:
        text = open(path, errors="replace").read()
    except OSError:
        continue
    for lineno, detail in scan(rel, text):
        print(f"{rel}:{lineno}: {detail}")
        total += 1

if total:
    print(f"\nFAIL: {total} vacuous binding(s).")
    print("FIX: expose the value instead of comparing it to another prover-chosen one —")
    print("  derive-and-expose:  pub = ec_mul_base(secret, K); constrain_instance(ec_get_x(pub));")
    print("  ...or pin one side: constrain_equal_base(derived, ONE)")
    sys.exit(1)

print(f"PASS: no vacuous bindings ({len(sys.argv) - 1} circuit(s) scanned)")
sys.exit(0)
PYEOF
