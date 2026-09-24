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
    python3 script/check_lean_axioms.py --self-test        # negative control, no Lean needed
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

# Where `scripts/lean-build.sh` writes what a guarded command prints to *stderr*. Under `--stream` the
# guard puts stdout on the caller's channel and stderr here, so the collector's own summary and its
# per-name "unknown declaration" lines are read from this file rather than from the process. This
# default must match the guard's `LOG="${LEAN_BUILD_LOG:-/tmp/lean-build.log}"`, and `run_collector`
# appends its own pid to it: one shared name has already cost this tree evidence once.
LEAN_BUILD_LOG = os.environ.get("LEAN_BUILD_LOG", "/tmp/lean-build.log")

# The collector's raw stdout, kept verbatim. Its rows *are* the gate's evidence, and the failure mode
# that produced this file is invisible in the parsed table: a row renamed in flight looks like a
# missing annotation on a declaration that does not exist. Keeping the bytes is what lets
# `missing @[axiom_budget N] on <name>` be re-derived instead of believed — and it is the tree's own
# rule that a test's full output goes to a file in /tmp, untruncated.
COLLECTOR_RAW = os.environ.get("COLLECTOR_RAW", "/tmp/check_lean_axioms.collector.out")

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
            # `mutual … end` opens a block whose `end` closes it — but it does **not** open a
            # namespace, so without this frame the `end` pops a *namespace* and every theorem declared
            # after it is keyed under a mangled name. The failure is silent in the direction that
            # matters: the parser simply produces fewer names, so those theorems are never queried and
            # their budgets are never checked. Found 2026-09-24 by the first module in this tree to use
            # `mutual` — one of whose nine theorems was visible to the parser and eight were not.
            # (`section` above has the opposite shape: it does not affect the name, and its frame
            # exists so its `end` does not pop a namespace either.)
            if re.match(r"mutual\b", s):
                stack.append(("mutual", ""))
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


def parse_rows(raw):
    """Parse the collector's TSV stdout into `{name: record}`, plus any names seen twice.

    A line that is not exactly seven tab-separated fields is not a row and is skipped — which is the
    reason a *partial* line is invisible here: the pieces of a split row each have the wrong field
    count, so they vanish silently while the count of good rows stays one short. That is what
    `reconcile` exists to catch, and the two are deliberately separate so the check can be exercised
    without Lean (see `self_test`, run as `--self-test`).
    """
    rows = {}
    dupes = []
    for line in raw.splitlines():
        parts = line.split("\t")
        if len(parts) != 7:
            continue
        name, _, axs, stmt_consts, trivial, projection, binders = parts
        unref, _, total = binders.partition("/")
        if name in rows:
            dupes.append(name)
        rows[name] = {
            "axioms": [a for a in axs.split(",") if a],
            "stmt_consts": [c for c in stmt_consts.split(",") if c],
            "trivial": trivial == "true",
            "projection": projection == "true",
            "unref_binders": int(unref) if unref.isdigit() else 0,
            "total_binders": int(total) if total.isdigit() else 0,
        }
    return rows, dupes


def unresolved_names(log_text):
    """The names the collector could not resolve, read from the guard's log.

    Its stderr goes there, not to the caller: `--stream` carries stdout only. See
    `scripts/lean-build.sh` for the measurement that forced the split.
    """
    return set(re.findall(
        r"^check_axioms: (?:not a theorem|unknown declaration): (\S+)$", log_text, re.MULTILINE))


def reconcile(names, rows, dupes, unresolved, log_path=None):
    """Return why the collector's rows do not account for exactly the names it was fed, or None.

    Identity, not count — the arm that holds whatever the mechanism turns out to be.

    `check_budgets` iterates the keys it *received*, so a row whose name was renamed in flight leaves
    the declaration the sources declare with no row and no message: its budget is never checked, while
    the row count still matches the number of names fed. Counting cannot see that (704 checked = 704
    fed, and one of those 704 was `nvariant`); comparing *sets* can. A renamed row is an `extra` key
    and the name it displaced is `missing`, so the failure line names both.

    `unresolved` is subtracted from `missing` because a name the collector reported it could not find
    is accounted for — it is diagnosed above, more precisely, by `run_collector`.
    """
    fed = set(names)
    returned = set(rows)
    extra = sorted(returned - fed)
    missing = sorted(fed - returned - unresolved)
    if not (dupes or extra or missing):
        return None
    detail = []
    if dupes:
        detail.append(f"{len(dupes)} row(s) seen twice, e.g. {dupes[0]}")
    if extra:
        detail.append(f"{len(extra)} row(s) for names never fed, e.g. {extra[0]}")
    if missing:
        detail.append(f"{len(missing)} name(s) with neither a row nor an 'unresolved' line, "
                      f"e.g. {missing[0]}")
    return ("the collector's rows do not reconcile with the names it was fed ("
            + "; ".join(detail) + f") — raw stdout in {COLLECTOR_RAW}, diagnostics in "
            f"{log_path or LEAN_BUILD_LOG}; a row was renamed, duplicated or dropped in flight, so at "
            "least one budget is silently unchecked")


def self_test():
    """The negative control for `reconcile` — the corruption that motivated it, reproduced.

    This file exists to make a false green impossible, so its own detector gets the same treatment: an
    arm that has never been shown to fail is a claim, not a check. Both directions are exercised — a
    clean stream must reconcile (else the gate would be red always, which teaches people to read past
    it) and a corrupted one must not.

    The corrupted stream is the 2026-09-24 failure exactly. `supply_chain_invariant`'s row was split 14
    characters into its name by the collector's one-line stderr summary landing mid-row at a 4096-byte
    stdout flush boundary; the tail of the split row is still seven fields, so it parses as a valid row
    named `nvariant`, the row *count* stays right, and a real budget goes unchecked. Deterministic: no
    Lean, no clock, no files.
    """
    def row(name):
        return (f"{name}\t5\tClassical.choice,coinbase_blind,pallasPrime\tAnd,Eq,Nat,Pedersen.Point"
                f"\tfalse\tfalse\t0/3")

    fed = ["a_sound_theorem", "supply_chain_invariant", "z_last_theorem"]
    clean = "".join(row(n) + "\n" for n in fed)
    problems = []
    rows, dupes = parse_rows(clean)
    if len(rows) != len(fed):
        problems.append(f"a clean stream parsed {len(rows)} rows for {len(fed)} names fed")
    if reconcile(fed, rows, dupes, set()) is not None:
        problems.append("a clean stream did not reconcile — the detector would red every run")

    # `row("supply_chain_invariant")[:14]` is `supply_chain_i`; the summary is exactly what landed
    # there. The halves become two lines: a 1-field prefix (skipped) and a 7-field row named `nvariant`.
    r = row("supply_chain_invariant")
    summary = "check_axioms: 3 theorems reported, 0 unresolved"
    corrupted = (row("a_sound_theorem") + "\n" + r[:14] + summary + "\n" + r[14:] + "\n"
                 + row("z_last_theorem") + "\n")
    rows, dupes = parse_rows(corrupted)
    why = reconcile(fed, rows, dupes, set())
    if why is None:
        problems.append("a row renamed in flight reconciled — the detector cannot fail")
    else:
        for token in ("nvariant", "supply_chain_invariant"):
            if token not in why:
                problems.append(f"the failure line does not name {token!r}: {why}")

    # A name the collector reported it could not resolve is accounted for, not missing: the distinct
    # case, and one that must not become a second false red on top of the first.
    rows, dupes = parse_rows("".join(row(n) + "\n" for n in fed[:2]))
    if reconcile(fed, rows, dupes, {"z_last_theorem"}) is not None:
        problems.append("an unresolved name was reported as missing")

    # Two rows under one name is rows colliding, not two theorems.
    rows, dupes = parse_rows(clean + row("a_sound_theorem") + "\n")
    if reconcile(fed, rows, dupes, set()) is None:
        problems.append("a duplicated row reconciled")

    if problems:
        for p in problems:
            fail(f"self-test: {p}")
        return 1
    ok("self-test: a clean stream reconciles, a renamed row is named, an unresolved name and a "
       "duplicate row are told apart")
    return 0


def run_collector():
    """Run src/CheckAxioms.lean over the names the sources declare.

    The name list is supplied on stdin: walking the whole environment instead would include all
    of Mathlib and never finish.
    """
    names = sorted(qualified_theorems())
    if not names:
        return None, "no theorem/lemma declarations found in the sources"
    # Through the repository's Lean guard, never bare. The collector is a `lean` process like any
    # other and carries the same hazard; `--stream` is what keeps its TSV on stdout, where the parser
    # below reads it. Refusing to run unguarded is deliberate — a fallback here would be a bypass of
    # the memory ceiling this tree added after a 4-threaded `lake build DarkFi` froze the host
    # (2026-09-24; see `scripts/lean-build.sh`).
    guard = os.path.join(REPO_ROOT, "scripts", "lean-build.sh")
    if not os.path.exists(guard):
        return None, f"{rel(guard)} does not exist — refusing to run the collector unguarded"
    # A per-run log, because one shared name is a hazard this tree has already paid for: an unrelated
    # build overwrote `/tmp/lean-build.log` once and destroyed evidence a register row cited. The guard
    # truncates whatever it is given, so this holds this run's diagnostics and nothing else — and it is
    # what `run_collector` reads the summary and the unresolved names out of.
    run_log = f"{LEAN_BUILD_LOG}.{os.getpid()}"
    cmd = [guard, "--stream", "env", "lean", "--run", "src/CheckAxioms.lean"]
    try:
        proc = subprocess.run(cmd, cwd=LEAN_DIR, input="\n".join(names) + "\n",
                              capture_output=True, text=True, timeout=3600,
                              env=dict(os.environ, LEAN_BUILD_LOG=run_log))
    except FileNotFoundError:
        return None, "`lake` not found on PATH"
    except subprocess.TimeoutExpired:
        return None, "collector timed out"
    if proc.returncode != 0:
        # The collector's own diagnostics are on stderr, which the guard sends to the log and echoes
        # back to *its* stderr on failure — so the log's last line is the child's error and the
        # caller's stderr begins with the guard's own chatter. Prefer the former.
        try:
            with open(run_log, encoding="utf-8", errors="replace") as fh:
                tail = [l for l in fh.read().splitlines() if l.strip()]
        except OSError:
            tail = []
        if tail:
            return None, tail[-1]
        first = (proc.stderr or proc.stdout or "").strip().splitlines()
        return None, (first[0] if first else f"exit {proc.returncode}")
    raw = proc.stdout or ""
    with open(COLLECTOR_RAW, "w", encoding="utf-8") as fh:
        fh.write(raw)
    rows, dupes = parse_rows(raw)
    # The collector's stderr — its closing summary and one line per name it could not resolve — goes to
    # the guard's log, not to this process (see `--stream` in `scripts/lean-build.sh`). Read this run's.
    try:
        with open(run_log, encoding="utf-8", errors="replace") as fh:
            log_text = fh.read()
    except OSError:
        log_text = ""
    if not rows:
        tail = [l for l in log_text.splitlines() if l.strip()]
        return None, f"collector reported no theorems ({tail[-1] if tail else 'no diagnostics'})"
    # Why unresolved names are fatal: `check_budgets` iterates over what the collector *found*, so a
    # declaration the sources declare but the environment does not hold, or one whose row never
    # arrived, would otherwise have no row, no message, and no budget check — the shape of "181
    # theorems nobody has measured" that this whole file exists to prevent. Measured before making it
    # fatal: `694 theorems reported, 0 unresolved`, so it cannot fire for a pre-existing reason. The
    # disposition follows the register's rule — a gate that is red for a reason nobody is working on
    # is one people learn to read past — and the number is zero.
    unresolved = unresolved_names(log_text)
    if unresolved:
        return None, (f"{len(unresolved)} of {len(names)} declared declarations did not resolve "
                      f"(e.g. {sorted(unresolved)[0]}), so their budgets are unverified rather than "
                      "checked")
    why = reconcile(names, rows, dupes, unresolved, run_log)
    if why:
        return None, why
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

    Three hard fails, all read off the *elaborated type and proof term* rather than the text:

      * `trivial` — the statement is `True`, or `a = b` / `a ≤ b` / `a < b` / `a ↔ a` with
        syntactically equal sides, or a `∧` of such;
      * `projection` — the proof term is `fun … => <binder>`, i.e. the conclusion *is* one of the
        hypotheses;
      * `binder-free` — the proof mentions **no** explicit binder, so it uses no argument the
        statement asked for. This is the class the other two leave between them: a statement that is
        neither an identity nor a restatement, whose proof simply ignores everything it was given.
        It is sharp in that form (all binders unused), which is why it can block; the weaker
        "some binder unused" cannot, and is the soft signal below.

    Two soft signals, reported but not failed. A statement mentioning no constant this project
    declares, and a theorem with *some* unused explicit binder. Both have real false positives: a
    genuine fact about `Int` mentions nothing of ours (`cross_mul_lt`), and a proof can legitimately
    ignore a parameter because another binder's *type* already carries it
    (`(a : α) (h : a ≤ a) : a ≤ a := h` ignores `a`). A gate that fails on either would be turned
    off within a week; that is the test the hard arms above are held to, and the reason they block.
    """
    if rows is None:
        skip("anti-vacuity: the collector could not run, so no theorem's statement was seen")
        return None
    roots = project_roots()
    fails, soft, soft_binders = [], [], []
    for name, r in sorted(rows.items()):
        if r["trivial"]:
            fails.append((name, "statement is true of nothing (True / x = x / x ≤ x / ∧ of such)"))
        if r["projection"]:
            fails.append((name, "conclusion restates a hypothesis (the proof is a projection)"))
        if r["total_binders"] > 0 and r["unref_binders"] == r["total_binders"]:
            fails.append((name, f"proof mentions none of its {r['total_binders']} explicit "
                                f"binder(s) — no argument is used"))
        if not any(c.split(".")[0] in roots for c in r["stmt_consts"]):
            soft.append(name)
        if 0 < r["unref_binders"] < r["total_binders"]:
            soft_binders.append(f"{name} ({r['unref_binders']}/{r['total_binders']})")
    if fails:
        # Counted by *theorem*, not by finding: the arms are independent and a single theorem can trip
        # more than one, which is corroboration rather than a second defect. The three theorems this
        # arm found on 2026-09-24 each tripped two, and a summary reading "6 tautologies" for three
        # theorems is the kind of number a reader stops trusting.
        n = len({name for name, _ in fails})
        fail(f"{n} tautolog{'y' if n == 1 else 'ies'} "
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
    if soft_binders:
        warn(f"{len(soft_binders)} theorem(s) leave an explicit binder unused — a signal, not a "
             f"failure: a binder whose type another binder already carries is legitimately unused")
        for entry in soft_binders:
            print(f"      {entry}")
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


def annotation_for(name, short, where, budgets):
    """The `@[axiom_budget]` that belongs to *this* declaration, or `(None, None)`.

    Keyed by **declaring file**, not by short name, and the difference is a hole this function
    exists to close. `declared_budgets` reads annotations out of the sources and can only key them
    by the name as written (`theorem foo` gives `foo`), while the collector reports fully-qualified
    names (`DarkFi.Capability.Value.foo`) — so the two sides can never meet on the qualified form,
    and the check fell back to `name.split(".")[-1]`. Any theorem sharing a short name with an
    annotated theorem *anywhere in the tree* then inherited that annotation:

      * `Capability/Value.lean`'s `value_conservation_no_wraparound` carries no annotation of its
        own, and passed check 4 on the strength of `CrossCutting.lean`'s theorem of the same name.
        Two different theorems, two different axiom sets, one annotation between them.

    Requiring the annotation to come from the file that declares the theorem closes it, because the
    file is the one thing both sides can agree on: `qualified_theorems` maps each fully-qualified
    name to its source path. A declaration the scanner cannot place (a name it cannot match, or one
    that only exists after elaboration) still falls back to a short-name match, but only when that
    match is unambiguous — an ambiguous one is reported as unannotated rather than guessed at.
    """
    if name in budgets:
        return budgets[name][0]
    cands = budgets.get(short)
    if not cands:
        return (None, None)
    decl_file = where.get(name, (None, None))[0]
    if decl_file is None:
        return cands[0] if len(cands) == 1 else (None, None)
    same = [c for c in cands if c[0] == decl_file]
    if len(same) == 1:
        return same[0]
    return (None, None)


def check_budgets(rows):
    """(4) Every theorem/lemma is annotated, and its annotation matches reality."""
    if rows is None:
        return None
    budgets = declared_budgets()
    where = qualified_theorems()
    bad = []
    unannotated = []
    for name, rec in sorted(rows.items()):
        axioms = rec["axioms"]
        short = name.split(".")[-1]
        proj, trust, classical, _ = classify(axioms)
        actual = len(proj) + len(trust) + len(classical)
        site, declared = annotation_for(name, short, where, budgets)
        if site is None:
            unannotated.append((name, actual, axioms))
            continue
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
        skip("trust declarations: the collector could not run, so no theorem's axiom set is known")
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
        print()
        print("(budget table unavailable: the collector could not run, so no budget was measured)")
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

    if "--self-test" in argv:
        return self_test()

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
        results["budgets"] = check_budgets(rows)

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
        # The tally used to read `could not run — see SKIP above` while only *one* of the skipped
        # checks printed a SKIP line, and then printed `PASS … intact` anyway. A reader had to notice
        # that the count and the number of SKIP lines disagreed. Each skipped check now prints its own
        # line (see the check functions) and the verdict is named here rather than claimed.
        #
        # And it exits **2, not 0**: this tree already has the convention that 2 means "ran, and did
        # not check" (`scripts/check-circuit-domain-separation.sh:32`,
        # `scripts/check-pubkey-binding.sh:38`), and with `run_gate` deciding on exit status alone a 0
        # here is indistinguishable from a pass — which is the defect this whole file was repaired for.
        names = ", ".join(k for k, v in results.items() if v is None)
        print(f"{YELLOW}INCOMPLETE:{NC} {skipped} check(s) did not run: {names}")
        print(f"{YELLOW}INCOMPLETE:{NC} this run does NOT establish that the boundary is intact")
        return 2
    print(f"{GREEN}PASS:{NC} Lean assumption boundary intact")
    return 0


if __name__ == "__main__":
    sys.exit(main())
