#!/usr/bin/env bash
#
# The heavyweight coverage TABLE, checked against the tree it describes.
#
# WHY THIS EXISTS. `doc/src/dev/testing/level-2-heavyweight.md` is the document a reader consults to
# learn which contracts have heavyweight coverage, and until 2026-10-08 **no gate read it**. It had
# drifted: its `STUB` row read `drain_protection (0/9), game_room (0/12), slot (0/4)` — "All
# `empty_witnesses`" — while those specs carried real rows (measured: 4/9, 11/12, 5/5) and
# `drain_protection_spec.rs` had said "Every endpoint's proof is real" since `OBL-C88`. A reader was
# told three contracts were unverified when they were not, which is a verification record that lies
# in the *safe-looking* direction — the same class as a green line that means nothing.
#
# WHAT IT CHECKS, and only this. `STUB` iff a contract's spec carries a row for **none** of its
# function-enum variants — the tier's own definition, made checkable in both directions: a contract
# declared STUB must have zero coverage, and a contract with zero coverage must be declared. The
# other tiers (`FULL`, `HARVESTABLE`, `UNDERPOWERED`) are left alone: they are judgments about
# *which* endpoints are covered, not a count, and the tree cannot settle them. `tender`'s movement
# from `FULL` to `HARVESTABLE` was made by hand, from its spec header, not by this gate.
#
# WHAT IT DOES NOT ESTABLISH (R7). `covered()` is name-appearance — a variant counts when its name or
# its snake_case form occurs in the spec — exactly the proxy `check_heavyweight_coverage.sh` uses and
# documents. So this gate proves **the table agrees with the spec files**, not that the spec exercises
# the variant. A spec could name a variant in a comment and satisfy it. It is narrower than the
# coverage claim it protects, and it says so here rather than implying otherwise.
#
# The decision logic is `check(doc_text, coverage)`, a pure function of two strings/mappings, so its
# negative control is a millisecond self-test over planted inputs and not a build. The sibling gate
# (`check_heavyweight_coverage.sh`) covers the genesis contracts' *coverage*; this one covers the
# document's *claims*. Extending the sibling was rejected: it is green, its `DECLARED` list is keyed
# to nine contracts, and widening its scope in place would put a green gate's verdict at risk.
#
# Usage: contrib/ci/check-coverage-table.sh
#        contrib/ci/check-coverage-table.sh --self-test   (planted contradictions; requires non-zero)
# Exit status: 1 on a contradiction; 0 otherwise. Also fails if the table has no STUB row at all.

set -uo pipefail

ROOT="${COVERAGE_TABLE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
export COVERAGE_TABLE_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, pathlib, re, sys

ROOT = pathlib.Path(os.environ["COVERAGE_TABLE_ROOT"])
DOC = ROOT / "doc" / "src" / "dev" / "testing" / "level-2-heavyweight.md"


def doc_stub_members(doc_text):
    """The contracts the tier table lists under STUB — `[]` for an explicit `(none)`, `None` when the
    table has no STUB row at all (which is its own failure: the tier must be expressible)."""
    for line in doc_text.splitlines():
        cells = line.split("|")
        if len(cells) >= 4 and cells[1].strip() == "STUB":
            cell = cells[3].strip()
            if cell.startswith("(none)") or cell in ("", "—", "-"):
                return []
            return [re.sub(r"\s*\(.*\)\s*$", "", p).strip() for p in cell.split(",") if p.strip()]
    return None


def enum_variants(lib_text):
    """A contract's function-enum variant names, in both declared forms (macro and `pub enum`)."""
    m = re.search(r'define_contract_function!\(\s*\w+\s*\{(.*?)\}\);', lib_text, re.S)
    body = m.group(1) if m else ""
    if not body:
        m = re.search(r'pub enum \w+Function\b[^{]*\{(.*?)\n\}', lib_text, re.S)
        body = m.group(1) if m else ""
    return re.findall(r'^\s*([A-Z][A-Za-z0-9_]*)\s*=', body, re.M)


def snake(v):
    return re.sub(r'(?<!^)(?=[A-Z])', '_', v).lower()


def coverage(root):
    """contract -> the count of its function-enum variants whose name appears in its own spec."""
    cdir, sdir = root / "src/contract", root / "bin/dwowd/src/tests/specs"
    out = {}
    for spec in sorted(sdir.glob("*_spec.rs")):
        c = spec.name[: -len("_spec.rs")]
        lib = cdir / c / "src" / "lib.rs"
        if not lib.exists():
            continue
        vs = enum_variants(lib.read_text())
        if not vs:
            continue
        st = spec.read_text()
        out[c] = sum(
            1 for v in vs
            if re.search(r'\b%s\b|\b%s\b' % (re.escape(v), re.escape(snake(v))), st)
        )
    return out


def check(doc_text, cov):
    """Pure: a document's STUB claims against a coverage map. Returns a list of problems."""
    declared = doc_stub_members(doc_text)
    if declared is None:
        return ["the tier table carries no `STUB` row — the tier must be expressible, even when empty"]
    problems = []
    for c in declared:
        if c not in cov:
            problems.append(f"STUB lists {c!r}, which has no spec+lib pair to measure")
        elif cov[c] > 0:
            problems.append(
                f"STUB lists {c!r}, but its spec covers {cov[c]} of its variants — "
                f"the table claims a contract the tree contradicts"
            )
    for c, n in sorted(cov.items()):
        if n == 0 and c not in declared:
            problems.append(f"{c!r} covers 0 variants and is not declared STUB")
    return problems


def self_test():
    """Two controls, each of which must fire — an instrument that cannot fail is the defect."""
    real_cov = coverage(ROOT)
    if not real_cov:
        print("FAIL: --self-test found no contracts to measure")
        return 1

    # Control 1 — a STUB membership the tree contradicts. `drain_protection` is the historical case.
    victim = "drain_protection" if real_cov.get("drain_protection", 0) > 0 else sorted(real_cov)[0]
    planted = f"| STUB | 1 | {victim} | planted |\n"
    p1 = check(planted, real_cov)
    if not any(victim in p for p in p1):
        print(f"FAIL: --self-test declared the covered contract {victim!r} as STUB and it was not reported")
        return 1
    print(f"OK: --self-test — a STUB claim the tree contradicts is reported ({victim})")

    # Control 2 — the converse: zero coverage, undeclared.
    synthetic = dict(real_cov)
    synthetic["PlantedStubContract"] = 0
    p2 = check("| STUB | 0 | (none) | planted |\n", synthetic)
    if not any("PlantedStubContract" in p for p in p2):
        print("FAIL: --self-test left a zero-coverage contract undeclared and it was not reported")
        return 1
    print("OK: --self-test — a zero-coverage contract left undeclared is reported")

    # Control 3 — a table with no STUB row must fail rather than pass vacuously.
    if not check("| Tier | Count |\n|---|---|\n", real_cov):
        print("FAIL: --self-test passed a table with no STUB row")
        return 1
    print("OK: --self-test — a table with no STUB row is refused rather than passed vacuously")
    return 0


if "--self-test" in sys.argv:
    sys.exit(self_test())

if not DOC.exists():
    print(f"FAIL: {DOC} does not exist")
    sys.exit(1)
cov = coverage(ROOT)
problems = check(DOC.read_text(), cov)
stub = doc_stub_members(DOC.read_text())
print(f"COVERAGE: measured {len(cov)} contract(s) from their function enums and specs.")
print(f"  the table declares {len(stub)} STUB contract(s): {', '.join(stub) if stub else '(none)'}")
zero = sorted(c for c, n in cov.items() if n == 0)
print(f"  contracts with zero covered variants: {', '.join(zero) if zero else '(none)'}")
if problems:
    for p in problems:
        print(f"FAIL: {p}")
    print(f"\n[FAIL] {len(problems)} contradiction(s) between the tier table and the tree.")
    sys.exit(1)
print("\n[PASS] the tier table's STUB claims agree with the tree — declared and measured.")
PY
