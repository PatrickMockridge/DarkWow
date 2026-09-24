#!/bin/bash
# OBL-C99: a client builder's params are the params the contract decodes.
#
# A contract has exactly one params type per function — the `model::*V1` the entrypoint hands to its
# `decode` — and exactly one encoder for it. A client that declares a *second* type for the same call
# is a second source of truth, and the two drift the way two encodings always do: the three
# drain_protection builders this rule was written for returned four-to-five-field client types where
# the contract decodes eight to ten, so a wallet following the builder API built a call the
# entrypoint refused as truncated (`OBL-C99`), and nothing reported it because nothing read both
# sides. The rule is the same one `contract-standards.md` states for circuits: the call the wallet
# builds is the call the contract executes.
#
# TWO RULES, and the difference between them is the difference between a defect and a smell:
#
#   (B) a client-declared struct carrying its own `encode`+`decode` pair that the contract's `model`
#       does not declare. This is the defect: a second codec for call data. Measured at zero sites on
#       2026-09-24 — the three that existed were removed in the same commit that added this gate —
#       so it is a clean ratchet: a new one fails, and there is no exception list for it because
#       there is no site to except.
#
#   (A) a client-declared struct whose **name** is a model type's name minus its version suffix
#       (`InitializeParams` beside `InitializeParamsV1`). This is the shape (B) grows from, and it
#       survives without a codec: two builders in `dao_escrow` return such types today and cannot
#       produce a decodable call either. Rule (A) is *name-based and therefore shallow*: it cannot
#       see a second params type called something else entirely, so a clean run is not proof that
#       the class is gone, only that this shape is. That limit is stated rather than papered over;
#       the same is true of `check-pubkey-binding.sh` and for the same reason.
#
# Usage:  scripts/check-client-params-alignment.sh
# Exit 0: clean, or every rule-(A) finding excepted in
#         `script/client_params_alignment_exceptions.txt` (printed as EXCEPTED with its reason).
# Exit 1: at least one *unexcepted* finding, reported as path:line.
# Exit 2: the exception list drifted — a stale entry, or a malformed one.
#
# The exception list is a ratchet, not an amnesty: an entry names the register row that schedules
# the repair, it stays visible in every run, and a stale entry (a site that no longer exists, or one
# that no longer matches) is exit 2 rather than silence — the phase gate learned that the hard way.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import os, pathlib, re, sys

repo = pathlib.Path(os.environ["REPO_ROOT"])
exceptions_path = repo / "script" / "client_params_alignment_exceptions.txt"


def strip_comments_and_strings(src):
    """Blank out //, /* */ and string literals, preserving line structure.

    Without this the scan reads prose: a doc comment explaining why a client must not declare its own
    params type would itself be read as one.
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
                elif src[i] == '*' and i + 1 < n and src[i + 1] == '/':
                    depth -= 1
                out.append(' ' if src[i] != '\n' else '\n')
                i += 1
        elif c == '"':
            out.append(' ')
            i += 1
            while i < n and src[i] != '"':
                if src[i] == '\\':
                    out.append(' ')
                    i += 1
                if i < n:
                    out.append(' ' if src[i] != '\n' else '\n')
                    i += 1
            if i < n:
                out.append(' ')
                i += 1
        else:
            out.append(c)
            i += 1
    return ''.join(out)


def line_of(src, pos):
    return src.count('\n', 0, pos) + 1


def client_structs(src):
    """(name, line, body) for each `pub struct` in a client source.

    `body` runs to the next `pub struct`, which bounds the codec search to this declaration and the
    impls that follow it rather than to the whole file — otherwise the last struct in a file would
    inherit every earlier codec.
    """
    starts = [m for m in re.finditer(r'pub struct\s+(\w+)', src)]
    for i, m in enumerate(starts):
        end = starts[i + 1].start() if i + 1 < len(starts) else len(src)
        yield m.group(1), line_of(src, m.start()), src[m.end():end]


# -- the exception list: "<contract> <struct> : <reason citing a register ID>" --
declared = {}
if exceptions_path.exists():
    for lineno, raw in enumerate(exceptions_path.read_text().splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith('#'):
            continue
        m = re.match(r'^([\w-]+)\s+(\w+)\s*:\s*(.+)$', line)
        if not m:
            print(f"EXIT2 {exceptions_path.relative_to(repo)}:{lineno}: malformed entry "
                  f"(want '<contract> <struct> : <reason>'): {line}", file=sys.stderr)
            sys.exit(2)
        declared[(m.group(1), m.group(2))] = (line, m.group(3))

findings = []
matched = set()
for cdir in sorted(p for p in (repo / "src" / "contract").iterdir() if p.is_dir()):
    client = cdir / "src" / "client"
    if not client.is_dir():
        continue
    model_dir = cdir / "src" / "model"
    model_names = set()
    if model_dir.is_dir():
        for f in sorted(model_dir.rglob("*.rs")):
            model_names |= set(re.findall(r'pub struct\s+(\w+)',
                                          strip_comments_and_strings(f.read_text(errors="ignore"))))
    for f in sorted(client.rglob("*.rs")):
        raw = f.read_text(errors="ignore")
        src = strip_comments_and_strings(raw)
        rel = f.relative_to(repo)
        for name, line, body in client_structs(src):
            if name in model_names:
                continue
            shadow = sorted(m for m in model_names if re.fullmatch(re.escape(name) + r'V\d', m))
            has_codec = bool(re.search(r'pub fn encode\(', body)) and bool(
                re.search(r'pub fn decode\(', body))
            if has_codec:
                findings.append(("B", cdir.name, name, rel, line, ""))
            elif shadow:
                findings.append(("A", cdir.name, name, rel, line, shadow[0]))

bad = 0
for rule, contract, name, rel, line, model_type in findings:
    key = (contract, name)
    if rule == "A" and key in declared:
        matched.add(key)
        print(f"EXCEPTED [{contract}] {name} ({rel}:{line}) vs model {model_type}")
        print(f"         {declared[key][1]}")
        continue
    bad += 1
    if rule == "B":
        print(f"FAIL [{contract}] {name} ({rel}:{line}) declares its own params codec — "
              f"the contract decodes a `model` type instead; return that type from the builder "
              f"(OBL-C99)")
    else:
        print(f"FAIL [{contract}] {name} ({rel}:{line}) shadows the model's {model_type} — "
              f"a second params type for the same call (OBL-C99)")

stale = sorted(set(declared) - matched)
for contract, name in stale:
    print(f"EXIT2 {exceptions_path.relative_to(repo)}: stale entry [{contract}] {name} — no such "
          f"client struct any more; remove the line", file=sys.stderr)

if stale:
    sys.exit(2)
if bad:
    print(f"\n{bad} finding(s). See this script's header for the two rules and their limit.")
    sys.exit(1)
print(f"OK: no client-declared params codec; {len(matched)} rule-(A) shadow(s) excepted "
      f"({len(declared)} declared).")
PYEOF
