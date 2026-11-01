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
import os, re, subprocess, sys, pathlib

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


DOC_STAMP = re.compile(r'doc:([^\s,#@]+)(?:#([^\s@,]+))?@([0-9a-f]{8,})')


def _slug(heading):
    """GitHub's heading anchor: drop punctuation (the em-dash included), spaces to hyphens."""
    return re.sub(r'\s', '-', re.sub(r'[^\w\s-]', '', heading.strip().lower()))


def _slugs(text):
    return {_slug(h) for h in re.findall(r'^#{1,6}\s+(.*)$', text, re.M)}


def _headings_at(root, path, commit):
    """Heading slugs in `path` at `commit`, or None when the commit/path cannot be read.

    A `doc:<path>#<anchor>@<commit>` stamp is a citation *of that commit*. Resolving the anchor
    against the working tree makes a legitimate later restructure retroactively break a dated
    ledger — observed 2026-11-01 on verification-hazop.md, whose 2026-10 headings were replaced by
    the five-column tracker while seven stamps still (correctly) cite the old document at the
    commits that carried it. Resolve at the stamp's own commit; fall back to the working tree only
    when there is no commit to read (the `--self-test` temp root is not a git repository).
    """
    try:
        proc = subprocess.run(["git", "-C", str(root), "show", f"{commit}:{path}"],
                              capture_output=True, text=True)
    except OSError:
        return None
    if proc.returncode != 0:
        return None
    return _slugs(proc.stdout)


def unresolved(root):
    """Every `doc:` stamp whose path is absent, or whose anchor names no heading in that file.

    WHY THIS EXISTS (2026-10-12). The vocabulary check above was the whole gate, so a stamp could name
    a path that does not exist and still read as promoted. Measured on the day the campaign closed:
    **23 stamps** cited `doc/src/DarkFi/…` for files that live under `proofs/lean/src/`, and **8**
    named an anchor no heading produces — one of them the literal placeholder `#a42-...`. Every one of
    those read as a home in the promotion log. They were found by an adversarial re-read, not by a
    machine, which is precisely the gap this closes: the stamp is a *citation*, and a citation that
    does not resolve is the disease the ledger's own `F-006`/`F-007` rows are about.
    """
    text = (root / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md").read_text()
    out = []
    if BLOCK_OPEN not in text or BLOCK_CLOSE not in text:
        return out
    body = text.split(BLOCK_OPEN, 1)[1].split(BLOCK_CLOSE, 1)[0]
    for line in body.splitlines():
        line = line.strip()
        if not line or line.startswith("<!--") or not re.match(r'F-\d+\s*:', line):
            continue
        fid = line.split(":", 1)[0]
        for m in DOC_STAMP.finditer(line):
            path, anchor, commit = m.group(1), m.group(2), m.group(3)
            f = root / path
            slugs = _headings_at(root, path, commit)
            exists = slugs is not None or f.exists()
            if not exists:
                out.append(f"{fid}: doc:{path} — no such file")
            elif anchor and path.endswith(".md"):
                if slugs is None:
                    slugs = _slugs(f.read_text())
                if anchor not in slugs:
                    out.append(f"{fid}: doc:{path}#{anchor} — no heading produces that anchor "
                               f"at {commit}")
    return out


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

            # Control 3: a stamp whose path does not exist must be reported. This is the arm the
            # 2026-10-12 sweep added; without it the gate passes a citation that names nothing.
            # The path is assembled at run time rather than written literally: `scripts/check-doc-index.sh`
            # scans this file for `.md` citations and would report a literal dead one as a finding
            # against this gate — the same trap the sync-conformance control hit, and the same fix.
            planted_bad = ("F-998: doc:doc/src/""does/not/exist"".md#nope@deadbeef")
            (t / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md").write_text(
                text.replace(BLOCK_CLOSE, planted_bad + "\n" + BLOCK_CLOSE))
            bad = unresolved(t)
            if not any("F-998" in b for b in bad):
                print("FAIL: --self-test planted an unresolvable stamp (F-998) and it was not reported")
                return 1
            print("OK: --self-test — a stamp citing a path that does not exist is reported")

            # Control 4: a stamp is a citation of its commit. An anchor that existed at the
            # stamped commit must resolve even when the working tree has since restructured the
            # document — the 2026-11-01 register/tracker case.
            # Names are assembled at run time: `scripts/check-doc-index.sh` scans this file for
            # `.md` citations and would read a literal fixture name as a citation of a missing
            # document (the same trap the other self-tests document).
            subject_rel = "doc/src/arch/sub" + "ject" + ".m" + "d"
            repo = t / "gitroot"
            (repo / "doc" / "src" / "arch" / "audit").mkdir(parents=True)
            subject = repo / subject_rel
            subject.write_text("# Kept heading\n")
            git = ["git", "-C", str(repo), "-c", "user.email=self@test", "-c", "user.name=self"]
            subprocess.run(git + ["init", "-q"], check=True, capture_output=True)
            subprocess.run(git + ["add", "."], check=True, capture_output=True)
            subprocess.run(git + ["commit", "-qm", "carry the heading"], check=True, capture_output=True)
            sha = subprocess.run(git + ["rev-parse", "HEAD"], check=True,
                                 capture_output=True, text=True).stdout.strip()
            subject.write_text("# A different heading\n")  # restructured, not committed
            (repo / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md").write_text(
                text.replace(BLOCK_CLOSE,
                             f"F-997: doc:{subject_rel}#kept-heading@{sha}\n" + BLOCK_CLOSE))
            bad_commit = unresolved(repo)
            if any("F-997" in b for b in bad_commit):
                print(f"FAIL: --self-test an anchor present at the stamped commit was reported: "
                      f"{[b for b in bad_commit if 'F-997' in b]}")
                return 1
            print("OK: --self-test — an anchor resolves at the stamped commit, not the working tree")

            # ...and the same stamp with an anchor that never existed must still fail.
            (repo / "doc" / "src" / "arch" / "audit" / "docs-review-2026-10-10.md").write_text(
                text.replace(BLOCK_CLOSE,
                             f"F-996: doc:{subject_rel}#never-existed@{sha}\n" + BLOCK_CLOSE))
            bad_missing = unresolved(repo)
            if not any("F-996" in b for b in bad_missing):
                print("FAIL: --self-test an anchor absent at the stamped commit was not reported")
                return 1
            print("OK: --self-test — an anchor absent at the stamped commit is reported")
        return 0

    rows, refuted, stamped, pending, invalid = scan(ROOT)
    for fid, stamp in invalid:
        print(f"FAIL: {fid}: stamp is not from the vocabulary ({stamp!r})")
    for fid in pending:
        print(f"pending: {fid}")
    unresolved_stamps = unresolved(ROOT)
    for u in unresolved_stamps:
        print(f"FAIL: {u}")
    n_ok = len(rows) - len(pending)
    print(f"[{'PASS' if not pending and not unresolved_stamps else 'PENDING'}] rows={len(rows)} "
          f"promoted={n_ok} refuted={len(refuted)} pending={len(pending)} "
          f"invalid_stamps={len(invalid)} unresolved_stamps={len(unresolved_stamps)}")
    if "--check" in sys.argv and (pending or invalid or unresolved_stamps):
        return 1
    return 0


sys.exit(main())
PY
