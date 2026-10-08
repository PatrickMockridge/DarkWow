#!/bin/bash
# Register artefact existence: does every file the register cites exist where the row says?
#
# WHY THIS EXISTS (2026-09-24). Three times in one afternoon the register described a tree that did not
# exist: commit 532aa0da85 landed a row saying `Circuits/Transcribed.lean` "is imported by
# `src/DarkFi.lean`" while the module, the generator and the gate line it named were all untracked; a
# session then pasted a sentence claiming the axiom consumer was "unaffected", justified by two source
# lines that did exist, and a build refuted it; and the same class recurred twice more in the commits
# that were correcting it. Every one of those was a *claim about a file*, and nothing checked a claim
# about a file against the tree.
#
# WHAT IT CHECKS. Every in-scope path citation in the register resolves to a file present in **HEAD** —
# not in the working tree, which is where the failure mode lives. A citation that exists only in the
# working tree is reported as its own case, because that is exactly the 532aa0da85 shape: the row is
# committed, the artefact is not.
#
# WHAT A PASS DOES NOT MEAN, stated because the register's own discipline is that a gate says what it
# is narrower than. It does not mean the cited file is the *right* file; it does not mean the file's
# contents, line numbers or function names are as the row describes (line suffixes are stripped, and no
# symbol is checked); it does not mean the row's proposition is true or that its "Enforced at" site
# enforces anything; and it does not mean every citation was examined — bare filenames, crate-relative
# paths and non-path identifiers are residue by construction (see the extraction rules below). This
# guards the form "this file exists here", which is the form that failed.
#
# EXTRACTIONS. A token is `(dir/)*name.ext` with an optional `:line` or `:line-line` suffix, ending in a
# source extension. Deliberately out of scope, each for a reason:
#   * bare filenames with no directory (`chain_state.rs`) — ambiguous in HEAD; `entrypoint.rs` matches
#     26 files. Docs are covered separately by check-doc-index.sh's basename check.
#   * crate-relative paths with a depth-1 dirname (`src/entrypoint.rs`, `tests/unit.rs`) — they need
#     crate inference to resolve.
#   * build artefacts (`.olean`, `.bin`) — the register cites them *as evidence of absence* ("no
#     `Transcribed.olean` exists"), so checking them would invert their meaning.
#
# EXEMPTIONS. Some citations are deliberately about files that do not exist: a row recording a stale
# citation names the stale path, and a row closed by deleting a module names the module it deleted.
# Those are declared, with a reason, in script/register_artifact_exceptions.txt — the same
# reason-per-entry idiom as script/circuit_free_instances.txt. A declared path that *does* resolve, or
# that the register no longer cites, is reported as a stale entry rather than silently admitted.
#
# Usage:
#   scripts/check-register-artifacts.sh                  # HEAD, the committed register
#   scripts/check-register-artifacts.sh --working-tree   # check against uncommitted files too (authoring)
#   REGISTER=<path> scripts/check-register-artifacts.sh  # check a specific register copy (negative control)
#
# Exit 0: every in-scope citation resolves. Exit 1: at least one finding, each printed as
# register:line: what is wrong.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

MODE="head"
case "${1:-}" in
  --working-tree) MODE="work" ;;
  "") ;;
  *) echo "usage: $0 [--working-tree]" >&2; exit 2 ;;
esac

REPO_ROOT="$REPO_ROOT" MODE="$MODE" python3 - <<'PYEOF'
import os, re, subprocess, sys

repo = os.environ["REPO_ROOT"]
mode = os.environ["MODE"]
register = os.environ.get(
    "REGISTER", os.path.join(repo, "doc", "src", "arch", "verification-hazop.md")
)
exceptions_path = os.environ.get(
    "EXCEPTIONS", os.path.join(repo, "script", "register_artifact_exceptions.txt")
)

SEG = r"[A-Za-z0-9_-]+"
EXT = "rs|lean|md|zk|py|sh|toml|txt|sage|yml|yaml|json|wasm|asm|h|c|cpp"
TOKEN_RE = re.compile(rf"(?<![\w.-])((?:{SEG}/)*{SEG}\.(?:{EXT}))(:\d+(?:-\d+)?)?(?![\w.-])")

# Namespaces the register only ever uses repo-relative, so a depth-1 dirname is still in scope here.
SAFE_TOP = {"script", "scripts", "contrib", "crates", "proofs", "doc", "vendor"}


def git_paths(*args):
    out = subprocess.run(
        ["git", "-C", repo, *args], capture_output=True, text=True, check=True
    ).stdout
    return {p for p in out.splitlines() if p}


head = git_paths("ls-tree", "-r", "HEAD", "--name-only")
if mode == "work":
    present = head | git_paths("ls-files") | git_paths("ls-files", "--others", "--exclude-standard")
else:
    present = head

# Every directory that exists in HEAD, so a cited dirname can be recognised as real.
head_dirs = set()
for p in head:
    parts = p.split("/")[:-1]
    for i in range(1, len(parts) + 1):
        head_dirs.add("/".join(parts[:i]))

if not os.path.exists(register):
    print(f"FAIL: no register at {register}")
    sys.exit(1)
text = open(register, encoding="utf-8", errors="replace").read()


def lean_exists(token):
    """A .lean citation resolves through the Lean tree's shorthand.

    The register writes Lean modules four ways — as a repo path
    (`proofs/lean/src/DarkFi/Axioms.lean`), as a namespace path
    (`Circuits/InstanceDerivation.lean`), with a leading `src/` (`src/DarkFi.lean`), and as a bare
    module name (`Axioms.lean`) — and all four denote a file under `proofs/lean/`. So each is tried as
    written, with `src/` stripped, and with `DarkFi/` stripped, against the `proofs/lean/**` set.
    """
    cands = {token}
    for prefix in ("src/", "DarkFi/"):
        if token.startswith(prefix):
            cands.add(token[len(prefix):])
    lean = {p for p in present if p.startswith("proofs/lean/")}
    for c in cands:
        if c in lean or any(p.endswith("/" + c) for p in lean):
            return True
    return False


def resolves(token):
    if token.endswith(".lean") and lean_exists(token):
        return True
    return token in present


def in_scope(token):
    """Which citations the gate adjudicates — see the header's extraction rules."""
    if "/" not in token:
        return False
    if token.endswith(".lean"):
        return True
    d = token.rsplit("/", 1)[0]
    if d in SAFE_TOP:
        return True
    return "/" in d and d in head_dirs


# Exemptions: `<cited path> : <row> : <reason>`.
exceptions = {}
if os.path.exists(exceptions_path):
    for line in open(exceptions_path, encoding="utf-8"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = [p.strip() for p in line.split(":", 2)]
        if len(parts) < 3:
            print(f"FAIL: malformed exemption entry (want `path : row : reason`): {line}")
            sys.exit(1)
        exceptions[parts[0]] = (parts[1], parts[2])

_line_counts = {}


def head_line_count(path):
    """Lines in the file at HEAD, or None when it cannot be read (a `.lean` shorthand, a dir)."""
    if path not in _line_counts:
        r = subprocess.run(["git", "-C", repo, "show", f"HEAD:{path}"],
                           capture_output=True, text=True)
        _line_counts[path] = (r.stdout.count("\n") + 1) if r.returncode == 0 else None
    return _line_counts[path]


def line_gone(token, suffix):
    """True when the citation names a line the file no longer has.

    The path resolving is not the line resolving. A row can cite `entrypoint.rs:1740` and the file can
    still exist with 1200 lines, because a later edit deleted the code the row is about — which is
    `OBL-C148`'s shape and the reason `TOKEN_RE` captured the suffix from the day it was written.
    Returns False for anything not checkable (no suffix, an unreadable file, a line range whose low
    end exceeds the file), so an absent signal is never read as a finding.
    """
    if not suffix or not suffix.startswith(":"):
        return False
    n = head_line_count(token)
    if n is None:
        return False
    try:
        nums = [int(x) for x in suffix[1:].split("-")]
    except ValueError:
        return False
    return max(nums) > n


findings, exempted, cited, seen = [], [], set(), set()
line_findings, seen_line = [], set()
for m in TOKEN_RE.finditer(text):
    token = m.group(1)
    if not in_scope(token):
        continue
    cited.add(token)
    lineno = text[: m.start()].count("\n") + 1
    if resolves(token):
        if line_gone(token, m.group(2)) and (token, m.group(2)) not in seen_line:
            seen_line.add((token, m.group(2)))
            line_findings.append((token, m.group(2), lineno))
        continue
    if token in exceptions:
        exempted.append((token, lineno, exceptions[token]))
        continue
    # One finding per (site, path): a row may cite the same missing file twice in one cell, and two
    # identical lines read as two defects.
    if (token, lineno) in seen:
        continue
    seen.add((token, lineno))
    findings.append((token, lineno))

# The 532aa0da85 shape gets its own message, so the fix it implies (commit the artefact with the row)
# is the one the reader sees: uncommitted-but-present is a different repair from absent.
work_only = (git_paths("ls-files") | git_paths("ls-files", "--others", "--exclude-standard")) - head

# A declared exemption that now resolves, or that the register no longer cites, is covering nothing.
# Only in HEAD mode: under `--working-tree` an in-flight exemption (the artefact is present but
# uncommitted) resolves by definition, and reporting it would make that mode unusable for the author
# it exists for. The stale check's purpose — emptying the list when the unit lands — is a HEAD fact.
stale = (
    [
        (path, row, reason)
        for path, (row, reason) in exceptions.items()
        if resolves(path) or path not in cited
    ]
    if mode == "head"
    else []
)

if findings or stale or line_findings:
    # A REGISTER= override may point outside the repo (the controls do), where a relative path would
    # be a row of "../" that names nothing.
    rel = os.path.relpath(register, repo)
    if rel.startswith(".."):
        rel = register
    for token, lineno in findings:
        case = (
            "exists in the working tree but not in HEAD — commit the artefact with the row"
            if token in work_only
            else "does not exist in HEAD"
        )
        print(f"FAIL: {rel}:{lineno}: cites {token}, which {case}")
    for path, row, reason in stale:
        print(f"FAIL: stale exemption for {path} ({row}) — it now resolves, or the register no longer")
        print(f"      cites it, so the entry is covering nothing: {reason}")
    # The line half: the path resolves and the line does not. `OBL-C148` — a later edit deletes the
    # code a row is about while the file stays, and a path-only resolver cannot see it. Blocking from
    # its second run: measured on introduction across 349 cited paths it found exactly **one**, the
    # `OBL-C103` citation whose builder library had been retired, and that one is corrected.
    for token, suffix, lineno in line_findings:
        print(f"FAIL: {rel}:{lineno}: cites {token}{suffix}, but the file has "
              f"{head_line_count(token)} line(s) at HEAD — the cited line is gone")

    print("")
    print("Fix: commit the artefact, correct the citation, or record it in")
    print("script/register_artifact_exceptions.txt with a reason.")
    sys.exit(1)

print(f"PASS: every in-scope register citation resolves against {mode.upper()}")
print(f"  ({len(cited)} distinct in-scope paths cited; {len(exceptions)} declared exemption(s), each with its reason:)")
for path, (row, reason) in sorted(exceptions.items()):
    print(f"    {path} — {row}: {reason}")
PYEOF
