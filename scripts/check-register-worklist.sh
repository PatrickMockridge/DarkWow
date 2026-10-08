#!/usr/bin/env bash
#
# The register's worklist, counted — and assigned to its root causes.
#
# WHY THIS EXISTS. `scripts/register-status.sh --check` guards the *form* of each row's status: that it
# is a word from the vocabulary. It says nothing about whether the open set is *understood*, and that is
# the property a worklist needs. The register holds 258 rows; 57 are not settled (26 work, 31
# `DECISION`). This gate requires each of those 57 to name its root cause, in the one block
# `<!-- root-cause-groups -->` in `doc/src/arch/verification-hazop.md`.
#
# **WHAT IT REPLACED, AND WHY.** Until 2026-10-08 this gate was a budget ratchet: it failed when
# `OPEN + PARTLY + FAILS` exceeded a hard-coded `BUDGET`. Measured over the sixty commits of
# 2026-10-06..08, that was the wrong instrument. The worklist sat exactly at its ceiling, so any
# net-new finding forced closing a row in the same commit — and the cheapest row to close is always a
# record or tooling row. Thirty-six of those sixty commits touched this register and only ~12 were
# product-source edits. A budget measures *how many* rows are open; it cannot tell a row being worked
# from one being ignored, and it rewards the cheapest close. The gate now measures whether each open row
# has been *reduced to a cause* — the HAZOP step, which a new finding cannot satisfy by closing another.
#
# THE ASSIGNMENT. Every row whose status is `OPEN`, `PARTLY`, `FAILS` or `DECISION` appears in exactly
# one group in the root-cause block. The gate fails: a row in none, a row named twice, a member that is
# not a row at all, and a member whose status is not an open one. You cannot lower it by closing rows;
# the only moves are finding a row's cause or correcting a stale member.
#
# WHAT IT DOES NOT ESTABLISH (R7). It counts rows, not defects, and it checks *that* a row names a cause,
# never that the cause is the right one — a row filed under the wrong cause passes. The reduction itself
# is reviewed text in the register; this gate keeps it complete, not correct.
#
# Usage: scripts/check-register-worklist.sh            (census + problems; exit 0)
#        scripts/check-register-worklist.sh --strict   (exit 1 when an open row names no cause)
#        scripts/check-register-worklist.sh --self-test (planted registers; requires the right exits)
# Exit: 0 ok, 1 an unassigned/duplicate/phantom/settled member under --strict, 2 --self-test failed or
#       the register (or its root-cause block) is unreadable or absent.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
export REGISTER_WORKLIST_ROOT="$REPO_ROOT"

python3 - "$@" <<'PY'
import contextlib, io, os, pathlib, re, sys

ROOT = pathlib.Path(os.environ["REGISTER_WORKLIST_ROOT"])
REGISTER = ROOT / "doc" / "src" / "arch" / "verification-hazop.md"

WORK = ("OPEN", "PARTLY", "FAILS")
ASSIGNED = ("OPEN", "PARTLY", "FAILS", "DECISION")
ALL = ("OPEN", "PARTLY", "FAILS", "DECISION", "ACCEPTED-WITH-REASON", "CLOSED", "FIXED",
       "SATISFIED", "RESTATED", "PROVED", "DEFINITIONAL", "MECHANIZED")

ROW = re.compile(r'^\|\s*(OBL-[A-Za-z0-9]+)\s*\|\s*\*\*([A-Z-]+)\*\*')
BLOCK = re.compile(r'<!-- root-cause-groups -->(.*?)<!-- /root-cause-groups -->', re.S)
OBL = re.compile(r'OBL-[A-Za-z0-9]+')


def rows(text):
    """{id: status} over every row whose Status cell opens with a *vocabulary* word.

    The vocabulary filter is load-bearing, not tidy: without it any bold-uppercase token counts, so a
    coined status — the thing `register-status.sh --check` exists to reject — would be counted as a real
    one and would hide an unassigned row behind a settled-looking one.
    """
    out = {}
    for ln in text.splitlines():
        m = ROW.match(ln)
        if m and m.group(2) in ALL:
            out[m.group(1)] = m.group(2)
    return out


def groups(text):
    """[(label, [id, ...]), ...] from the root-cause block; None if the block is absent."""
    m = BLOCK.search(text)
    if not m:
        return None
    out = []
    for ln in m.group(1).splitlines():
        ln = ln.strip()
        if not ln:
            continue
        label = ln.split("·", 1)[0].strip()
        ids = OBL.findall(ln)
        if not label or not ids:
            print(f"FAIL: malformed root-cause line (need `<label> · <text>: OBL-…`): {ln!r}")
            return []
        out.append((label, ids))
    return out


def census(rs):
    out = {}
    for s in rs.values():
        out[s] = out.get(s, 0) + 1
    return out


def assignment_problems(rs, gs):
    openish = {i for i, s in rs.items() if s in ASSIGNED}
    seen, problems = {}, []
    for label, ids in gs:
        for i in ids:
            if i in seen:
                problems.append(f"{i} is named twice ({seen[i]} and {label})")
            seen[i] = label
            if i not in rs:
                problems.append(f"{label} names {i}, which is not a row")
            elif rs[i] not in ASSIGNED:
                problems.append(f"{label} names {i}, whose status is {rs[i]} — not an open row")
    for i in sorted(openish):
        if i not in seen:
            problems.append(f"{i} ({rs[i]}) names no root cause")
    return problems


def report(rs, gs):
    c = census(rs)
    wl = sum(c.get(s, 0) for s in WORK)
    total = sum(c.get(s, 0) for s in ALL)
    print(f"COVERAGE: {total} rows with a status in the vocabulary.")
    print("  work      " + ", ".join(f"{s}={c.get(s,0)}" for s in WORK) + f"   -> worklist {wl}")
    print(f"  decision  DECISION={c.get('DECISION',0)}")
    print(f"  settled   {total - wl - c.get('DECISION',0)}")
    if gs is None:
        print("\n[FAIL] the root-cause block is missing from the register.")
        print("  Expected `<!-- root-cause-groups --> … <!-- /root-cause-groups -->`.")
        return 1
    print(f"  causes    {len(gs)} group(s): " + ", ".join(f"{label}={len(ids)}" for label, ids in gs))
    problems = assignment_problems(rs, gs)
    if not problems:
        n = sum(1 for s in rs.values() if s in ASSIGNED)
        print(f"\n[PASS] every one of the {n} open rows names a root cause.")
        return 0
    print(f"\n[FAIL] {len(problems)} problem(s) in the root-cause assignment:")
    for p in problems:
        print(f"  - {p}")
    print("  Fix: add the row to the cause that produces it, or remove the stale member.")
    return 1


def _quiet_report(rs, gs):
    with contextlib.redirect_stdout(io.StringIO()):
        return report(rs, gs)


def self_test():
    """Six controls. The first four are the assignment's failure modes; the fifth is the vocabulary
    filter; the sixth is the positive case, without which four negatives prove only that it fails."""
    def reg(body, block):
        return ("| ID | Status |\n|---|---|\n" + body + "\n"
                "<!-- root-cause-groups -->\n" + block + "\n<!-- /root-cause-groups -->\n")

    if _quiet_report(rows(reg("| OBL-C1 | **OPEN** |\n| OBL-C2 | **CLOSED** |\n| OBL-C3 | **DECISION** |\n",
                              "RC1 · x: OBL-C1 OBL-C3")),
                     groups(reg("| OBL-C1 | **OPEN** |\n| OBL-C2 | **CLOSED** |\n| OBL-C3 | **DECISION** |\n",
                                "RC1 · x: OBL-C1 OBL-C3"))) != 0:
        print("FAIL: --self-test: a complete assignment was rejected"); return 2

    if _quiet_report(rows(reg("| OBL-C1 | **OPEN** |\n| OBL-C2 | **OPEN** |\n", "RC1 · x: OBL-C1")),
                     groups(reg("| OBL-C1 | **OPEN** |\n| OBL-C2 | **OPEN** |\n", "RC1 · x: OBL-C1"))) == 0:
        print("FAIL: --self-test: an open row in no group passed"); return 2

    if _quiet_report(rows(reg("| OBL-C1 | **OPEN** |\n", "RC1 · x: OBL-C1 OBL-C9")),
                     groups(reg("| OBL-C1 | **OPEN** |\n", "RC1 · x: OBL-C1 OBL-C9"))) == 0:
        print("FAIL: --self-test: a group naming a non-row passed"); return 2

    if _quiet_report(rows(reg("| OBL-C1 | **OPEN** |\n", "RC1 · x: OBL-C1\nRC2 · y: OBL-C1")),
                     groups(reg("| OBL-C1 | **OPEN** |\n", "RC1 · x: OBL-C1\nRC2 · y: OBL-C1"))) == 0:
        print("FAIL: --self-test: a row named in two groups passed"); return 2

    if _quiet_report(rows(reg("| OBL-C1 | **OPEN** |\n| OBL-C2 | **CLOSED** |\n", "RC1 · x: OBL-C1 OBL-C2")),
                     groups(reg("| OBL-C1 | **OPEN** |\n| OBL-C2 | **CLOSED** |\n", "RC1 · x: OBL-C1 OBL-C2"))) == 0:
        print("FAIL: --self-test: a settled row listed as a member passed"); return 2

    if rows("| OBL-X | **NOTAWORD** |\n"):
        print("FAIL: --self-test: a row with no vocabulary status was counted"); return 2

    print("OK: --self-test — an unassigned open row, a phantom member, a doubly-named row and a settled "
          "member each fail; a complete assignment passes; a coined status is not counted")
    return 0


if "--self-test" in sys.argv:
    sys.exit(self_test())

if not REGISTER.exists():
    print(f"FAIL: {REGISTER} does not exist"); sys.exit(2)
text = REGISTER.read_text(encoding="utf-8")
rs = rows(text)
if not rs:
    print("FAIL: no row carried a vocabulary status — the parse is wrong, not the register")
    sys.exit(2)
rc = report(rs, groups(text))
sys.exit(rc if "--strict" in sys.argv else 0)
PY
