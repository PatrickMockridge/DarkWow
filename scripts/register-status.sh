#!/bin/bash
# The obligation register's per-row status, counted rather than recited.
#
# Why this exists. Three times the register has been wrong about its own bookkeeping and nothing
# noticed: `OBL-C10`'s closure marker was written in a form no grep for "CLOSED" finds; the summary
# said "twenty-one" and "twenty-two" about the same list of twenty; and `OBL-C68`-`C71` had their
# fixes recorded only in a paragraph *below* the table, so the rows themselves read as open. Each was
# found by a human counting by hand. `check-doc-index.sh` checks that every cited `OBL-*` id
# *resolves*; it never reads a status. This prints the view that was missing.
#
# REPORT-ONLY, deliberately — the same call the OBL-Z18 detector made. Many rows still carry no
# status token, and a blocking gate that is red for a reason nobody disputes is a gate that gets
# ignored. Wire it into run-all-tests.sh once every row carries a marker.
#
# HOW IT COUNTED, AND WHY IT IS ONLY A HEURISTIC — the reason this is a report and not a gate.
# A first version of this script scanned only the cells *after* the first two, on the assumption that
# a row's status lives in its evidence cell. That was wrong, and it reported `OBL-C63`-`C71` as
# unmarked when `C63` carries "CLOSED 2026-09-22" inside its *proposition* cell and `C68`-`C71` carry
# markers in a trailing cell. The register has no stated convention for where a status goes: it
# appears in the proposition cell, the evidence cell, a trailing cell after the severity, and in some
# rows in the severity cell itself. So this version scans the **whole row** and accepts that a row
# may name another row's status in its own prose — that is a finding about the row, and it is
# reported rather than hidden. The real fix is a convention, not a parser.
#
# What it does not do: it does not decide whether a status is *true*. A row holding two tokens
# ("CLOSED for oracle", "still FAILS") is counted under both.
#
# Exit 0 always, unless the register cannot be read.
#
# Usage:
#   scripts/register-status.sh                            # count the register in this repo
#   REGISTER=/tmp/register-copy scripts/register-status.sh  # count another copy (the negative control)
#
# The override path is deliberately extensionless: `check-doc-index.sh` resolves every `*.md` string
# in the tree as a document citation, so an example ending in `.md` made the index gate report a
# deleted document that never existed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REGISTER_OVERRIDE="${REGISTER:-}" \
REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import os, re, sys, collections

repo = os.environ["REPO_ROOT"]
path = os.environ.get("REGISTER") or os.path.join(repo, "doc", "src", "arch", "verification-hazop.md")

try:
    with open(path, errors="replace") as fh:
        text = fh.read()
except OSError as e:
    print(f"FAIL: cannot read the register at {path}: {e}")
    sys.exit(1)

# The same row-id pattern check-doc-index.sh uses, so the two agree on what a row is.
ROW_RE = re.compile(r'^\|\s*(OBL-[CZT]\d+)\s*\|(.*)$', re.M)

# The vocabulary is the register's own, now stated in its "status vocabulary" note: a status is one
# of these words **in bold**. FIXED is accepted as a synonym of CLOSED (the finality rows use it);
# NEW and done are non-statuses that appear in the same position and are counted separately so they
# are visible rather than silently treated as statuses.
VOCAB = (
    ("CLOSED",      r'\bCLOSED\b'),
    ("FIXED",       r'\bFIXED\b'),
    ("SATISFIED",   r'\bSATISFIED\b'),
    ("RESTATED",    r'\bRESTATED\b'),
    ("ACCEPTED-WITH-REASON", r'\bACCEPTED-WITH-REASON\b'),
    ("OPEN",        r'\bOPEN\b'),
    ("FAILS",       r'\bFAILS\b'),
    ("PARTLY",      r'\bpartly\b'),
    ("PROVED",      r'\bproved\b'),
    ("DEFINITIONAL", r'\bdefinitional\b|\bdefinition\b'),
    ("MECHANIZED",  r'\bmechanized\b'),
    ("NEW",         r'\bNEW\b'),
    ("done",        r'\bdone\b'),
)

# Count only inside **bold** spans. A row's *prose* legitimately names other rows' statuses ("this
# closes as a consequence of OBL-C52's fix", "the concern folds into"), and an unbolded mention is a
# reference rather than this row's status. This narrowing is why the earlier whole-body scan
# mis-reported; it is stated here rather than left as a quiet heuristic.
#
# The span pattern must tolerate a single `*` inside, because these markers are full of *italics*;
# `\*\*([^*]+)\*\*` truncates the span at the first inner asterisk and silently drops any token that
# came after it — which undercounted SATISFIED and PROVED until it was caught by comparing the two
# rules against each other.
BOLD_RE = re.compile(r'\*\*((?:[^*]|\*(?!\*))+)\*\*')

rows = []
for m in ROW_RE.finditer(text):
    rid, body = m.group(1), m.group(2)
    bolded = " ".join(BOLD_RE.findall(body))
    found = [tok for tok, pat in VOCAB if re.search(pat, bolded)]
    rows.append((rid, found))

seen = [r[0] for r in rows]
dupes = sorted(i for i, c in collections.Counter(seen).items() if c > 1)

hist = collections.Counter()
unmarked, multi = [], []
for rid, found in rows:
    if not found:
        unmarked.append(rid)
        continue
    if len(found) > 1:
        multi.append((rid, found))
    for t in found:
        hist[t] += 1

print(f"register: {path}")
print(f"rows: {len(rows)}   unique ids: {len(set(seen))}"
      + (f"   DUPLICATE IDS: {', '.join(dupes)}" if dupes else ""))
print("")
print("rows per status token (a row with two tokens is counted under both):")
for token, n in sorted(hist.items(), key=lambda kv: (-kv[1], kv[0])):
    print(f"  {n:>3}  {token}")
print(f"  {len(unmarked):>3}  (no status token at all)")
print("")
print(f"rows carrying more than one token ({len(multi)}) — read these, the count may be double:")
for rid, found in multi:
    print(f"  {rid}: {', '.join(found)}")
print("")
print(f"rows with no status token ({len(unmarked)}):")
for i in range(0, len(unmarked), 12):
    print("  " + "  ".join(unmarked[i:i + 12]))

print("")
print("summary sentences the register makes about itself (compare against the counts above):")
for line in text.splitlines():
    s = line.strip()
    if s.startswith('|') or not s:
        continue
    if re.search(r'\b(satisfied|closed|are\b.*\brows)\b', s, re.I) and \
       re.search(r'\b(twenty|two|three|four|five|six|seven|eight|nine|ten|\d+)\b', s, re.I):
        print("  " + (s[:190] + "…" if len(s) > 190 else s))
PYEOF
