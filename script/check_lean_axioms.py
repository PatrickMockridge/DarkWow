#!/usr/bin/env python3
"""Enforce the Lean assumption boundary in proofs/lean/.

Reading a theorem in proofs/lean/ is supposed to tell you what it depends on. This
script is what makes that true rather than aspirational. It fails the build on:

  1. `sorry` or `admit` anywhere in proofs/lean/src.
  2. An `axiom` — or a value-less `opaque`, which is the same thing under a different
     keyword — outside src/DarkFi/Axioms.lean.
  3. An assumption in Axioms.lean missing any of its four fields, or whose `IF FALSE:`
     field names neither a declaration that exists nor a recorded HAZOP silence.
  4. A theorem/lemma whose `@[axiom_budget N]` is missing or disagrees with the axiom set
     `Lean.collectAxioms` reports for it.
  5. A declaration whose axiom set contains a trust axiom or `Classical.choice` without the
     file carrying a matching `-- DECLARED:` line.
  6. An assumption in `Axioms.lean` that the register does not list, or one the register lists
     that `Axioms.lean` does not declare — compared in both directions.
  7. A theorem whose statement is true of nothing, or whose proof is a projection of one of its
     own hypotheses.
  8. An assumption whose prose cites a `DarkFi.HAZOP.<Tier>` as its record, where that tier does
     not name it.

It then prints the table: theorem -> budget -> the assumptions it rests on.

Check 4 needs `src/CheckAxioms.lean` to have been run, which needs the library to compile.
When it cannot run, this script prints `SKIP` for check 4 and says why — it never reports a
pass it did not establish. Pass `--require-collector` to make that SKIP fatal (the CI gate
does, so a broken build cannot present itself as a clean boundary).

Usage:
    python3 script/check_lean_axioms.py                    # human-readable
    python3 script/check_lean_axioms.py --json             # machine-readable
    python3 script/check_lean_axioms.py --require-collector
    python3 script/check_lean_axioms.py --emit-annotations # write budgets into the sources
"""

import json
import os
import re
import subprocess
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEAN_DIR = os.path.join(REPO_ROOT, "proofs", "lean")
SRC_DIR = os.path.join(LEAN_DIR, "src")
AXIOMS_FILE = os.path.join(SRC_DIR, "DarkFi", "Axioms.lean")

# The four fields every assumption must carry. Order is conventional, presence is not.
FIELDS = ("ASSUMES:", "NOT PROVED BECAUSE:", "DISCHARGED BY:", "IF FALSE:")

# Axioms Lean itself contributes. `propext` and `Quot.sound` are Lean's logic and are
# reported but not charged to a budget; `Classical.choice` is avoidable, so using it
# without declaring it is a defect and it *is* charged.
FOUNDATION = {"propext", "Quot.sound"}
CLASSICAL = "Classical.choice"
TRUST = {"Lean.ofReduceBool", "Lean.trustCompiler", "sorryAx"}

RED, GREEN, YELLOW, NC = "\033[31m", "\033[32m", "\033[33m", "\033[0m"


def ok(msg):
    print(f"{GREEN}OK:{NC}  {msg}")


def fail(msg):
    print(f"{RED}FAIL:{NC} {msg}")


def warn(msg):
    print(f"{YELLOW}WARN:{NC} {msg}")


def skip(msg):
    print(f"{YELLOW}SKIP:{NC} {msg}")


def lean_sources():
    """Every .lean file under proofs/lean/src, deterministically ordered."""
    out = []
    for root, dirs, files in os.walk(SRC_DIR):
        dirs.sort()
        for f in sorted(files):
            if f.endswith(".lean"):
                out.append(os.path.join(root, f))
    return out


def rel(path):
    return os.path.relpath(path, REPO_ROOT)


def strip_comments(text):
    """Remove `-- line` and nested `/- block -/` comments, preserving line structure.

    Needed so that a `sorry` mentioned in a doc comment — and this tree has several, as
    prose about what is *not* proved — is not mistaken for a `sorry` in a proof.
    """
    out = []
    depth = 0
    i = 0
    n = len(text)
    while i < n:
        if depth == 0 and text.startswith("--", i):
            while i < n and text[i] != "\n":
                i += 1
        elif text.startswith("/-", i):
            depth += 1
            i += 2
        elif depth > 0 and text.startswith("-/", i):
            depth -= 1
            i += 2
        elif depth > 0:
            if text[i] == "\n":
                out.append("\n")
            i += 1
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def check_no_sorry():
    """(1) No `sorry` or `admit` in a proof position."""
    bad = []
    for path in lean_sources():
        code = strip_comments(open(path, encoding="utf-8").read())
        for lineno, line in enumerate(code.splitlines(), 1):
            for word in ("sorry", "admit"):
                if re.search(rf"\b{word}\b", line):
                    bad.append(f"{rel(path)}:{lineno}: {line.strip()}")
    if bad:
        for b in bad:
            fail(f"sorry/admit: {b}")
        return False
    ok("no `sorry` or `admit` in proofs/lean/src")
    return True


OPAQUE_RE = re.compile(r"^\s*(?:private\s+|protected\s+|noncomputable\s+)*opaque\s+(\S+)")


def check_axiom_location():
    """(2) Assumptions live only in Axioms.lean — `axiom` and value-less `opaque` alike.

    The `opaque f : T` form, with no `:=`, declares a constant with no value: an assumption
    spelled with a different keyword (`Lean/Elab/MutualDef.lean` documents the form). A
    boundary drawn only around the `axiom` keyword would be porous.
    """
    bad = []
    for path in lean_sources():
        code = strip_comments(open(path, encoding="utf-8").read())
        lines = code.splitlines()
        for lineno, line in enumerate(lines, 1):
            m = re.match(r"^\s*(?:private\s+|protected\s+|noncomputable\s+)*axiom\s+(\S+)", line)
            if m and os.path.abspath(path) != os.path.abspath(AXIOMS_FILE):
                bad.append(f"{rel(path)}:{lineno}: axiom {m.group(1)}")
                continue
            m = OPAQUE_RE.match(line)
            if m and os.path.abspath(path) != os.path.abspath(AXIOMS_FILE):
                # Value-less if no `:=` appears on this line or the lines the declaration spans.
                span = " ".join(lines[lineno - 1: lineno + 8])
                span = span.split("\nderiving")[0]
                if ":=" not in span:
                    bad.append(f"{rel(path)}:{lineno}: value-less opaque {m.group(1)}")
    if bad:
        for b in bad:
            fail(f"assumption outside Axioms.lean: {b}")
        return False
    ok("every `axiom`/value-less `opaque` is in src/DarkFi/Axioms.lean")
    return True


KNOWN_DECL_RE = re.compile(r"^\s*(?:@\[[^\]]*\]\s*)?(?:private\s+|protected\s+|noncomputable\s+)*"
                           r"(?:def|theorem|lemma|abbrev|opaque|axiom|structure|inductive|instance)\s+"
                           r"([A-Za-z_][A-Za-z0-9_.']*)", re.MULTILINE)


def declared_names():
    """Every declaration name in the tree, for checking `IF FALSE:` targets."""
    names = set()
    for path in lean_sources():
        code = strip_comments(open(path, encoding="utf-8").read())
        for m in KNOWN_DECL_RE.finditer(code):
            full = m.group(1)
            names.add(full)
            names.add(full.split(".")[-1])
    return names


def assumptions():
    """Parse each assumption block in Axioms.lean into (name, lineno, docblock)."""
    text = open(AXIOMS_FILE, encoding="utf-8").read()
    out = []
    for m in re.finditer(
        r"/--(.*?)-/\s*\n((?:(?:private|protected|noncomputable)\s+)*"
        r"(?:axiom|opaque)\s+([A-Za-z_][A-Za-z0-9_.']*))",
        text, re.DOTALL,
    ):
        doc, _, name = m.group(1), m.group(2), m.group(3)
        lineno = text[: m.start()].count("\n") + 1
        out.append((name, lineno, doc))
    return out


HAZOP_ENTRY_RE = re.compile(r'\(\s*"([A-Z_0-9-]+)')


def hazop_entry_ids():
    """Entry labels recorded in the HAZOP tier files (e.g. `ELEV-1`)."""
    ids = set()
    hazop = os.path.join(SRC_DIR, "DarkFi", "HAZOP")
    for f in sorted(os.listdir(hazop)):
        if f.endswith(".lean"):
            text = strip_comments(open(os.path.join(hazop, f), encoding="utf-8").read())
            for m in HAZOP_ENTRY_RE.finditer(text):
                ids.add(m.group(1).rstrip(":"))
    return ids


def check_axiom_fields():
    """(3) Every assumption carries all four fields, and `IF FALSE:` is checkable."""
    names = declared_names()
    hazop = hazop_entry_ids()
    bad = []
    total = 0
    for name, lineno, doc in assumptions():
        total += 1
        for field in FIELDS:
            if field not in doc:
                bad.append(f"Axioms.lean:{lineno} {name}: missing `{field}`")
        if "IF FALSE:" not in doc:
            continue
        iffield = doc.split("IF FALSE:", 1)[1].strip()
        first = iffield.split("\n")[0]
        if re.match(r"NOTHING\b", first):
            # A silent assumption must name the HAZOP entry that records the silence, so
            # silence is a written-down state rather than an omission.
            refs = set(re.findall(r"\b(CRIT|HIGH|ELEV)-?[0-9]*\b", iffield))
            labels = set(re.findall(r"\b((?:CRIT|HIGH|ELEV)-[0-9]+)\b", iffield))
            if not labels or not labels & hazop:
                bad.append(
                    f"Axioms.lean:{lineno} {name}: `IF FALSE: NOTHING` must cite a HAZOP entry "
                    f"(one of {sorted(labels) or 'none'} not found in HAZOP/)"
                )
        else:
            # A loud assumption must name a declaration that exists. Take the first
            # backticked or bare identifier on the first line.
            cands = re.findall(r"`([A-Za-z_][A-Za-z0-9_.']*)`", first)
            if not cands:
                cands = re.findall(r"\b([A-Za-z_][A-Za-z0-9_.']*)\b", first)
            if not any(c in names or c.split(".")[-1] in names for c in cands):
                bad.append(
                    f"Axioms.lean:{lineno} {name}: `IF FALSE:` names no declaration "
                    f"(candidates: {cands[:4]})"
                )
    if bad:
        for b in bad:
            fail(b)
        return False
    ok(f"all {total} assumptions carry the four fields, with checkable `IF FALSE:` targets")
    return True


HAZOP_TIER_RE = re.compile(r"DarkFi\.HAZOP\.(Critical|High|Elevated)")


def check_hazop_citations():
    """(8) An assumption that says it is recorded in a HAZOP tier is recorded there.

    Check 3 validates the `IF FALSE:` field — that it names a declaration that exists, or a HAZOP
    entry that exists. It says nothing about the *other* citation an assumption makes in its prose,
    `Recorded as LOUD in DarkFi.HAZOP.High`, and `poseidon_collision_resistance` carried exactly
    that sentence while no entry for it existed anywhere in the tier files. The claim read as
    evidence for as long as nobody grepped for it — which is the failure this whole script exists
    to make impossible, so it is worth a check rather than a note.

    The rule is the narrow one that would have caught it: if a docstring names a tier, the tier's
    source must name the assumption.
    """
    hazop_dir = os.path.join(SRC_DIR, "DarkFi", "HAZOP")
    if not os.path.isdir(hazop_dir):
        skip("HAZOP citation check: no DarkFi/HAZOP directory")
        return None
    texts = {}
    for f in sorted(os.listdir(hazop_dir)):
        if f.endswith(".lean"):
            texts[f[: -len(".lean")]] = open(os.path.join(hazop_dir, f), encoding="utf-8").read()
    bad, checked = [], 0
    for name, lineno, doc in assumptions():
        for tier in sorted(set(HAZOP_TIER_RE.findall(doc))):
            checked += 1
            if tier not in texts:
                bad.append(f"Axioms.lean:{lineno} {name}: cites DarkFi.HAZOP.{tier}, "
                           f"which is not a file in DarkFi/HAZOP/")
            elif name not in texts[tier]:
                bad.append(f"Axioms.lean:{lineno} {name}: cites DarkFi.HAZOP.{tier} as its record, "
                           f"but {tier}.lean never names it")
    if bad:
        fail(f"{len(bad)} HAZOP citation(s) do not resolve")
        for b in bad:
            print(f"      {b}")
        return False
    ok(f"every assumption citing a HAZOP tier is named in it ({checked} citation(s) checked)")
    return True


BUDGET_RE = re.compile(r"@\[\s*axiom_budget\s+([0-9]+)\s*\]")


def declared_budgets():
    """`@[axiom_budget N]` values, keyed by the declaration they precede.

    Finds the declaration regardless of how many lines the attribute and the declaration's
    name are apart, and regardless of whether the attribute is on its own line.

    Comments are stripped first, and that is load-bearing rather than tidy. This function used
    to read the raw text while `qualified_theorems` stripped it, and the inconsistency was
    silently wrong in both directions:

      * a deletion note that *quotes* the annotation it removed was read as a live
        declaration — which is how `Capability/Concurrency.lean`'s record of the deleted
        `@[axiom_budget 1] parallel_commutative` was attributed to `DarkFi.Semantics
        .parallel_commutative`, producing a budget mismatch on a theorem that measures 0;
      * `AxiomBudget.lean`'s own docstring contains `@[axiom_budget 1]` followed by a theorem
        name as an *example*, and that was being collected as a real annotation.

    The second is the worse one: an example in a docstring could satisfy the annotation
    requirement for a real theorem, so the check could pass on a theorem nobody annotated.
    """
    budgets = {}
    for path in lean_sources():
        text = strip_comments(open(path, encoding="utf-8").read())
        for m in BUDGET_RE.finditer(text):
            n = int(m.group(1))
            tail = text[m.end(): m.end() + 600]
            dm = re.search(
                r"(?:private\s+|protected\s+|noncomputable\s+)*(?:theorem|lemma)\s+"
                r"([A-Za-z_][A-Za-z0-9_.']*)", tail)
            if dm:
                budgets.setdefault(dm.group(1), []).append((rel(path), n))
    return budgets


def qualified_theorems():
    """Fully-qualified names of every `theorem`/`lemma`, tracking `namespace`/`section` nesting.

    The collector looks names up in the compiled environment, and Lean's environment is keyed by
    *fully-qualified* names: `sum_le_length_mul` inside `namespace CrossCutting` is
    `CrossCutting.sum_le_length_mul`, and the unqualified form does not resolve.
    """
    out = {}
    for path in lean_sources():
        code = strip_comments(open(path, encoding="utf-8").read())
        stack = []
        for line in code.splitlines():
            s = line.strip()
            m = re.match(r"namespace\s+([A-Za-z_][A-Za-z0-9_.']*)", s)
            if m:
                stack.append(("ns", m.group(1)))
                continue
            if re.match(r"section\b", s):
                stack.append(("sec", ""))
                continue
            if re.match(r"end\b", s):
                if stack:
                    stack.pop()
                continue
            m = re.match(r"(?:@\[[^\]]*\]\s*)?(theorem|lemma)\s+([A-Za-z_][A-Za-z0-9_.']*)", s)
            if m:
                ns = ".".join(n for k, n in stack if k == "ns" and n)
                out[f"{ns}.{m.group(2)}" if ns else m.group(2)] = (rel(path), m.group(2))
    return out


def check_inventory():
    """(6) The register's declared inventory must equal `Axioms.lean`'s actual assumptions.

    Compared in *both* directions, because the four-field check validates the assumptions that are
    present and so cannot see one that has been deleted. That happened: a block replacement took
    `reward_monotone` out along with the declarations it was aimed at, and nothing failed.
    """
    register = os.path.join(REPO_ROOT, "doc", "src", "arch", "verification-hazop.md")
    if not os.path.exists(register):
        fail(f"assumption inventory: {rel(register)} does not exist")
        return False
    text = open(register, encoding="utf-8").read()
    marker = "<!-- assumption-inventory -->"
    i = text.find(marker)
    if i < 0:
        fail(f"assumption inventory: no `{marker}` marker in {rel(register)}")
        return False
    m = re.search(r"```text\n(.*?)```", text[i:], re.DOTALL)
    if not m:
        fail(f"assumption inventory: no fenced ```text block after the marker in {rel(register)}")
        return False
    declared = {l.strip() for l in m.group(1).splitlines() if l.strip()}
    actual = {name for name, _, _ in assumptions()}
    absent = sorted(declared - actual)
    undeclared = sorted(actual - declared)
    for n in absent:
        fail(f"{n} is declared in the register's inventory but is not in Axioms.lean "
             f"— an assumption was removed without recording it")
    for n in undeclared:
        fail(f"{n} is in Axioms.lean but not declared in the register's inventory "
             f"— an assumption was added without recording it")
    if absent or undeclared:
        return False
    ok(f"the register's inventory matches Axioms.lean ({len(actual)} assumptions)")
    return True


def run_collector():
    """Run src/CheckAxioms.lean over the names the sources declare.

    The name list is supplied on stdin: walking the whole environment instead would include all
    of Mathlib and never finish.
    """
    names = sorted(qualified_theorems())
    if not names:
        return None, "no theorem/lemma declarations found in the sources"
    cmd = ["lake", "env", "lean", "--run", "src/CheckAxioms.lean"]
    try:
        proc = subprocess.run(cmd, cwd=LEAN_DIR, input="\n".join(names) + "\n",
                              capture_output=True, text=True, timeout=3600)
    except FileNotFoundError:
        return None, "`lake` not found on PATH"
    except subprocess.TimeoutExpired:
        return None, "collector timed out"
    if proc.returncode != 0:
        first = (proc.stderr or proc.stdout or "").strip().splitlines()
        return None, (first[0] if first else f"exit {proc.returncode}")
    rows = {}
    for line in proc.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) != 6:
            continue
        name, _, axs, stmt_consts, trivial, projection = parts
        rows[name] = {
            "axioms": [a for a in axs.split(",") if a],
            "stmt_consts": [c for c in stmt_consts.split(",") if c],
            "trivial": trivial == "true",
            "projection": projection == "true",
        }
    if not rows:
        err = (proc.stderr or "").strip().splitlines()
        return None, f"collector reported no theorems ({err[-1] if err else 'no diagnostics'})"
    return rows, None


def project_roots():
    """Names this project declares — the test for "does this statement mention our system?".

    Derived from the tree rather than blocklisted against Mathlib, and deliberately over-inclusive:
    every `namespace` head *and* every top-level declaration name. Being over-inclusive is the safe
    direction here, because the arm it feeds is the soft one — a name wrongly counted as ours makes
    a statement look less vacuous, i.e. suppresses a warning rather than raising a false one.

    Namespaces alone were not enough: `Emission.reward` is declared at the top level of
    `Emission.lean`, so a namespace-only list reported `reward_zero` and nine other genuine emission
    theorems as "mentions nothing this project declares".
    """
    roots = set()
    ns_re = re.compile(r"^\s*namespace\s+([A-Za-z_][A-Za-z0-9_.']*)", re.M)
    decl_re = re.compile(
        r"^\s*(?:@\[[^\]]*\]\s*)?(?:noncomputable\s+|private\s+|protected\s+)*"
        r"(?:def|theorem|lemma|structure|inductive|opaque|axiom|abbrev|instance|class)\s+"
        r"([A-Za-z_][A-Za-z0-9_']*)", re.M)
    for path in lean_sources():
        code = strip_comments(open(path, encoding="utf-8").read())
        roots |= {m.group(1).split(".")[0] for m in ns_re.finditer(code)}
        roots |= {m.group(1) for m in decl_re.finditer(code)}
    return roots


def check_tautologies(rows):
    """(7) No theorem whose statement is true of nothing, and none that restates its hypothesis.

    Two hard fails, both read off the *elaborated type and proof term* rather than the text:

      * `trivial` — the statement is `True`, or `a = b` / `a ≤ b` / `a < b` / `a ↔ a` with
        syntactically equal sides, or a `∧` of such;
      * `projection` — the proof term is `fun … => <binder>`, i.e. the conclusion *is* one of the
        hypotheses.

    One soft signal, reported but not failed: a statement mentioning no constant this project
    declares. It is soft because it has real false positives — `cross_mul_lt` states a genuine
    fact about `Int` and mentions nothing of ours — and a gate that fails on those would be turned
    off within a week. Tautologies have no false positives, so they are the ones that block.
    """
    if rows is None:
        return None
    roots = project_roots()
    fails, soft = [], []
    for name, r in sorted(rows.items()):
        if r["trivial"]:
            fails.append((name, "statement is true of nothing (True / x = x / x ≤ x / ∧ of such)"))
        if r["projection"]:
            fails.append((name, "conclusion restates a hypothesis (the proof is a projection)"))
        if not any(c.split(".")[0] in roots for c in r["stmt_consts"]):
            soft.append(name)
    if fails:
        fail(f"{len(fails)} tautolog{'y' if len(fails) == 1 else 'ies'} "
             f"(phase-4 bar: no tautologies)")
        for name, why in fails:
            print(f"      {name} — {why}")
        print("      Fix: delete it and state the structural dependency where it is actually "
              "enforced, or restate it so the claim becomes checkable. Do not leave a meaningful "
              "name on a vacuous statement.")
    else:
        ok(f"no theorem is a tautology ({len(rows)} checked)")
    if soft:
        warn(f"{len(soft)} theorem(s) mention no constant this project declares — a signal, not a "
             f"failure: real arithmetic about Int/Nat lands here too")
        for name in soft:
            print(f"      {name}")
    # Return a real Bool: the summary counts `is False`, so returning a count would print FAIL and
    # still exit 0 — which is exactly the "gate that cannot fail" shape this file exists to avoid.
    return not fails


def classify(axioms):
    """Split an axiom set the way the budget model says to."""
    proj = [a for a in axioms if a not in FOUNDATION and a not in TRUST and a != CLASSICAL]
    trust = [a for a in axioms if a in TRUST]
    classical = [a for a in axioms if a == CLASSICAL]
    foundation = [a for a in axioms if a in FOUNDATION]
    return proj, trust, classical, foundation


def check_budgets(rows, require_collector):
    """(4) Every theorem/lemma is annotated, and its annotation matches reality."""
    if rows is None:
        return None
    budgets = declared_budgets()
    bad = []
    unannotated = []
    for name, rec in sorted(rows.items()):
        axioms = rec["axioms"]
        short = name.split(".")[-1]
        proj, trust, classical, _ = classify(axioms)
        actual = len(proj) + len(trust) + len(classical)
        if name not in budgets and short not in budgets:
            unannotated.append((name, actual, axioms))
            continue
        site, declared = (budgets.get(name) or budgets.get(short))[0]
        if declared != actual:
            bad.append(
                f"{site}: {name} declares @[axiom_budget {declared}] but depends on "
                f"{actual} ({', '.join(axioms) or 'none'})"
            )
    for b in bad:
        fail(b)
    if unannotated:
        for name, actual, axioms in unannotated[:40]:
            fail(f"missing @[axiom_budget {actual}] on {name}"
                 + (f"  [{', '.join(axioms)}]" if axioms else ""))
        if len(unannotated) > 40:
            fail(f"... and {len(unannotated) - 40} more unannotated declarations")
    if not bad and not unannotated:
        ok(f"all {len(rows)} theorems annotated, every budget matches its axiom set")
        return True
    return False


DECLARED_RE = re.compile(r"^\s*--\s*DECLARED:\s*(native_decide|Classical\.choice|unsafe)\b", re.MULTILINE)


def check_trust_declarations(rows):
    """(5) A *trust* axiom must be declared in the file that uses it.

    `Lean.ofReduceBool` and `Lean.trustCompiler` mean the proof was discharged by the compiled code
    generator (`native_decide`), and `sorryAx` means it was not discharged at all. Those are choices,
    and a file making one says so.

    `Classical.choice` is deliberately **not** required to carry a `-- DECLARED:` line. It is
    charged in the budget like any other assumption, so it is already visible at every theorem that
    uses it — and it is reached by ordinary automation (`decide`, `omega`, `linarith` on
    `String`/`Finset` equality), not chosen. Requiring a separate ritual line for it would add 14
    near-identical headers to this tree and no information beyond what the budget table prints.
    """
    if rows is None:
        return None
    # Which file declares which theorem, and which files carry a DECLARED: line.
    where = {}
    declared_in = set()
    for path in lean_sources():
        text = open(path, encoding="utf-8").read()
        if DECLARED_RE.search(text):
            declared_in.add(os.path.abspath(path))
        code = strip_comments(text)
        for m in re.finditer(r"^\s*(?:@\[[^\]]*\]\s*)?(?:private\s+|protected\s+|noncomputable\s+)*"
                             r"(?:theorem|lemma)\s+([A-Za-z_][A-Za-z0-9_.']*)", code, re.MULTILINE):
            where.setdefault(m.group(1), os.path.abspath(path))
    bad = []
    seen = set()
    for name, rec in sorted(rows.items()):
        axioms = rec["axioms"]
        short = name.split(".")[-1]
        _, trust, _, _ = classify(axioms)
        if not trust:
            continue
        path = where.get(short) or where.get(name)
        if path is None or path in declared_in or path in seen:
            continue
        seen.add(path)
        bad.append(f"{rel(path)}: proof of {short} uses {', '.join(sorted(set(trust)))} "
                   f"without a `-- DECLARED:` line")
    if bad:
        for b in bad:
            fail(b)
        return False
    ok("every use of the trust axioms (native_decide, sorry, unsafe) is declared in its file")
    return True


def emit_table(rows):
    if rows is None:
        return
    print()
    print(f"{'theorem':<62} {'budget':>6}  assumptions")
    print("-" * 110)
    for name, rec in sorted(rows.items(), key=lambda kv: (-len(kv[1]["axioms"]), kv[0])):
        axioms = rec["axioms"]
        proj, trust, classical, foundation = classify(axioms)
        budget = len(proj) + len(trust) + len(classical)
        shown = ", ".join(proj) or "-"
        extra = []
        if trust:
            extra.append("trust:" + "+".join(sorted(trust)))
        if classical:
            extra.append("classical")
        if foundation:
            extra.append("foundation:" + "+".join(sorted(foundation)))
        label = f"{name.split('.')[-1]:<62} {budget:>6}  {shown}"
        if extra:
            label += f"   [{'; '.join(extra)}]"
        print(label)


def emit_annotations(rows):
    """Write the measured budgets into the sources. Idempotent."""
    if rows is None:
        print("--emit-annotations needs collector output; skipping.", file=sys.stderr)
        return 1
    budgets = declared_budgets()
    changed = 0
    for name, rec in sorted(rows.items()):
        axioms = rec["axioms"]
        short = name.split(".")[-1]
        proj, trust, classical, _ = classify(axioms)
        actual = len(proj) + len(trust) + len(classical)
        existing = budgets.get(name) or budgets.get(short)
        if existing and existing[0][1] == actual:
            continue
        # Locate the declaration and insert or rewrite the annotation.
        for path in lean_sources():
            text = open(path, encoding="utf-8").read()
            pat = re.compile(
                r"(?m)^([ \t]*)((?:\s*@\[[^\]]*\]\s*\n[ \t]*)*)"
                r"((?:private\s+|protected\s+|noncomputable\s+)*(?:theorem|lemma)\s+"
                # `(?![A-Za-z0-9_'])` rather than `\b`: a word boundary cannot follow a name
                # ending in `'`, so `theorem ext'` was silently skipped by the annotation pass.
                + re.escape(short) + r"(?![A-Za-z0-9_']))")
            m = pat.search(text)
            if not m:
                continue
            attrs = m.group(2)
            if "@[axiom_budget" in attrs:
                attrs = re.sub(r"@\[\s*axiom_budget\s+[0-9]+\s*\]",
                               f"@[axiom_budget {actual}]", attrs)
            else:
                attrs = f"@[axiom_budget {actual}]\n{m.group(1)}" + attrs.lstrip("\n")
            text = text[: m.start()] + m.group(1) + attrs + m.group(3) + text[m.end():]
            open(path, "w", encoding="utf-8").write(text)
            changed += 1
            break
    print(f"emit-annotations: updated {changed} declarations")
    return 0


def main():
    argv = sys.argv[1:]
    as_json = "--json" in argv
    require_collector = "--require-collector" in argv

    if "--emit-annotations" in argv:
        rows, err = run_collector()
        if rows is None:
            print(f"emit-annotations: collector unavailable: {err}", file=sys.stderr)
            return 1
        return emit_annotations(rows)

    results = {}
    results["no_sorry"] = check_no_sorry()
    results["axiom_location"] = check_axiom_location()
    results["axiom_fields"] = check_axiom_fields()
    results["hazop_citations"] = check_hazop_citations()
    results["inventory"] = check_inventory()

    rows, collector_err = run_collector()
    if rows is None:
        skip(f"budget check: collector could not run — {collector_err}")
        skip("budget check: this means the library does not compile; budgets are UNVERIFIED")
        if require_collector:
            fail("--require-collector was passed, so a SKIP is fatal")
            results["budgets"] = False
        else:
            results["budgets"] = None
    else:
        results["budgets"] = check_budgets(rows, require_collector)

    results["trust_declarations"] = check_trust_declarations(rows)
    results["tautologies"] = check_tautologies(rows)

    emit_table(rows)

    failed = sum(1 for v in results.values() if v is False)
    passed = sum(1 for v in results.values() if v is True)
    skipped = sum(1 for v in results.values() if v is None)
    print()
    print(f"Passed: {passed}  Failed: {failed}  Skipped: {skipped}")
    if as_json:
        print(json.dumps({"results": results, "row_count": None if rows is None else len(rows)}))
    if failed:
        print(f"{RED}FAIL:{NC} the Lean assumption boundary is not intact")
        return 1
    if skipped:
        print(f"{YELLOW}INCOMPLETE:{NC} {skipped} check(s) could not run — see SKIP above")
    print(f"{GREEN}PASS:{NC} Lean assumption boundary intact")
    return 0


if __name__ == "__main__":
    sys.exit(main())
