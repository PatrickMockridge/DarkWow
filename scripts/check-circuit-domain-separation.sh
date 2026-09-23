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
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import glob, os, re, sys, collections

repo = os.environ["REPO_ROOT"]

# ROLE COMPARISON IS NAME AGAINST NAME, NOT VALUE AGAINST A CONVENTIONAL VALUE, and the first
# version of this rule got that wrong in a way worth recording. Comparing a circuit's *local value*
# for a domain against the 1..7 convention flagged `computed_nullifier = poseidon_hash(
# DOMAIN_NULLIFIER, ...)` in circuits that number DOMAIN_NULLIFIER 2 or 3 — correct code, flagged
# for not following a convention that the collision rule below shows is only a convention. What the
# role rule can honestly ask is whether the *purpose* the domain's name declares is the purpose the
# derivation's name declares. Both sides are read by keyword; the more specific keyword wins, so
# OPERATOR_COMMITMENT is a commitment and TOKEN_COMMIT is not one.
DOMAIN_ROLE = [
    ("NULLIFIER", "nullifier"),
    ("TX_BINDING", "tx_binding"),
    ("USER_DATA_ENC", "user_data_enc"),
    ("SIGNATURE_SECRET", "signature_secret"),
    ("TOKEN_COMMIT", "token_commit"),
    ("TOK_COMMIT", "token_commit"),
    ("MERKLE_LEAF", "leaf"),
    ("COMMITMENT", "commitment"),   # also CAP_COMMIT, MEMBER_COMMITMENT, OPERATOR_COMMITMENT
]
VAR_ROLE = DOMAIN_ROLE


def role_of(text):
    for keyword, role in VAR_ROLE:
        if keyword.lower() in text.lower():
            return role
    return None

# Scope: the contract circuits, as this gate has always scoped itself. `proofs/core/` and
# `bin/darkirc/proof/` are NOT in it — they are the older example circuits, they carry no DOMAIN_
# constants at all, and they are scanned by the sibling gates on their own terms. Widening the
# corpus here would make this gate red on ~30 pre-existing sites that no row tracks, which is how a
# gate gets ignored; if that class is to be worked it needs its own row and its own decision.
paths = sorted(glob.glob(f"{repo}/src/contract/*/proof/*.zk"))

# Each finding is (identity, message). The identity is stable under edits above it — file plus
# the value or the variable name — because a line-numbered key retires an entry the moment a
# comment is added above it, which is the failure mode this repository has hit before.
findings = []

for path in paths:
    rel = os.path.relpath(path, repo)
    src = open(path, errors="replace").read()
    # Blank comment lines in place so character offsets/line numbers survive:
    # zkas comments are full-line `#`; `//` handled defensively (unused today).
    lines = []
    for ln in src.split("\n"):
        ln = ln.split("//")[0]
        if ln.lstrip().startswith("#"):
            ln = ""
        lines.append(ln)
    scannable = "\n".join(lines)

    # 1. PRESENCE — the call-level scan, unchanged from the 2026-09 rewrite.
    for m in re.finditer(r'poseidon_hash\s*\(', scannable):
        i = m.end() - 1  # index of the opening '('
        depth, j, end = 0, i, None
        while j < len(scannable):
            if scannable[j] == "(":
                depth += 1
            elif scannable[j] == ")":
                depth -= 1
                if depth == 0:
                    end = j
                    break
            j += 1
        if end is None:
            continue  # malformed call — not our corpus
        if not re.search(r'DOMAIN_|witness_base', scannable[i + 1:end]):
            lineno = src[:m.start()].count("\n") + 1
            findings.append((f"{rel} :: presence",
                             f"{rel}:{lineno}: poseidon_hash call without DOMAIN_/witness_base"))

    # 2. COLLISION — the named domains this circuit defines, grouped by value.
    named = {}          # DOMAIN_X -> value
    redefined = {}      # DOMAIN_X -> set(values), for one name given two values
    for ln in lines:
        d = re.match(r'\s*(DOMAIN_[A-Z_]+)\s*=\s*witness_base\((\d+)\)', ln)
        if d:
            named.setdefault(d.group(1), int(d.group(2)))
            redefined.setdefault(d.group(1), set()).add(int(d.group(2)))
    by_value = collections.defaultdict(list)
    for name, value in named.items():
        by_value[value].append(name)
    for value, names in sorted(by_value.items()):
        if len(names) > 1:
            findings.append((f"{rel} :: collision :: {value}",
                             f"{rel}: {value} is the domain of {', '.join(sorted(names))} — two "
                             f"differently-named domains in one circuit share a value"))
    for name, values in sorted(redefined.items()):
        if len(values) > 1:
            findings.append((f"{rel} :: redefined :: {name}",
                             f"{rel}: {name} is defined with more than one value "
                             f"({', '.join(str(v) for v in sorted(values))})"))

    # 3. ROLE — the purpose the domain's name declares must be the purpose the derivation's name
    # declares. Only a variable whose name declares a role is checked; a neutral name like
    # `derived_stake_id` says nothing to compare against.
    for i, ln in enumerate(lines, 1):
        m = re.match(r'\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*poseidon_hash\(\s*([A-Z_][A-Z0-9_]*)', ln)
        if not m:
            continue
        var, dom_name = m.group(1), m.group(2)
        if dom_name not in named:
            continue
        var_role, dom_role = role_of(var), role_of(dom_name)
        if var_role and dom_role and var_role != dom_role:
            findings.append((f"{rel} :: role :: {var}",
                             f"{rel}:{i}: {var} (a {var_role}) is derived in {dom_name} "
                             f"(a {dom_role}) — the domain does not carry the derivation's purpose"))

exceptions = {}
exc_path = os.path.join(repo, "script", "circuit_domain_exceptions.txt")
if os.path.isfile(exc_path):
    for raw in open(exc_path, errors="replace").read().splitlines():
        entry = raw.split("#")[0].strip()
        if not entry:
            continue
        # Split on " : " rather than ":", because the identity itself contains " :: " — which
        # never contains a space-colon-space, so the boundary stays unambiguous.
        parts = [p.strip() for p in re.split(r'\s+:\s+', entry, maxsplit=1)]
        if len(parts) != 2:
            print(f"ERROR: malformed exception line in {os.path.basename(exc_path)}: {entry!r}")
            print("       expected: <rel path> :: <class> :: <detail> : <reason citing a register ID>")
            sys.exit(2)
        exceptions.setdefault(parts[0], parts[1])

excepted, failing = [], []
for identity, message in findings:
    if identity in exceptions:
        excepted.append((message, exceptions[identity]))
    else:
        failing.append((identity, message))

if not findings:
    print("PASS: all contract circuits are domain-separated "
          "(presence, within-circuit collisions, and role)")
    sys.exit(0)

for message, reason in excepted:
    print(f"EXCEPTED: {message}")
    print(f"          {reason}")

if failing:
    print(f"FAIL: {len(failing)} unexcepted domain-separation finding(s):")
    for _identity, message in failing:
        print(f"  {message}")
    print("")
    print("Fix: give the derivation the domain its purpose calls for, and never two names one value.")
    print("  DOMAIN_NULLIFIER     = witness_base(1)")
    print("  DOMAIN_TOKEN_COMMIT  = witness_base(2)  (some circuits: DOMAIN_TOK_COMMIT)")
    print("  DOMAIN_TX_BINDING    = witness_base(3)")
    print("  DOMAIN_COMMITMENT    = witness_base(4)  (dex: DOMAIN_CAP_COMMIT)")
    print("  DOMAIN_MERKLE_LEAF   = witness_base(5)")
    print("  DOMAIN_USER_DATA_ENC = witness_base(6)")
    print("  DOMAIN_SIGNATURE_SECRET = witness_base(7)")
    print("  If the finding has been read and is deliberate, add it to")
    print("  script/circuit_domain_exceptions.txt with its register ID.")
    sys.exit(1)

print(f"PASS: no unexcepted domain-separation findings "
      f"({len(excepted)} adjudicated in script/circuit_domain_exceptions.txt)")
sys.exit(0)
PYEOF
