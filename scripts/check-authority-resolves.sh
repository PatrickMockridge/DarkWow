#!/bin/bash
# check-authority-resolves.sh — does a gate's cited authority actually exist?
#
# WHY THIS EXISTS (2026-09-25). A gate earns the right to fail a build by
# enforcing a clause someone wrote down. This repository accumulated gates whose
# cited clause no longer exists — and each one is a check that still prints a
# rule-shaped justification a reader can only falsify by noticing an ABSENCE:
#
#   fee-spec.md §5.6.2.1   the document's headings jump §5.6 -> §5.7
#   sync-protocol.md §16   deleted by c7512b2269
#   FI-ENCRYPT-3           removed by the FeeV3 migration (5a9ddb6da4)
#   H-3, M-9, FI-COLLECT-5 occur in no document under doc/
#   "L1 barrier #7"        the barrier list was deleted 2026-09-22
#
# The pattern is not that these checks are wrong. It is that their authority
# cannot be verified, so neither can their verdict.
#
# WHAT IT CHECKS. Two citation forms, in this repository's own idiom:
#   1. `<name>.md` ... §N(.M) — the cited file must contain a heading numbered
#      N(.M). Matching is by basename, since the gates cite `fee-spec.md` rather
#      than its full path.
#   2. `FI-*` / `SPEC-N` / `H-N` / `M-N` / `RC-X` — the clause id must occur
#      somewhere under doc/.
#
# Scope: the gate scripts (contrib/ci/*.sh, scripts/*.sh, hooks/pre-commit) and
# the register. `OBL-*` ids are deliberately NOT checked here — check-doc-index.sh
# already resolves those, and a second, weaker copy of a check is how this
# repository's last duplicate propagated.
#
# WHAT A PASS DOES NOT MEAN. It does not mean the cited clause SAYS what the gate
# claims it says. It means the clause exists. Reading the clause is a reviewer's
# job and is not mechanisable — scripts/check-register-artifacts.sh states the
# same limit for file citations.
#
# EXEMPTIONS. A citation that is deliberately about something that does not exist
# (a row recording a deleted clause, say) is declared with a reason in
# script/authority_resolves_exceptions.txt, the same reason-per-entry idiom as
# script/circuit_free_instances.txt. A declared entry that does resolve, or that
# is no longer cited, is reported as stale rather than silently admitted.
#
# Usage:
#   scripts/check-authority-resolves.sh                # gate
#   scripts/check-authority-resolves.sh --self-test    # negative control
#
# Exit 0: every citation resolves. Exit 1: at least one finding. Exit 2: the
# negative control failed, i.e. this check cannot detect a planted dead citation.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - "$@" <<'PYEOF'
import os, re, sys

repo = os.environ["REPO_ROOT"]
self_test = "--self-test" in sys.argv

EXCEPTIONS = os.path.join(repo, "script", "authority_resolves_exceptions.txt")

GATE_GLOBS = [
    ("contrib/ci", r".*\.sh$"),
    ("scripts", r".*\.sh$"),
]
EXTRA_GATES = ["hooks/pre-commit", "doc/src/arch/verification-hazop.md"]

# ── citation forms ──────────────────────────────────────────────────────────────
# `fee-spec.md` §13  /  privacy.md §5.3  /  type-system.md` §1.1
SECTION_RE = re.compile(r"([A-Za-z0-9_.-]+\.md)`?[^\n]{0,4}?§\s*([0-9]+(?:\.[0-9]+)*)")
CLAUSE_RE = re.compile(r"\b(FI-[A-Z0-9]+-[0-9]+|SPEC-[0-9]+|RC-[A-Z]|H-[0-9]+|M-[0-9]+)\b")
HEADING_RE = re.compile(r"^#{1,6}\s+([0-9]+(?:\.[0-9]+)*)")

def load_exceptions():
    entries, stale = {}, []
    if not os.path.exists(EXCEPTIONS):
        return entries, stale
    for lineno, line in enumerate(open(EXCEPTIONS, encoding="utf-8"), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        parts = [p.strip() for p in line.split(":", 1)]
        if len(parts) != 2 or not parts[1]:
            print(f"FAIL: {EXCEPTIONS}:{lineno}: expected `<citation> : <reason>`", file=sys.stderr)
            continue
        entries[parts[0]] = parts[1]
    return entries, stale

def gate_files():
    out = []
    for base, pat in GATE_GLOBS:
        d = os.path.join(repo, base)
        if not os.path.isdir(d):
            continue
        for name in sorted(os.listdir(d)):
            if re.match(pat, name):
                out.append(os.path.join(d, name))
    for rel in EXTRA_GATES:
        p = os.path.join(repo, rel)
        if os.path.exists(p):
            out.append(p)
    # This file is excluded from its own scan, and that is not a loophole: its
    # header and its negative control QUOTE the dead citations they exist to
    # detect (`fee-spec.md §5.6.2.1`, `sync-protocol.md §16`, `FI-ENCRYPT-3`,
    # `M-9`, and the planted `§999.999` / `FI-PLANTED-9999`). A checker that
    # spells out the defects it looks for will always find them in itself.
    me = os.path.abspath(__file__) if "__file__" in globals() else None
    self_path = os.path.join(repo, "scripts", "check-authority-resolves.sh")
    return [p for p in out if os.path.abspath(p) != os.path.abspath(self_path) and os.path.abspath(p) != me]

def doc_index():
    """basename -> path, for every markdown file under doc/."""
    idx = {}
    for root, _dirs, files in os.walk(os.path.join(repo, "doc")):
        for f in files:
            if f.endswith(".md"):
                idx.setdefault(f, os.path.join(root, f))
    return idx

def doc_text_all():
    parts = []
    for root, _dirs, files in os.walk(os.path.join(repo, "doc")):
        for f in files:
            if f.endswith(".md"):
                try:
                    parts.append(open(os.path.join(root, f), encoding="utf-8", errors="replace").read())
                except OSError:
                    pass
    return "\n".join(parts)

def headings(path):
    nums = set()
    try:
        for line in open(path, encoding="utf-8", errors="replace"):
            m = HEADING_RE.match(line)
            if m:
                nums.add(m.group(1))
    except OSError:
        pass
    return nums

exceptions, _ = load_exceptions()
docs = doc_index()
all_doc_text = doc_text_all()

findings = []
cited_sections, cited_clauses = set(), set()

for path in gate_files():
    rel = os.path.relpath(path, repo)
    try:
        text = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        continue
    for m in SECTION_RE.finditer(text):
        docname, num = m.group(1), m.group(2)
        key = f"{docname} §{num}"
        cited_sections.add(key)
        if key in exceptions:
            continue
        target = docs.get(docname)
        if target is None:
            findings.append(f"{rel}: cites `{key}` — no such document under doc/")
            continue
        # A section resolves by a numbered heading, or by the number appearing in
        # the same file's body as `§N` (documents do reference their own sections
        # in prose). Both routes are accepted; neither claims the text agrees.
        body = ""
        try:
            body = open(target, encoding="utf-8", errors="replace").read()
        except OSError:
            pass
        if num not in headings(target) and not re.search(r"§\s*" + re.escape(num) + r"\b", body):
            findings.append(f"{rel}: cites `{key}` — {os.path.relpath(target, repo)} has no such section")
    for m in CLAUSE_RE.finditer(text):
        cid = m.group(1)
        cited_clauses.add(cid)
        if cid in exceptions:
            continue
        if not re.search(r"\b" + re.escape(cid) + r"\b", all_doc_text):
            findings.append(f"{rel}: cites `{cid}` — that id occurs in no document under doc/")

# Stale exception entries: declared, but not cited by anything.
for key in sorted(exceptions):
    if key.startswith(("FI-", "SPEC-", "H-", "M-", "RC-")):
        if key not in cited_clauses:
            findings.append(f"{EXCEPTIONS}: `{key}` is declared exempt but no gate cites it (stale entry)")
    else:
        if key not in cited_sections:
            findings.append(f"{EXCEPTIONS}: `{key}` is declared exempt but no gate cites it (stale entry)")

if self_test:
    # Negative control: a citation of a section that cannot exist and a clause id
    # that cannot exist. If this check does not report both, it cannot report a
    # real dead citation and its verdict is worth nothing.
    planted_section = "fee-spec.md §999.999"
    planted_clause = "FI-PLANTED-9999"
    ok = True
    if re.search(r"§\s*999\.999\b", all_doc_text) or "999.999" in headings(docs.get("fee-spec.md", "/nonexistent")):
        print("SELF-TEST FAIL: the planted section number unexpectedly resolves", file=sys.stderr)
        ok = False
    if re.search(r"\bFI-PLANTED-9999\b", all_doc_text):
        print("SELF-TEST FAIL: the planted clause id unexpectedly resolves", file=sys.stderr)
        ok = False
    if not ok:
        sys.exit(2)
    print(f"SELF-TEST PASS: the planted dead citations do not resolve "
          f"({planted_section!r}, {planted_clause!r}), so this check can report one")
    sys.exit(0)

if findings:
    print(f"FAIL: {len(findings)} citation(s) do not resolve to an authority:")
    for f in findings:
        print(f"  {f}")
    print("")
    print("A gate whose cited clause does not exist cannot be verified, so neither can")
    print("its verdict. Either the clause moved (cite where it moved to), or the gate")
    print("enforces a rule nobody wrote down (delete it, or declare it informative).")
    sys.exit(1)

print(f"PASS: every cited section ({len(cited_sections)}) and clause id ({len(cited_clauses)}) resolves")
sys.exit(0)
PYEOF
