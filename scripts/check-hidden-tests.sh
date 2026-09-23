#!/bin/bash
# OBL-C76: a test that does not run in the configuration used to claim verification is not
# verification, so **the tests that do not run by default are enumerated and declared**.
#
# The row's measurement is the reason this exists: `cargo test -p dwow_chain --lib` lists 172 tests
# and the same command with `--all-features` lists 263. The 91 that differ are the consensus surface
# the campaign works on. `make test` runs `--release --all-features --workspace`, so the *gates* always
# exercised all 263 — the defect is in the ad-hoc command one reaches for to check a single crate, and
# in the claim that follows it: an unqualified pass count is indistinguishable from a run that skipped
# the test.
#
# This gate cannot enforce the convention ("a verification claim names its command and its feature
# set") — that is a rule a reader follows. What it can do is make the hidden set mechanical, so the
# number a claim is missing is a number nobody has to remember. Three mechanisms hide tests here, and
# each is enumerated:
#
#   * `#[ignore]` — the test runs only under `--ignored`.
#   * `#[cfg(feature = "...")]` on a test function — the test is not compiled without that feature.
#   * `#[cfg(all(test, feature = "..."))]` on a test module — the whole module is absent without it.
#
# Every site must be declared in `script/hidden_test_exceptions.txt` with a reason and how to run it.
# A *new* one fails, which is the point: the list is the record of what a default run does not cover,
# and adding to it is a deliberate act that has to name a way to run the test.
#
# Exit 0: every hidden test is declared.
# Exit 1: at least one is not.
# Exit 2: the declaration file is malformed — a broken list must not read as a pass.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import glob, os, re, sys

repo = os.environ["REPO_ROOT"]

paths = (sorted(glob.glob(f"{repo}/src/**/*.rs", recursive=True))
         + sorted(glob.glob(f"{repo}/bin/**/*.rs", recursive=True)))

DEF_RE = re.compile(r'^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)')
MOD_RE = re.compile(r'^\s*(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)')

hidden = []
for path in paths:
    rel = os.path.relpath(path, repo)
    try:
        lines = open(path, errors="replace").read().split("\n")
    except OSError:
        continue
    # Walk attribute blocks: a run of `#[...]` and doc/comment lines ending at a fn or mod.
    i = 0
    while i < len(lines):
        s = lines[i].strip()
        if not s.startswith("#["):
            i += 1
            continue
        block = []
        j = i
        while j < len(lines):
            t = lines[j].strip()
            if t.startswith("#["):
                block.append(t)
                j += 1
            elif t.startswith("//") or not t:
                j += 1
            else:
                break
        target = lines[j].strip() if j < len(lines) else ""
        m = DEF_RE.match(target) or MOD_RE.match(target)
        if m and block:
            name = m.group(1)
            attrs = " ".join(block)
            why = None
            if any(a.startswith("#[ignore") for a in block):
                why = "#[ignore] — runs only under `--ignored`"
            elif re.search(r'cfg\([^)]*\btest\b[^)]*feature\s*=', attrs):
                why = "feature-gated test module — not compiled without that feature"
            elif any(re.match(r'#\[cfg\(\s*feature\s*=', a) for a in block) and \
                    any(a == "#[test]" for a in block):
                why = "feature-gated test fn — not compiled without that feature"
            if why:
                hidden.append((f"{rel} :: {name}", f"{rel}:{i + 1}: {name} — {why}"))
        i = j + 1 if j > i else i + 1

# FOURTH MECHANISM, and the gate's first run found it in its own coverage: a module gated in its
# *parent*, whose `#[cfg(test)]` submodule then never compiles. `sync_boundary` and
# `sync_connection` are declared `#[cfg(feature = "sync-p2p")] mod …;` in `lib.rs`, so a bare
# `cargo test -p dwow_chain` compiles neither them nor their tests — and nothing inside those files
# says so. This is the mechanism OBL-C76's own measurement attributed five differing tests to
# (`sync_boundary` 4, `sync_connection` 1) without naming the gate that causes it. Walking only
# files, as this gate first did, cannot see it: the gate is in a file the tests are not in.
for path in paths:
    rel = os.path.relpath(path, repo)
    try:
        lines = open(path, errors="replace").read().split("\n")
    except OSError:
        continue
    for i, ln in enumerate(lines):
        m = re.match(r'\s*#\[cfg\(([^\]]*)\)\]\s*$', ln)
        if not m or 'feature' not in m.group(1):
            continue
        j = i + 1
        while j < len(lines) and (lines[j].strip().startswith("#[")
                                  or lines[j].strip().startswith("//")
                                  or not lines[j].strip()):
            j += 1
        if j >= len(lines):
            continue
        d = re.match(r'\s*(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;', lines[j])
        if not d:
            continue
        name = d.group(1)
        base = os.path.dirname(path)
        for cand in (os.path.join(base, f"{name}.rs"), os.path.join(base, name, "mod.rs")):
            if not os.path.isfile(cand):
                continue
            if not re.search(r'#\[cfg\(test\)\]\s*(?:pub\s+)?mod\s+tests', open(cand, errors="replace").read()):
                continue
            crel = os.path.relpath(cand, repo)
            # A *negative* gate hides its module in the configuration where the feature is ON —
            # `native_token`'s entrypoint is `#[cfg(not(feature = "no-entrypoint"))]`, present by
            # default here and absent under the `no-entrypoint` build the python SDK uses. Saying
            # "without that feature" for it would be backwards, and a reason that is backwards is
            # worse than none.
            negated = "not(" in m.group(1).replace(" ", "")
            state = ("compile only when that feature is DISABLED"
                     if negated else "do not compile without that feature")
            hidden.append((f"{crel} :: tests",
                           f"{crel}: tests — hidden by its PARENT: `#[cfg({m.group(1)})] mod {name};` "
                           f"at {rel}:{i + 1}, so neither the module nor its tests {state}"))

declared = {}
exc_path = os.path.join(repo, "script", "hidden_test_exceptions.txt")
if os.path.isfile(exc_path):
    for raw in open(exc_path, errors="replace").read().splitlines():
        entry = raw.split("#")[0].strip()
        if not entry:
            continue
        parts = [p.strip() for p in re.split(r'\s+:\s+', entry, maxsplit=1)]
        if len(parts) != 2:
            print(f"ERROR: malformed line in {os.path.basename(exc_path)}: {entry!r}")
            print("       expected: <rel path> :: <name> : <reason and how to run it>")
            sys.exit(2)
        declared.setdefault(parts[0], parts[1])

undeclared = [(k, v) for k, v in hidden if k not in declared]
stale = [k for k in declared if k not in {k for k, _ in hidden}]

if stale:
    print("NOTE: declared but no longer hidden — remove these lines, they hide a fixed test:")
    for k in sorted(stale):
        print(f"  {k}")

if hidden:
    print(f"Declared hidden tests ({len(hidden) - len(undeclared)} of {len(hidden)}):")
    for k, _ in hidden:
        if k in declared:
            print(f"  {k}")
            print(f"      {declared[k]}")
    print("")
    print("Run the whole set — including the hidden ones — with the configuration the gates use:")
    print("  make test                     (--release --all-features --workspace)")
    print("  cargo test --all-features -p <crate>")
    print("  cargo test -- --ignored       (the #[ignore]d opt-in runs)")

if undeclared:
    print("")
    print(f"FAIL: {len(undeclared)} test(s) do not run in the default configuration and are not")
    print("      declared. A pass count that includes them without saying so is not a claim:")
    for k, v in undeclared:
        print(f"  {v}")
    print("")
    print("Declare each in script/hidden_test_exceptions.txt with a reason and how to run it —")
    print("or make it run by default, which is the better answer when the feature is not")
    print("genuinely optional.")
    sys.exit(1)

print("")
print("PASS: every test that does not run by default is declared.")
sys.exit(0)
PYEOF
