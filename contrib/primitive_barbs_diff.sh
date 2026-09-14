#!/usr/bin/env bash
#
# The type→barb mapping, extracted from both implementations and diffed.
#
# WHY THIS EXISTS. `contrib/barb_alphabet_diff.sh` checks that the four *alphabets* agree. That is
# necessary and not sufficient: two implementations can share a vocabulary and still disagree about
# which barbs each type exhibits, and the type→barb table is the one the properties are stated over.
# It exists twice — `Primitive::barbs()` in `src/sdk/src/capability.rs` for the Rust side, and the
# `PrimitiveType` definitions in `Types.lean` for the model — and a claim that one mirrors the other
# is worth exactly what the check behind it is worth.
#
# WHAT IT DOES. Extracts `Primitive::X => &[Barb::A, Barb::B]` from the Rust match arm and
# `{ name := "X", barbs := {Barb.a, Barb.b} }` from the Lean definitions, normalises both to a
# primitive name plus a kebab-case barb set, and diffs. It reports three things separately: types
# present in both implementations (where the sets must be equal), types the Lean model carries and
# Rust does not, and the reverse. Only the first is a violation.
#
# Usage: contrib/primitive_barbs_diff.sh
# Exit status: 1 if a shared primitive's barb set differs, or if Rust carries a primitive the model
#              does not; 0 otherwise. A model-only type is reported and is not a failure.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="/tmp/primitive-barbs-diff.txt"
LEAN=proofs/lean/src/DarkFi/Capability/Types.lean
RS=src/sdk/src/capability.rs

for f in "$LEAN" "$RS"; do
    [ -f "$f" ] || { echo "primitive_barbs_diff: missing $f" >&2; exit 2; }
done

python3 - "$LEAN" "$RS" "$OUT" <<'PY'
import re, sys

lean_f, rs_f, out_f = sys.argv[1:4]

def kebab(name: str) -> str:
    name = name.strip()
    if '-' in name:
        return name.lower()
    return re.sub(r'(?<!^)(?=[A-Z])', '-', name).lower()

def rust_table(text: str) -> dict[str, set[str]]:
    """`Primitive::X => &[Barb::A, ...]` arms of the `barbs` match."""
    m = re.search(r'pub fn barbs\(self\).*?match self \{(.*?)\n        \}', text, re.S)
    body = m.group(1) if m else ''
    table = {}
    for arm in re.finditer(r'Primitive::(\w+)\s*=>\s*&\[([^\]]*)\]', body):
        name, barbs = arm.group(1), arm.group(2)
        table[name] = {kebab(b) for b in re.findall(r'Barb::(\w+)', barbs)}
    return table

def lean_table(text: str) -> dict[str, set[str]]:
    """`def <ident> : PrimitiveType := { name := "X", barbs := {Barb.a, ...} }`, keyed by name.

    Lean's field syntax puts the separator *first* on the continuation line —
    `{ name := "SecretKey"` / `, barbs := {Barb.spend} }` — so the two fields are not on one line and
    a single-line pattern matches nothing. Matching the definition block and reading the fields out
    of it is both simpler and less brittle than one pattern per layout.
    """
    table = {}
    for m in re.finditer(r'def\s+\w+\s*:\s*PrimitiveType\s*:=\s*\{((?:[^{}]|\{[^{}]*\})*?)\n\s*\}', text, re.S):
        block = m.group(1)
        nm = re.search(r'name\s*:=\s*"(\w+)"', block)
        bs = re.search(r'barbs\s*:=\s*\{([^}]*)\}', block)
        if nm and bs:
            table[nm.group(1)] = {kebab(b) for b in re.findall(r'Barb\.(\w+)', bs.group(1))}
    return table

rs = rust_table(open(rs_f).read())
lean = lean_table(open(lean_f).read())

shared = sorted(set(rs) & set(lean))
rs_only = sorted(set(rs) - set(lean))
lean_only = sorted(set(lean) - set(rs))

lines = []
lines.append(f"{'type':<20} {'Rust':<34} {'Lean':<34} verdict")
lines.append('-' * 100)
mismatches = []
for t in shared:
    r, l = sorted(rs[t]), sorted(lean[t])
    ok = set(r) == set(l)
    if not ok:
        mismatches.append(t)
    lines.append(f"{t:<20} {' '.join(r):<34} {' '.join(l):<34} {'agree' if ok else 'DIFFER'}")

lines.append('')
lines.append(f"counts: Rust {len(rs)} primitives, Lean {len(lean)} primitives, {len(shared)} shared")
lines.append('')
def show(label, names): lines.append(f"{label}: {', '.join(names) if names else '(none)'}")
show('in Rust, not in the model (a violation)', rs_only)
show('in the model, not in Rust (not a violation)', lean_only)
if mismatches:
    lines.append('')
    lines.append(f"barb-set mismatches: {', '.join(mismatches)}")

report = '\n'.join(lines) + '\n'
open(out_f, 'w').write(report)
print(report)

sys.exit(0 if (not mismatches and not rs_only) else 1)
PY
status=$?
echo "primitive_barbs_diff: written to $OUT" >&2
exit "$status"
