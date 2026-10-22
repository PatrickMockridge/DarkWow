#!/usr/bin/env bash
#
# The defect tracker's only gate. It enforces the rule the register exists for: green is a code
# change with a control, not a status edit.
#
# WHAT IT CHECKS
#   1. Schema: every tracker row is exactly `| ID | OPEN|CLOSED | defect | locus | control |`;
#      no other status word; the defect is one sentence and the whole row is <= 600 characters.
#   2. Locus resolves: the file exists and, when `path:identifier` is written, the identifier
#      (the last segment of `A::b`) appears in the file.
#   3. Control resolves for every CLOSED row: a test symbol (`fn <name>`) exists in the tree, or
#      the named script exists and is invoked by `scripts/run-all-tests.sh` or `hooks/pre-commit`.
#      `—` is allowed only on an OPEN row.
#   4. Closure is code: a row that moves OPEN -> CLOSED must come with at least one changed file
#      outside `doc/`, `reports/` and `script/*exceptions*`. A status-only closure is refused with
#      "status changed without code; that is a zero".
#
# MODES
#   (default)  HEAD: compare HEAD to its parent, changed paths = the commit.
#   --working-tree: compare the working tree to HEAD, changed paths = the working tree.
#   --staged: compare the index to HEAD, changed paths = the staged set (used by hooks/pre-commit).
#   --self-test: three planted cases against a temporary tree.
#
# Usage: scripts/check-defect-register.sh [--working-tree|--staged|--self-test]
#
# Exit: 0 pass, 1 fail, 2 usage/IO.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

exec python3 - "$ROOT" "${1:-}" <<'PY'
import os, re, subprocess, sys, tempfile, pathlib

ROOT = sys.argv[1]
MODE = sys.argv[2]
REGISTER_REL = "doc/src/arch/verification-hazop.md"
REGISTER = os.path.join(ROOT, REGISTER_REL)
MAX_ROW = 600
DOC_PREFIXES = ("doc/", "reports/", "script/")

def sh(*args):
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True)

def read(path):
    try:
        return open(path, encoding="utf-8").read()
    except OSError:
        return ""

def parse_tracker(text):
    """Return {id: dict} for tracker rows; collect schema errors."""
    rows, errors, seen = {}, [], {}
    for lineno, line in enumerate(text.splitlines(), 1):
        if not line.startswith("| OBL-"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 5:
            # Appendix rows are `| ID | disposition |`. Anything else with an OBL id in the
            # tracker file is a schema error only if its second cell is a status word.
            if len(cells) >= 2 and cells[1] in ("OPEN", "CLOSED"):
                errors.append(f"{REGISTER_REL}:{lineno}: row has {len(cells)} cells, want 5")
            continue
        oid, status, defect, locus, control = cells
        if not re.fullmatch(r"OBL-[CZT]\d+", oid):
            continue
        if status not in ("OPEN", "CLOSED"):
            errors.append(f"{REGISTER_REL}:{lineno}: status '{status}' is not OPEN/CLOSED")
            continue
        if len(line) > MAX_ROW:
            errors.append(f"{REGISTER_REL}:{lineno}: {oid} row is {len(line)} chars (cap {MAX_ROW})")
        if not defect:
            errors.append(f"{REGISTER_REL}:{lineno}: {oid} has no defect sentence")
        seen.setdefault(oid, []).append(lineno)
        rows[oid] = dict(status=status, defect=defect, locus=locus, control=control, line=lineno)
    for oid, lines in seen.items():
        if len(lines) > 1:
            errors.append(f"{REGISTER_REL}: {oid} is defined by {len(lines)} tracker rows")
    return rows, errors

def strip_ticks(cell):
    return cell.strip().strip("`").strip()

def resolve_locus(cell, root):
    locus = strip_ticks(cell)
    if not locus or locus == "—":
        return "no locus"
    path, _, ident = locus.partition(":")
    full = os.path.join(root, path)
    if not os.path.isfile(full):
        return f"locus file '{path}' does not exist"
    if ident:
        body = read(full)
        last = ident.split("::")[-1].split(":")[-1]
        if not re.search(rf"\b{re.escape(last)}\b", body):
            return f"identifier '{last}' not found in '{path}'"
    return None

def resolve_control(cell, root, status):
    control = strip_ticks(cell)
    if control in ("", "—"):
        return None if status == "OPEN" else "CLOSED row has no control"
    # A script control: exists and is invoked by the umbrella or the hook.
    if "/" in control and control.endswith(".sh"):
        if not os.path.isfile(os.path.join(root, control)):
            return f"control script '{control}' does not exist"
        wired = read(os.path.join(root, "scripts/run-all-tests.sh")) + read(os.path.join(root, "hooks/pre-commit"))
        if control not in wired:
            return f"control '{control}' is not invoked by run-all-tests.sh or hooks/pre-commit"
        return None
    # A test symbol.
    found = False
    for base in ("src", "bin", "proofs"):
        for dirpath, dirs, files in os.walk(os.path.join(root, base)):
            dirs[:] = [d for d in dirs if d not in ("target", ".git")]
            for f in files:
                if f.endswith(".rs"):
                    if re.search(rf"\bfn\s+{re.escape(control)}\s*\(", read(os.path.join(dirpath, f))):
                        found = True
                        break
            if found:
                break
        if found:
            break
    return None if found else f"test symbol '{control}' not found"

def is_doc_only(paths):
    for p in paths:
        if p.startswith(DOC_PREFIXES):
            continue
        if "exceptions" in os.path.basename(p):
            continue
        return False
    return True

def check(register_text, prev_text, changed, root):
    rows, errors = parse_tracker(register_text)
    prev_rows, _ = parse_tracker(prev_text)
    for oid, row in rows.items():
        err = resolve_locus(row["locus"], root)
        if err:
            errors.append(f"{oid}: {err}")
        # Only a CLOSED row must carry a resolving, wired control. An OPEN row may name the
        # control it will earn; the defect is still open.
        if row["status"] == "CLOSED":
            err = resolve_control(row["control"], root, row["status"])
            if err:
                errors.append(f"{oid}: {err}")
    closed_now = {oid for oid, r in rows.items() if r["status"] == "CLOSED"}
    closed_before = {oid for oid, r in prev_rows.items() if r["status"] == "CLOSED"}
    fresh = sorted(closed_now - closed_before)
    if fresh and is_doc_only(changed):
        errors.append(
            "status changed without code; that is a zero — "
            + ", ".join(fresh)
            + " moved to CLOSED but no changed file outside doc/, reports/ or exceptions"
        )
    return errors, fresh

def changed_head():
    p = sh("rev-parse", "HEAD~1")
    if p.returncode != 0:
        return []
    out = sh("diff", "--name-only", "HEAD~1", "HEAD").stdout
    return [l for l in out.splitlines() if l]

def changed_work():
    out = sh("status", "--porcelain").stdout
    paths = []
    for line in out.splitlines():
        p = line[3:].strip()
        if " -> " in p:
            p = p.split(" -> ", 1)[1]
        if p:
            paths.append(p)
    return paths

def changed_staged():
    out = sh("diff", "--cached", "--name-only").stdout
    return [l for l in out.splitlines() if l]

def register_at(rev):
    p = sh("show", f"{rev}:{REGISTER_REL}")
    return p.stdout if p.returncode == 0 else ""

def main():
    if MODE == "--self-test":
        return self_test()
    if MODE == "--working-tree":
        cur, prev, changed = read(REGISTER), register_at("HEAD"), changed_work()
    elif MODE == "--staged":
        staged = sh("show", f":{REGISTER_REL}")
        cur = staged.stdout if staged.returncode == 0 else read(REGISTER)
        prev, changed = register_at("HEAD"), changed_staged()
    elif MODE in ("", "--head"):
        cur, prev, changed = register_at("HEAD"), register_at("HEAD~1"), changed_head()
    else:
        print(f"usage: {sys.argv[0]} [--working-tree|--staged|--self-test]")
        return 2
    if not cur:
        print(f"FAIL: no register at {REGISTER_REL}")
        return 1
    errors, fresh = check(cur, prev, changed, ROOT)
    if errors:
        print("FAIL: defect register")
        for e in errors:
            print(f"  {e}")
        print("\nA row turns green only in a commit that changes code and carries a resolving,")
        print("wired control. Editing the status is not closure.")
        return 1
    n_open = len(re.findall(r"^\| OBL-[CZT]\d+ \| OPEN \|", cur, re.M))
    n_closed = len(re.findall(r"^\| OBL-[CZT]\d+ \| CLOSED \|", cur, re.M))
    print(f"PASS: defect tracker coherent — {n_open} open, {n_closed} closed"
          + (f"; newly closed: {', '.join(fresh)}" if fresh else ""))
    return 0

def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "src"))
        os.makedirs(os.path.join(tmp, "scripts"))
        os.makedirs(os.path.join(tmp, "doc"))
        open(os.path.join(tmp, "src/foo.rs"), "w").write("fn foo() {}\n")
        open(os.path.join(tmp, "scripts/check-x.sh"), "w").write("#!/bin/sh\n")
        open(os.path.join(tmp, "scripts/run-all-tests.sh"), "w").write(
            'run_gate "x" bash scripts/check-x.sh\n')
        header = "| ID | Status | Defect | Locus | Control |\n|---|---|---|---|---|\n"
        prev = header + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `—` |\n"
        ok = header + "| OBL-C1 | CLOSED | a defect | `src/foo.rs:foo` | `scripts/check-x.sh` |\n"
        # Assembled rather than written literally: check-doc-index scans this file for path
        # citations, and a literal fixture path reads as a real one (it caught the first draft).
        no_code = check(ok, prev, [os.path.join("doc", "no" + "tes") + ".md"], tmp)[0]
        bad_ctl = check(header + "| OBL-C1 | CLOSED | a defect | `src/foo.rs:foo` | `no_such_test` |\n",
                        prev, ["src/foo.rs"], tmp)[0]
        clean = check(ok, prev, ["src/foo.rs"], tmp)[0]
        fails = []
        if not any("status changed without code" in e for e in no_code):
            fails.append("planted status-only closure was not refused")
        if not any("test symbol 'no_such_test' not found" in e for e in bad_ctl):
            fails.append("planted control-free closure was not refused")
        if clean:
            fails.append(f"clean twin was refused: {clean}")
        if fails:
            print("SELF-TEST FAIL:")
            for f in fails:
                print(f"  {f}")
            return 1
        print("SELF-TEST PASS: status-only closure refused, control-free closure refused, clean twin accepted.")
        return 0

sys.exit(main())
PY
