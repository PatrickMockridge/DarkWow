#!/usr/bin/env bash
#
# The launch risk register's integrity gate.
#
# WHY THIS EXISTS. `doc/src/arch/launch-risk-register.md` is the disposition view over the obligation
# register: which risks block public testnet, which block mainnet, which are accepted, and what closes
# each. A view like that drifts in two directions, and both have already happened in this tree.
#
#   * It becomes a second obligation ledger. A row says "C146 is OPEN" or "C237 is FIXED", the
#     obligation register moves, and the two disagree. The launch register therefore stores no status:
#     `Disposition` is a launch decision, not an obligation status. This gate reads the cited rows'
#     status cells from `verification-hazop.md` at run time, and it rejects a row that restates one.
#   * It silently stops covering the register. A new severity-C or H obligation appears, is not
#     ranked, and a green launch register says nothing about it. The coverage check fails until every
#     open C/H obligation is either ranked or excluded with a reason.
#
# WHAT IT CHECKS (default mode). The ranked table's schema and vocabulary; unique ranks and ids;
# non-empty scenarios, evidence and exit criteria; that every cited `OBL-*` resolves; that an
# `Action=FIX` row cites at least one unsettled obligation; that `RETIRED` rows cite only settled
# obligations and carry a commit or a dated measurement; that `ACCEPTED` rows have a decision of
# record naming the row and its obligations; that every decision id resolves and is dated; that every
# unsettled severity-C/H obligation is ranked or excluded; and that no row restates an obligation status.
#
# WHAT IT DOES NOT CHECK (R7). Whether a risk is real, whether its rank or severity is right, whether
# an exit criterion is sufficient or achievable, whether an `ACCEPTED` decision was made by the right
# authority, or whether the cited obligation's status is current. It checks form, citation and
# coverage — the properties that can be mechanical. The reading is the reviewer's.
#
# THREE MODES.
#   (default)             integrity. Green means the register is honest, not that the launch is clear.
#   --self-test           negative control: planted defects must fail and be named; a clean fixture
#                         must pass. Every planted id is assembled at run time so `check-doc-index.sh`,
#                         which scans this file for `OBL-*` citations, does not read one as real.
#   --require-clear STAGE release checklist. STAGE is TESTNET or MAINNET. Fails while any row for that
#                         stage is `BLOCKS` or `CONDITIONAL`. MAINNET treats every row as blocking,
#                         because mainnet is downstream of the public testnet. Deliberately NOT wired
#                         into `run-all-tests.sh`: a register that records a blocker must not make the
#                         ordinary test run permanently red.
#
# Exit 0: the register is coherent (or, in --require-clear, the stage is clear). Exit 1: at least one
# defect, printed as `FAIL: <what>`. Exit 2: usage error or an unreadable input.
#
# Usage:
#   scripts/check-launch-register.sh
#   scripts/check-launch-register.sh --self-test
#   scripts/check-launch-register.sh --require-clear TESTNET

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REGISTER="$REPO_ROOT/doc/src/arch/launch-risk-register.md"
OBLIGATIONS="$REPO_ROOT/doc/src/arch/verification-hazop.md"

run_check() {
  LAUNCH_REGISTER="$1" OBL_REGISTER="$2" REQUIRE_CLEAR="${3:-}" python3 - <<'PYEOF'
import os, re, sys

launch_path = os.environ["LAUNCH_REGISTER"]
obl_path = os.environ["OBL_REGISTER"]
require = os.environ.get("REQUIRE_CLEAR", "").strip()

OPEN_ISH = {"OPEN", "PARTLY", "FAILS", "DECISION"}
STATUS_TOKENS = re.compile(
    r"\b(OPEN|PARTLY|FAILS|DECISION|CLOSED|FIXED|SATISFIED|RESTATED|"
    r"ACCEPTED-WITH-REASON|PROVED|DEFINITIONAL|MECHANIZED)\b"
)
OBL_ID = re.compile(r"OBL-[CZT]\d+")
LR_ID = re.compile(r"LR-\d+")
LR_D_ID = re.compile(r"LR-D\d+")
DATE = re.compile(r"\d{4}-\d{2}-\d{2}")
COMMIT_OR_DATE = re.compile(r"\b(?:[0-9a-f]{7,40}|measured\s+\d{4}-\d{2}-\d{2})\b")

problems = []


def fail(msg):
    problems.append(msg)


def cells_of(line):
    return [c.strip() for c in line.strip().strip("|").split("|")]


# ---------------------------------------------------------------- obligation register
try:
    obl_lines = open(obl_path, errors="replace").read().splitlines()
except OSError as e:
    print(f"FAIL: cannot read the obligation register at {obl_path}: {e}")
    sys.exit(2)

obl = {}
status_idx = sev_idx = None
for line in obl_lines:
    if re.match(r"^\|\s*ID\s*\|", line):
        head = cells_of(line)
        status_idx = head.index("Status") if "Status" in head else None
        sev_idx = head.index("Sev") if "Sev" in head else None
        continue
    m = re.match(r"^\|\s*(OBL-[CZT]\d+)\s*\|", line)
    if not m:
        continue
    cells = cells_of(line)
    if status_idx is None or sev_idx is None:
        continue
    if len(cells) <= max(status_idx, sev_idx):
        continue
    status = re.sub(r"[*\s]", "", cells[status_idx]).split(",")[0].upper()
    sev = re.sub(r"[*\s]", "", cells[sev_idx]).upper()
    # An unescaped pipe inside a code span can shift the cells. Fall back to any later cell that is
    # exactly a severity token; the status is always the second cell, which no shift reaches.
    if sev not in {"C", "H", "M", "L"}:
        for c in cells[sev_idx + 1:]:
            if re.sub(r"[*\s]", "", c).upper() in {"C", "H", "M", "L"}:
                sev = re.sub(r"[*\s]", "", c).upper()
                break
    obl[m.group(1)] = (status, sev)

if not obl:
    print(f"FAIL: no obligation rows parsed from {obl_path}")
    sys.exit(2)

# ---------------------------------------------------------------- launch register
try:
    text = open(launch_path, errors="replace").read()
except OSError as e:
    print(f"FAIL: cannot read the launch register at {launch_path}: {e}")
    sys.exit(2)

block = re.search(r"<!-- launch-risk-rows -->(.*?)<!-- /launch-risk-rows -->", text, re.S)
if not block:
    print("FAIL: the launch-risk-rows marker block is missing")
    sys.exit(1)

rows = []
header = None
for line in block.group(1).splitlines():
    if not line.startswith("|"):
        continue
    if re.match(r"^\|\s*Rank\s*\|", line):
        header = cells_of(line)
        continue
    if re.match(r"^\|[\s:|-]+\|?\s*$", line):
        continue
    if header is None:
        fail("a row appears before the table header")
        continue
    cells = cells_of(line)
    if len(cells) != len(header):
        fail(
            f"row {line[:50]!r} has {len(cells)} cells but the header has {len(header)} "
            "(an unescaped '|' inside a cell splits a GFM row)"
        )
        continue
    rows.append(dict(zip(header, cells)))

required = [
    "Rank", "LR", "Stage", "Class", "Sev", "Failure scenario",
    "Evidence", "Disposition", "Action", "Exit criterion",
]
if header is None:
    fail("the ranked table header is missing")
else:
    missing = [c for c in required if c not in header]
    if missing:
        fail(f"the ranked table is missing column(s): {', '.join(missing)}")
    if "Status" in header:
        fail("the ranked table has a Status column; obligation status belongs to verification-hazop.md")

stages = {"TESTNET", "MAINNET", "BOTH"}
classes = {"DEFECT", "ASSURANCE", "OPERATIONS", "ECONOMICS", "DECISION"}
sevs = {"C", "H", "M", "L"}
dispositions = {"BLOCKS", "CONDITIONAL", "ACCEPTED", "RETIRED", "WATCH"}
actions = {"FIX", "DECIDE", "MEASURE", "SEQUENCE", "NONE"}

seen_ids = set()
seen_ranks = set()
ranked_obl = set()
by_lr = {}

for row in rows:
    rid = row.get("LR", "")
    rank = row.get("Rank", "")
    if not re.fullmatch(r"\d+", rank):
        fail(f"{rid or '<no LR>'}: Rank {rank!r} is not an integer")
    elif int(rank) in seen_ranks:
        fail(f"duplicate Rank {rank}")
    else:
        seen_ranks.add(int(rank))
    if not LR_ID.fullmatch(rid):
        fail(f"row with Rank {rank}: LR id {rid!r} is not of the form LR-<n>")
    elif rid in seen_ids:
        fail(f"duplicate LR id {rid}")
    else:
        seen_ids.add(rid)
        by_lr[rid] = row

    stage = row.get("Stage", "")
    if stage not in stages:
        fail(f"{rid}: invalid Stage {stage!r} (expected TESTNET, MAINNET or BOTH)")
    cls = row.get("Class", "")
    if cls not in classes:
        fail(f"{rid}: invalid Class {cls!r}")
    sev = row.get("Sev", "")
    if sev not in sevs:
        fail(f"{rid}: invalid Sev {sev!r} (expected C, H, M or L)")
    disp = row.get("Disposition", "")
    if disp not in dispositions:
        fail(f"{rid}: invalid Disposition {disp!r}")
    action = row.get("Action", "")
    if action not in actions:
        fail(f"{rid}: invalid Action {action!r}")

    for col in ("Failure scenario", "Evidence", "Exit criterion"):
        if not row.get(col, "").strip():
            fail(f"{rid}: empty {col}")
    if not re.search(r"`[^`]+`|LR-D\d+", row.get("Exit criterion", "")):
        fail(f"{rid}: Exit criterion contains neither a code span nor an LR-D decision")

    row_text = " ".join(row.values())
    hit = STATUS_TOKENS.search(row_text)
    if hit:
        fail(f"{rid}: restates the obligation status {hit.group(0)!r}; cite the row, do not copy its status")

    cited = sorted(set(OBL_ID.findall(row_text)))
    ranked_obl.update(cited)
    for oid in cited:
        if oid not in obl:
            fail(f"{rid}: cites {oid}, which is not a row in the obligation register")

    if action == "FIX":
        if not any(obl.get(oid, ("", ""))[0] in OPEN_ISH for oid in cited):
            fail(f"{rid}: Action=FIX but no cited obligation is unsettled; the row is stale")
    if disp == "RETIRED":
        if not cited:
            fail(f"{rid}: RETIRED without a cited obligation")
        open_cited = [oid for oid in cited if obl.get(oid, ("", ""))[0] in OPEN_ISH]
        if open_cited:
            fail(f"{rid}: RETIRED but cites unsettled obligation(s): {', '.join(open_cited)}")
        if not COMMIT_OR_DATE.search(row_text):
            fail(f"{rid}: RETIRED without a commit hash or a 'measured YYYY-MM-DD' run")

# ---------------------------------------------------------------- decisions of record
decisions = {}
dec_header = None
for line in text.splitlines():
    if not line.startswith("|"):
        continue
    if re.match(r"^\|\s*ID\s*\|\s*Date\s*\|", line):
        dec_header = cells_of(line)
        continue
    if dec_header is None:
        continue
    if re.match(r"^\|[\s:|-]+\|?\s*$", line):
        continue
    cells = cells_of(line)
    if len(cells) != len(dec_header):
        continue
    row = dict(zip(dec_header, cells))
    did = row.get("ID", "")
    if not re.fullmatch(r"LR-D\d+", did):
        continue
    if did in decisions:
        fail(f"duplicate decision id {did}")
    decisions[did] = row
    if not DATE.fullmatch(row.get("Date", "")):
        fail(f"{did}: Date {row.get('Date','')!r} is not YYYY-MM-DD")
    for col in ("Decision", "Rationale", "Scope"):
        if not row.get(col, "").strip():
            fail(f"{did}: empty {col}")
    if not LR_ID.search(row.get("Scope", "")):
        fail(f"{did}: Scope names no LR row")

for rid, row in by_lr.items():
    if row.get("Disposition") == "ACCEPTED":
        cited = sorted(set(OBL_ID.findall(" ".join(row.values()))))
        matching = [
            d for d, dr in decisions.items()
            if rid in dr.get("Scope", "") and all(oid in (dr.get("Scope", "") + " " + dr.get("Decision", "")) for oid in cited)
        ]
        if not matching:
            fail(f"{rid}: ACCEPTED without a decision of record naming {rid} and its cited obligations")

# ---------------------------------------------------------------- coverage and exclusions
excl_block = re.search(r"<!-- launch-risk-exclusions -->(.*?)<!-- /launch-risk-exclusions -->", text, re.S)
excluded = set()
if excl_block:
    for line in excl_block.group(1).splitlines():
        if line.strip().startswith("<!--") and "none" in line.lower():
            continue
        for oid in OBL_ID.findall(line):
            excluded.add(oid)
            if oid not in obl:
                fail(f"exclusion names {oid}, which is not a row in the obligation register")
                continue
            status, sev = obl[oid]
            if sev not in {"C", "H"}:
                fail(f"exclusion names {oid} at severity {sev}; only C/H obligations may be excluded")
            if status not in OPEN_ISH:
                fail(f"exclusion names {oid}, which is settled; remove the stale exclusion")
            if oid in ranked_obl:
                fail(f"exclusion names {oid}, which is also ranked")
            if "none" not in line.lower() and not line.strip().startswith("<!--"):
                fail(f"exclusion {oid} has no stated reason")

for oid, (status, sev) in sorted(obl.items()):
    if sev in {"C", "H"} and status in OPEN_ISH and oid not in ranked_obl and oid not in excluded:
        fail(f"{oid} is an unsettled severity-{sev} obligation and is neither ranked nor excluded")

# ---------------------------------------------------------------- report
if problems:
    for p in problems:
        print(f"FAIL: {p}")
    sys.exit(1)

# ---------------------------------------------------------------- require-clear
if require:
    if require not in {"TESTNET", "MAINNET"}:
        print(f"FAIL: --require-clear expects TESTNET or MAINNET, got {require!r}")
        sys.exit(2)
    offending = []
    for rid, row in by_lr.items():
        disp = row.get("Disposition", "")
        if disp not in {"BLOCKS", "CONDITIONAL"}:
            continue
        stage = row.get("Stage", "")
        if require == "MAINNET" or stage in {"TESTNET", "BOTH"}:
            offending.append(f"{rid} ({stage}, {disp})")
    if offending:
        print(f"FAIL: launch is not clear for {require}: " + "; ".join(offending))
        sys.exit(1)

if require:
    print(f"PASS: launch is clear for {require} — {len(rows)} rows, no blocking disposition")
    sys.exit(0)

open_high = sum(
    1 for oid, (status, sev) in obl.items()
    if sev in {"C", "H"} and status in OPEN_ISH
)
print(
    f"PASS: launch risk register coherent — {len(rows)} rows, "
    f"{open_high} unsettled C/H obligation(s) ranked or excluded, "
    f"{len(decisions)} decision(s) of record"
)
PYEOF
}

case "${1:-}" in
  --self-test)
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    # Planted ids are assembled at run time: check-doc-index.sh scans every .sh for OBL-* citations.
    OPEN_H="OBL-""C""990"
    SET_H="OBL-""C""991"
    MED="OBL-""C""992"
    EXTRA="OBL-""C""993"
    UNRANKED="OBL-""C""994"
    cat > "$tmp/obl" <<EOF
| ID | Status | Proposition | Source | Sev |
|---|---|---|---|---|
| $OPEN_H | **OPEN** | planted open high | \`src/tx/mod.rs\` | H |
| $SET_H | **FIXED** | planted settled high | \`src/tx/mod.rs\` | H |
| $MED | **OPEN** | planted open medium | \`src/tx/mod.rs\` | M |
EOF
    cat > "$tmp/clean" <<EOF
<!-- launch-risk-rows -->
| Rank | LR | Stage | Class | Sev | Failure scenario | Evidence | Disposition | Action | Exit criterion |
|---:|---|---|---|---|---|---|---|---|---|
| 1 | LR-1 | TESTNET | DEFECT | H | planted scenario | \`$OPEN_H\` | BLOCKS | FIX | \`bash scripts/check-launch-register.sh\` |
<!-- /launch-risk-rows -->

| ID | Date | Decision | Rationale | Scope |
|---|---|---|---|---|
| LR-D1 | 2026-10-20 | planted decision | planted rationale | LR-1; $OPEN_H |

<!-- launch-risk-exclusions -->
<!-- (none) -->
<!-- /launch-risk-exclusions -->
EOF

    if ! out="$(run_check "$tmp/clean" "$tmp/obl" "" 2>&1)"; then
      echo "FAIL: --self-test: the conforming fixture did not pass"; echo "$out"; exit 1
    fi
    echo "OK: --self-test — the conforming fixture passes"

    expect_fail() {
      local label="$1" fixture="$2" needle="$3"
      local out
      if out="$(run_check "$fixture" "$tmp/obl" "" 2>&1)"; then
        echo "FAIL: --self-test: $label passed but should have failed"; exit 1
      fi
      if ! grep -q "$needle" <<<"$out"; then
        echo "FAIL: --self-test: $label failed without naming '$needle'"; echo "$out"; exit 1
      fi
      echo "OK: --self-test — $label is rejected and named"
    }

    # 1. duplicate LR id
    awk '{print} /^\| 1 \| LR-1 / {print "| 2 | LR-1 | TESTNET | DEFECT | H | planted | `'"$OPEN_H"'` | BLOCKS | FIX | `bash` |"}' \
      "$tmp/clean" > "$tmp/dup_lr"
    expect_fail "a duplicate LR id" "$tmp/dup_lr" "duplicate LR id"

    # 2. invalid stage
    python3 - "$tmp/clean" "$tmp/bad_stage" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
t = open(src).read().replace("| TESTNET |", "| MOON |", 1)
open(dst, "w").write(t)
PY
    expect_fail "an invalid Stage" "$tmp/bad_stage" "invalid Stage"

    # 3. missing exit criterion
    python3 - "$tmp/clean" "$tmp/no_exit" <<'PY'
import re, sys
src, dst = sys.argv[1], sys.argv[2]
t = open(src).read()
t = re.sub(r"\| `bash scripts/check-launch-register\.sh` \|$", "|  |", t, count=1, flags=re.M)
open(dst, "w").write(t)
PY
    expect_fail "an empty Exit criterion" "$tmp/no_exit" "empty Exit criterion"

    # 4. dangling obligation id
    python3 - "$tmp/clean" "$tmp/dangling" "$EXTRA" <<'PY'
import sys
src, dst, extra = sys.argv[1], sys.argv[2], sys.argv[3]
t = open(src).read()
t = t.replace("`" + "OBL-" + "C" + "990" + "`", "`" + extra + "`", 1)
open(dst, "w").write(t)
PY
    expect_fail "a dangling OBL id" "$tmp/dangling" "not a row in the obligation register"

    # 5. status restatement
    python3 - "$tmp/clean" "$tmp/status" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
t = open(src).read().replace("| BLOCKS | FIX |", "| OPEN | FIX |", 1)
open(dst, "w").write(t)
PY
    expect_fail "a restated obligation status" "$tmp/status" "restates the obligation status"

    # 6. an unranked open C/H obligation (restore the OBL fixture afterwards)
    cp "$tmp/obl" "$tmp/obl.base"
    printf '| %s | **OPEN** | planted unranked high | `src/tx/mod.rs` | H |\n' "$UNRANKED" >> "$tmp/obl"
    expect_fail "an unranked open C/H obligation" "$tmp/clean" "$UNRANKED"
    cp "$tmp/obl.base" "$tmp/obl"

    # 7. duplicate rank
    awk '{print} /^\| 1 \| LR-1 / {print "| 1 | LR-2 | TESTNET | DEFECT | H | planted | `'"$OPEN_H"'` | BLOCKS | FIX | `bash` |"}' \
      "$tmp/clean" > "$tmp/dup_rank"
    expect_fail "a duplicate Rank" "$tmp/dup_rank" "duplicate Rank"

    # 8. --require-clear fails on a blocking row and passes when it is watched
    if out="$(run_check "$tmp/clean" "$tmp/obl" "TESTNET" 2>&1)"; then
      echo "FAIL: --self-test: --require-clear TESTNET passed with a blocking row"; exit 1
    fi
    grep -q "not clear for TESTNET" <<<"$out" || { echo "FAIL: --self-test: --require-clear did not name the stage"; echo "$out"; exit 1; }
    python3 - "$tmp/clean" "$tmp/clear" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
t = open(src).read().replace("| BLOCKS | FIX |", "| WATCH | FIX |", 1)
open(dst, "w").write(t)
PY
    if ! out="$(run_check "$tmp/clear" "$tmp/obl" "TESTNET" 2>&1)"; then
      echo "FAIL: --self-test: --require-clear TESTNET failed with no blocking row"; echo "$out"; exit 1
    fi
    grep -q "launch is clear for TESTNET" <<<"$out" || { echo "FAIL: --self-test: --require-clear did not report clear"; exit 1; }
    echo "OK: --self-test — --require-clear fails on a blocking row and passes when it is watched"
    echo "OK: --self-test — 8 planted defects rejected and named, 1 conforming fixture accepted"
    exit 0
    ;;
  --require-clear)
    stage="${2:-}"
    if [ "$stage" != "TESTNET" ] && [ "$stage" != "MAINNET" ]; then
      echo "usage: $0 --require-clear TESTNET|MAINNET" >&2
      exit 2
    fi
    run_check "$REGISTER" "$OBLIGATIONS" "$stage"
    ;;
  ""|--check)
    run_check "$REGISTER" "$OBLIGATIONS" ""
    ;;
  *)
    echo "usage: $0 [--check|--self-test|--require-clear TESTNET|MAINNET]" >&2
    exit 2
    ;;
esac
