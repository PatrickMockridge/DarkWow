#!/bin/bash
# The obligation register's per-row status: counted, reported, and checked.
#
# Why this exists. Four times the register has been wrong about its own bookkeeping and nothing
# noticed: `OBL-C10`'s closure marker was written in a form no grep for "CLOSED" finds; the summary
# said "twenty-one" and "twenty-two" about the same list of twenty; `OBL-C68`-`C71` had their fixes
# recorded only in a paragraph *below* the table, so the rows read as open; and on 2026-09-22 three
# markers were written with words the register does not use ("STALE", "CONFIRMED") or with no status
# word at all. Each was found by a human reading a diff. `check-doc-index.sh` checks that every cited
# `OBL-*` id *resolves*; it never reads a status.
#
# Two modes:
#   (default)   report — counts, the unmarked list, the weak-token list, the register's own summary
#               sentences. Always exits 0 unless the register is unreadable.
#   --check     gate — exits 1 if any row's *last cell* opens with an ALL-CAPS word that is not a
#               status in the vocabulary below, i.e. a coined marker no count can find. This is the
#               mechanical guard for the class that cost three fixes on 2026-09-22.
#
# HOW IT COUNTS, AND WHY THE COUNTS ARE ONLY A REPORT. A token counts anywhere inside a bold span,
# case-insensitively:
#   * case — the register writes lowercase ("partly", "proved") while the markers added on 2026-09-22
#     are uppercase; a case-sensitive pattern silently counted the new ones as unmarked.
#   * position — anchoring to the start of a span was tried and reverted: it dropped real mid-span
#     markers (`OBL-Z12`'s "the syntactic half is now **mechanized**") and the unmarked list is the
#     primary output, so missing a marked row is the worse error.
# The cost of matching anywhere is that prose counts: `OBL-Z9` reads "**Nothing static is disclosed**"
# and is counted under NOTHING as well as its real CLOSED. That is why rows with more than one token
# are printed for a human, and why per-token counts over this file's prose are not evidence — only the
# list of rows with *no* marker is. A third limitation, same theme: a row whose code spans contain
# `**` (`OBL-T10` writes `proofs/lean/src/**/*.lean`) derails span pairing for the whole row, so its
# real marker is missed. The marker is still greppable by hand; only this parser is defeated.
#
# Exit 0: report mode, or check mode with no coined marker.
# Exit 1: check mode found a coined marker; or the register cannot be read.
#
# Usage:
#   scripts/register-status.sh [--check]
#   REGISTER=path/to/copy scripts/register-status.sh [--check]   # the negative control

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CHECK=0
for a in "$@"; do [ "$a" = "--check" ] && CHECK=1; done

CHECK="$CHECK" REGISTER_OVERRIDE="${REGISTER:-}" REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import os, re, sys, collections

repo = os.environ["REPO_ROOT"]
check_mode = os.environ.get("CHECK") == "1"
path = os.environ.get("REGISTER") or os.path.join(repo, "doc", "src", "arch", "verification-hazop.md")

try:
    with open(path, errors="replace") as fh:
        text = fh.read()
except OSError as e:
    print(f"FAIL: cannot read the register at {path}: {e}")
    sys.exit(1)

# The same row-id pattern check-doc-index.sh uses, so the two agree on what a row is.
ROW_RE = re.compile(r'^\|\s*(OBL-[CZT]\d+)\s*\|(.*)$', re.M)

# The vocabulary is the register's own, stated in its "status vocabulary" note. STRONG words can only
# be statuses and drive the count; WEAK ones are ordinary English a status happens to share —
# `nothing`, `false`, `new`, `done` all occur in prose ("**Nothing static is disclosed**" is OBL-Z9's
# prose, not its status) — so they are reported separately and a row whose only marker is one of them
# is still surfaced rather than hidden.
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
COMPILED = tuple((n, re.compile(r'\b(?:' + p + r')\b', re.I)) for n, p in VOCAB)
COMPILED_WEAK = tuple((n, re.compile(r'\b(?:' + p + r')\b', re.I)) for n, p in WEAK)

# Bold spans must tolerate a single `*` inside, because these markers are full of *italics*;
# `\*\*([^*]+)\*\*` truncates at the first inner asterisk and silently drops tokens after it.
BOLD_RE = re.compile(r'\*\*((?:[^*]|\*(?!\*))+)\*\*')

rows = []
for m in ROW_RE.finditer(text):
    rid, body = m.group(1), m.group(2)
    bolded = " ".join(BOLD_RE.findall(body))
    found = [tok for tok, pat in COMPILED if pat.search(bolded)]
    weak = [tok for tok, pat in COMPILED_WEAK if pat.search(bolded)]
    rows.append((rid, body, found, weak))

seen = [r[0] for r in rows]
dupes = sorted(i for i, c in collections.Counter(seen).items() if c > 1)

hist = collections.Counter()
unmarked, weak_only, multi = [], [], []
for rid, _body, found, weak in rows:
    if not found:
        unmarked.append(rid)
        if weak:
            weak_only.append((rid, weak))
        continue
    if len(found) > 1:
        multi.append((rid, found))
    for t in found:
        hist[t] += 1

# --- the coined-marker check ---------------------------------------------------------------------
# Narrow by construction, because a rule that cries wolf gets switched off: only the row's LAST cell
# (where an appended marker lands), only a bold span that OPENS that cell, and only an ALL-CAPS first
# word. A row whose marker lives inside the proposition cell — `OBL-C63` is one — is not examined at
# all. This guards the convention; it does not replace reading.
COINED_RE = re.compile(r'^\s*\*\*([A-Z][A-Z-]{1,})')
KNOWN = {n for n, _ in VOCAB} | {n for n, _ in WEAK}
coined = []
for rid, body, _f, _w in rows:
    cells = body.split('|')
    last = cells[-2] if cells and cells[-1].strip() == '' else (cells[-1] if cells else '')
    hit = COINED_RE.match(last)
    if hit and hit.group(1) not in KNOWN:
        coined.append((rid, hit.group(1)))

if check_mode:
    if coined:
        print(f"FAIL: {len(coined)} row(s) open their last cell with a word that is not a status:")
        for rid, word in coined:
            print(f"  {rid}: {word}")
        print("A marker no count can find is a marker no audit can use. Use the register's own word, or")
        print("add the new one to its status-vocabulary note.")
        sys.exit(1)
    print(f"OK: every row whose last cell opens with an ALL-CAPS word uses a status from the vocabulary "
          f"({len(rows)} rows walked).")
    sys.exit(0)

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
print("register's prose contains every one of these words in non-status senses. Three real examples")
print("from this file, each of which the counts get wrong: \"**partly closed.**\" is PARTLY but counts")
print("as both PARTLY and CLOSED; a marker reading \"bounded rather than **closed**\" counts as CLOSED")
print("while saying the opposite; \"**ACCEPTED-WITH-REASON 2026-09-22, premise restated.**\" counts as")
print("RESTATED as well as its real token. Two vocabularies were tried and neither fixes this.")
print("What IS reliable, and what this script is for, is the list of rows with **no** status marker.")
print("The real repair is a convention — a status column, or a generated table — not a better regex.")
if coined:
    print("")
    print(f"COINED MARKERS ({len(coined)}) — `--check` fails on these:")
    for rid, word in coined:
        print(f"  {rid}: {word}")
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
