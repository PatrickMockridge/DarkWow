#!/usr/bin/env bash
#
# The four observation alphabets, extracted mechanically and diffed.
#
# WHY THIS EXISTS. A calculus founded on barbs cannot have four alphabets. There are four claims to
# the same vocabulary — the Lean model, the core `BarbId`, the sdk `Barb`, and the normative table in
# `type-system.md` §1.1 — and two source comments claim a "1:1 mirror of the Lean4 inductive" that
# the counts alone show is not true. Reading the four sets by hand produced a miscount earlier in
# this work, so the diff is a script and not a paragraph.
#
# WHAT IT DOES. Extracts the barb names from each source *without* interpreting them: the variants of
# `enum BarbId` in src/barb.rs, the variants of `enum Barb` in src/sdk/src/capability.rs, the
# constructors of `inductive Barb` in Types.lean, and the `↓name` entries of the table under
# `### 1.1 Barbs` in type-system.md. Names are normalised to kebab-case so that Lean's
# `proveInclusion`, Rust's `ProveInclusion` and the doc's `↓prove-inclusion` compare equal.
#
# WHAT IT DOES NOT DO. It does not decide which set is right, and it does not touch meaning: a barb
# that appears in all four may still mean four different things. The point is to make disagreement
# visible and countable, which is the precondition for the model and the Rust agreeing.
#
# Usage: contrib/barb_alphabet_diff.sh
# Exit status: 1 if the four sets differ, 0 if they are identical.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="/tmp/barb-alphabet-diff.txt"
LEAN=proofs/lean/src/DarkFi/Capability/Types.lean
DOC=doc/src/arch/type-system.md
CORE=src/barb.rs
SDK=src/sdk/src/capability.rs

for f in "$LEAN" "$DOC" "$CORE" "$SDK"; do
    [ -f "$f" ] || { echo "barb_alphabet_diff: missing $f" >&2; exit 2; }
done

python3 - "$LEAN" "$DOC" "$CORE" "$SDK" "$OUT" <<'PY'
import re, sys

lean_f, doc_f, core_f, sdk_f, out_f = sys.argv[1:6]

def kebab(name: str) -> str:
    """ProveInclusion / proveInclusion / prove-inclusion -> prove-inclusion.

    A name that already carries its hyphens keeps them: stripping first and re-inserting at case
    boundaries turns the doc's `↓sync-barrier` into `syncbarrier` while Lean's `syncBarrier` becomes
    `sync-barrier`, and the diff then reports a disagreement that does not exist. That is exactly the
    miscount this script exists to prevent, so it is worth the two lines.
    """
    name = name.strip().strip('|').strip()
    if '-' in name:
        return name.lower()
    return re.sub(r'(?<!^)(?=[A-Z])', '-', name).lower()

def enum_variants(text: str, enum_name: str) -> list[str]:
    """Variants of `enum <enum_name>`, read from its opening brace to the matching close."""
    m = re.search(rf'\benum\s+{re.escape(enum_name)}\b[^{{]*{{', text)
    if not m:
        return []
    depth, i = 1, m.end()
    start = i
    while i < len(text) and depth:
        if text[i] == '{': depth += 1
        elif text[i] == '}': depth -= 1
        i += 1
    body = text[start:i]
    # strip comments first, so a commented-out variant is not counted
    body = re.sub(r'//[^\n]*', '', body)
    body = re.sub(r'#\[[^\]]*\]', '', body)
    names = []
    for part in body.split(','):
        part = part.strip()
        if not part:
            continue
        token = re.match(r'([A-Za-z_][A-Za-z0-9_]*)', part)
        if token:
            names.append(kebab(token.group(1)))
    return names

def lean_constructors(text: str, inductive: str) -> list[str]:
    """Constructors of `inductive <inductive>`, up to the `deriving` line."""
    m = re.search(rf'^inductive\s+{re.escape(inductive)}\b.*?where\s*$', text, re.M)
    if not m:
        return []
    rest = text[m.end():]
    rest = rest.split('deriving')[0]
    return [kebab(n) for n in re.findall(r'^\s*\|\s*([A-Za-z_][A-Za-z0-9_]*)', rest, re.M)]

def doc_barbs(text: str, heading: str, next_heading: str) -> list[str]:
    """`↓name` entries in the first table after `heading`, before `next_heading`."""
    i = text.find(heading)
    j = text.find(next_heading, i + 1) if i >= 0 else -1
    section = text[i:j] if i >= 0 and j > i else ''
    names = []
    for line in section.split('\n'):
        if not line.lstrip().startswith('|'):
            continue
        cell = line.split('|')[1].strip() if line.count('|') >= 2 else ''
        m = re.match(r'[`↓\s]*↓([a-z0-9-]+)', cell) or re.match(r'`↓([a-z0-9-]+)`', cell)
        if m:
            names.append(kebab(m.group(1)))
    return names

lean = lean_constructors(open(lean_f).read(), 'Barb')
doc = doc_barbs(open(doc_f).read(), '### 1.1 Barbs', '### 1.2')
core = enum_variants(open(core_f).read(), 'BarbId')
sdk = enum_variants(open(sdk_f).read(), 'Barb')

sets = {'TypeSystem §1.1': doc, 'Lean Types.lean': lean, 'core BarbId': core, 'sdk Barb': sdk}
for k, v in sets.items():
    assert len(v) == len(set(v)), f"{k} has duplicates: {[x for x in v if v.count(x) > 1]}"

order, seen = [], set()
for src in (doc, lean, core, sdk):
    for n in src:
        if n not in seen:
            seen.add(n); order.append(n)
order.sort()

def cell(v): return 'yes' if v else '  .'

lines = []
lines.append(f"{'barb':<20} {'doc':>4} {'lean':>5} {'core':>5} {'sdk':>4}")
lines.append('-' * 42)
for n in order:
    lines.append(f"↓{n:<19} {cell(n in doc):>4} {cell(n in lean):>5} {cell(n in core):>5} {cell(n in sdk):>4}")

lines.append('')
lines.append(f"counts: doc {len(doc)}, lean {len(lean)}, core {len(core)}, sdk {len(sdk)}")
lines.append('')
def show(label, names): lines.append(f"{label}: {', '.join('↓'+n for n in sorted(names)) or '(none)'}")
show('in doc, not in core BarbId', set(doc) - set(core))
show('in core BarbId, not in Lean', set(core) - set(lean))
show('in Lean, not in sdk Barb', set(lean) - set(sdk))
show('in core BarbId, not in sdk Barb', set(core) - set(sdk))
show('in doc, not in Lean', set(doc) - set(lean))

# The intended relations, not raw equality. §1.1 is normative; `BarbId` implements it; the sdk's
# capability `Barb` is the subset that types a capability; the Lean model is meant to equal §1.1 and
# is behind by the rows listed above (P5's work, in the plan). Encoding the *relations* means this
# script stays a check after the four sets stop being four sets — a raw-equality test would have to
# be deleted the moment the subset was made deliberate.
lines.append('')
lines.append('intended relations:')
core_ok = set(core) == set(doc)
sdk_ok = set(sdk) <= set(doc)
lean_ok = set(lean) == set(doc)
lines.append(f"  core BarbId == §1.1            {'HOLDS' if core_ok else 'VIOLATED'}")
lines.append(f"  sdk Barb    ⊆  §1.1            {'HOLDS' if sdk_ok else 'VIOLATED'}")
lines.append(f"  Lean Barb   == §1.1            {'HOLDS' if lean_ok else f'VIOLATED (missing {len(set(doc) - set(lean))}, P5)'}")

report = '\n'.join(lines) + '\n'
open(out_f, 'w').write(report)
print(report)

sys.exit(0 if (core_ok and sdk_ok and lean_ok) else 1)

PY
status=$?
echo "barb_alphabet_diff: written to $OUT" >&2
exit "$status"
