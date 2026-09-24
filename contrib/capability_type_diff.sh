#!/usr/bin/env bash
#
# The capability-type correspondence, extracted from all three implementations and diffed.
#
# WHY THIS EXISTS. `barb_alphabet_diff.sh` checks that the four *barb* vocabularies agree and
# `primitive_barbs_diff.sh` checks which barbs each *primitive* exhibits. Both are about primitives.
# Neither can see the claim `capability.rs:509-510` makes in prose — *"Every construction that is
# proved in Lean4 must also succeed here"* — which is about **capability types**, and nothing
# mechanized it. Measured 2026-09-24: `Composition.lean` proves 14 types, the Rust test module
# constructs 9 of them, and four of the 14 cannot be constructed by `wallet_construct` at all
# because the primitives they name (`dleqProof`, `bridgeAddress`, `chainDepositProof`,
# `bridgeCapNullifier`) have no `Primitive` variant to carry their barb. Five of the fourteen
# assertions that sentence promises are simply absent from the file that promises them, and nothing
# said so. This gate says so on every run.
#
# WHAT IT DOES. Extracts three tables and joins them on `(resource, action)` — the pair a caller
# constructs by, and the only thing all three implementations state as a string:
#   * Lean — `<ident> : CapabilityType <res> <act> := { primitives := [...] }`, with the resource's
#     `requiredBarbs` resolved through its `def` and each primitive identifier resolved through its
#     `PrimitiveType` def in `Types.lean`;
#   * Rust — the positive `wallet_construct(...)` calls in `capability.rs`'s test module. A call
#     whose following assert is `is_none` is a *negative control* (it must not construct) and is
#     reported rather than diffed;
#   * Python — `wallet_construct("r", "s", PRIMS, BARBS)` in `wallet_model.py` where both list
#     arguments are module-level constants. A call whose arguments are locals is reported as
#     unresolved, never silently dropped.
#
# Verdicts per `(resource, action)` pair: `agree`; `DIFFER`; `Rust-unconstructible` (the type names
# a primitive with no Rust variant); `untested` (Rust could construct it and has no test);
# `Rust-only`, `Python-only`, `Lean-only`. The kebab-case normalisation is the sibling gates', so a
# primitive that shares a spelling there shares it here.
#
# WHAT FAILS THE GATE — four things, and each is falsifiable by editing one side:
#   1. a `DIFFER`: a pair present in two implementations whose primitive sets disagree;
#   2. a Lean type that no Rust test constructs and that `KNOWN_ABSENT` below does not declare;
#   3. a `Rust-only` or `Python-only` pair that the same tables do not declare;
#   4. a declared entry that has gone **stale** — the type is constructed now, the blocking
#      primitive exists now, or the declared divergence now agrees.
# Condition 4 is the point of the lists. They are a statement about today's tree, not a permanent
# excuse, and the tree's other allowlists (`check-register-artifacts.sh`) are checked the same way.
#
# WHAT IT DOES NOT CHECK, and cannot: whether `coversBarbs` holds (the kernel does, `by decide`),
# whether a constructed type's barbs are *sufficient* for the action (each implementation's own
# tests), or the Python model's **coverage** of the type space — the Python declares a table for 1
# of the 14, which is reported below as a fact about the model and is not this gate's business to
# fail on.
#
# Usage: contrib/capability_type_diff.sh
# Exit status: 1 on any of conditions 1-4; 0 otherwise. 2 if a source file is missing.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="/tmp/capability-type-diff.txt"
LEAN_COMP=proofs/lean/src/DarkFi/Capability/Composition.lean
LEAN_TYPES=proofs/lean/src/DarkFi/Capability/Types.lean
RS=src/sdk/src/capability.rs
PY=contrib/model/wallet_model.py

for f in "$LEAN_COMP" "$LEAN_TYPES" "$RS" "$PY"; do
    [ -f "$f" ] || { echo "capability_type_diff: missing $f" >&2; exit 2; }
done

python3 - "$LEAN_COMP" "$LEAN_TYPES" "$RS" "$PY" "$OUT" <<'PY'
import re, sys

lean_comp_f, lean_types_f, rs_f, py_f, out_f = sys.argv[1:6]

# ---------------------------------------------------------------------------------------------
# The declared state of the tree. Every entry is checked for staleness (condition 4).
# ---------------------------------------------------------------------------------------------

# Lean types no Rust test constructs. `(blocking primitive idents | None, why)`.
KNOWN_ABSENT = {
    'tenderBidType':      (['dleqProof'],
                           'needs dleqProof, which has no Primitive variant'),
    'bridgeDepositType':  (['bridgeAddress', 'chainDepositProof'],
                           'needs bridgeAddress and chainDepositProof, neither of which has a '
                           'Primitive variant'),
    'bridgeWithdrawType': (['bridgeAddress', 'bridgeCapNullifier', 'dleqProof'],
                           'needs bridgeAddress, bridgeCapNullifier and dleqProof, none of which '
                           'has a Primitive variant'),
    'oracleOperatorType': (['dleqProof'],
                           'needs dleqProof, which has no Primitive variant'),
    'purseDepositType':   (None,
                           'constructible in Rust — every primitive it names has a variant — and '
                           'no test constructs it'),
}

# Pairs present in one implementation and not the Lean model, declared. Empty: every pair either
# implementation constructs today is a Lean type. The mechanism stays because it is the check that
# fires if one of them invents a type the model does not have.
KNOWN_ONLY = {}

# The Python model states its capability tables as module-level constant pairs, and its runtime gate
# takes them as *arguments* — `wallet_construct(resource, action, primitives, required_barbs)` inside
# `select_caps_covering`, called once with `NATIVE_TRANSFER_*`. So the (resource, action) a table
# belongs to is a declared fact here rather than an extracted one, and it is checked both ways: every
# name must exist, and every `*_PRIMITIVES` / `*_REQUIRED_BARBS` constant must be claimed.
PY_TABLES = {
    ('native_token', 'transfer'): ('NATIVE_TRANSFER_PRIMITIVES', 'NATIVE_TRANSFER_REQUIRED_BARBS'),
}

# Pairs whose *required barbs* disagree between implementations. Keyed by pair, valued by the
# explanation. A pair here that now agrees everywhere is stale and fails the gate.
KNOWN_BARB_DIVERGENCES = {
    ('native_token', 'transfer'):
        'Python requires prove-inclusion as well (7 barbs to Lean/Rust 6). The composition covers '
        'it either way, so nothing is unconstructible — but the three sides do not state the same '
        'requirement for the same resource.',
}

# ---------------------------------------------------------------------------------------------
# Extraction
# ---------------------------------------------------------------------------------------------

def kebab(name):
    """The sibling gates' normalisation: `ProveInclusion` -> `prove-inclusion`, kept verbatim so
    the same primitive is spelled the same way in all three tables."""
    name = name.strip()
    if '-' in name:
        return name.lower()
    return re.sub(r'(?<!^)(?=[A-Z])', '-', name).lower()


def defs_with_body(text, head_pat, nheads):
    """`def <head_pat> := { ... }` — the body is matched with one level of nested braces allowed.
    Returns `(head_groups, body, start)`."""
    pat = r'def\s+' + head_pat + r'\s*:=\s*\{((?:[^{}]|\{[^{}]*\})*?)\s*\}'
    out = []
    for m in re.finditer(pat, text, re.S):
        out.append((m.groups()[:nheads], m.group(nheads + 1), m.start()))
    return out


errors = []

# --- Lean: primitive ident -> kebab name ; resource/action ident -> name ; the 14 types ---------
lt = open(lean_types_f).read()
lean_prim_name = {}
for (ident,), block, _ in defs_with_body(lt, r'(\w+)\s*:\s*PrimitiveType', 1):
    nm = re.search(r'name\s*:=\s*"([^"]*)"', block)
    if nm:
        lean_prim_name[ident] = kebab(nm.group(1))

lc = open(lean_comp_f).read()
lean_resources = {}
for (ident,), block, _ in defs_with_body(lc, r'(\w+)\s*:\s*Resource', 1):
    nm = re.search(r'name\s*:=\s*"([^"]*)"', block)
    bs = re.search(r'requiredBarbs\s*:=\s*\{([^}]*)\}', block)
    lean_resources[ident] = (
        nm.group(1) if nm else None,
        {kebab(b) for b in re.findall(r'Barb\.(\w+)', bs.group(1))} if bs else set(),
    )

lean_actions = {}
for m in re.finditer(r'def\s+(\w+)\s*:\s*Action\s*:=\s*\{\s*name\s*:=\s*"([^"]*)"\s*\}', lc):
    lean_actions[m.group(1)] = m.group(2)

lean = {}          # (res, act) -> dict
lean_defname = {}  # def name -> (res, act)
for (ident, rident, sident), block, _ in defs_with_body(
        lc, r'(\w+)\s*:\s*CapabilityType\s+(\w+)\s+(\w+)', 3):
    if rident not in lean_resources or sident not in lean_actions:
        errors.append(f'Lean: `{ident}` names unknown resource/action `{rident}`/`{sident}`')
        continue
    pm = re.search(r'primitives\s*:=\s*\[([^\]]*)\]', block, re.S)
    if not pm:
        errors.append(f'Lean: `{ident}` has no `primitives := [...]`')
        continue
    idents = [p.strip() for p in pm.group(1).split(',') if p.strip()]
    prims, missing = [], []
    for p in idents:
        if p not in lean_prim_name:
            missing.append(p)
        else:
            prims.append(lean_prim_name[p])
    if missing:
        errors.append(f'Lean: `{ident}` names primitive(s) with no PrimitiveType def: {missing}')
    rname, rbarbs = lean_resources[rident]
    key = (rname, lean_actions[sident])
    lean_defname[ident] = key
    lean[key] = {'def': ident, 'prims': set(prims), 'barbs': rbarbs,
                 'raw_idents': idents}

# --- Rust: the primitive vocabulary, and the test module's constructions -------------------------
rs = open(rs_f).read()
m = re.search(r'pub fn barbs\(self\).*?match self \{(.*?)\n        \}', rs, re.S)
rust_prim_names = set()
if m:
    for arm in re.finditer(r'Primitive::(\w+)\s*=>', m.group(1)):
        rust_prim_names.add(kebab(arm.group(1)))
else:
    errors.append('Rust: could not find `Primitive::barbs`')

rust = {}
rust_controls = []
for m in re.finditer(r'wallet_construct\(', rs):
    seg = rs[m.end():m.end() + 1200]
    end = seg.find(');')
    body = seg[:end] if end >= 0 else seg
    call = re.match(r'\s*"([^"]*)"\s*,\s*"([^"]*)"\s*,\s*vec!\[([^\]]*)\]\s*,'
                    r'\s*&\[([^\]]*)\]\s*,?\s*$', body, re.S)
    if not call:
        continue
    res, act, prims, barbs = call.groups()
    prims = {kebab(p) for p in re.findall(r'Primitive::(\w+)', prims)}
    barbs = {kebab(b) for b in re.findall(r'Barb::(\w+)', barbs)}
    tail = rs[m.end() + end:m.end() + end + 400] if end >= 0 else ''
    am = re.search(r'assert!\(ct\.(is_some|is_none)\(\),\s*"([^"]*)"', tail)
    kind = am.group(1) if am else 'unknown'
    msg = am.group(2) if am else ''
    if kind == 'is_none':
        rust_controls.append((res, act, msg))
    elif kind == 'is_some':
        rust[(res, act)] = {'prims': prims, 'barbs': barbs, 'msg': msg}
    else:
        errors.append(f'Rust: `wallet_construct(\"{res}\", \"{act}\", ...)` has no '
                      f'is_some/is_none assert to classify it')

# --- Python: module-level constant lists, then the calls that reference them ---------------------
py = open(py_f).read()
consts = {}
for m in re.finditer(r'^([A-Z][A-Z0-9_]*)\s*=\s*\[(.*?)^\]', py, re.S | re.M):
    consts[m.group(1)] = m.group(2)

python = {}
claimed = set()
for (res, act), (plist, blist) in sorted(PY_TABLES.items()):
    for nm in (plist, blist):
        if nm not in consts:
            errors.append(f'Python: PY_TABLES names `{nm}` for {res}/{act}, which is not a '
                          f'module-level list constant')
    if plist in consts and blist in consts:
        claimed |= {plist, blist}
        python[(res, act)] = {
            'prims': {kebab(p) for p in re.findall(r'Primitive\.(\w+)', consts[plist])},
            'barbs': {kebab(b) for b in re.findall(r'Barb\.(\w+)', consts[blist])},
            'consts': (plist, blist),
        }

# The direction that matters when the model grows: a table the model carries and this gate does
# not know about. Only the two table suffixes, so the enum and `WALLET_SQL` are not caught.
for nm in sorted(consts):
    if nm not in claimed and (nm.endswith('_PRIMITIVES') or nm.endswith('_REQUIRED_BARBS')):
        errors.append(f'Python: module-level table `{nm}` is in the model and not in PY_TABLES — '
                      f'the gate cannot see it; register which (resource, action) it belongs to')

# The model's *runtime* gate: `wallet_construct("r", "s", <locals>)` inside `select_caps_covering`
# and its tests. These are the pairs the model actually constructs by, and they are reported rather
# than diffed — the composition they gate on arrives as function arguments, so only a reader (or a
# dataflow pass this gate is not) can say which table a given caller supplies.
py_calls = []
for m in re.finditer(r'wallet_construct\(\s*"([^"]*)"\s*,\s*"([^"]*)"\s*,\s*(\w+)\s*,\s*(\w+)\s*\)',
                     py):
    res, act, plist, blist = m.groups()
    py_calls.append((res, act, plist, blist, plist in consts and blist in consts))

# ---------------------------------------------------------------------------------------------
# Join on (resource, action)
# ---------------------------------------------------------------------------------------------

all_keys = sorted(set(lean) | set(rust) | set(python))
fail = []

def verdict(k):
    l, r, p = lean.get(k), rust.get(k), python.get(k)
    if l and r and l['prims'] != r['prims']:
        return 'DIFFER'
    if l and p and l['prims'] != p['prims']:
        return 'DIFFER'
    if r and p and r['prims'] != p['prims']:
        return 'DIFFER'
    if l and not r:
        if any(pr not in rust_prim_names for pr in l['prims']):
            return 'Rust-unconstructible'
        return 'untested'
    if l and r and not p:
        return 'agree'
    if r and not l:
        return 'Rust-only'
    if p and not l:
        return 'Python-only'
    if l and r and p:
        return 'agree'
    return 'Lean-only'

lines = []
lines.append(f'{"capability type (resource/action)":<40} {"Lean":>4} {"Rust":>4} {"Py":>3}  verdict')
lines.append('-' * 88)
for k in all_keys:
    l, r, p = lean.get(k), rust.get(k), python.get(k)
    label = f'{k[0]}/{k[1]}'
    lines.append(f'{label:<40} {len(l["prims"]) if l else "-":>4} '
                 f'{len(r["prims"]) if r else "-":>4} {len(p["prims"]) if p else "-":>3}  '
                 f'{verdict(k)}')

# --- the DIFFERs, named --------------------------------------------------------------
differs = [k for k in all_keys if verdict(k) == 'DIFFER']
lines.append('')
lines.append(f'counts: Lean {len(lean)} capability types, Rust {len(rust)} constructed, '
             f'Python {len(python)} tables, {len(all_keys)} pairs')
if differs:
    lines.append('')
    lines.append('PRIMITIVE-SET DIFFERENCES — these fail the gate:')
    for k in differs:
        l, r, p = lean.get(k), rust.get(k), python.get(k)
        for label, side in (('Rust', r), ('Python', p)):
            if side is None or (l and l['prims'] == side['prims']):
                continue
            lines.append(f'  {k[0]}/{k[1]} vs {label}: Lean-only '
                         f'{sorted(l["prims"] - side["prims"]) if l else []}, '
                         f'{label}-only {sorted(side["prims"] - (l["prims"] if l else set()))}')
    fail.extend(f'primitive-set difference on {k[0]}/{k[1]}' for k in differs)

# --- known absent: declared, and checked for staleness --------------------------------
untested = sorted(k for k in all_keys if verdict(k) in ('Rust-unconstructible', 'untested'))
lines.append('')
lines.append('Lean types the Rust test module does not construct:')
declared_absent = set()
for k in untested:
    ident = lean[k]['def']
    declared = KNOWN_ABSENT.get(ident)
    if declared:
        declared_absent.add(ident)
        why = declared[1]
    else:
        why = '** UNDECLARED — fails the gate'
        fail.append(f'{ident} ({k[0]}/{k[1]}) is not constructed in Rust and not declared in '
                    f'KNOWN_ABSENT')
    lines.append(f'  {ident:<22} {f"({k[0]}/{k[1]})":<32} {verdict(k):<20} {why}')
missing_from_report = sorted(set(KNOWN_ABSENT) - declared_absent)
for ident in missing_from_report:
    fail.append(f'KNOWN_ABSENT declares `{ident}`, which is no longer absent — the entry is stale')
    lines.append(f'  STALE: KNOWN_ABSENT declares `{ident}`, which this run finds constructed '
                 f'(or absent from the Lean model). Remove the entry.')

# staleness of the *blocking reason*
for ident in sorted(declared_absent):
    blockers = KNOWN_ABSENT[ident][0] or []
    now_ok = [b for b in blockers if kebab(b) in rust_prim_names]
    if now_ok:
        fail.append(f'KNOWN_ABSENT entry `{ident}` says it is blocked by {now_ok}, which Rust can '
                    f'represent now — the entry is stale')
        lines.append(f'  STALE: `{ident}` is declared blocked by {now_ok}, now Rust-representable.')

# --- pairs present in only one implementation -----------------------------------------
only = sorted((side, k) for k in all_keys
              for side, src in (('rust', rust), ('python', python))
              if k in src and k not in lean)
undeclared_only = [(side, k) for side, k in only if (side, k[0] + '/' + k[1]) not in KNOWN_ONLY]
declared_only = [(side, k) for side, k in only if (side, k[0] + '/' + k[1]) in KNOWN_ONLY]
lines.append('')
if only:
    lines.append('pairs in one implementation and not in the Lean model:')
    for side, k in only:
        name = k[0] + '/' + k[1]
        if (side, name) in KNOWN_ONLY:
            lines.append(f'  {side + "-only":<12} {name:<28} declared: {KNOWN_ONLY[(side, name)]}')
        else:
            lines.append(f'  {side + "-only":<12} {name:<28} ** UNDECLARED — fails the gate')
    fail.extend(f'{side}-only pair {k[0]}/{k[1]} is undeclared' for side, k in undeclared_only)
for (side, name) in sorted(KNOWN_ONLY):
    if not any(k[0] + '/' + k[1] == name for s, k in only if s == side):
        fail.append(f'KNOWN_ONLY declares the {side}-only pair `{name}`, which this run does not '
                    f'find — the entry is stale')
        lines.append(f'  STALE: KNOWN_ONLY declares `{name}` {side}-only; this run does not.')

# --- required barbs -------------------------------------------------------------------
lines.append('')
lines.append('required barbs per pair (Lean resource / Rust test / Python table):')
barb_divergence = []
for k in all_keys:
    sides = [(n, src[k]['barbs']) for n, src in (('Lean', lean), ('Rust', rust), ('Python', python))
             if k in src]
    if len(sides) < 2:
        continue
    sets = {frozenset(s) for _, s in sides}
    if len(sets) == 1:
        continue
    barb_divergence.append(k)
    lines.append(f'  {k[0]}/{k[1]}: ' + ' · '.join(f'{n}={len(s)}' for n, s in sides))
    for n, s in sides:
        lines.append(f'      {n:<7} {sorted(s)}')
    if k in KNOWN_BARB_DIVERGENCES:
        lines.append(f'      declared: {KNOWN_BARB_DIVERGENCES[k]}')
    else:
        lines.append('      ** UNDECLARED — fails the gate')
        fail.append(f'required-barb divergence on {k[0]}/{k[1]} is undeclared')
for k in sorted(KNOWN_BARB_DIVERGENCES):
    if k not in barb_divergence:
        fail.append(f'KNOWN_BARB_DIVERGENCES declares {k[0]}/{k[1]}, which now agrees on every '
                    f'side — the entry is stale')
        lines.append(f'  STALE: KNOWN_BARB_DIVERGENCES declares {k[0]}/{k[1]}; it now agrees.')

# --- controls and unresolved Python ----------------------------------------------------
lines.append('')
lines.append('negative controls in the Rust test module (must NOT construct):')
for res, act, msg in rust_controls:
    lines.append(f'  {res}/{act}: {msg}')

lines.append('')
lines.append('Python `wallet_construct` calls — the model\'s runtime gate. The composition arrives '
             'as arguments, so these are what the model *gates on*, not tables:')
for res, act, plist, blist, resolved in py_calls:
    lines.append(f'  {res}/{act}: ({plist}, {blist})'
                 + ('' if resolved else '   [locals — not a table]'))

if errors:
    lines.append('')
    lines.append('EXTRACTION ERRORS — fails the gate:')
    for e in errors:
        lines.append('  ' + e)
    fail.extend(errors)

lines.append('')
if fail:
    lines.append(f'{len(fail)} violation(s):')
    for f in fail:
        lines.append('  ' + f)
else:
    lines.append('no violations')

report = '\n'.join(lines) + '\n'
open(out_f, 'w').write(report)
print(report)

sys.exit(1 if fail else 0)
PY
status=$?
echo "capability_type_diff: written to $OUT" >&2
exit "$status"
