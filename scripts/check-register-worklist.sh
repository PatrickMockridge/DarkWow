#!/usr/bin/env bash
#
# The register's worklist, counted — and ratcheted.
#
# WHY THIS EXISTS. `scripts/register-status.sh --check` guards the *form* of each row's status: that it
# is a word from the vocabulary. It says nothing about what the statuses add up to, and that is the
# measurement that was missing. Measured 2026-10-08: the register's open set was **70 rows on
# 2026-09-29 and 70 rows on 2026-10-08** — nine days, fourteen rows minted, six closed, net +2, and
# the only movement in the last hour was eight rows closed by hand. A count that no command derives is
# a count nobody notices is not moving.
#
# WHAT IT COUNTS, and why the statuses had to change first. Until 2026-10-08 `OPEN` meant three
# different things — *we will do this*, *we are waiting for a decision*, and *we have decided not to* —
# so the number could not fall: a row blocked on a design choice cannot be closed by working it.
# The register now says which is which:
#
#   OPEN     work      — a named repair, nobody's decision but the doer's
#   PARTLY   work      — the same, with a piece already landed
#   FAILS    work      — the property is *currently violated*, so it is not merely undone
#   DECISION a choice  — blocked on a design or policy decision nobody has made; NOT a worklist entry
#   ACCEPTED-WITH-REASON, CLOSED, FIXED, SATISFIED, RESTATED, PROVED, DEFINITIONAL, MECHANIZED
#                      — no longer work
#
# THE RATCHET. `BUDGET` below is the worklist's declared ceiling. A commit that adds work without
# closing work fails the gate, and `--strict` is how the umbrella runs it. Lowering `BUDGET` is the
# point; it is lowered in the commit that closes rows, by the same idiom as every other declared list
# in this tree (`script/store_key_agreement_exceptions.txt`, `script/codec_size_derivation_exceptions.txt`).
# Raising it is a decision, and it should be argued in the commit message that raises it.
#
# WHAT IT DOES NOT ESTABLISH (R7). It counts rows, not defects: a row is not a bug, and the register's
# own closures this week included eight rows whose repair had landed days earlier. A falling count is
# evidence about the record, and only about the record.
#
# Usage: scripts/check-register-worklist.sh            (census; exit 0 even over budget)
#        scripts/check-register-worklist.sh --strict   (exit 1 when the worklist exceeds BUDGET)
#        scripts/check-register-worklist.sh --self-test (planted rows; requires a non-zero exit)
# Exit: 0 ok, 1 over budget under --strict, 2 --self-test failed or the register is unreadable.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
export REGISTER_WORKLIST_ROOT="$REPO_ROOT"

python3 - "$@" <<'PY'
import os, pathlib, re, sys

ROOT = pathlib.Path(os.environ["REGISTER_WORKLIST_ROOT"])
REGISTER = ROOT / "doc" / "src" / "arch" / "verification-hazop.md"

WORK = ("OPEN", "PARTLY", "FAILS")
ALL = ("OPEN", "PARTLY", "FAILS", "DECISION", "ACCEPTED-WITH-REASON", "CLOSED", "FIXED",
       "SATISFIED", "RESTATED", "PROVED", "DEFINITIONAL", "MECHANIZED")

# The declared ceiling on rows that are work. Lower it when you close rows; raise it only with a
# reason in the commit message. Introduced 2026-10-08 at 29 (15 OPEN + 10 PARTLY + 4 FAILS), and
# **raised to 30 the same day** when `OBL-C148` moved from `DECISION` to `PARTLY`: its line-citation
# resolver landed, which leaves the symbol half as work that did not exist as work before. That is the
# ratchet doing its job — reclassifying a blocked row into real work is a change to the worklist, and
# it costs a sentence rather than happening silently.
#
# **Lowered three times later the same day**, which is the direction the ratchet exists for:
# `OBL-C124` closed (the checker's summary now names each failure class it counts, guarded by a fourth
# assertion in that gate's `--self-test`), `OBL-C142` closed (the freshness verdict is content-based and
# its self-test plants the mtime regression), and `OBL-C135` closed (the verdict table keys every suite
# on its binary, so the whole-chunk tally is available and the rows stop collapsing). The ceiling comes
# down with each rather than leaving slack that a future row could occupy without anyone deciding to.
BUDGET = 26

ROW = re.compile(r'^\|\s*(OBL-[A-Za-z0-9]+)\s*\|\s*\*\*([A-Z-]+)\*\*')


def census(text):
    """{status: count} over every row whose Status cell opens with a *vocabulary* word.

    The vocabulary filter is load-bearing, not tidy: without it any bold-uppercase token counts, so a
    coined status — the thing `register-status.sh --check` exists to reject — would be counted as a
    real one here and would inflate the settled total while the worklist looked healthy. The
    `--self-test` plants exactly that row and requires it to be ignored.
    """
    out = {}
    for ln in text.splitlines():
        m = ROW.match(ln)
        if not m or m.group(2) not in ALL:
            continue
        out[m.group(2)] = out.get(m.group(2), 0) + 1
    return out


def worklist(counts):
    return sum(counts.get(s, 0) for s in WORK)


def report(counts, budget):
    total = sum(counts.get(s, 0) for s in ALL)
    wl = worklist(counts)
    print(f"COVERAGE: {total} rows with a status in the vocabulary.")
    print("  work      " + ", ".join(f"{s}={counts.get(s,0)}" for s in WORK) + f"   -> worklist {wl}")
    print(f"  decision  DECISION={counts.get('DECISION',0)}  (a choice nobody has made; not work)")
    done = total - wl - counts.get("DECISION", 0)
    print(f"  settled   {done} (closed / fixed / satisfied / restated / accepted / mechanized)")
    print(f"  budget    {budget}")
    if wl > budget:
        print(f"\n[FAIL] the worklist is {wl}, over its declared budget of {budget}.")
        print("  Close rows and lower BUDGET in the same commit, or raise it with a reason.")
        return 1
    print(f"\n[PASS] the worklist is {wl}, within its declared budget of {budget}.")
    return 0


def self_test():
    """Three controls: over budget fails, within budget passes, a non-vocabulary status is not counted."""
    over = "| ID | Status |\n|---|---|\n" + "".join(f"| OBL-S{i} | **OPEN** |\n" for i in range(5))
    under = "| ID | Status |\n|---|---|\n" + "| OBL-S0 | **OPEN** |\n"
    if worklist(census(over)) != 5:
        print("FAIL: --self-test could not count a plain worklist"); return 2
    if report(census(over), 2) == 0:
        print("FAIL: --self-test: a register over budget passed"); return 2
    if report(census(under), 2) != 0:
        print("FAIL: --self-test: a register within budget failed"); return 2
    if census("| OBL-X | **NOTAWORD** |\n"):
        print("FAIL: --self-test: a row with no vocabulary status was counted"); return 2
    print("OK: --self-test — over-budget fails, within-budget passes, and a non-vocabulary status is not counted")
    return 0


if "--self-test" in sys.argv:
    sys.exit(self_test())

if not REGISTER.exists():
    print(f"FAIL: {REGISTER} does not exist"); sys.exit(2)
counts = census(REGISTER.read_text(encoding="utf-8"))
if not counts:
    print("FAIL: no row carried a vocabulary status — the parse is wrong, not the register")
    sys.exit(2)
rc = report(counts, BUDGET)
sys.exit(rc if "--strict" in sys.argv else 0)
PY
