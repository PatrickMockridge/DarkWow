#!/bin/bash
# Documentation index and cross-reference integrity.
#
# The 2026-09 docs spring-clean removed twenty-five documents and repointed every
# referrer by hand. This gate is what keeps that from silently un-happening: the
# orphans and the dangling lesson numbers it checks for were all created the same
# way — by a deletion or a renumbering that nothing verified.
#
# Checks
#   1. SUMMARY.md <-> doc/src, both directions. The one-way check is how docs
#      accumulate unlisted; the other direction is how a SUMMARY keeps a dead link.
#   2. Every relative .md link inside doc/src resolves.
#   3. Every {{#include}} target exists and its :anchor is present in the target.
#   4. No duplicate heading text within a file (a second "Lesson 22").
#   5. Every `OBL-*` id cited anywhere resolves to a row in the register.
#   6. Every `safety.md RC<n>` / `safety.md Lesson <n>` reference resolves.
#   7. No citation of a .md file that does not exist under doc/src. This is the
#      general form of what went wrong eleven times in the clean-up: a code comment
#      naming a document that had been deleted.
#
# Exit 0: the doc index is coherent.
# Exit 1: at least one defect, each printed as file:line: what is wrong.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import os, re, sys, glob, collections

repo = os.environ["REPO_ROOT"]
docs = os.path.join(repo, "doc", "src")
problems = []


def slurp(path):
    """Read a file, or "" if it is unreadable (broken symlinks exist in this tree)."""
    try:
        with open(path, errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""

# ---------------------------------------------------------------- 1. SUMMARY
summary = os.path.join(docs, "SUMMARY.md")
listed = set()
for m in re.finditer(r'\]\(([^)]+)\)', slurp(summary)):
    target = m.group(1).split("#")[0]
    if not target or "://" in target or not target.endswith(".md"):
        continue
    listed.add(os.path.normpath(os.path.join("doc/src", target)))

if not os.path.exists(summary):
    problems.append(f"doc/src/SUMMARY.md: missing")

# README.md/SUMMARY.md are directory furniture, conventionally unlisted.
FURNITURE = {"README.md", "SUMMARY.md"}
for path in sorted(glob.glob(f"{docs}/**/*.md", recursive=True)):
    rel = os.path.relpath(path, repo)
    if os.path.basename(path) in FURNITURE:
        continue
    if rel not in listed:
        problems.append(f"{rel}: not listed in doc/src/SUMMARY.md")

for rel in sorted(listed):
    if not os.path.exists(os.path.join(repo, rel)):
        problems.append(f"doc/src/SUMMARY.md: lists {rel}, which does not exist")

# ------------------------------------------------- 2/3. links and includes
for path in sorted(glob.glob(f"{docs}/**/*.md", recursive=True)):
    rel = os.path.relpath(path, repo)
    base = os.path.dirname(path)
    text = slurp(path)
    for m in re.finditer(r'\]\(([^)]+)\)', text):
        target = " ".join(m.group(1).split())  # a URL may be wrapped across lines
        if target.startswith(("http://", "https://", "mailto:", "#")):
            continue
        target = target.split("#")[0]
        if not target:
            continue
        if not os.path.exists(os.path.normpath(os.path.join(base, target))):
            lineno = text[:m.start()].count("\n") + 1
            problems.append(f"{rel}:{lineno}: link target does not exist: {target}")
    for m in re.finditer(r'\{\{#include\s+([^\s}:]+)(?::([^}]+))?\}\}', text):
        inc, anchor = m.group(1), m.group(2)
        target = os.path.normpath(os.path.join(base, inc))
        if not os.path.exists(target):
            lineno = text[:m.start()].count("\n") + 1
            problems.append(f"{rel}:{lineno}: include target does not exist: {inc}")
        elif anchor and not re.fullmatch(r'[\d:,-]+', anchor):
            # `:18:81` is a line range, `:name` is an anchor.
            body = slurp(target)
            if f"ANCHOR: {anchor}" not in body:
                lineno = text[:m.start()].count("\n") + 1
                problems.append(f"{rel}:{lineno}: include anchor not found in {inc}: {anchor}")

# ------------------------------------------------------- 4. duplicate headings
# Only identifier-shaped headings. A heading may legitimately repeat when a
# document describes several things in turn ("Encryption" per scheme, a config
# block per node profile); what must never repeat is a *numbered* one, because
# that is what a citation resolves to. The duplicate "Lesson 22" is the model.
ID_HEADING = re.compile(r'\b(Lesson|RC|OBL|Step|Phase|Part|Stage|Finding|Rule)\s*[-#]?\s*\d', re.I)
for path in sorted(glob.glob(f"{docs}/**/*.md", recursive=True)):
    rel = os.path.relpath(path, repo)
    seen = collections.Counter()
    for m in re.finditer(r'^(#{2,3})\s+(.+?)\s*$', slurp(path), re.M):
        if ID_HEADING.search(m.group(2)):
            seen[m.group(2)] += 1
    for heading, n in seen.items():
        if n > 1:
            problems.append(f"{rel}: numbered heading appears {n} times: {heading}")

# ------------------------------------------------------------- corpus of ids

register_rel = "doc/src/arch/verification-hazop.md"
safety_rel = "doc/src/dev/contracts/safety.md"
register = slurp(os.path.join(repo, register_rel))
safety = slurp(os.path.join(repo, safety_rel))

SCAN_EXT = ("*.md", "*.rs", "*.sh", "*.py", "*.lean", "*.toml", "*.zk", "Makefile", "*.yml")
scan_files = []
for ext in SCAN_EXT:
    scan_files += glob.glob(f"{repo}/**/{ext}", recursive=True)
SKIP_DIRS = ("/.git/", "/target/", "/doc/book/", "/.lake/", "/vendor/", "/node_modules/", "/plans/")
scan_files = [p for p in scan_files if not any(d in p for d in SKIP_DIRS)]

# --------------------------------------------------- 5. OBL ids resolve
defined_obl = set(re.findall(r'\|\s*(OBL-[CZT]\d+)\s*\|', register))
cited = collections.defaultdict(list)
for path in scan_files:
    for m in re.finditer(r'\bOBL-[CZT]\d+\b', slurp(path)):
        cited[m.group(0)].append(os.path.relpath(path, repo))
for obl, where in sorted(cited.items()):
    if obl not in defined_obl:
        problems.append(f"{where[0]}: cites {obl}, which has no row in {register_rel}")

# ------------------------------- 6. safety.md cross-references resolve
# The alias table keeps legacy numbers resolvable; a renumbering that skips it
# shows up here.
for path in scan_files:
    if not path.endswith((".md", ".rs", ".sh", ".py", ".lean")):
        continue
    rel = os.path.relpath(path, repo)
    text = slurp(path)
    for m in re.finditer(r'safety\.md[^\n]{0,4}?(RC\d+|Lesson\s*\d+|lesson\s*#\d+|C\d+)', text):
        ref = re.sub(r'\s+', ' ', m.group(1))
        if ref not in safety:
            lineno = text[:m.start()].count("\n") + 1
            problems.append(f"{rel}:{lineno}: cites safety.md but it does not mention: {ref}")

# ------------------------------------- 7. no citation of a deleted .md
# Basename-level, against every .md in the repository rather than only doc/src —
# a citation of a README under contrib/ or src/ is legitimate, and what this check
# is for is the citation whose file is gone entirely.
ALLOWED = {"README.md", "CLAUDE.md", "MEMORY.md", "SUMMARY.md", "CHANGELOG.md", "LICENSE.md"}

# Generated into doc/src by `doc/Makefile` / `doc/generate_seminar_ics.py`; absent
# from a fresh clone by design, so their absence is not a dangling reference.
GENERATED = {"seminars.md"}

# Cited, known, and deliberately not fixed here. Each names what fixing it costs.
# An exception is a declared mechanism, not an excuse — this is the same discipline
# as `script/circuit_free_instances.txt`, which requires the mechanism to be named.
EXCEPTIONS = {
    ("src/sdk/src/blockchain.rs", "mining-tokenomics.md"):
        "the reward-schedule rationale now lives in doc/src/arch/consensus-coinbase.md; "
        "this file is covered by EVERY contract's SOURCE_MANIFEST, so editing the comment "
        "invalidates all 32 artifacts and re-rolls the genesis pin — a cost a docs change "
        "should not impose, recorded instead",
    ("scripts/check-doc-index.sh", "mining-tokenomics.md"):
        "the exception text above names the citation it exempts; this entry exists so the "
        "gate does not report itself",
}

known_basenames = {
    os.path.basename(p)
    for p in glob.glob(f"{repo}/**/*.md", recursive=True)
    if not any(d in p for d in SKIP_DIRS)
}
for path in scan_files:
    rel = os.path.relpath(path, repo)
    text = slurp(path)
    for m in re.finditer(r'([A-Za-z0-9_./-]+\.md)\b', text):
        cited_path = m.group(1)
        if cited_path.startswith(("http", "//")):
            continue
        name = os.path.basename(cited_path)
        if name in ALLOWED or name in known_basenames or name in GENERATED:
            continue
        if (rel, name) in EXCEPTIONS:
            continue
        # only judge things that look like this repository's documents
        if "/" not in cited_path and not re.match(r'[a-z0-9-]+\.md$', cited_path):
            continue
        lineno = text[:m.start()].count("\n") + 1
        problems.append(f"{rel}:{lineno}: cites {cited_path}, which no longer exists")

if not problems:
    n = len(glob.glob(f"{docs}/**/*.md", recursive=True))
    print(f"PASS: doc index coherent — {n} documents, {len(defined_obl)} obligations, all cross-references resolve")
    if EXCEPTIONS:
        print(f"  ({len(EXCEPTIONS)} declared exception(s), each with its reason recorded:)")
        for (rel, name), why in sorted(EXCEPTIONS.items()):
            print(f"    {rel} cites {name} — {why}")
    sys.exit(0)

print(f"FAIL: {len(problems)} documentation index defect(s):")
for p in problems:
    print(f"  {p}")
print("")
print("Fix: update the index or the reference. Do not delete a document while")
print("something still cites it, and do not renumber without the alias table.")
sys.exit(1)
PYEOF
