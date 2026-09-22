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
# A third limitation, found on `OBL-T10`: a row whose *code spans* contain `**` (it writes
# `proofs/lean/src/**/*.lean`) derails bold-span pairing for that whole row, so its real marker is
# missed and the row is reported as unmarked. The marker is still greppable by hand; only this
# report's span parser is defeated.
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
# Two vocabularies, because two classes of word. The STRONG set is technical — a word that in this
# register can only be a status — and it drives the headline count and the unmarked list. The WEAK set
# is ordinary English that a status happens to share: `nothing`, `false`, `new`, `open`, `done` all
# occur in propositions ("**Nothing static is disclosed**" is OBL-Z9's prose, not its status; "**the
# converse is false**" is OBL-T6's). Counting them alongside the strong ones produced ten NOTHINGs and
# twenty-one multi-token rows, which is noise masquerading as structure. They are still reported —
# a row whose only marker is a weak word is *not* silently unmarked — but separately, so the reader
# can tell a measurement from an accident.
VOCAB = (
    ("CLOSED",      r'closed'),
    ("FIXED",       r'fixed'),
    ("SATISFIED",   r'satisfied'),
    ("RESTATED",    r'restated'),
    ("ACCEPTED-WITH-REASON", r'accepted-with-reason'),
    ("FAILS",       r'fails'),
    ("PARTLY",      r'partly'),
    ("PROVED",      r'proved'),
    ("DEFINITIONAL", r'definitional'),
    ("MECHANIZED",  r'mechanized'),
    ("OPEN",        r'open'),
)
WEAK = (
    ("FALSE",       r'false'),
    ("NOTHING",     r'nothing'),
    ("NEW",         r'new'),
    ("done",        r'done'),
)

# A token counts anywhere inside a bold span, case-insensitively. Both halves of that were decided by
# the report disagreeing with the register, and the residue is named rather than hidden:
#
#  * case: the register writes lowercase ("partly", "proved") while every marker added on 2026-09-22 is
#    uppercase ("PARTLY", "DEFINITIONAL"), so a case-sensitive pattern counted the new ones as unmarked.
#  * position: anchoring the token to the *start* of a span was tried and reverted. It removed
#    incidental prose ("a **new template**") but also removed real markers that name their status
#    mid-span — `OBL-Z12`'s "the syntactic half is now **mechanized**" — and the unmarked list is this
#    report's primary output, so missing a marked row is the worse error.
COMPILED = tuple((name, re.compile(r'\b(?:' + pat + r')\b', re.I)) for name, pat in VOCAB)
COMPILED_WEAK = tuple((name, re.compile(r'\b(?:' + pat + r')\b', re.I)) for name, pat in WEAK)

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
    found = [tok for tok, pat in COMPILED if pat.search(bolded)]
    weak = [tok for tok, pat in COMPILED_WEAK if pat.search(bolded)]
    rows.append((rid, found, weak))

seen = [r[0] for r in rows]
dupes = sorted(i for i, c in collections.Counter(seen).items() if c > 1)

hist = collections.Counter()
unmarked, multi = [], []
weak_only = []
for rid, found, weak in rows:
    if not found:
        unmarked.append(rid)
        if weak:
            weak_only.append((rid, weak))
        continue
    if len(found) > 1:
        multi.append((rid, found))
    for t in found:
        hist[t] += 1

print(f"register: {path}")
print(f"rows: {len(rows)}   unique ids: {len(set(seen))}"
      + (f"   DUPLICATE IDS: {', '.join(dupes)}" if dupes else ""))
print("")
print("rows per status token — UNRELIABLE, read the caveat below before quoting any number:")
for token, n in sorted(hist.items(), key=lambda kv: (-kv[1], kv[0])):
    print(f"  {n:>3}  {token}")
print(f"  {len(unmarked):>3}  (no status token at all)")
print("")
print(f"rows carrying more than one token ({len(multi)}) — read these, the count may be double:")
for rid, found in multi:
    print(f"  {rid}: {', '.join(found)}")
print("")
print(f"rows whose only marker is a weak word — ordinary English, may be prose not status ({len(weak_only)}):")
for rid, weak in weak_only:
    print(f"  {rid}: {', '.join(weak)}")
print("")
print(f"rows with no status token ({len(unmarked)}):")
for i in range(0, len(unmarked), 12):
    print("  " + "  ".join(unmarked[i:i + 12]))

print("")
print("WHY THE COUNTS ABOVE ARE NOT EVIDENCE. They are word occurrences inside bold spans, and this")
print("register's prose contains every one of these words in non-status senses. Three real examples from")
print("this file, each of which the counts get wrong: \"**partly closed.**\" is PARTLY but counts as both")
print("PARTLY and CLOSED; a marker reading \"bounded rather than **closed**\" counts as CLOSED while saying")
print("the opposite; \"**ACCEPTED-WITH-REASON 2026-09-22, premise restated.**\" counts as RESTATED as well")
print("as its real token. Two vocabularies were tried and neither fixes this — anchoring to the span's")
print("start drops real mid-span markers (OBL-Z12), and matching anywhere admits the negations above.")
print("What IS reliable, and what this script is for, is the list of rows with **no** status marker: a row")
print("absent from that list has one, and that is a fact about the file rather than about prose. The real")
print("repair is a convention — a dedicated status column, or a generated table — not a better regex.")
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
