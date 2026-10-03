"""Find values a `.zk` circuit computes that reach no constraint.

The checker behind `scripts/check-circuit-dead-values.sh`; see that file's header for the
rule, its two halves, the incident that earned it (github issue #3) and — importantly — what
a pass does and does not mean. This module holds only the parsing and the liveness closure.

## The rule

    H1  THE DEAD WITNESS       a `witness` name referenced by no derivation, no equality and
                               no instance.
    H2  THE DEAD DERIVATION    a value assigned in-circuit that neither is instanced nor
                               reaches a `constrain_equal_*`, directly or through another
                               assignment.

"Reaches" is a fixed point over the assignment graph, not a single step: `a = f(b)` and
`constrain_instance(a)` makes `b` live, and `c = g(b)` where `c` is live makes `b` live by a
second hop. The graph is small and the closure is cheap; the alternative — a one-step check —
misses exactly the chains the `consume_claim.zk` form relies on.

## The assumptions, stated so they can be argued with

* **A constraint is a seed.** `constrain_instance` and `constrain_equal_*` arguments are live
  by construction. This module does not ask whether the constraint *means* anything — a value
  compared against a value the prover also chose is live here and is
  `check-pubkey-binding.sh`'s business, not this one's.
* **Only the circuit body is walked.** Witness declarations and the body are read; the
  `constant` block's names are treated as live always, because a constant that no longer
  appears is a declaration to remove rather than a value that reaches no constraint, and
  conflating the two would put a second kind of finding in a list whose entries are all read
  the same way.
* **Names are matched syntactically.** An identifier followed by `(` is a call, not a
  reference, so `witness_base(3)` and `poseidon_hash(...)` are excluded. A name reused for
  two different values inside one circuit would confuse this — and `AGENTS.md` R6 records that
  names in this tree have lied before, which is why the checker's output is a list a reader
  adjudicates rather than a verdict.
"""

import os
import re
from dataclasses import dataclass

WITNESS_BLOCK = re.compile(r'\bwitness\s+"[^"]*"\s*\{(.*?)\}', re.S)
CIRCUIT_BLOCK = re.compile(r'\bcircuit\s+"[^"]*"\s*\{(.*?)\n\}', re.S)

# `Base name,` / `Scalar name,` / `EcPoint name` — the declared type then the field name.
# `\b` before `Base` is what keeps `EcFixedPointBase NULLIFIER_K` out: the position before
# `Base` inside that word is not a word boundary.
DECLARED = re.compile(r'\b(?:Base|Scalar|EcPoint|Uint32|Uint64)\s+([A-Za-z_][A-Za-z0-9_]*)')

ASSIGN = re.compile(r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)$', re.S)
INSTANCE = re.compile(r'constrain_instance\s*\(([^)]*)\)')
EQUAL = re.compile(r'constrain_equal_[a-z]+\s*\(([^)]*)\)')
# An identifier that is not immediately called. Lookbehind for a word char so `witness_base`
# is not read as `witness` + `_base`, and lookahead for `(` so calls are excluded.
IDENT = re.compile(r'(?<![A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]*)(?!\s*\()')
NUMBER = re.compile(r'^\d+$')


@dataclass
class Finding:
    path: str
    line: int
    name: str
    # "DEAD-WITNESS" | "DEAD-DERIVATION" | "DEAD-CONSTANT"
    kind: str


# An alias for a constant — `ZERO = witness_base(0)`, `PREFIX_EVL = witness_base(2)`. These
# are assigned and often unused, and they are **not** the class this checker exists for: a
# value computed from a *witness* that reaches no constraint either authorizes something
# nobody proves (H1) or is a check a reader believes was made (H2), while a constant alias
# computes nothing from a witness and can neither authorize nor check. Reported in their own
# section so the list a reviewer reads stays the list of the class; `AGENTS.md` R2 still says
# to delete them.
CONSTANT_ALIAS = re.compile(r'^(?:witness_base|constant_base)\s*\(\s*\d+\s*\)$')


def identifiers(expr):
    """Bare identifiers in `expr`, excluding calls and numeric literals."""
    out = set()
    for m in IDENT.finditer(expr):
        name = m.group(1)
        if NUMBER.match(name):
            continue
        out.add(name)
    return out


def _strip_comments(text):
    """Blank out `#` comments **without moving a single offset**.

    The comment text is replaced by spaces rather than deleted so that every later offset —
    and therefore every reported line number — still points at the line it was computed from.

    This exists because its absence was the checker's first bug, and it failed in the worst
    direction: a comment line sitting between two statements was glued to the following
    statement by the `;` split, the assignment regex then saw `#` where it wanted an
    identifier and matched nothing, and the assignment was simply never parsed — so every
    witness feeding it was reported dead. Run against the repaired attestation circuits, which
    carry exactly that comment style, it reported the fix as not made. A checker whose parse
    can be broken by a comment reports on its own parser, not on the tree.
    """
    out = []
    for line in text.split("\n"):
        idx = line.find("#")
        out.append(line if idx < 0 else line[:idx] + " " * (len(line) - idx))
    return "\n".join(out)


def _split_statements(body):
    """Split a circuit body into statements, keeping each one's line number.

    Statements end with `;` in this dialect and two are frequently on one line
    (`constrain_instance(a); constrain_instance(b);`), so the split is on the semicolon and
    the line number is recovered from the offset. Comments are blanked first, for the reason
    `_strip_comments` records.
    """
    out = []
    offset = 0
    for stmt in _strip_comments(body).split(";"):
        start = offset
        offset += len(stmt) + 1
        text = stmt.strip()
        if text:
            out.append((start, text))
    return out


def check_file(path, repo):
    """Findings for one `.zk` file. `path` may be absolute; `repo` relativises it."""
    rel = os.path.relpath(path, repo) if os.path.isabs(path) else path
    text = open(path, errors="replace").read()

    wm = WITNESS_BLOCK.search(text)
    if not wm:
        return []
    witnesses = {}
    base_line = text[: wm.start(1)].count("\n") + 1
    for name in DECLARED.findall(wm.group(1)):
        # Position of the name inside the witness block, for a line number.
        rel_off = wm.group(1).find(name)
        witnesses[name] = base_line + wm.group(1)[:rel_off].count("\n")

    cm = CIRCUIT_BLOCK.search(text)
    if not cm:
        return []
    body = cm.group(1)
    body_start_line = text[: cm.start(1)].count("\n") + 1

    assignments = {}   # name -> set(dep identifiers)
    assign_lines = {}
    constant_aliases = set()
    seeds = set()

    for start, stmt in _split_statements(body):
        line = body_start_line + body[:start].count("\n")
        for m in INSTANCE.finditer(stmt):
            seeds |= identifiers(m.group(1))
        for m in EQUAL.finditer(stmt):
            seeds |= identifiers(m.group(1))
        am = ASSIGN.match(stmt)
        if am:
            name, expr = am.group(1), am.group(2)
            # `constrain_instance(x)` and friends are not assignments; the regex above would
            # not match them (they contain a `(` before `=`), but guard anyway.
            if name in ("constrain_instance", "constrain_equal_base"):
                continue
            assignments.setdefault(name, set())
            assignments[name] |= identifiers(expr)
            assign_lines[name] = line
            if CONSTANT_ALIAS.match(expr.strip()):
                constant_aliases.add(name)

    # Liveness to a fixed point: a value is live if it is a seed, or if it feeds a live value.
    live = set(seeds)
    changed = True
    while changed:
        changed = False
        for name, deps in assignments.items():
            if name in live and not deps <= live:
                live |= deps
                changed = True

    findings = []
    for name, line in sorted(witnesses.items(), key=lambda kv: kv[1]):
        if name not in live:
            findings.append(Finding(rel, line, name, "DEAD-WITNESS"))
    for name, line in sorted(assign_lines.items(), key=lambda kv: kv[1]):
        if name not in live:
            kind = "DEAD-CONSTANT" if name in constant_aliases else "DEAD-DERIVATION"
            findings.append(Finding(rel, line, name, kind))
    return findings
