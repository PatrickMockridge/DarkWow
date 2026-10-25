#!/usr/bin/env bash
#
# The verification preflight: a verification surface may not issue a verdict while a finding is
# undecided.
#
# The register has two decided states — CLOSED (the defect is impossible in current code, with a
# failing-before control) and RESIDUAL (an accepted risk with a signed ALARP record). OPEN, BLOCKED
# and DEFERRED are undecided. A Docker run, a pipeline pass, or an umbrella summary computed over an
# undecided finding answers a question nobody asked, and the answer is worthless: it verifies code
# the register calls defective.
#
# src/arch/risk-management.md §10 and src/dev/hazop-rules.md ("The binary finding lifecycle") are
# normative. There is no override flag.
#
# Usage:
#   scripts/check-tracker-settled.sh                      # read the working tree
#   scripts/check-tracker-settled.sh --ref <git-rev>      # read the register at a revision
#   scripts/check-tracker-settled.sh --ref <rev> --require-clean
#                                                         # also require HEAD == rev and no
#                                                         # uncommitted tracker edits (the pipeline
#                                                         # builds origin/<branch>, so the tracker
#                                                         # it is checked against must be that one)
#   scripts/check-tracker-settled.sh --register <path>    # a file (self-test fixtures)
#   scripts/check-tracker-settled.sh --self-test
#
# Exit: 0 settled, 1 undecided or a register/schema error, 2 usage/IO.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

exec python3 - "$ROOT" "$@" <<'PY'
import os, subprocess, sys, tempfile

ROOT = sys.argv[1]
ARGS = sys.argv[2:]
sys.path.insert(0, os.path.join(ROOT, "scripts"))
from register_parse import (  # noqa: E402
    BLOCKING, STATUSES, parse_register, risk_band, REGISTER_REL,
)

def usage():
    print("usage: check-tracker-settled.sh [--ref REV [--require-clean]] "
          "[--register PATH] [--self-test]")
    return 2

def sh(*args):
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True)

def read(path):
    try:
        return open(path, encoding="utf-8").read()
    except OSError:
        return ""

def register_at(rev):
    p = sh("show", f"{rev}:{REGISTER_REL}")
    if p.returncode != 0:
        return None, f"cannot read {REGISTER_REL} at '{rev}' (push first, or fetch the remote)"
    return p.stdout, None

def verdict(text, subject):
    rows, appendix, errors = parse_register(text, REGISTER_REL)
    if errors:
        print(f"REFUSED: the tracker does not parse ({subject}) — a schema error is not settled.")
        for e in errors:
            print(f"  {e}")
        return 1
    blocking = [(oid, r) for oid, r in rows.items() if r["status"] in BLOCKING]
    if blocking:
        print(f"REFUSED: verification requires zero undecided findings ({subject}).")
        for oid, r in sorted(blocking):
            band = risk_band(f"{r['sev']}/{r['lik']}")
            locus = r["locus"].strip("` ")
            print(f"  {oid}  {r['status']:<8} {r['sev']}/{r['lik']} {band:<20} {locus}")
        n = {s: sum(1 for r in rows.values() if r["status"] == s) for s in STATUSES}
        n_res = sum(1 for d in appendix.values() if d.strip().startswith("residual"))
        print(f"{len(blocking)} undecided ({n['OPEN']} OPEN, {n['BLOCKED']} BLOCKED, "
              f"{n['DEFERRED']} DEFERRED); {n['CLOSED']} CLOSED, {n_res} RESIDUAL.")
        print("Legal exits: CLOSED with a failing-before control, or RESIDUAL with an ALARP record.")
        print("No override exists.")
        return 1
    n = {s: sum(1 for r in rows.values() if r["status"] == s) for s in STATUSES}
    n_res = sum(1 for d in appendix.values() if d.strip().startswith("residual"))
    print(f"PASS: tracker settled — {n['OPEN']} open, {n['BLOCKED']} blocked, "
          f"{n['DEFERRED']} deferred, {n['CLOSED']} closed, {n_res} residual ({subject}).")
    return 0

def self_test():
    head = "| ID | Status | Defect | Locus | Control | Risk |\n|---|---|---|---|---|---|\n"
    cases = {
        "open": (head + "| OBL-T1 | OPEN | d | `src/x.rs` | `plan: p` | C/P |\n", 1, "OBL-T1"),
        "blocked": (head + "| OBL-T1 | BLOCKED | d | `src/x.rs` | blocked: upstream | M/P |\n",
                    1, "OBL-T1"),
        "deferred": (head + "| OBL-T1 | DEFERRED | d | `src/x.rs` | deferred:release | M/P |\n",
                     1, "OBL-T1"),
        "closed": (head + "| OBL-T1 | CLOSED | d | `src/x.rs` | `test_x` | M/P |\n", 0, "PASS"),
        "residual-only": ("\n## Appendix\n\n| ID | Disposition |\n|---|---|\n"
                          "| OBL-T1 | residual (R-1) |\n", 0, "PASS"),
        "unknown-status": (head + "| OBL-T1 | OPENISH | d | `src/x.rs` | `plan: p` | M/P |\n",
                           1, "OPENISH"),
    }
    with tempfile.TemporaryDirectory() as tmp:
        fails = []
        for name, (body, want_rc, want_text) in cases.items():
            path = os.path.join(tmp, name + ".md")
            open(path, "w").write(body)
            import io
            from contextlib import redirect_stdout
            buf = io.StringIO()
            with redirect_stdout(buf):
                rc = verdict(read(path), f"fixture {name}")
            out = buf.getvalue()
            if rc != want_rc or want_text not in out:
                fails.append(f"{name}: rc={rc} want={want_rc} text='{want_text}' out={out!r}")
        if fails:
            print("SELF-TEST FAIL:")
            for f in fails:
                print(f"  {f}")
            return 1
        print("SELF-TEST PASS: the preflight refuses OPEN/BLOCKED/DEFERRED and an unknown status, "
              "and passes a closed or residual-only tracker.")
        return 0

def main():
    if "--self-test" in ARGS:
        return self_test()
    ref = None
    register = None
    require_clean = False
    i = 0
    while i < len(ARGS):
        a = ARGS[i]
        if a == "--ref" and i + 1 < len(ARGS):
            ref = ARGS[i + 1]; i += 2
        elif a == "--register" and i + 1 < len(ARGS):
            register = ARGS[i + 1]; i += 2
        elif a == "--require-clean":
            require_clean = True; i += 1
        else:
            return usage()
    if register:
        text, err = read(register), None
        subject = f"file {register}"
    elif ref:
        if require_clean:
            head = sh("rev-parse", "HEAD").stdout.strip()
            target = sh("rev-parse", ref).stdout.strip()
            if not target:
                print(f"REFUSED: cannot resolve '{ref}' — push the branch before verifying it.")
                return 1
            if head != target:
                print(f"REFUSED: HEAD ({head[:12]}) is not '{ref}' ({target[:12]}) — "
                      "the pipeline builds the pushed ref, so the tracker and code must be that ref.")
                return 1
            dirty = sh("status", "--porcelain", "--", REGISTER_REL).stdout.strip()
            if dirty:
                print(f"REFUSED: uncommitted tracker edits ({REGISTER_REL}) — commit and push "
                      "before verifying; a working-tree tracker is not what the pipeline builds.")
                return 1
        text, err = register_at(ref)
        subject = f"{REGISTER_REL} at {ref}"
    else:
        text, err = read(os.path.join(ROOT, REGISTER_REL)), None
        subject = "working tree"
        if require_clean:
            dirty = sh("status", "--porcelain", "--", REGISTER_REL).stdout.strip()
            if dirty:
                print(f"REFUSED: uncommitted tracker edits ({REGISTER_REL}) — a closure in the "
                      "working tree is not a committed disposition; commit it before verifying.")
                return 1
    if err:
        print(f"REFUSED: {err}")
        return 1
    if not text:
        print(f"REFUSED: no register at {REGISTER_REL}")
        return 1
    return verdict(text, subject)

sys.exit(main())
PY
