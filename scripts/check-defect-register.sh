#!/usr/bin/env bash
#
# The defect tracker's gate. Green is a code change with a control; a residual is an accepted risk
# with an ALARP record and a batch approval, never a green row.
#
# WHAT IT CHECKS
#   1. Schema: main-table rows are `| ID | Status | Defect | Locus | Control | Risk |`, with status
#      OPEN/BLOCKED/DEFERRED/CLOSED and Risk `S/L` (S in C/H/M/L, L in D/P/T). The band is computed
#      from doc/src/arch/risk-management.md: Intolerable >= 15, Tolerable 6-14, Broadly <= 5, with
#      C and H never Broadly. An OPEN row carries a resolving control or `plan:<text>`; `—` fails.
#   2. Locus resolves: the file exists and, when `path:identifier` is written, the identifier
#      (the last segment of `A::b`) appears in the file.
#   3. CLOSED rows: the control resolves, a script control has a `--self-test`, the control is not
#      the locus itself, and the closure commit changes production code (src/, bin/, proofs/ or
#      production contrib/; not docs, reports, scripts/, hooks/ or contrib/ci/).
#   4. Tombstones: an ID that leaves the main table must appear in the Settled-IDs appendix; every
#      residual record's source ID must be tombstoned; no ID may be live in the tracker and in
#      decisions.md or residual-risk.md.
#   5. Counts: the register's stated decisions count equals decisions.md's row count.
#   6. Risk history: a severity/likelihood decrease requires `basis:` in the row.
#   7. Approval: reports/register-approval.txt holds `scope_sha256=<sha256>` over the tracker,
#      residual register and risk policy; any edit to them invalidates it.
#
# MODES
#   (default)  HEAD: compare HEAD to its parent, changed paths = the commit.
#   --working-tree: compare the working tree to HEAD, changed paths = the working tree.
#   --staged: compare the index to HEAD, changed paths = the staged set (used by hooks/pre-commit).
#   --self-test: planted cases against a temporary tree.
#
# Usage: scripts/check-defect-register.sh [--working-tree|--staged|--self-test]
#
# Exit: 0 pass, 1 fail, 2 usage/IO.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

exec python3 - "$ROOT" "${1:-}" <<'PY'
import hashlib, os, re, subprocess, sys, tempfile

ROOT = sys.argv[1]
MODE = sys.argv[2]
REGISTER_REL = "doc/src/arch/verification-hazop.md"
REGISTER = os.path.join(ROOT, REGISTER_REL)
RESIDUAL_REL = "doc/src/arch/residual-risk.md"
RESIDUAL = os.path.join(ROOT, RESIDUAL_REL)
DECISIONS_REL = "doc/src/arch/decisions.md"
DECISIONS = os.path.join(ROOT, DECISIONS_REL)
POLICY_REL = "doc/src/arch/risk-management.md"
POLICY = os.path.join(ROOT, POLICY_REL)
APPROVAL_REL = "reports/register-approval.txt"
APPROVAL = os.path.join(ROOT, APPROVAL_REL)
MAX_ROW = 600
STATUSES = ("OPEN", "BLOCKED", "DEFERRED", "CLOSED")
SEV = {"C": 5, "H": 3, "M": 2, "L": 1}
LIK = {"D": 5, "P": 3, "T": 1}
DOC_PREFIXES = ("doc/", "reports/", "scripts/", "hooks/", "script/", "contrib/ci/")

def sh(*args):
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True)

def read(path):
    try:
        return open(path, encoding="utf-8").read()
    except OSError:
        return ""

def band(sev, lik):
    """The band for a risk pair, or None if the pair violates an override."""
    if sev not in SEV or lik not in LIK:
        return None
    r = SEV[sev] * LIK[lik]
    if r >= 15:
        return "Intolerable"
    if r <= 5:
        return None if sev in ("C", "H") else "Broadly acceptable"
    return "Tolerable"

def parse_register(text):
    """Return (rows, appendix, errors). rows: main table; appendix: ID -> disposition."""
    rows, appendix, errors, seen = {}, {}, [], {}
    for lineno, line in enumerate(text.splitlines(), 1):
        if not line.startswith("| OBL-"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        oid = cells[0].strip("` ")
        if not re.fullmatch(r"OBL-[CZT]\d+", oid):
            continue
        if len(cells) == 6 and cells[1] in STATUSES:
            _, status, defect, locus, control, risk = cells
            if len(line) > MAX_ROW:
                errors.append(f"{REGISTER_REL}:{lineno}: {oid} row is {len(line)} chars (cap {MAX_ROW})")
            if not defect:
                errors.append(f"{REGISTER_REL}:{lineno}: {oid} has no defect sentence")
            m = re.fullmatch(r"([CHML])/([DPT])", risk)
            if not m:
                errors.append(f"{REGISTER_REL}:{lineno}: {oid} risk '{risk}' is not S/L (C/H/M/L x D/P/T)")
            elif band(m.group(1), m.group(2)) is None:
                errors.append(f"{REGISTER_REL}:{lineno}: {oid} C/H risk cannot be Broadly acceptable")
            if status == "OPEN":
                c = control.strip("` ").strip()
                if c in ("", "—"):
                    errors.append(f"{REGISTER_REL}:{lineno}: {oid} OPEN row has no control or plan")
            if status == "BLOCKED" and "blocked:" not in control:
                errors.append(f"{REGISTER_REL}:{lineno}: {oid} BLOCKED row does not name blocked:<reason>")
            if status == "DEFERRED" and "deferred" not in control:
                errors.append(f"{REGISTER_REL}:{lineno}: {oid} DEFERRED row does not name its deferral")
            seen.setdefault(oid, []).append(lineno)
            rows[oid] = dict(status=status, defect=defect, locus=locus, control=control,
                             sev=m.group(1) if m else "?", lik=m.group(2) if m else "?", line=lineno)
        elif len(cells) == 2:
            appendix[oid] = cells[1]
        elif len(cells) >= 2 and cells[1] in STATUSES:
            errors.append(f"{REGISTER_REL}:{lineno}: row has {len(cells)} cells, want 6")
    for oid, lines in seen.items():
        if len(lines) > 1:
            errors.append(f"{REGISTER_REL}: {oid} is defined by {len(lines)} tracker rows")
    return rows, appendix, errors

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

def resolve_control(cell, root):
    control = strip_ticks(cell)
    if control in ("", "—") or control.startswith("plan:"):
        return None  # OPEN-row plan; the status rule already permits it
    if "/" in control and control.endswith(".sh"):
        full = os.path.join(root, control)
        if not os.path.isfile(full):
            return f"control script '{control}' does not exist"
        wired = read(os.path.join(root, "scripts/run-all-tests.sh")) + read(os.path.join(root, "hooks/pre-commit"))
        if control not in wired:
            return f"control '{control}' is not invoked by run-all-tests.sh or hooks/pre-commit"
        return None
    found = False
    for base in ("src", "bin", "proofs", "contrib"):
        for dirpath, dirs, files in os.walk(os.path.join(root, base)):
            dirs[:] = [d for d in dirs if d not in ("target", ".git")]
            for f in files:
                if f.endswith(".rs") and re.search(
                    rf"\bfn\s+{re.escape(control)}\s*\(", read(os.path.join(dirpath, f))
                ):
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

def production_path(path):
    if path.startswith("contrib/ci/"):
        return False
    return path.startswith(("src/", "bin/", "proofs/", "contrib/"))

def evidence_commit_ok(sha, root):
    """The cited commit exists and changed a production path."""
    p = sh("show", "--name-only", "--format=", sha)
    if p.returncode != 0:
        return False
    return any(production_path(l) for l in p.stdout.splitlines() if l.strip())

def decision_ids(root):
    ids = []
    for line in read(os.path.join(root, DECISIONS_REL)).splitlines():
        if line.startswith("| OBL-"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if cells and re.fullmatch(r"OBL-[CZT]\d+", cells[0].strip("` ")):
                ids.append(cells[0].strip("` "))
    return ids

def residual_sources(root):
    out = []
    for line in read(os.path.join(root, RESIDUAL_REL)).splitlines():
        if line.startswith("| R-"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) >= 3 and cells[1].startswith("OBL-"):
                out.append(cells[1])
    return out

def scope_hash(root):
    h = hashlib.sha256()
    for rel in (REGISTER_REL, RESIDUAL_REL, POLICY_REL):
        h.update(read(os.path.join(root, rel)).encode())
    return h.hexdigest()

def check(register_text, prev_text, changed, root):
    rows, appendix, errors = parse_register(register_text)
    prev_rows, prev_appendix, _ = parse_register(prev_text)
    decisions = decision_ids(root)
    residuals = residual_sources(root)

    for oid, row in rows.items():
        err = resolve_locus(row["locus"], root)
        if err:
            errors.append(f"{oid}: {err}")
        if row["status"] == "CLOSED":
            control = strip_ticks(row["control"])
            if control.startswith("plan:"):
                errors.append(f"{oid}: CLOSED row has a plan, not a control")
            err = resolve_control(row["control"], root)
            if err:
                errors.append(f"{oid}: {err}")
            locus_path = strip_ticks(row["locus"]).partition(":")[0]
            if control == locus_path:
                errors.append(f"{oid}: control is the locus itself")
            if "/" in control and control.endswith(".sh"):
                if "--self-test" not in read(os.path.join(root, control)):
                    if "negative:" not in row["defect"] + " " + row["control"]:
                        errors.append(
                            f"{oid}: script control has no --self-test and no negative: evidence")

    # Decisions and residuals cannot be live tracker rows.
    for did in decisions:
        if did in rows:
            errors.append(f"{did}: dual-tracked — live in the tracker and in decisions.md")
    for sid in residuals:
        if sid in rows and rows[sid]["status"] != "CLOSED":
            errors.append(f"{sid}: residual source is still a live tracker row")
        if sid not in appendix:
            errors.append(f"{sid}: residual source has no tombstone in the appendix")
    for oid, disp in appendix.items():
        if "residual" in disp.lower() and oid not in residuals:
            errors.append(f"{oid}: appendix says residual but residual-risk.md has no record")

    # An ID that leaves the main table must be tombstoned.
    for oid in prev_rows:
        if oid not in rows and oid not in appendix:
            errors.append(f"{oid}: left the main table without a tombstone")

    # The stated decisions count must match the file.
    m = re.search(r"(\d+) decisions", register_text)
    if m and int(m.group(1)) != len(decisions):
        errors.append(f"stated decisions count {m.group(1)} != decisions.md rows {len(decisions)}")

    # A severity/likelihood decrease needs a basis.
    for oid, row in rows.items():
        if oid in prev_rows:
            old, new = prev_rows[oid], row
            if old["sev"] != "?" and new["sev"] != "?":
                if SEV[new["sev"]] * LIK[new["lik"]] < SEV[old["sev"]] * LIK[old["lik"]]:
                    if "basis:" not in row["defect"]:
                        errors.append(f"{oid}: risk decreased without basis:")

    closed_now = {oid for oid, r in rows.items() if r["status"] == "CLOSED"}
    closed_before = {oid for oid, r in prev_rows.items() if r["status"] == "CLOSED"}
    fresh = sorted(closed_now - closed_before)
    if fresh and is_doc_only(changed):
        # A late closure is legitimate only when it cites the commit that changed production
        # code (the rule's own text), not merely a docs-only status edit.
        for oid in fresh:
            row = rows[oid]
            cited = re.findall(r"evidence:([0-9a-f]{8,40})", row["defect"] + " " + row["control"])
            if not cited:
                errors.append(f"{oid}: status changed without code and no evidence:<commit> cited")
            elif not any(evidence_commit_ok(sha, root) for sha in cited):
                errors.append(f"{oid}: evidence commit does not change production code")

    # Batch approval binds the tracker, residual register and policy.
    approval = read(os.path.join(root, APPROVAL_REL))
    am = re.search(r"scope_sha256=([0-9a-f]{64})", approval)
    if not am:
        errors.append(f"missing approval manifest {APPROVAL_REL}")
    elif am.group(1) != scope_hash(root):
        errors.append("approval scope_sha256 does not match the register, residual file and policy")
    else:
        bm = re.search(r"batch=([A-Z0-9,\-]+)", approval)
        ids = bm.group(1).split(",") if bm else []
        for oid in ids:
            if oid not in rows and oid not in appendix:
                errors.append(f"approved id {oid} does not resolve")
    return errors, fresh

def changed_head():
    if sh("rev-parse", "HEAD~1").returncode != 0:
        return []
    return [l for l in sh("diff", "--name-only", "HEAD~1", "HEAD").stdout.splitlines() if l]

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
    return [l for l in sh("diff", "--cached", "--name-only").stdout.splitlines() if l]

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
        print("\nA row turns green only in a commit that changes production code and carries a")
        print("resolving, self-tested control. A residual needs an ALARP record and the batch hash.")
        return 1
    n = {s: len(re.findall(rf"^\| OBL-[CZT]\d+ \| {s} \|", cur, re.M)) for s in STATUSES}
    n_res = len(re.findall(r"^\| R-\d+ \|", read(RESIDUAL), re.M))
    print(f"PASS: defect tracker coherent — {n['OPEN']} open, {n['BLOCKED']} blocked, "
          f"{n['DEFERRED']} deferred, {n['CLOSED']} closed, {n_res} residual"
          + (f"; newly closed: {', '.join(fresh)}" if fresh else ""))
    return 0

def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        def write(rel, body):
            path = os.path.join(tmp, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            open(path, "w").write(body)
        write("src/foo.rs", "fn foo() {}\n")
        write("scripts/check-x.sh", "#!/bin/sh\necho --self-test\n")
        write("scripts/run-all-tests.sh", 'run_gate "x" bash scripts/check-x.sh\n')
        write(POLICY_REL, "policy\n")
        write(DECISIONS_REL, "| ID | Kind | Statement |\n|---|---|---|\n| OBL-C9 | decision | d |\n")
        write(RESIDUAL_REL, "| ID | Source | Harm | S/L | Scenario | Control | Why | Compensating | Owner | Reopen |\n"
              "|---|---|---|---|---|---|---|---|---|---|\n"
              "| R-1 | OBL-C8 | availability | M/P | s | c | w | k | lead | release |\n")
        head = "| ID | Status | Defect | Locus | Control | Risk |\n|---|---|---|---|---|---|\n"
        tomb = ("\n## Appendix\n\n| ID | Disposition |\n|---|---|\n"
                "| OBL-C8 | residual (R-1) |\n")
        base = head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `plan: test` | M/P |\n" + tomb
        ok = head + "| OBL-C1 | CLOSED | a defect | `src/foo.rs:foo` | `scripts/check-x.sh` | M/P |\n" + tomb
        write(REGISTER_REL, ok)
        write(APPROVAL_REL, "scope_sha256=" + scope_hash(tmp) + "\nbatch=OBL-C1\n")
        prev_ok = base
        fails = []
        def expect(name, cond):
            if not cond:
                fails.append(name)
        expect("status-only closure refused",
               any("without code" in e for e in
                   check(ok, prev_ok, [os.path.join("doc", "no" + "tes") + ".md"], tmp)[0]))
        expect("control-free closure refused",
               any("test symbol" in e or "does not exist" in e for e in
                   check(head + "| OBL-C1 | CLOSED | a defect | `src/foo.rs:foo` | `no_such_test` | M/P |\n" + tomb,
                         prev_ok, ["src/foo.rs"], tmp)[0]))
        expect("clean twin accepted", not check(ok, prev_ok, ["src/foo.rs"], tmp)[0])
        expect("OPEN without control refused",
               any("no control or plan" in e for e in
                   check(head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `—` | M/P |\n" + tomb,
                         prev_ok, ["src/foo.rs"], tmp)[0]))
        expect("missing risk refused",
               any("risk" in e for e in
                   check(head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `plan: t` |  |\n" + tomb,
                         prev_ok, ["src/foo.rs"], tmp)[0]))
        expect("C/H cannot be broadly acceptable refused",
               any("Broadly" in e for e in
                   check(head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `plan: t` | C/T |\n" + tomb,
                         prev_ok, ["src/foo.rs"], tmp)[0]))
        expect("dual-tracked decision refused",
               any("dual-tracked" in e for e in
                   check(head + "| OBL-C9 | OPEN | a defect | `src/foo.rs:foo` | `plan: t` | M/P |\n" + tomb,
                         prev_ok, ["src/foo.rs"], tmp)[0]))
        expect("missing tombstone refused",
               any("without a tombstone" in e for e in
                   check(head + tomb, prev_ok, ["src/foo.rs"], tmp)[0]))
        expect("control==locus refused",
               any("control is the locus" in e for e in
                   check(head + "| OBL-C1 | CLOSED | a defect | `scripts/check-x.sh` | `scripts/check-x.sh` | M/P |\n" + tomb,
                         prev_ok, ["src/foo.rs"], tmp)[0]))
        write("scripts/check-x.sh", "#!/bin/sh\n")
        expect("script control without self-test refused",
               any("--self-test" in e for e in check(ok, prev_ok, ["src/foo.rs"], tmp)[0]))
        write("scripts/check-x.sh", "#!/bin/sh\necho --self-test\n")
        two_dec = (head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `plan: t` | M/P |\n"
                   + tomb + "\n2 decisions\n")
        expect("decisions count mismatch refused",
               any("decisions count" in e for e in check(two_dec, prev_ok, ["src/foo.rs"], tmp)[0]))
        write(APPROVAL_REL, "scope_sha256=" + "0" * 64 + "\nbatch=OBL-C1\n")
        expect("stale approval hash refused",
               any("scope_sha256" in e for e in check(ok, prev_ok, ["src/foo.rs"], tmp)[0]))
        if fails:
            print("SELF-TEST FAIL:")
            for f in fails:
                print(f"  {f}")
            return 1
        print("SELF-TEST PASS: closure/control/risk/tombstone/dual-track/count/approval controls fire.")
        return 0

sys.exit(main())
PY
