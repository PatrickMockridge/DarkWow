#!/usr/bin/env bash
#
# Genesis endpoint coverage: every genesis function-enum variant is exercised through
# `accept_block` by that contract's own heavyweight spec.
#
# WHY THIS EXISTS. `heavyweight-spec.md` speaks of testing "every endpoint"; §4 is the list of
# patterns that make such a test a control that cannot fail. This gate is the coverage half — for
# each genesis contract, does the spec that carries its heavyweight test drive every variant of its
# function enum through the block path? The question is the same one `OBL-C139` names, and
# `OBL-C88`'s repair used this measure for `drain_protection`.
#
# WHAT IT READS NOW, AND WHY IT MOVED (REPAIR + REPOINT, 2026-10-03, OBL-C137). The old checker read
# a single `bin/dwowd/src/tests/heavyweight_pipeline.rs`, and its verdict was unsupported by its own
# code — a defect that left it "wired and blind" rather than merely "unwired":
#   * `check_variant_in_test` took the test-function name and never used it, grepping the whole
#     ~145 KB module, so "covered" meant "the name appears somewhere in the file";
#   * `check_has_accept_block` searched from the test function to the next `^fn ` line — a window
#     that, for the schema-mandated four-line wrapper, collapsed to the signature line, so it
#     returned `NO_ACCEPT_BLOCK` for **all nine** genesis contracts, including the two it marked OK.
# The heavyweight tests have since moved into one spec per contract
# (`bin/dwowd/src/tests/specs/<contract>_spec.rs`), driven by `uniform_runner`, which submits every
# endpoint through `accept_block`. The file *is* the contract's test now — so the search scope is
# this contract's spec rather than a shared module, and "exercised through accept_block" becomes
# "the variant appears in the spec, and the spec goes through the runner".
#
# THE DECLARED LIST EXPIRES, deliberately. The variants the tree does not exercise as endpoints are
# declared below with the reason each is not: a consensus coinbase path submitted by block assembly,
# an init hook, a host lookup driven as a *child* call. The gate fails if a declared variant
# *becomes* covered — the declaration is a debt, and adding the row is what pays it. An allowlist
# that never expires is the "instrument that cannot fail" defect with a longer half-life; this one
# expires. The count is not stated here: it was 38 on 2026-09-25 against the old file and 5 on
# 2026-10-03 against the spec tree, and a number in prose is what goes stale first.
#
# Usage: contrib/ci/check_heavyweight_coverage.sh
#        contrib/ci/check_heavyweight_coverage.sh --self-test   (planted gap; requires a non-zero exit)
# Exit status: 1 on an undeclared gap or a stale declaration; 0 otherwise.

set -uo pipefail

ROOT="${HEAVY_COVERAGE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
export HEAVY_COVERAGE_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, pathlib, tempfile, shutil

GENESIS = {
    "native_token":    ("NativeTokenFunction",    "native_token"),
    "identity":        ("IdentityFunction",       "identity"),
    "attestation":     ("AttestationFunction",    "attestation"),
    "multisig":        ("MultiSigFunction",       "multisig"),
    "oracle":          ("OracleFunction",         "oracle"),
    "promissory_note": ("PromissoryNoteFunction", "promissory_note"),
    "purse":           ("PurseFunction",          "purse"),
    "box":             ("BoxFunction",            "box"),
    "deployooor":      ("DeployFunction",         "deployooor"),
}

# The measured debt (re-measured 2026-10-03 against the spec tree): the variants each contract's spec
# leaves unexercised as an endpoint, with the reason each is not. expiry = the variant gaining a
# matching row (checked below, and a declared variant that IS covered now fails as stale).
DECLARED = {
    ("native_token", "PoWRewardV1"):
        "consensus coinbase (0x05), plaintext/no-proof — submitted by block assembly, not an endpoint call",
    ("native_token", "UncleMintV1"):
        "consensus uncle mint (0x07), plaintext/no-proof — submitted by the uncle path, not an endpoint call",
    ("identity", "InitializeV1"):
        "driven by the spec's `initialize` hook (`has_initialize: true`), not a named endpoint row",
    ("multisig", "InitializeV1"):
        "the fixture sets `has_initialize: false`; the variant is not driven as an endpoint",
    ("attestation", "CheckAttestationV1"):
        "non-ZK host lookup (0x0d) driven as a *child* call (e.g. `labor_market`'s create_job), not as attestation's own row",
}

def variants(lib_text, enum):
    """The enum's variant names, in both declared forms (macro and hand-written `pub enum`)."""
    m = re.search(r'define_contract_function!\(\s*%s\s*\{(.*?)\}\);' % re.escape(enum), lib_text, re.S)
    body = m.group(1) if m else ""
    if not body:
        m = re.search(r'pub enum %s\b[^{]*\{(.*?)\n\}' % re.escape(enum), lib_text, re.S)
        body = m.group(1) if m else ""
    return re.findall(r'^\s*([A-Z][A-Za-z0-9_]*)\s*=', body, re.M)

def snake(v):
    return re.sub(r'(?<!^)(?=[A-Z])', '_', v).lower()

def covered(variant, spec_text):
    """A variant is covered when its name or its snake_case form appears in the contract's spec.
    The spec file IS the contract's test, so the whole-file scope is the test's scope."""
    return bool(re.search(r'\b%s\b|\b%s\b' % (re.escape(variant), re.escape(snake(variant))), spec_text))

def check(root):
    cdir = root / "src" / "contract"
    sdir = root / "bin" / "dwowd" / "src" / "tests" / "specs"
    rows, gaps = [], []
    for c, (enum, base) in GENESIS.items():
        lib = cdir / c / "src" / "lib.rs"
        spec = sdir / (base + "_spec.rs")
        if not lib.exists():
            rows.append((c, "NO_LIB_FILE", 0, 0, 0)); continue
        vs = variants(lib.read_text(), enum)
        if not vs:
            rows.append((c, "ENUM_NOT_FOUND", 0, 0, 0)); continue
        if not spec.exists():
            rows.append((c, "NO_SPEC_FILE", 0, 0, 0)); continue
        st = spec.read_text()
        runner = "HAS_RUNNER" if "uniform_runner" in st else "NO_RUNNER"
        cov = decl = 0
        for v in vs:
            if covered(v, st):
                cov += 1
            elif (c, v) in DECLARED:
                decl += 1
            else:
                gaps.append((c, v))
        rows.append((c, runner, cov, len(vs), decl))
    stale = []
    for (c, v) in DECLARED:
        spec = sdir / (GENESIS[c][1] + "_spec.rs")
        if spec.exists() and covered(v, spec.read_text()):
            stale.append((c, v))
    return rows, gaps, stale

def main():
    if "--self-test" in sys.argv:
        with tempfile.TemporaryDirectory() as tmp:
            t = pathlib.Path(tmp)
            src_root = pathlib.Path(os.environ["HEAVY_COVERAGE_ROOT"])
            for c, (enum, base) in GENESIS.items():
                lib = src_root / "src" / "contract" / c / "src" / "lib.rs"
                spec = src_root / "bin" / "dwowd" / "src" / "tests" / "specs" / (base + "_spec.rs")
                if lib.exists():
                    d = t / "src" / "contract" / c / "src"; d.mkdir(parents=True, exist_ok=True)
                    shutil.copy(lib, d / "lib.rs")
                if spec.exists():
                    d = t / "bin" / "dwowd" / "src" / "tests" / "specs"; d.mkdir(parents=True, exist_ok=True)
                    shutil.copy(spec, d / spec.name)
            # Control 1 — plant an uncovered variant: the spec carries no row for it, so the checker
            # must report it as an undeclared gap and exit non-zero (not merely die for another reason).
            lib = t / "src" / "contract" / "native_token" / "src" / "lib.rs"
            txt = lib.read_text()
            anchor = "UncleMintV1 = 0x07,"
            if anchor not in txt:
                print("FAIL: --self-test could not find native_token's enum to plant into")
                return 1
            lib.write_text(txt.replace(anchor, anchor + "\n    PlantedProbeV1 = 0x09,", 1))
            _, gaps, _ = check(t)
            probe = [g for g in gaps if g[1] == "PlantedProbeV1"]
            if not probe:
                print("FAIL: --self-test planted an uncovered variant and the checker did not report it")
                return 1
            print(f"OK: --self-test — the planted uncovered variant is reported ({probe[0][0]}/{probe[0][1]})")

            # Control 2 — make a *declared* variant covered: the declaration must expire, so the
            # checker must report it stale. An allowlist that cannot expire is the defect this list
            # exists to avoid, so a control that required the report to *stop* would re-introduce it.
            lib.write_text(txt)
            spec = t / "bin" / "dwowd" / "src" / "tests" / "specs" / "native_token_spec.rs"
            spec.write_text(spec.read_text() + "\n// PoWRewardV1 — now exercised as a row\n")
            _, _, stale = check(t)
            if ("native_token", "PoWRewardV1") not in stale:
                print("FAIL: --self-test covered a declared variant and the checker did not report it stale")
                return 1
            print("OK: --self-test — a declared variant that became covered is reported stale")
        return 0

    root = pathlib.Path(os.environ["HEAVY_COVERAGE_ROOT"])
    rows, gaps, stale = check(root)
    total_f = sum(r[3] for r in rows)
    covered_f = sum(r[2] for r in rows)
    declared_f = sum(r[4] for r in rows)
    json_mode = "--json" in sys.argv
    if json_mode:
        import json
        print(json.dumps({"contracts": len(GENESIS), "total_functions": total_f,
                          "covered_functions": covered_f, "declared": declared_f,
                          "gaps": len(gaps), "stale": len(stale)}))
        return 1 if (gaps or stale) else 0
    for c, status, cov, tot, decl in rows:
        if status in ("NO_LIB_FILE", "ENUM_NOT_FOUND", "NO_SPEC_FILE"):
            print(f"[WARN] {c}: {status}")
        else:
            mark = "[OK] " if (tot - cov - decl) == 0 else "[GAP]"
            print(f"{mark} {c}: {cov}/{tot} covered, {decl} declared, runner={status}")
    for c, v in gaps:
        print(f"FAIL: {c}::{v} is not covered by its spec and is not declared")
    for c, v in stale:
        print(f"FAIL: the declared variant {c}::{v} is now covered — remove it and let the count fall")
    print("\n=== Coverage Summary ===")
    print(f"Contracts: {len(GENESIS)}")
    print(f"Total functions: {total_f}")
    print(f"Covered: {covered_f}")
    print(f"Declared: {declared_f}")
    print(f"Gaps: {len(gaps)}")
    print(f"Stale: {len(stale)}")
    if gaps or stale:
        print(f"\n[FAIL] {len(gaps)} undeclared gap(s), {len(stale)} stale declaration(s).")
        return 1
    print("\n[PASS] every genesis variant is covered or declared; no declaration is stale.")
    return 0

sys.exit(main())
PY
