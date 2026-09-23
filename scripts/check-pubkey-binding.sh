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
# Exit 0: no vacuous bindings, or every finding adjudicated in
#         script/circuit_pubkey_binding_exceptions.txt (each printed as EXCEPTED with the
#         mechanism that makes it sound, or the register row that schedules its repair).
# Exit 1: at least one *unexcepted* finding, reported with file:line.
# Exit 2: the exception list is malformed.
#
# THE LIST IS A RATCHET, NOT AN AMNESTY, and this is the whole of what changed on 2026-09-23.
# The detector stayed deliberately shallow — it cannot see a host — and the 57 candidates it
# reports are adjudicated by *reading*, recorded one line at a time in that list. Fifteen of them
# are genuine defects and their entries say so, naming OBL-C81, C82, C83, C84 and OBL-C75's first
# measured instance; the rest name the mechanism. A new site anywhere still fails, which is what
# the report-only mode was waiting for: before the sweep the class was invisible, and after it the
# class is scheduled and watched. See script/circuit_pubkey_binding_exceptions.txt's own header
# for why the classifier was not made cleverer to shrink the list.

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

def rel_to_repo(p):
    """Normalise a path to repo-relative, so the hook's absolute paths key the same as the
    exception entries. The hook passes `$REPO_ROOT/<file>`; a bare invocation passes the
    repo-relative path. Both must match one list."""
    p = p.replace("\\", "/")
    prefix = repo.rstrip("/") + "/"
    return p[len(prefix):] if p.startswith(prefix) else p

findings = []
for rel in sys.argv[1:]:
    path = os.path.join(repo, rel)
    if not os.path.isfile(path):
        continue
    try:
        text = open(path, errors="replace").read()
    except OSError:
        continue
    for lineno, detail in scan(rel_to_repo(rel), text):
        findings.append((rel_to_repo(rel), lineno, detail))

# Reviewed exceptions (script/circuit_pubkey_binding_exceptions.txt). Keyed on (path, the equality
# as it appears in the source) — content, not a line number, so an edit above a site moves nothing.
exceptions = {}
exc_path = os.path.join(repo, "script", "circuit_pubkey_binding_exceptions.txt")
if os.path.isfile(exc_path):
    for raw in open(exc_path, errors="replace").read().splitlines():
        entry = raw.split("#")[0].strip()
        if not entry:
            continue
        parts = [p.strip() for p in entry.split(":", 2)]
        if len(parts) != 3:
            print(f"ERROR: malformed exception line in {os.path.basename(exc_path)}: {entry!r}")
            print("       expected: <zk path> : <equality as written> : <reason citing a register ID>")
            sys.exit(2)
        exceptions.setdefault((parts[0], parts[1]), parts[2])

excepted, failing = [], []
for rel, lineno, detail in findings:
    key = (rel, detail.split(" binds two", 1)[0].strip())
    if key in exceptions:
        excepted.append((rel, lineno, key[1], exceptions[key]))
    else:
        failing.append((rel, lineno, detail))

for rel, lineno, pair, reason in excepted:
    print(f"EXCEPTED: {rel}:{lineno}: {pair}")
    print(f"          {reason}")

for rel, lineno, detail in failing:
    print(f"FAIL: {rel}:{lineno}: {detail}")

if failing:
    print(f"\nFAIL: {len(failing)} unexcepted vacuous binding(s).")
    print("FIX: expose the value instead of comparing it to another prover-chosen one —")
    print("  derive-and-expose:  pub = ec_mul_base(secret, K); constrain_instance(ec_get_x(pub));")
    print("  ...or pin one side: constrain_equal_base(derived, ONE)")
    print("  If both sides have been read and the finding is sound, add it to")
    print("  script/circuit_pubkey_binding_exceptions.txt with its mechanism or register row.")
    if excepted:
        print(f"  ({len(excepted)} other finding(s) are excepted and are not among the above.)")
    sys.exit(1)

scheduled = (f"; {len(excepted)} adjudicated in script/circuit_pubkey_binding_exceptions.txt"
             if excepted else "")
print(f"PASS: no unexcepted vacuous bindings ({len(sys.argv) - 1} circuit(s) scanned{scheduled})")
sys.exit(0)
PYEOF
