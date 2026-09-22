#!/bin/bash
# OBL-C72 / OBL-C73: the exec/apply phase rules.
#
#   Apply writes blindly. No read-triad function (db_get, db_contains_key,
#   get_object_size, get_object_bytes) is reachable from an `apply` function —
#   the ACL denies them in `ContractSection::Update`.
#
#   Exec does not write. No state-mutating function (db_set, db_del, merkle_add,
#   sparse_merkle_insert_batch, merkle_anchor_add) is reachable from an `exec`
#   function — the ACL denies those outside `Update`.
#
# **These are not style rules.** `vm_runtime.rs:954` runs apply as
# `ContractSection::Update`, the host returns `CALLER_ACCESS_DENIED` when the
# section is not in a function's `acl_allow` list (`src/runtime/import/db.rs:638`
# and siblings), and there is no workaround idiom: no read function admits
# `Update` at all. A violation is a call that cannot succeed.
#
# The rule is normative in `contract-wasm-type-system.md` §A.4.7 and §B.2.2, and
# §B.2.2 names the runtime error verbatim. All nine genesis contracts observe it
# without exception; 66 sites across 11 non-genesis contracts do not. Nothing
# checked it until this script — which is why it drifted, and why the ACL is
# parsed out of `src/runtime/import/*.rs` at run time here rather than written
# down: a hardcoded copy is the second source of truth that `safety.md` RC5 is
# about, and this repository has had that failure four times.
#
# Usage:  scripts/check-phase-host-functions.sh
# Exit 0: clean.  Exit 1: at least one site, reported with file:line.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import os, pathlib, re, sys, collections

repo = pathlib.Path(os.environ["REPO_ROOT"])


def strip_comments_and_strings(src):
    """Blank out //, /* */ and string literals, preserving line structure.

    Without this the scanner reads prose: the first run of this analysis flagged
    `native_token` (a genesis contract) for a `db_contains_key` that appears only
    inside a comment explaining why apply must not call it.
    """
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == '/' and i + 1 < n and src[i + 1] == '/':
            while i < n and src[i] != '\n':
                out.append(' ')
                i += 1
        elif c == '/' and i + 1 < n and src[i + 1] == '*':
            depth = 1
            out.append('  ')
            i += 2
            while i < n and depth:
                if src[i] == '/' and i + 1 < n and src[i + 1] == '*':
                    depth += 1
                    out.append('  ')
                    i += 2
                elif src[i] == '*' and i + 1 < n and src[i + 1] == '/':
                    depth -= 1
                    out.append('  ')
                    i += 2
                else:
                    out.append('\n' if src[i] == '\n' else ' ')
                    i += 1
        elif c == '"':
            out.append(' ')
            i += 1
            while i < n and src[i] != '"':
                if src[i] == '\\':
                    out.append('  ')
                    i += 2
                else:
                    out.append('\n' if src[i] == '\n' else ' ')
                    i += 1
            if i < n:
                out.append(' ')
                i += 1
        else:
            out.append(c)
            i += 1
    return ''.join(out)


def balanced(src, open_idx, op='(', cl=')'):
    """Return the text inside the parens starting at open_idx, or None."""
    depth = 0
    i = open_idx
    while i < len(src):
        if src[i] == op:
            depth += 1
        elif src[i] == cl:
            depth -= 1
            if depth == 0:
                return src[open_idx + 1:i]
        i += 1
    return None


def fn_defs(src):
    """name -> list of (line, body). All definitions, so duplicates stay distinct."""
    defs = collections.defaultdict(list)
    for m in re.finditer(r'\bfn\s+([A-Za-z0-9_]+)\s*(?:<[^>]*>)?\s*\(', src):
        name = m.group(1)
        j = m.end()
        depth, started, k = 0, False, j
        while k < len(src):
            c = src[k]
            if c == '{':
                depth += 1
                started = True
            elif c == '}':
                depth -= 1
                if started and depth == 0:
                    break
            k += 1
        defs[name].append((src[:m.start()].count('\n') + 1, src[m.start():k + 1]))
    return defs


# ---------------------------------------------------------------- the ACL table
# Read from the runtime, not from this file. A function's sections are whatever
# its own acl_allow call names; a function with no acl_allow is unrestricted and
# is not this gate's business.
ACL = {}          # host fn name -> frozenset of ContractSection names
for p in sorted((repo / "src/runtime/import").glob("*.rs")):
    if p.name == "acl.rs":
        continue
    src = strip_comments_and_strings(p.read_text(errors="replace"))
    for m in re.finditer(r'\bfn\s+([a-z_][A-Za-z0-9_]*)\s*\(', src):
        name = m.group(1)
        j = m.end()
        depth, started, k = 0, False, j
        while k < len(src):
            c = src[k]
            if c == '{':
                depth += 1
                started = True
            elif c == '}':
                depth -= 1
                if started and depth == 0:
                    break
            k += 1
        body = src[m.start():k + 1]
        calls = list(re.finditer(r'\bacl_allow\s*\(', body))
        if not calls:
            continue
        sections = set()
        for c in calls:
            args = balanced(body, c.end() - 1)
            if args:
                sections |= set(re.findall(r'ContractSection::([A-Za-z]+)', args))
        if sections:
            ACL[name] = frozenset(sections)

if not ACL:
    print("ERROR: parsed no ACLs from src/runtime/import/*.rs — the parser has drifted from")
    print("       the runtime, which is the failure this gate replaced a hardcoded table to avoid.")
    sys.exit(2)

READS = {"db_get", "db_contains_key", "get_object_size", "get_object_bytes",
         "db_get_local", "db_contains_key_local"}
WRITES = {"db_set", "db_del", "merkle_add", "sparse_merkle_insert_batch",
          "merkle_anchor_add", "db_set_local", "db_del_local"}

fail = 0
seen_unclassified = set()
report = []

for crate in sorted(p for p in (repo / "src/contract").iterdir() if p.is_dir()):
    files = sorted(crate.glob("src/**/*.rs"))
    if not files:
        continue
    defs = collections.defaultdict(list)     # name -> [(path, line, body)]
    exec_roots, apply_roots, meta_roots = [], [], []
    for f in files:
        text = f.read_text(errors="replace")
        src = strip_comments_and_strings(text)
        rel = f.relative_to(repo)
        for name, bodies in fn_defs(src).items():
            for (line, body) in bodies:
                defs[name].append((rel, line, body))
        for m in re.finditer(r'define_contract(?:_with_spend_hook)?!\s*\(', src):
            args = balanced(src, m.end() - 1)
            if not args:
                continue
            for key, sink in (("exec", exec_roots), ("apply", apply_roots),
                              ("metadata", meta_roots)):
                hit = re.search(rf'\b{key}\s*:\s*([A-Za-z_][A-Za-z0-9_]*)', args)
                if hit:
                    sink.append(hit.group(1))

    if not exec_roots:
        exec_roots = [n for n in defs if n == "process_instruction"
                      or n.endswith("_instruction") or n.endswith("_instruction_v1")]
    if not apply_roots:
        apply_roots = [n for n in defs if n == "process_update"
                       or n.startswith("apply_") or n.endswith("_apply_v1")]

    def closure(roots):
        reached, stack = set(), [r for r in roots if r in defs]
        while stack:
            name = stack.pop()
            if name in reached:
                continue
            reached.add(name)
            for (_, _, body) in defs[name]:
                inner = body.split('{', 1)[1] if '{' in body else body
                for ident in set(re.findall(r'[A-Za-z_][A-Za-z0-9_]*', inner)):
                    if ident in defs and ident not in reached:
                        stack.append(ident)
        return reached

    E, A, M = closure(exec_roots), closure(apply_roots), set(meta_roots)
    # A function reachable from both phases is reported under whichever phase it
    # can be reached from: it fails when called from apply, so it is an apply
    # finding. (Measured: no helper in this tree is reachable from both and
    # touches the triad, so the split below is currently exhaustive.)
    for name, bodies in defs.items():
        for (rel, line, body) in bodies:
            inner = body.split('{', 1)[1] if '{' in body else body
            called = set(re.findall(r'\b([a-z_][A-Za-z0-9_]*)\s*\(', inner))
            host = called & set(ACL)
            for h in sorted(host):
                secs = ACL[h]
                secs = tuple(sorted(secs))
                if name in A and "Update" not in secs:
                    report.append((rel, line, name, h, secs,
                                   "apply-phase function calls a host function the ACL denies in Update"))
                    fail += 1
                elif name in E and not ({"Exec", "Metadata"} & set(secs)):
                    report.append((rel, line, name, h, secs,
                                   "exec-phase function calls a host function the ACL denies in Exec"))
                    fail += 1
                elif name in M and "Metadata" not in secs:
                    report.append((rel, line, name, h, secs,
                                   "metadata-phase function calls a host function the ACL denies in Metadata"))
                    fail += 1
            # Drift alarm: a name that looks like a host import but is not in the
            # parsed table means the runtime grew a function this gate cannot see.
            for h in sorted(called):
                if h in READS or h in WRITES:
                    if h not in ACL:
                        seen_unclassified.add((rel, h))

for rel, line, name, h, secs, why in sorted(set(report)):
    print(f"FAIL: {rel}:{line}: {name}() -> {h}()  (legal in {secs})")
    print(f"      {why}")

if seen_unclassified:
    print()
    print("NOTE: these look like host imports but have no acl_allow in src/runtime/import/:")
    for rel, h in sorted(seen_unclassified):
        print(f"  {rel}: {h}")
    print("      Either the runtime renamed it, or it is a guest-side wrapper that")
    print("      delegates to a host import (e.g. db_mark_spent -> db_set). Confirm by")
    print("      hand before treating it as unclassified.")

if fail:
    print()
    print(f"FAIL: {fail} phase violation(s).")
    print("FIX: apply writes blindly — read in exec, carry the value through the update")
    print("     struct, write in apply. exec does not write at all. See")
    print("     contract-wasm-type-system.md §B.2.2 and the register's OBL-C72/OBL-C73.")
    print("     The genesis contracts are the worked example; three of them say so in")
    print("     comments (native_token entrypoint/mod.rs:1405, box:143, purse:202).")
    sys.exit(1)

print(f"PASS: no phase violations ({len(ACL)} host functions' ACLs parsed from the runtime)")
sys.exit(0)
PYEOF
