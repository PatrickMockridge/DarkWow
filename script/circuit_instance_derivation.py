#!/usr/bin/env python3
"""Check the Orchard-class rule over the `.zk` sources.

THE RULE. For every circuit and every `constrain_instance(X)`, `X` must be one of:

  1. **derived** — `X` was assigned a pure opcode expression over values that are themselves
     witnesses, constants, or prior derived values;
  2. **bound** — a `constrain_equal_base(derived, X)` (or `constrain_equal_point`) appears
     *before* the expose, with the first argument being a pure opcode expression;
  3. **redundant** — `X` is a witness that already appears inside some *other* exposed expression
     which is itself derived. Exposing a witness the prover was going to choose anyway grants no
     additional freedom: the prover's degree of freedom in `X` is the same before and after the
     expose. The canonical case is
         tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce);
         constrain_instance(tx_binding); constrain_instance(tx_nonce);
     where the `tx_nonce` expose is a *disclosure* to the host, not a new variable.
  4. **declared free** — `X` is a witness and is named in `script/circuit_free_instances.txt`
     together with its host-side justification.

Anything else is an Orchard-class vulnerability: a public input the prover can set arbitrarily.

WHAT "REDUNDANT" DOES *NOT* SAY. It says the expose adds no freedom, not that `X` is safe. A
witness that is redundant here is still prover-chosen, and whether that matters depends on what
the host does with the sibling it is disclosed beside — which is a property of the entrypoint,
not of the `.zk` source, and therefore invisible to this checker. Every redundancy finding names
the exposed expression that discharges it, so that obligation is addressable by name; the
remaining host-side question is recorded in OBL-Z1 of `doc/src/arch/verification-hazop.md`.

WHY A PASS IS NOT A SOUNDNESS PROOF. Classifications 1 and 3 both establish a statement about
the *prover's freedom*, under the assumption that the host validates the exposed derived value.
Establishing that assumption is not this script's job and is not mechanizable from `.zk` text.

WHY THIS EXISTS. The rule was prose. `scripts/check-circuit-metadata-alignment.sh` compares
*counts* of `constrain_instance` against metadata pushes; `scripts/check-circuit-domain-separation.sh`
checks that a `poseidon_hash` call carries *some* domain prefix; `hooks/pre-commit` catches one
binding pattern on staged files. None of them checks that a public input is derived, so nothing in
the repository did. See OBL-Z1 in `doc/src/arch/verification-hazop.md`.

WHAT A PASS MEANS. That this checker's model of the derivation held for every instance it walked.
It is a structural check over source text, not a soundness proof: it does not know what the
opcodes *mean*, and it reads the `.zk` source rather than the compiled `.zk.bin` that actually
matters. `script/validate_zk_bins.sh` covers binary validity; the opcode semantics are the Lean
layer's business (`ECOps.lean`, `HashOps.lean`, `Pedersen.lean`).

CONSERVATIVE BY CONSTRUCTION. Anything this script cannot classify is a FAIL, not a PASS — a
false alarm is visible and triageable, a false pass is not.

THE SECOND RULE, added with the quotient-remainder repairs. Every operand of a `less_than_*`
comparison must rest on witnesses that a `range_check` precedes. The comparison chip range-checks
its *offset*, never its operands, so an unbounded operand makes the comparison one over field
residues rather than integers — which is how `q · d ≤ n < (q + 1) · d` can be satisfied by a
quotient ~10⁷⁶ away from the true one (`BaseDivGadget.qr_needs_bound`, kernel-checked). The rule
follows assignments to the leaf witnesses, so it is about the values the comparison *rests on*,
and it is deliberately coarse: a bounded leaf does not imply the bound is wide enough for what is
built on top of it. It is checked because the six circuits repaired on 2026-09-20 are exactly the
ones that pass it and the 15 findings are exactly the ones that do not.

Usage:
    python3 script/circuit_instance_derivation.py            # table + verdict
    python3 script/circuit_instance_derivation.py --json
    python3 script/circuit_instance_derivation.py --quiet     # only failures
"""

import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(REPO_ROOT, "script", "circuit_free_instances.txt")
OPCODES_RS = os.path.join(REPO_ROOT, "src", "zkas", "opcode.rs")

RED, GREEN, YELLOW, NC = "\033[31m", "\033[32m", "\033[33m", "\033[0m"


def ok(msg):
    print(f"{GREEN}OK:{NC}  {msg}")


def fail(msg):
    print(f"{RED}FAIL:{NC} {msg}")


def skipped(msg):
    print(f"{YELLOW}SKIP:{NC} {msg}")


# ---------------------------------------------------------------------------
# The opcode vocabulary, read from the implementation


def known_opcodes():
    """Opcode names, parsed from `src/zkas/opcode.rs`.

    Taken from the source rather than hard-coded so the checker follows the VM: an opcode added
    there is recognised here without editing this file.
    """
    if not os.path.exists(OPCODES_RS):
        return None
    text = open(OPCODES_RS, encoding="utf-8").read()
    return set(re.findall(r'"([a-z_][a-z0-9_]*)"', text))


# ---------------------------------------------------------------------------
# Lexing the .zk surface


def strip_comments(text):
    """`#` to end of line. `.zk` has no other comment form."""
    return "\n".join(re.sub(r"#.*$", "", line) for line in text.split("\n"))


def parse_typed_block(body):
    """Names from a `constant`/`witness` block body: `Type name, Type name, ...`.

    The name is the last identifier of each comma-separated item, so `EcFixedPointShort
    VALUE_COMMIT_VALUE` yields `VALUE_COMMIT_VALUE` and `Base[] commitments` yields `commitments`.
    """
    names = []
    for item in body.split(","):
        item = item.strip()
        if not item:
            continue
        tokens = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", item)
        if tokens:
            names.append(tokens[-1])
    return names


def braced_block(text, start):
    """Body of the `{...}` beginning at `start` (the index of `{`), and the index after `}`."""
    depth, i = 0, start
    while i < len(text):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start + 1:i], i + 1
        i += 1
    return None, len(text)


def parse_circuit(text):
    """(constants, witnesses, statements) for one `.zk` file."""
    constants, witnesses, statements_out = [], [], []
    for m in re.finditer(r"\b(constant|witness)\s+\"([^\"]*)\"\s*\{", text):
        body, _ = braced_block(text, text.index("{", m.start()))
        if body is None:
            continue
        names = parse_typed_block(body)
        if m.group(1) == "constant":
            constants.extend(names)
        else:
            witnesses.extend(names)
    for m in re.finditer(r"\bcircuit\s+\"[^\"]*\"\s*\{", text):
        body, _ = braced_block(text, text.index("{", m.start()))
        if body is not None:
            statements_out.extend(statements(body))
    return set(constants), set(witnesses), statements_out


def statements(body):
    """Top-level statements in a circuit or block body, in source order.

    Nested `if`/`for` bodies are flattened into the same stream: what matters for the rule is
    textual order, and a `constrain_equal_base` inside a branch still precedes the expose that
    follows it.
    """
    out, i, n = [], 0, len(body)
    current = ""
    while i < n:
        c = body[i]
        if c == ";":
            s = current.strip()
            if s:
                out.append(s)
            current = ""
        elif c == "{":
            inner, j = braced_block(body, i)
            head = current.strip()
            if head:
                # Keep the head (the `if (...)` / `for (...)` condition) as context, and walk the
                # body too.
                out.append(head)
            out.extend(statements(inner if inner else ""))
            current = ""
            i = j
            continue
        elif c == "}":
            current = ""
        else:
            current += c
        i += 1
    s = current.strip()
    if s:
        out.append(s)
    return out


# ---------------------------------------------------------------------------
# Classifying


CALL_RE = re.compile(r"^([a-z_][a-zA-Z0-9_]*)\s*\((.*)\)$", re.DOTALL)
NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
LITERAL_RE = re.compile(r"^(0x[0-9a-fA-F]+|[0-9]+)$")

# Opcodes that introduce a value rather than deriving one from witnesses. A call to one of these
# on the right-hand side is fine; a call to one of these *as* an instance argument is not a
# derivation, because the value came from outside the circuit.
EXTERNAL = {"witness_base", "constant_base", "witness_point", "constant_point"}


def split_args(argtext):
    """Top-level comma split, respecting nested parens and brackets."""
    out, depth, current = [], 0, ""
    for c in argtext:
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        if c == "," and depth == 0:
            out.append(current.strip())
            current = ""
        else:
            current += c
    if current.strip():
        out.append(current.strip())
    return out


def is_derived_expr(expr, derived, constants, witnesses, opcodes):
    """Is `expr` a pure opcode expression over values the circuit holds?

    `derived`/`constants`/`witnesses` are name sets. A witness counts as *held* here — the point
    of the rule is that the *instance* be derived, and a witness used to compute some other value
    is ordinary circuit wiring.
    """
    expr = expr.strip()
    if not expr:
        return False
    if LITERAL_RE.match(expr):
        return True
    if NAME_RE.match(expr):
        return expr in derived or expr in constants or expr in witnesses
    m = CALL_RE.match(expr)
    if m:
        name, args = m.group(1), split_args(m.group(2))
        if name not in opcodes:
            return False
        return all(is_derived_expr(a, derived, constants, witnesses, opcodes) for a in args)
    # Arithmetic / comparison / field access: recurse over identifiers, and reject any unknown one.
    identifiers = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", expr)
    if not identifiers:
        return True
    return all(i in derived or i in constants or i in witnesses for i in identifiers)


def is_determined(expr, derived, constants, witnesses, opcodes):
    """Is `expr`, *as a whole*, a value the circuit determines rather than a bare witness?

    Differs from `is_derived_expr` at the top level only: there a bare name counts if it is any
    of derived/constant/witness, because those are the leaf inputs a derivation is built from.
    Here a bare witness does **not** count — `constrain_equal_base(some_witness, X)` does not pin
    `X`, it merely re-exposes a variable the prover was already holding. Only a name the circuit
    *assigned* (or a declared constant), or a compound expression, is a determination.
    """
    expr = expr.strip()
    if NAME_RE.match(expr):
        return expr in derived or expr in constants
    return is_derived_expr(expr, derived, constants, witnesses, opcodes)


def free_identifiers(expr):
    """Every identifier appearing in `expr`. Used for the redundancy test, so opcode names and
    declared constants appear in the result too — harmless, since the queried name is a witness."""
    return set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", expr))


def support(expr, assign):
    """Every identifier reachable from `expr` by following the assignment chain.

    A determination does not stop at the names it mentions. `root = merkle_root(pos, path,
    coin_incl)` pins `coin_incl`, which pins `coin`, which pins `coin_spend_hook` — so the witness
    that matters is three hops away and invisible to a one-level scan. `assign` is the whole
    circuit's `lhs -> rhs` map; the `seen` set makes the walk terminating and linear in the number
    of distinct names.
    """
    seen, stack = set(), [expr]
    while stack:
        for ident in re.findall(r"[A-Za-z_][A-Za-z0-9_]*", stack.pop()):
            if ident not in seen:
                seen.add(ident)
                if ident in assign:
                    stack.append(assign[ident])
    return seen


def resolve(expr, assign, depth=16):
    """Follow a name through the assignment chain to the expression it denotes.

    `constrain_equal_base(computed_nullifier, close_nullifier)` binds `close_nullifier` to the
    *name* `computed_nullifier`; the witnesses actually pinned by that binding are the ones in
    `poseidon_hash(DOMAIN_NULLIFIER, bet_id, house_secret)`. The depth bound is a cycle guard:
    `.zk` has no self-reference, but a malformed input should not spin.
    """
    seen = set()
    for _ in range(depth):
        if not NAME_RE.match(expr) or expr not in assign or expr in seen:
            break
        seen.add(expr)
        expr = assign[expr]
    return expr


def load_manifest():
    """`path : instance : justification`, `#` comments ignored."""
    entries = {}
    if not os.path.exists(MANIFEST):
        return entries
    for lineno, line in enumerate(open(MANIFEST, encoding="utf-8"), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        parts = [p.strip() for p in line.split(":", 2)]
        if len(parts) != 3 or not parts[2]:
            print(f"{RED}FAIL:{NC} {MANIFEST}:{lineno}: expected "
                  f"`<path> : <instance> : <justification>`", file=sys.stderr)
            continue
        entries[(parts[0], parts[1])] = parts[2]
    return entries


def comparison_operand_findings(path, constants, witnesses, stmts, opcodes):
    """Every operand of a `less_than_*` comparison must rest on range-checked witnesses.

    The comparison chip is 253 bits wide and range-checks its **offset** — the difference of its
    two operands — never the operands themselves. So a comparison over unbounded operands is a
    comparison over residues: a prover chooses values whose products or differences wrap modulo
    `p` back into the accepted region. With every contributing witness `< 2^64` the sums are
    `< 2^66` and the products `< 2^130`, well inside 2^253, and the field comparison is the
    integer comparison. `BaseDivGadget.qr_needs_bound` in the Lean layer is the kernel-checked
    witness that this is not hypothetical.

    The rule is applied to the **leaf witnesses** under each operand, following assignments, not
    to the operand's surface text: `less_than_strict(n, (q+1)*d)` is bounded by `range_check` on
    `q` and `d`, neither of which appears literally in the operand. This is a *sufficient*
    condition and deliberately a coarse one — it does not check that the bound is wide enough for
    the arithmetic performed on top of it, only that each leaf is bounded at all.
    """
    ranged = set()
    assign = {}
    findings = []
    for stmt in stmts:
        # `range_check(64, x)` / `range_check(253, x)` — record the checked name.
        m = re.match(r"^range_check\s*\(\s*\d+\s*,\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)$", stmt)
        if m:
            ranged.add(m.group(1))
            continue
        a = re.match(r"^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)$", stmt, re.DOTALL)
        if a and not stmt.startswith("constrain"):
            assign[a.group(1)] = a.group(2)
            continue
        c = CALL_RE.match(stmt)
        if not c:
            continue
        name, args = c.group(1), split_args(c.group(2))
        if name not in ("less_than_strict", "less_than_or_equal", "less_than_loose"):
            continue
        if len(args) < 2:
            findings.append((path, name, "comparison with fewer than two operands"))
            continue
        for operand in args[:2]:
            leaves = {ident for ident in support(operand, assign) if ident in witnesses}
            unchecked = sorted(leaves - ranged)
            if unchecked:
                findings.append((
                    path,
                    f"{name}({', '.join(args[:2])})",
                    f"operand `{operand}` rests on {', '.join(unchecked)}, which no `range_check` "
                    f"precedes — the comparison is over field residues, not integers",
                ))
    return findings


def classify(path, constants, witnesses, stmts, opcodes, manifest):
    """Walk the circuit, returning (findings, instances) where findings are failures.

    Order matters for classifications 1 and 2 (an assignment or an equality must precede the
    expose), but classification 3 is a property of the exposure set as a whole, so the raw-witness
    exposures are held back and resolved against every determination the circuit ever exposes.
    """
    derived, bound = set(), {}          # bound: name -> the determined expression it equals
    assign = {}                         # name -> rhs
    determined_exposures = []           # expressions that pin an expose
    pending = []                        # raw-witness exposes, resolved at the end
    findings, instances, notes = [], [], []

    for stmt in stmts:
        # Assignment: `x = expr`
        m = re.match(r"^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)$", stmt, re.DOTALL)
        if m and not stmt.startswith("constrain") and not stmt.startswith("range_check"):
            lhs, rhs = m.group(1), m.group(2)
            assign[lhs] = rhs
            if is_determined(rhs, derived, constants, witnesses, opcodes):
                derived.add(lhs)
            continue
        c = CALL_RE.match(stmt)
        if not c:
            continue
        name, args = c.group(1), split_args(c.group(2))
        if name == "constrain_equal_base" or name == "constrain_equal_point":
            if len(args) == 2:
                a, b = args
                a_det = is_determined(a, derived, constants, witnesses, opcodes)
                b_det = is_determined(b, derived, constants, witnesses, opcodes)
                if a_det and NAME_RE.match(b):
                    bound.setdefault(b, a)
                if b_det and NAME_RE.match(a):
                    bound.setdefault(a, b)
            continue
        if name == "constrain_instance":
            if len(args) != 1:
                findings.append((path, stmt, "constrain_instance takes one argument"))
                continue
            arg = args[0]
            if NAME_RE.match(arg):
                if arg in derived:
                    instances.append((arg, "derived"))
                    determined_exposures.append(resolve(assign.get(arg, arg), assign))
                elif arg in bound:
                    instances.append((arg, "bound"))
                    determined_exposures.append(resolve(bound[arg], assign))
                elif arg in constants:
                    instances.append((arg, "constant"))
                elif arg in witnesses:
                    pending.append((arg, stmt))
                else:
                    instances.append((arg, "unknown"))
                    findings.append((path, arg, "exposed name is neither a witness nor assigned "
                                                "in-circuit"))
            elif is_determined(arg, derived, constants, witnesses, opcodes):
                instances.append((arg, "derived-inline"))
                determined_exposures.append(arg)
            else:
                findings.append((path, arg, "exposed expression is not determinable from the "
                                            "witnesses in scope"))

    for arg, stmt in pending:
        pin = next((e for e in determined_exposures if arg in support(e, assign)), None)
        if pin is not None:
            # Redundant: not a failure. Recorded so the disclosure is visible and so the
            # expression that discharges it is named rather than assumed.
            instances.append((arg, "redundant"))
            notes.append((arg, "pinned by `%s`" % " ".join(pin.split())))
        elif (path, arg) in manifest:
            instances.append((arg, "declared-free"))
        else:
            findings.append(
                (path, arg, "witness exposed with no derivation, no binding, and no place in any "
                            "exposed determination — Orchard-class, or add it to "
                            "script/circuit_free_instances.txt"))
    return findings, instances, notes


# ---------------------------------------------------------------------------


def zk_files():
    import glob
    roots = ["src/contract/*/proof", "proofs/core", "bin/darkirc/proof"]
    out = []
    for root in roots:
        out.extend(sorted(glob.glob(os.path.join(REPO_ROOT, root, "*.zk"))))
    return out


def main():
    argv = sys.argv[1:]
    quiet = "--quiet" in argv
    as_json = "--json" in argv

    opcodes = known_opcodes()
    if opcodes is None:
        fail(f"cannot read the opcode vocabulary from {os.path.relpath(OPCODES_RS, REPO_ROOT)}")
        return 1
    opcodes |= EXTERNAL

    files = zk_files()
    if not files:
        fail("no .zk files found")
        return 1
    manifest = load_manifest()

    all_findings, all_cmp_findings, rows = [], [], []
    for path in files:
        rel = os.path.relpath(path, REPO_ROOT)
        text = strip_comments(open(path, encoding="utf-8").read())
        constants, witnesses, stmts = parse_circuit(text)
        instances = [s for s in stmts if s.startswith("constrain_instance")]
        findings, classified, notes = classify(rel, constants, witnesses, stmts, opcodes, manifest)
        cmp_findings = comparison_operand_findings(rel, constants, witnesses, stmts, opcodes)
        all_findings.extend(findings)
        all_cmp_findings.extend(cmp_findings)
        rows.append({
            "path": rel,
            "witnesses": len(witnesses),
            "instances": len(instances),
            "classified": classified,
            "notes": [{"instance": n[0], "why": n[1]} for n in notes],
            "findings": [{"instance": f[1], "why": f[2]} for f in findings],
            "comparison_findings": [{"comparison": f[1], "why": f[2]} for f in cmp_findings],
        })

    if not quiet:
        print(f"{'circuit':<62} {'inst':>4}  classification")
        print("-" * 100)
        for r in rows:
            kinds = {}
            for _, kind in r["classified"]:
                kinds[kind] = kinds.get(kind, 0) + 1
            summary = ", ".join(f"{k}:{v}" for k, v in sorted(kinds.items()))
            mark = f"{RED}FAIL{NC}" if r["findings"] else f"{GREEN}ok{NC}  "
            print(f"{mark} {r['path']:<57} {r['instances']:>4}  {summary}")

    total_instances = sum(r["instances"] for r in rows)
    kinds = {}
    for r in rows:
        for _, k in r["classified"]:
            kinds[k] = kinds.get(k, 0) + 1
    print()
    for f in all_findings:
        fail(f"{f[0]}: {f[1]} — {f[2]}")
    print()
    for f in all_cmp_findings:
        fail(f"{f[0]}: {f[1]} — {f[2]}")
    print()
    print(f"circuits: {len(rows)}   constrain_instance: {total_instances}   "
          + "   ".join(f"{k}:{v}" for k, v in sorted(kinds.items()))
          + f"   unclassified: {len(all_findings)}")
    print("(redundant = the prover was already choosing this value; the expose discloses it to "
          "the host and adds no freedom — see the module docstring for what it does not say)")
    cmp_circuits = len({f[0] for f in all_cmp_findings})
    print(f"comparison operands with no range_check beneath them: {len(all_cmp_findings)} "
          f"in {cmp_circuits} circuit(s)")
    if as_json:
        print(json.dumps({"rows": rows, "kinds": kinds, "unclassified": len(all_findings),
                          "unbounded_comparisons": len(all_cmp_findings)}))
    if all_findings or all_cmp_findings:
        if all_findings:
            print(f"{RED}FAIL:{NC} {len(all_findings)} instance(s) are neither derived, bound, "
                  f"redundant, nor declared free")
        if all_cmp_findings:
            print(f"{RED}FAIL:{NC} {len(all_cmp_findings)} comparison operand(s) rest on "
                  f"witnesses no `range_check` bounds")
        return 1
    ok(f"every constrain_instance in {len(rows)} circuits is derived, bound, redundant, "
       f"or declared free, and every comparison operand is range-checked")
    return 0


if __name__ == "__main__":
    sys.exit(main())
