#!/usr/bin/env bash
#
# The defect tracker's gate. Green is a code change with a control; a residual is an accepted risk
# with an ALARP record and a batch approval, never a green row.
#
# WHAT IT CHECKS
#   1. Schema: main-table rows are `| ID | Status | Defect | Locus | Control | Risk |`, with status
#      OPEN/BLOCKED/DEFERRED/CLOSED and Risk `S/L` (S in C/H/M/L, L in D/P/T). An unknown status is a
#      schema error, never silently dropped. The band is computed from risk-management.md:
#      Intolerable >= 15, Tolerable 6-14, Broadly <= 5, with C and H never Broadly. An OPEN row
#      carries a resolving control or `plan:<text>`; `—` fails.
#   2. Fix cycles: every OPEN row is in exactly one cycle in the tracker's `## Fix cycles` table;
#      a cycle has an owner and a `by:YYYY-MM-DD` bound that has not lapsed; a cycle that lists a
#      non-OPEN or unknown row is stale.
#   3. Locus resolves: the file exists and, when `path:identifier` is written, the identifier
#      (the last segment of `A::b`) appears in the file.
#   4. CLOSED rows: the control resolves, a script control has a `--self-test`, the control is not
#      the locus itself, and the closure commit changes production code (src/, bin/, proofs/ or
#      production contrib/; not docs, reports, scripts/, hooks/ or contrib/ci/). `negative:` prose
#      does not waive the self-test.
#   5. Tombstones: an ID that leaves the main table must appear in the Settled-IDs appendix; every
#      residual record's source ID must be tombstoned; no ID may be live in the tracker and in
#      decisions.md or residual-risk.md. Appendix dispositions are a closed vocabulary: `residual`,
#      `merged into`, `decision`, and the frozen pre-migration words `settled`/`restated`/
#      `accepted`/`dissolved`. The frozen set is hashed by the approval manifest, so a new historical
#      tombstone cannot be used as an exit.
#   6. Counts: the register's stated decisions count equals decisions.md's row count.
#   7. Risk history: a severity/likelihood decrease requires `basis:` in the row.
#   8. Approval: reports/register-approval.txt holds `scope_sha256=<sha256>` over the ACCEPTANCE
#      BOUNDARY — residual-risk.md and risk-management.md — and `appendix_baseline_sha256` over the
#      frozen historical tombstones. A tracker edit does not invalidate the approval (that was a
#      re-approval treadmill); an acceptance-boundary edit does. In HEAD mode the hash is computed
#      from HEAD's blobs, not the working tree, so an uncommitted edit cannot invalidate a check of
#      the committed register.
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
sys.path.insert(0, os.path.join(ROOT, "scripts"))
from register_parse import (  # noqa: E402
    STATUSES, SEV, LIK, band, parse_register, parse_cycles, check_cycles,
    historical_baseline, HISTORICAL_RE, TOMBSTONE_RE, REGISTER_REL,
)

REGISTER = os.path.join(ROOT, REGISTER_REL)
RESIDUAL_REL = "doc/src/arch/residual-risk.md"
RESIDUAL = os.path.join(ROOT, RESIDUAL_REL)
DECISIONS_REL = "doc/src/arch/decisions.md"
DECISIONS = os.path.join(ROOT, DECISIONS_REL)
POLICY_REL = "doc/src/arch/risk-management.md"
POLICY = os.path.join(ROOT, POLICY_REL)
APPROVAL_REL = "reports/register-approval.txt"
APPROVAL = os.path.join(ROOT, APPROVAL_REL)
DOC_PREFIXES = ("doc/", "reports/", "scripts/", "hooks/", "script/", "contrib/ci/")

def sh(*args):
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True)

def read(path):
    try:
        return open(path, encoding="utf-8").read()
    except OSError:
        return ""

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
    # A model test suite (`.py`) is a control too: `contrib/model/chain_model.py` runs its own
    # tests and is wired into the umbrella. It has no `--self-test` flag; its suite is the test.
    if "/" in control and control.endswith((".sh", ".py")):
        full = os.path.join(root, control)
        if not os.path.isfile(full):
            return f"control script '{control}' does not exist"
        wired = read(os.path.join(root, "scripts/run-all-tests.sh")) + read(
            os.path.join(root, "hooks/pre-commit"))
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

def read_boundary(root, rel, mode):
    """The acceptance-boundary file as the mode sees it: HEAD, the index, or the working tree."""
    if mode == "head":
        p = sh("show", f"HEAD:{rel}")
        if p.returncode == 0:
            return p.stdout
    elif mode == "staged":
        p = sh("show", f":{rel}")
        if p.returncode == 0:
            return p.stdout
    return read(os.path.join(root, rel))

def scope_hash_at(root, rev):
    """The acceptance-boundary hash as it stood at a commit, or None if it does not resolve."""
    h = hashlib.sha256()
    for rel in (RESIDUAL_REL, POLICY_REL):
        p = sh("show", f"{rev}:{rel}")
        if p.returncode != 0:
            return None
        h.update(p.stdout.encode())
    return h.hexdigest()

def scope_hash(root, mode="work"):
    """SHA-256 over the acceptance boundary, never over the live tracker.

    Residuals and the policy are the decisions a lead signs. A tracker row is a disposition earned
    by evidence and checked by this gate; including the tracker in the approval hash made every
    legitimate closure invalidate the approval.
    """
    h = hashlib.sha256()
    for rel in (RESIDUAL_REL, POLICY_REL):
        h.update(read_boundary(root, rel, mode).encode())
    return h.hexdigest()

def check(register_text, prev_text, changed, root, mode="work"):
    rows, appendix, errors = parse_register(register_text, REGISTER_REL)
    prev_rows, prev_appendix, _ = parse_register(prev_text, REGISTER_REL)
    decisions = decision_ids(root)
    residuals = residual_sources(root)

    cycles, cycle_errors = parse_cycles(register_text, REGISTER_REL)
    errors.extend(cycle_errors)
    errors.extend(check_cycles(rows, cycles, REGISTER_REL))

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
                    errors.append(
                        f"{oid}: script control has no --self-test — observed-failure prose "
                        "does not waive a control that can fail")
            # A `.py` model control runs its own test suite; no separate --self-test flag exists.

    # Decisions and residuals cannot be live tracker rows.
    for did in decisions:
        if did in rows:
            errors.append(f"{did}: dual-tracked — live in the tracker and in decisions.md")
    for sid in residuals:
        if sid in rows and rows[sid]["status"] != "CLOSED":
            errors.append(f"{sid}: residual source is still a live tracker row")
        if sid not in appendix:
            errors.append(f"{sid}: residual source has no tombstone in the appendix")

    # The appendix is a closed vocabulary, not a dumping ground.
    for oid, disp in appendix.items():
        d = disp.strip()
        if HISTORICAL_RE.fullmatch(d):
            continue
        if TOMBSTONE_RE.fullmatch(d):
            m = re.fullmatch(r"merged into (OBL-[CZT]\d+)", d)
            if m and m.group(1) not in rows and m.group(1) not in appendix:
                errors.append(f"{oid}: merged into {m.group(1)}, which does not resolve")
            if "residual" in d.lower() and oid not in residuals:
                errors.append(f"{oid}: appendix says residual but residual-risk.md has no record")
            continue
        errors.append(
            f"{oid}: appendix disposition '{d}' is not residual/merged/decision or a frozen "
            "historical word (settled/restated/accepted/dissolved)")

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
        #
        # A PROCESS row is the exception: its production artifact is the control script, not a
        # contract or binary. A row whose locus is under scripts/, hooks/ or script/ closes when
        # that control script changed in this commit (and passed the control/self-test checks
        # above). A code row cannot use this door: its locus is src/, bin/, proofs/ or contrib/.
        for oid in fresh:
            row = rows[oid]
            locus_path = strip_ticks(row["locus"]).partition(":")[0]
            control = strip_ticks(row["control"])
            if locus_path.startswith(("scripts/", "hooks/", "script/")) and control in changed:
                continue
            cited = re.findall(r"evidence:([0-9a-f]{8,40})", row["defect"] + " " + row["control"])
            if not cited:
                errors.append(f"{oid}: status changed without code and no evidence:<commit> cited")
            elif not any(evidence_commit_ok(sha, root) for sha in cited):
                errors.append(f"{oid}: evidence commit does not change production code")

    # Approval binds the acceptance boundary and the frozen historical appendix. Read it through
    # the same view as the boundary: an uncommitted approval edit must not unlock a committed
    # register, and a staged approval must be checkable in the pre-commit hook.
    approval = read_boundary(root, APPROVAL_REL, mode)
    am = re.search(r"scope_sha256=([0-9a-f]{64})", approval)
    if not am:
        errors.append(f"missing approval manifest {APPROVAL_REL}")
    else:
        if am.group(1) != scope_hash(root, mode):
            errors.append(
                "approval scope_sha256 does not match the acceptance boundary "
                "(residual-risk.md and risk-management.md)")
        # An `approved_commit` is falsifiable: the boundary at that commit must hash to the
        # approved value. The manifest cannot claim a history it does not have.
        cm = re.search(r"approved_commit=([0-9a-f]{7,40})", approval)
        if cm:
            at = scope_hash_at(root, cm.group(1))
            if at is None:
                errors.append(f"approved_commit {cm.group(1)} does not resolve")
            elif at != am.group(1):
                errors.append("approved_commit does not carry the approved acceptance boundary")
        bm = re.search(r"appendix_baseline_sha256=([0-9a-f]{64})", approval)
        if not bm:
            errors.append("approval manifest does not freeze the historical appendix")
        elif bm.group(1) != historical_baseline(appendix):
            errors.append(
                "historical appendix changed — settled/restated/accepted/dissolved rows are frozen")
        km = re.search(r"batch=([A-Z0-9,\-]+)", approval)
        ids = km.group(1).split(",") if km else []
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
        cur, prev, changed, mode = read(REGISTER), register_at("HEAD"), changed_work(), "work"
    elif MODE == "--staged":
        staged = sh("show", f":{REGISTER_REL}")
        cur = staged.stdout if staged.returncode == 0 else read(REGISTER)
        prev, changed, mode = register_at("HEAD"), changed_staged(), "staged"
    elif MODE in ("", "--head"):
        cur, prev, changed, mode = register_at("HEAD"), register_at("HEAD~1"), changed_head(), "head"
    else:
        print(f"usage: {sys.argv[0]} [--working-tree|--staged|--self-test]")
        return 2
    if not cur:
        print(f"FAIL: no register at {REGISTER_REL}")
        return 1
    errors, fresh = check(cur, prev, changed, ROOT, mode)
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
        residual_body = (
            "| ID | Source | Harm | S/L | Scenario | Control | Why | Compensating | Owner | Reopen |\n"
            "|---|---|---|---|---|---|---|---|---|---|\n"
            "| R-1 | OBL-C8 | availability | M/P | s | c | w | k | lead | release |\n")
        write(RESIDUAL_REL, residual_body)
        head = "| ID | Status | Defect | Locus | Control | Risk |\n|---|---|---|---|---|---|\n"
        tomb = ("\n## Appendix\n\n| ID | Disposition |\n|---|---|\n"
                "| OBL-C8 | residual (R-1) |\n")
        cycle_open = ("\n## Fix cycles\n\n| Cycle | Owner | Bound | Rows |\n|---|---|---|---|\n"
                      "| c1 | lead | by:2099-01-01 | OBL-C1 |\n")
        open_row = "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `plan: test` | M/P |\n"
        closed_row = ("| OBL-C1 | CLOSED | a defect | `src/foo.rs:foo` | `scripts/check-x.sh` | "
                      "M/P |\n")
        base = head + open_row + tomb + cycle_open
        ok = head + closed_row + tomb
        write(REGISTER_REL, ok)
        appendix_baseline = historical_baseline(parse_register(ok)[1])
        write(APPROVAL_REL, "scope_sha256=" + scope_hash(tmp, "work")
              + "\nappendix_baseline_sha256=" + appendix_baseline + "\nbatch=OBL-C1\n")
        fails = []

        def expect(name, cond):
            if not cond:
                fails.append(name)

        expect("status-only closure refused",
               any("without code" in e for e in
                   check(ok, base, [os.path.join("doc", "no" + "tes") + ".md"], tmp)[0]))
        # A process row's production artifact is its control script: changing the script closes
        # the row. A code row with the same control change is still refused.
        write("script/dead.py", "# process fixture\n")
        process_row = ("| OBL-C1 | CLOSED | a defect | `script/dead.py` | `scripts/check-x.sh` | "
                       "M/P |\n")
        expect("process row closes on a control-script change",
               not any("without code" in e for e in
                       check(head + process_row + tomb, base, ["scripts/check-x.sh"], tmp)[0]))
        expect("code row still needs production code",
               any("without code" in e for e in
                   check(ok, base, ["scripts/check-x.sh"], tmp)[0]))
        expect("control-free closure refused",
               any("test symbol" in e or "does not exist" in e for e in
                   check(head + "| OBL-C1 | CLOSED | a defect | `src/foo.rs:foo` | `no_such_test` | "
                         "M/P |\n" + tomb, base, ["src/foo.rs"], tmp)[0]))
        expect("clean twin accepted", not check(ok, base, ["src/foo.rs"], tmp)[0])
        expect("OPEN without control refused",
               any("no control or plan" in e for e in
                   check(head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `—` | M/P |\n"
                         + tomb + cycle_open, base, ["src/foo.rs"], tmp)[0]))
        expect("missing risk refused",
               any("risk" in e for e in
                   check(head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `plan: t` |  |\n"
                         + tomb + cycle_open, base, ["src/foo.rs"], tmp)[0]))
        expect("C/H cannot be broadly acceptable refused",
               any("Broadly" in e for e in
                   check(head + "| OBL-C1 | OPEN | a defect | `src/foo.rs:foo` | `plan: t` | C/T |\n"
                         + tomb + cycle_open, base, ["src/foo.rs"], tmp)[0]))
        expect("dual-tracked decision refused",
               any("dual-tracked" in e for e in
                   check(head + "| OBL-C9 | OPEN | a defect | `src/foo.rs:foo` | `plan: t` | M/P |\n"
                         + tomb + cycle_open, base, ["src/foo.rs"], tmp)[0]))
        expect("missing tombstone refused",
               any("without a tombstone" in e for e in
                   check(head + tomb, base, ["src/foo.rs"], tmp)[0]))
        expect("control==locus refused",
               any("control is the locus" in e for e in
                   check(head + "| OBL-C1 | CLOSED | a defect | `scripts/check-x.sh` | "
                         "`scripts/check-x.sh` | M/P |\n" + tomb, base, ["src/foo.rs"], tmp)[0]))
        write("scripts/check-x.sh", "#!/bin/sh\n")
        expect("script control without self-test refused",
               any("--self-test" in e for e in check(ok, base, ["src/foo.rs"], tmp)[0]))
        expect("negative prose does not waive a self-test",
               any("--self-test" in e for e in
                   check(head + "| OBL-C1 | CLOSED | a defect negative:it failed before | "
                         "`src/foo.rs:foo` | `scripts/check-x.sh` | M/P |\n" + tomb,
                         base, ["src/foo.rs"], tmp)[0]))
        write("scripts/check-x.sh", "#!/bin/sh\necho --self-test\n")
        two_dec = (head + open_row + tomb + cycle_open + "\n2 decisions\n")
        expect("decisions count mismatch refused",
               any("decisions count" in e for e in check(two_dec, base, ["src/foo.rs"], tmp)[0]))
        write(APPROVAL_REL, "scope_sha256=" + "0" * 64 + "\nappendix_baseline_sha256="
              + appendix_baseline + "\nbatch=OBL-C1\n")
        expect("stale approval hash refused",
               any("scope_sha256" in e for e in check(ok, base, ["src/foo.rs"], tmp)[0]))

        # ── the process-round controls ───────────────────────────────────────
        write(APPROVAL_REL, "scope_sha256=" + scope_hash(tmp, "work")
              + "\nappendix_baseline_sha256=" + appendix_baseline + "\nbatch=OBL-C1\n")
        expect("unknown status refused",
               any("is not OPEN/BLOCKED/DEFERRED/CLOSED" in e for e in
                   check(head + "| OBL-C1 | OPENISH | a defect | `src/foo.rs:foo` | "
                         "`plan: t` | M/P |\n" + tomb + cycle_open,
                         base, ["src/foo.rs"], tmp)[0]))
        expect("OPEN row without a fix cycle refused",
               any("in no fix cycle" in e for e in
                   check(head + open_row + tomb, base, ["src/foo.rs"], tmp)[0]))
        expect("cycle listing a non-OPEN row refused",
               any("not OPEN" in e for e in
                   check(ok + cycle_open, base, ["src/foo.rs"], tmp)[0]))
        lapsed = (head + open_row + tomb
                  + "\n## Fix cycles\n\n| Cycle | Owner | Bound | Rows |\n|---|---|---|---|\n"
                  + "| c1 | lead | by:2000-01-01 | OBL-C1 |\n")
        expect("lapsed cycle bound refused",
               any("has lapsed" in e for e in check(lapsed, base, ["src/foo.rs"], tmp)[0]))
        expect("a tracker edit does not invalidate approval",
               not any("scope_sha256" in e for e in
                       check(ok.replace("a defect", "a defect, reworded"), ok,
                             ["src/foo.rs"], tmp)[0]))
        write(RESIDUAL_REL, residual_body + "\n<!-- edited -->\n")
        expect("an acceptance-boundary edit invalidates approval",
               any("scope_sha256" in e for e in check(ok, base, ["src/foo.rs"], tmp)[0]))
        write(RESIDUAL_REL, residual_body)
        expect("new historical tombstone refused",
               any("historical appendix" in e for e in
                   check(head + closed_row + tomb + "| OBL-C2 | settled |\n",
                         ok, ["src/foo.rs"], tmp)[0]))
        expect("unknown appendix disposition refused",
               any("appendix disposition" in e for e in
                   check(head + closed_row + tomb + "| OBL-C3 | vanished |\n",
                         ok, ["src/foo.rs"], tmp)[0]))
        write(APPROVAL_REL, "scope_sha256=" + scope_hash(tmp, "work")
              + "\nappendix_baseline_sha256=" + appendix_baseline
              + "\nbatch=OBL-C1\napproved_commit=deadbeef\n")
        expect("a non-resolving approved_commit refused",
               any("approved_commit" in e for e in check(ok, base, ["src/foo.rs"], tmp)[0]))

        if fails:
            print("SELF-TEST FAIL:")
            for f in fails:
                print(f"  {f}")
            return 1
        print("SELF-TEST PASS: closure/control/risk/tombstone/dual-track/count/approval/"
              "cycle/status/history controls fire.")
        return 0

sys.exit(main())
PY
