#!/usr/bin/env python3
"""Check that each circuit's *compiled* artifact is the one the source analysis is about.

## Why this exists, and the gap it fills

**The gap is narrow and worth stating exactly, because the broader claims are already covered.**
`.zk.bin` is in `.gitignore` (`:9`) — zero tracked against the sources — so *staleness* cannot bite,
and two gates already say so: the Makefiles declare `proof/%.zk.bin: proof/%.zk`, and `ZK_SRC :=
$(wildcard proof/*.zk)` is in every contract's `SOURCE_MANIFEST`, so an edited circuit makes the
contract stale and the `contract artifact freshness` gate names it. `zkas validate` (behind the `ZK
binaries well-formed` gate) answers "is this a well-formed binary". None of those compares the
compiled circuit to the *source-level analysis the layer reasons with*, which is what this does.

One thing to be precise about, since the obvious version of the claim is wrong: the `ZK binaries
well-formed` gate walks `.zk.bin` files, so a tree with none of them makes it green without checking
anything — `Summary: 0 OK, 0 corrupted`, exit 0, re-measured 2026-09-24 on an empty directory, same
code path. In `scripts/run-all-tests.sh` that is mitigated by gate order, because `build contract ZK
circuits` runs immediately before it; it bites when the script is invoked alone on a tree whose
binaries have not been built. That gate now also reports its coverage, so the vacuity is visible
rather than silent. This script is immune by construction: it walks **sources**, which are tracked
and always present.

## What it checks

Every circuit the layer analyses — the same set `script/circuit_instance_derivation.py` walks, so
the two cannot drift apart — is compiled and its decoded bytecode compared against the source:

  * **the cardinality of the property**: the number of `constrain_instance(X)` statements in the
    source equals the number of `ConstrainInstance` opcodes in the compiled circuit. This is the
    count the instance-derivation rule is *about*, and it is the one fact that makes "the data is
    the sources' data" stronger than a fidelity assumption: the deployed artifact is the compiler's
    output on that source, and the opcodes are what the prover's circuit contains.
  * **the identity**: the compiled circuit's `namespace` equals the source's `circuit "<name>"`
    string, so a binary belonging to another source cannot pass.

## What it does not check

It is not a semantics check. The compiled opcode *arguments* are not compared to the source's
operands — an exact comparison needs name-to-index resolution and is the natural next step, not
this one. And it does not verify the compiler: `zkas` is a prebuilt binary at the repository root
(gitignored, so it is provided rather than committed) and this script trusts it, which is the same
trust the build already places in it.

## The compiler's absence is a failure rather than a skip

`./zkas` is gitignored, so a tree can legitimately lack it, and `scripts/validate_zk_bins.sh`
already carries the house fallback (`target/release/zkas`, "for CI where only target/ exists").
This script takes the same two paths. If neither exists it **exits 1 and says so**, because a gate
whose green line means "there was no compiler" is the defect this file was written to remove — the
one thing worse than a red gate is a green one that checked nothing.

Usage:
    python3 scripts/check-circuit-fidelity.py            # table + verdict
    python3 scripts/check-circuit-fidelity.py --quiet     # only failures
"""
import importlib.util
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
# `ConstrainInstance` appears as a bare opcode in the decoder's opcode list, one per line, indented.
OPCODE_RE = re.compile(r"^\s+ConstrainInstance,\s*$", re.M)
NAMESPACE_RE = re.compile(r'^\s*namespace:\s*"([^"]+)"', re.M)
CIRCUIT_RE = re.compile(r'^\s*circuit\s+"([^"]+)"', re.M)

RED, GREEN, NC = "\033[31m", "\033[32m", "\033[0m"


def checker():
    """The instance-derivation checker, imported so the circuit *set* has one definition."""
    spec = importlib.util.spec_from_file_location(
        "circuit_instance_derivation", REPO / "script" / "circuit_instance_derivation.py"
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def find_zkas():
    """The compiler, by the same two paths `scripts/validate_zk_bins.sh` uses."""
    for cand in (REPO / "zkas", REPO / "target" / "release" / "zkas"):
        if os.access(cand, os.X_OK):
            return cand
    return None


def source_facts(ner, path):
    """The source's `constrain_instance` count and its `circuit "<name>"` identity."""
    text = Path(path).read_text()
    _, _, stmts = ner.parse_circuit(ner.strip_comments(text))
    n = sum(1 for s in stmts
            if (m := ner.CALL_RE.match(s)) and m.group(1) == "constrain_instance")
    names = CIRCUIT_RE.findall(text)
    if len(names) != 1:
        raise ValueError(f"expected exactly one `circuit \"...\"` block, found {len(names)}")
    return n, names[0]


def compiled_facts(zkas, path, tmp):
    """Compile a copy of the source and decode it.

    A *copy*, because `zkas` writes `<input>.zk.bin` beside the input and **ignores `-o`**
    (measured: `-o /tmp/x.bin` still printed `Wrote output to src/contract/box/proof/take.zk.bin`).
    Compiling in place would make this gate write into the working tree; a copy keeps it read-only.
    The sources are self-contained — `include` appears in this corpus only inside English prose in
    comments — so a lone copy compiles the same.
    """
    work = Path(tmp) / Path(path).name
    shutil.copyfile(path, work)
    r = subprocess.run([str(zkas), "-e", str(work)], capture_output=True, text=True, cwd=tmp)
    if r.returncode != 0:
        raise RuntimeError(f"zkas exited {r.returncode}: {r.stderr.strip()[:200]}")
    out = re.sub(r"\x1b\[[0-9;]*m", "", r.stdout)
    ns = NAMESPACE_RE.search(out)
    if ns is None:
        raise RuntimeError(f"no `namespace` in the decoded output for {work.name}")
    return len(OPCODE_RE.findall(out)), ns.group(1)


def main():
    quiet = "--quiet" in sys.argv
    ner = checker()
    zkas = find_zkas()
    if zkas is None:
        print(f"{RED}FAIL:{NC} no compiler — looked for ./zkas and ./target/release/zkas. This check "
              "compiles every source, so its absence is a failure rather than a pass.", file=sys.stderr)
        return 1

    version = subprocess.run([str(zkas), "--version"], capture_output=True, text=True)
    paths = sorted(ner.zk_files())
    failures, checked = [], 0
    with tempfile.TemporaryDirectory() as tmp:
        for path in paths:
            rel = os.path.relpath(path, REPO)
            try:
                src_n, src_name = source_facts(ner, path)
                bin_n, bin_name = compiled_facts(zkas, path, tmp)
            except Exception as e:                                    # noqa: BLE001
                failures.append(f"{rel}: {e}")
                continue
            checked += 1
            if src_n != bin_n:
                failures.append(f"{rel}: source has {src_n} `constrain_instance`, the compiled "
                                f"circuit has {bin_n} `ConstrainInstance`")
            if src_name != bin_name:
                failures.append(f"{rel}: source's circuit is {src_name!r}, the compiled circuit's "
                                f"namespace is {bin_name!r}")
            if not quiet and src_n == bin_n and src_name == bin_name:
                print(f"  ok  {rel:58s} {src_n:3d} constrain_instance = {bin_n:3d} ConstrainInstance")

    print(f"\n  circuits: {len(paths)}   checked: {checked}   failures: {len(failures)}"
          f"   compiler: {version.stdout.strip() or zkas}")
    for f in failures:
        print(f"  {RED}FAIL:{NC} {f}")
    if failures:
        return 1

    # The control: a loop that stopped walking would report success over nothing, which is exactly
    # the defect this gate was written to replace. 178 is the circuit count measured 2026-09-24 and
    # the transcription carries the same set; a drop is a finding, not a pass.
    if checked < 170:
        print(f"{RED}FAIL:{NC} only {checked} circuit(s) checked — the walk stopped early")
        return 1
    print(f"{GREEN}OK:{NC}  every circuit's compiled opcode count and identity match its source "
          f"({checked} checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
