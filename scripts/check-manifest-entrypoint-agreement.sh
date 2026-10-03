#!/usr/bin/env bash
#
# The manifest a wallet reads from is the contract the chain runs.
#
# WHY THIS EXISTS. Three declarations of the same facts exist for every contract: the manifest's
# `[[functions]]` (`name`, `code`, `requires_proof`, `proof_circuit`), the contract's own function
# enum in `src/lib.rs`, and the entrypoint's dispatch arms. `check-circuit-metadata-alignment.sh`
# compares circuit <-> entrypoint <-> client; `check-client-params-alignment.sh` compares client <->
# model; the manifest is a third declaration compared to neither. This gate reads what nothing does
# (OBL-C155).
#
# WHAT IT CHECKS — and it is deliberately narrower than "all of C3", because a second check beside a
# working one is R2's defect rather than a repair. `src/contract/test-harness/tests/
# manifest_proof_declarations.rs` already implements OBL-C91 (A)–(D): `requires_proof = true`
# resolves to a circuit in `[[circuits]]`, `[[circuits]]` is a subset of `proof/*.zk`, and the built
# set is a subset of the declared one — over 31 contracts with exception tables. So this gate reads
# the readings OBL-C91 does NOT:
#   C1 the manifest's function-code set equals the enum's active-code set, both directions;
#   C2 for each shared code, the manifest `name` and the enum variant agree after a normalizer that
#      strips a trailing `_vN`/`vN`, splits camelCase, lowercases, then drops non-alphanumerics — so
#      `pow_reward` matches `PoWRewardV1` (an acronym defeats a naive split);
#   C3 a `requires_proof = true` function has an entrypoint dispatch arm for its variant (the
#      reading OBL-C91 does not make: it checks the *circuit*, not the *dispatch*);
#   C4 a function with `requires_proof` absent or false declares no `proof_circuit`. The key is
#      `#[serde(default)]`, so absent and `= false` are one set — never require it to be present.
# Folding the circuit-side checks (C3(b)/(c)) in here would open RED on the five known-benign sites
# OBL-C91 already adjudicates; do not.
#
# `#[cfg(...)]`-attributed enum variants are skipped: they are not built in the default
# configuration, so they are not in the manifest and must not be required to be.
#
# Usage: scripts/check-manifest-entrypoint-agreement.sh
#        scripts/check-manifest-entrypoint-agreement.sh --self-test   (planted defect; non-zero exit)
# Exit status: 1 on any finding; 0 otherwise.

set -uo pipefail

ROOT="${MANIFEST_ENTRYPOINT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export MANIFEST_ENTRYPOINT_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, tomllib, pathlib, tempfile, shutil

ROOT = pathlib.Path(os.environ["MANIFEST_ENTRYPOINT_ROOT"])

def norm(s):
    s = re.sub(r'(?<=[a-z0-9])(?=[A-Z])', ' ', s)   # split camelCase
    s = re.sub(r'[^A-Za-z0-9]', '', s).lower()      # drop non-alphanumerics, lowercase
    s = re.sub(r'v\d+$', '', s)                     # strip a trailing vN
    return s

def enum_name(lib_text):
    for line in lib_text.splitlines():
        m = re.match(r'pub enum (\w+Function)\b', line) \
            or re.match(r'define_contract_function!\((\w+Function)', line)
        if m:
            return m.group(1)
    return None

def enum_codes(lib_text, enum):
    """Active code -> variant. Both declared forms; `#[cfg]`-attributed variants are skipped."""
    m = re.search(r'define_contract_function!\(\s*%s\s*\{(.*?)\}\);' % re.escape(enum), lib_text, re.S)
    body = m.group(1) if m else ""
    if not body:
        m = re.search(r'pub enum %s\b[^{]*\{(.*?)\n\}' % re.escape(enum), lib_text, re.S)
        body = m.group(1) if m else ""
    out, prev = {}, "ok"
    for line in body.splitlines():
        if re.match(r'\s*#\[cfg', line):
            prev = "cfg"; continue
        mm = re.match(r'\s*([A-Z][A-Za-z0-9_]*)\s*=\s*(0x[0-9a-fA-F]+|\d+)', line)
        if mm and prev != "cfg":
            out[int(mm.group(2), 0)] = mm.group(1)
        prev = "ok"
    return out

def check(root):
    findings, contracts, functions = [], 0, 0
    for man in sorted(root.glob("src/contract/*/manifest.toml")):
        d = man.parent; name = d.name
        contracts += 1
        try:
            M = tomllib.loads(man.read_text())
        except Exception as e:
            findings.append((name, f"manifest does not parse: {e}")); continue
        funcs = M.get("functions", [])
        functions += len(funcs)
        libp = d / "src" / "lib.rs"
        if not libp.exists():
            findings.append((name, "no src/lib.rs")); continue
        lib = libp.read_text()
        enum = enum_name(lib)
        if not enum:
            findings.append((name, "no function enum in src/lib.rs")); continue
        codes = enum_codes(lib, enum)
        allsrc = "\n".join(p.read_text() for p in (d / "src").rglob("*.rs"))
        arms = set(re.findall(r'%s::(\w+)\s*=>' % re.escape(enum), allsrc))
        man_codes = {f["code"] for f in funcs}
        if man_codes != set(codes):
            findings.append((name,
                f"C1 manifest codes differ from the enum: manifest-only={sorted(man_codes - set(codes))}, "
                f"enum-only={sorted(set(codes) - man_codes)}"))
        for f in funcs:
            c = f["code"]; v = codes.get(c)
            if v and norm(f["name"]) != norm(v):
                findings.append((name, f"C2 code {c}: manifest {f['name']!r} vs enum {v!r}"))
            rp = f.get("requires_proof", False); pc = f.get("proof_circuit")
            if rp and v and v not in arms:
                findings.append((name, f"C3 {f['name']!r} requires a proof but has no dispatch arm for {v}"))
            if not rp and pc:
                findings.append((name, f"C4 {f['name']!r} does not require a proof but declares circuit {pc!r}"))
    return contracts, functions, findings

def main():
    if "--self-test" in sys.argv:
        with tempfile.TemporaryDirectory() as tmp:
            t = pathlib.Path(tmp)
            src = pathlib.Path(os.environ["MANIFEST_ENTRYPOINT_ROOT"])
            c = "dao_escrow"
            shutil.copytree(src / "src" / "contract" / c, t / "src" / "contract" / c,
                            ignore=shutil.ignore_patterns("proof"))
            man = t / "src" / "contract" / c / "manifest.toml"
            base = man.read_text()

            # Control 1 — C1: give a function a code the enum does not assign.
            man.write_text(base.replace('name = "pay_premium"\ncode = 2',
                                        'name = "pay_premium"\ncode = 12', 1))
            _, _, f1 = check(t)
            if not any(x[1].startswith("C1") for x in f1):
                print("FAIL: --self-test gave pay_premium an unassigned code and C1 did not fire")
                return 1
            print("OK: --self-test — C1 reports a manifest code with no enum variant")

            # Control 2 — C3: a proof-bearing function whose variant has no dispatch arm.
            lib = t / "src" / "contract" / c / "src" / "lib.rs"
            lib.write_text(lib.read_text().replace("CancelClaimV1 = 0x0d,",
                                                   "CancelClaimV1 = 0x0d,\n    ProbeReportV1 = 0x0e,", 1))
            man.write_text(base + '\n[[functions]]\nname = "probe_report"\ncode = 14\n'
                                  'requires_proof = true\nproof_circuit = "ProbeReportV2"\n')
            _, _, f2 = check(t)
            if not any(x[1].startswith("C3") for x in f2):
                print("FAIL: --self-test added a requires_proof function with no dispatch arm and C3 did not fire")
                return 1
            print("OK: --self-test — C3 reports a proof-bearing function with no dispatch arm")
        return 0

    root = pathlib.Path(os.environ["MANIFEST_ENTRYPOINT_ROOT"])
    contracts, functions, findings = check(root)
    for name, msg in findings:
        print(f"FAIL: {name}: {msg}")
    tag = "PASS" if not findings else "FAIL"
    print(f"[{tag}] contracts={contracts} functions={functions} findings={len(findings)}")
    return 1 if findings else 0

sys.exit(main())
PY
