#!/usr/bin/env bash
#
# The review ledger's findings must reach a home.
#
# WHY THIS EXISTS. `doc/src/arch/audit/docs-review-2026-10-10.md` is intake, not home: it names the
# confirmed findings, and remediation moves each to a doc edit, a new `OBL-*` row, or a new gate. Without
# an instrument that reads "is every confirmed row promoted?", "the campaign is complete" is a claim no
# check makes — which is the exact disease the review diagnosed (register RC5, "the instrument overstates
# its verdict"). This gate reads the `<!-- promotion-log -->` block and fails on a confirmed row still
# `pending`.
#
# WHAT IT CHECKS. The ledger's finding tables carry an id (`| F-<n> | ... |`) but no verdict or stamp
# cell (their column arity varies 3..6 across the phases), so the machine-readable per-row record lives
# in one `<!-- promotion-log -->` block, the register's own idiom for machine-checked data
# (`<!-- root-cause-groups -->`, `<!-- assumption-inventory -->`). The block has, per line:
#
#   refuted: F-154 F-190 F-517                 findings the refuter pass struck — no stamp owed
#   F-<n>: <stamp>                             a promoted row, stamp from the closed vocabulary:
#                                              doc:<path>#<anchor>@<commit> | row:OBL-<id> |
#                                              gate:<script> | struck:REFUTED:<id> | partial:<correction>
#
# A row may carry several stamps, comma-separated. A table finding id that is neither `refuted` nor
# stamped is `pending`. An empty block is legal (the tree starts with every row pending).
#
# Usage: scripts/check-ledger-promotion.sh                 (report-only: prints the census, exits 0)
#        scripts/check-ledger-promotion.sh --check         (exit 1 while any row is pending)
#        scripts/check-ledger-promotion.sh --self-test     (planted unstamped row; non-zero exit)
# Exit status: --check 1 on any pending/invalid row; report-only 0.

set -uo pipefail

ROOT="${LEDGER_PROMOTION_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export LEDGER_PROMOTION_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, pathlib

ROOT = pathlib.Path(os.environ["LEDGER_PROMOTION_ROOT"])
LEDGER = ROOT / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md"

ROW = re.compile(r'^\|\s*(F-\d+)\s*\|')
BLOCK_OPEN, BLOCK_CLOSE = "<!-- promotion-log -->", "<!-- /promotion-log -->"
VOCAB = ("doc:", "row:", "gate:", "struck:", "partial:")


def parse(text):
    rows = [m.group(1) for line in text.splitlines() if (m := ROW.match(line))]
    refuted, stamped, invalid = set(), {}, []
    body = None
    if BLOCK_OPEN in text and BLOCK_CLOSE in text:
        body = text.split(BLOCK_OPEN, 1)[1].split(BLOCK_CLOSE, 1)[0]
    for line in (body or "").splitlines():
        line = line.strip()
        if not line or line.startswith("<!--"):
            continue
        if line.startswith("refuted:"):
            refuted |= set(line[len("refuted:"):].split())
            continue
        m = re.match(r'(F-\d+)\s*:\s*(.*)$', line)
        if not m:
            continue
        fid, stamp = m.group(1), m.group(2).strip()
        if stamp and not stamp.split()[0].startswith(VOCAB) and not any(
                p.startswith(VOCAB) for p in re.split(r',\s*', stamp)):
            invalid.append((fid, stamp))
        stamped[fid] = stamp
    return rows, refuted, stamped, invalid


def scan(root):
    text = (root / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md").read_text()
    rows, refuted, stamped, invalid = parse(text)
    pending = [r for r in rows if r not in refuted and not stamped.get(r)]
    return rows, refuted, stamped, pending, invalid


def main():
    if "--self-test" in sys.argv:
        import tempfile, shutil
        text = LEDGER.read_text()
        # Plant an unstamped confirmed row at the head of the tables (an F-row with no log entry).
        planted = "| F-999 | DOC-STALE | M | planted | planted | planted, unstamped |"
        with tempfile.TemporaryDirectory() as tmp:
            t = pathlib.Path(tmp)
            (t / "doc" / "src" / "arch" / "audit").mkdir(parents=True)
            (t / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md").write_text(
                text + "\n" + planted + "\n")
            *_, pending, _inv = scan(t)
            if "F-999" not in pending:
                print("FAIL: --self-test planted an unstamped row (F-999) and it was not reported pending")
                return 1
            print("OK: --self-test — a planted unstamped row (F-999) is reported pending")

            # Reflect the planted row into the log — it must then leave the pending set.
            (t / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md").write_text(
                text.replace(BLOCK_CLOSE, "F-999: doc:ledger#self-test@deadbeef\n" + BLOCK_CLOSE)
                + "\n" + planted + "\n")
            *_, pending2, _inv2 = scan(t)
            if "F-999" in pending2:
                print("FAIL: --self-test stamped F-999 but it is still pending")
                return 1
            print("OK: --self-test — a stamped row leaves the pending set")
        return 0

    rows, refuted, stamped, pending, invalid = scan(ROOT)
    for fid, stamp in invalid:
        print(f"FAIL: {fid}: stamp is not from the vocabulary ({stamp!r})")
    for fid in pending:
        print(f"pending: {fid}")
    n_ok = len(rows) - len(pending)
    print(f"[{'PASS' if not pending else 'PENDING'}] rows={len(rows)} promoted={n_ok} "
          f"refuted={len(refuted)} pending={len(pending)} invalid_stamps={len(invalid)}")
    if "--check" in sys.argv and (pending or invalid):
        return 1
    return 0


sys.exit(main())
PY
