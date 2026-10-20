#!/usr/bin/env bash
#
# G1/G3's syntactic half: no denylisted ambient-authority call in the genesis functions.
#
# WHAT THIS IS. `genesis.md:155-173` requires the ceremony to be a pure function: no clock, RNG,
# network, or undeclared file. The report's G1/G3 are readings because no instrument decided them.
# This scanner extracts five function bodies by name and reports a denylisted call inside one:
#
#   bin/dwowd/src/lib.rs: build_genesis_deployment_txs, build_genesis_block, init_genesis
#   src/linear/src/execution.rs: apply_genesis_deployments
#   bin/dwowd/src/registry/model.rs: build_linear_coinbase_effective
#
# A file-scoped grep is not the instrument: `model.rs:744` has `SystemTime::now()` in the mining
# template, which is not reachable from genesis, and a file grep would flag it. The function body is
# the subject.
#
# WHAT IT DOES NOT ESTABLISH (R7). It is a *syntactic* check of the named functions. A call through
# an alias, a macro, a trait object, a dependency, or a WASM host function is invisible; it does not
# prove semantic purity, and it does not make G1's "no ambient authority" clause a theorem. It also
# does not follow callees: `init_genesis` calls code outside these five bodies, and this scanner does
# not build a call graph. It is evidence, not a proof.
#
# Usage: scripts/check-genesis-purity.sh [--self-test]
# Exit 0: no denylisted call in the five bodies. Exit 1: a finding, or a function that cannot be
# extracted (fail closed).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="${GENESIS_PURITY_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
export GENESIS_PURITY_ROOT="$ROOT"

python3 - "$@" <<'PYEOF'
import os, re, sys

ROOT = os.environ.get("GENESIS_PURITY_ROOT") or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
ROOT = os.path.abspath(ROOT)

TARGETS = {
    "bin/dwowd/src/lib.rs": ["build_genesis_deployment_txs", "build_genesis_block", "init_genesis"],
    "src/linear/src/execution.rs": ["apply_genesis_deployments"],
    "bin/dwowd/src/registry/model.rs": ["build_linear_coinbase_effective"],
}

DENY = [
    "SystemTime", "Instant::now", "OsRng", "thread_rng", "getrandom", "rand::",
    "std::fs", "File::open", "TcpStream", "TcpListener", "UdpSocket", "std::env", "env::var",
]


def function_body(src, name):
    """The body of `fn name`, brace-matched, ignoring braces in comments and string literals."""
    m = re.search(r"\bfn\s+" + re.escape(name) + r"\b", src)
    if not m:
        return None
    i = src.find("{", m.end())
    if i < 0:
        return None
    depth = 0
    j = i
    n = len(src)
    state = "code"
    while j < n:
        c = src[j]
        nxt = src[j + 1] if j + 1 < n else ""
        if state == "code":
            if c == "/" and nxt == "/":
                state = "line"
                j += 2
                continue
            if c == "/" and nxt == "*":
                state = "block"
                j += 2
                continue
            if c == '"':
                state = "str"
                j += 1
                continue
            if c == "'":
                # A char literal (not a lifetime): only if a closing quote is within two chars.
                if j + 2 < n and src[j + 2] == "'" and src[j + 1] != "\\":
                    j += 3
                    continue
                if j + 3 < n and src[j + 1] == "\\" and src[j + 3] == "'":
                    j += 4
                    continue
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    return src[i + 1:j]
            j += 1
        elif state == "line":
            if c == "\n":
                state = "code"
            j += 1
        elif state == "block":
            if c == "*" and nxt == "/":
                state = "code"
                j += 2
                continue
            j += 1
        elif state == "str":
            if c == "\\":
                j += 2
                continue
            if c == '"':
                state = "code"
            j += 1
    return None


def scan(root):
    findings = []
    scanned = 0
    for rel, names in TARGETS.items():
        path = os.path.join(root, rel)
        try:
            src = open(path, errors="replace").read()
        except OSError as e:
            findings.append(f"cannot read {rel}: {e}")
            continue
        for name in names:
            body = function_body(src, name)
            if body is None:
                findings.append(f"{rel}: fn {name} not found (fail closed)")
                continue
            scanned += 1
            for pat in DENY:
                if pat in body:
                    line = src[:src.find(body)].count("\n") + 1 + body[:body.find(pat)].count("\n")
                    findings.append(f"{rel}:{line}: fn {name} contains {pat}")
    return scanned, findings


if sys.argv[1:] == ["--self-test"]:
    # Plant a clock inside build_genesis_block in a temp copy of lib.rs; the clean copy must pass.
    rel = "bin/dwowd/src/lib.rs"
    src = open(os.path.join(ROOT, rel), errors="replace").read()
    m = re.search(r"\bfn\s+build_genesis_block\b", src)
    if not m:
        print("FAIL: --self-test: build_genesis_block not found")
        sys.exit(1)
    brace = src.find("{", m.end())
    planted = src[:brace + 1] + "\n    let _ = std::time::SystemTime::now();\n" + src[brace + 1:]
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        os.makedirs(os.path.join(td, os.path.dirname(rel)), exist_ok=True)
        # Copy the other targets unchanged so the scan reaches its real files.
        for other in TARGETS:
            dst = os.path.join(td, other)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            if other == rel:
                open(dst, "w").write(planted)
            else:
                open(dst, "w").write(open(os.path.join(ROOT, other), errors="replace").read())
        os.environ["GENESIS_PURITY_ROOT"] = td
        scanned, findings = scan(td)
    if not any("SystemTime" in f and "build_genesis_block" in f for f in findings):
        print("FAIL: --self-test: the planted clock was not reported"); print("\n".join(findings)); sys.exit(1)
    print("OK: --self-test — a planted clock in build_genesis_block is reported")
    scanned, findings = scan(ROOT)
    if findings:
        print("FAIL: --self-test: the real tree is not clean"); print("\n".join(findings)); sys.exit(1)
    print(f"OK: --self-test — the real tree is clean ({scanned} functions)")
    sys.exit(0)

scanned, findings = scan(ROOT)
if findings:
    for f in findings:
        print(f"FAIL: {f}")
    sys.exit(1)
print(f"PASS: genesis path has no denylisted ambient call ({scanned} functions)")
PYEOF
