#!/usr/bin/env bash
#
# The capability kernel's two rules, over `bin/dww/src/**` — the wallet is a generic engine, and the
# places where it is not are enumerated rather than assumed.
#
# WHY THIS EXISTS. `wallet-capability-kernel` states the architecture in one sentence: the wallet
# discovers capabilities from *any* contract by AEAD decryption, the AEAD tag is the discriminator, and
# "new contracts work without wallet code changes". `wallet.md` states the same in its own words twice —
# §6.4 ("the write path SHALL have exactly one bespoke citizen: **NativeToken**… every other contract
# SHALL be constructed generically from its manifest") and §9 ("one bespoke scan path; a second bespoke
# path SHALL be rejected"). Both are prose. Nothing checked them, so a second bespoke path would have
# been a code review's problem rather than a red gate — which is the same defect this campaign found in
# `src/Main.lean` (a claim with no instrument) and in the barb alphabet (agreement checked by a script
# nobody ran).
#
# WHAT IT CHECKS, and the two rules fail differently on purpose:
#
#   1. **The ratchet.** Every contract-specific scan function in the wallet must be one of the two
#      NativeToken ones. A name matching `(scan|discover)_<contract>_…` is bespoke by construction, so a
#      new one fails this gate however well written it is — and the fix is to route it through the
#      generic path or to argue in a register row that a *second* bespoke citizen is correct, which is a
#      decision and not an oversight.
#
#   2. **The declared absence.** §6.4.1's invariant 2 says "the selected capability is filtered by
#      `ContractId` + barbs, never `caps[0]`". Measured 2026-09-24, the wallet's write path filters by
#      **asset id and contract** — `dispatch.rs` and `lib.rs` contain no `Barb::`, no `required_barbs`
#      and no `covers(`, so the *barb* half of that invariant is not implemented on the write path at
#      all. That is not a defect this gate can fix and it is not a thing to leave implicit: it is
#      **declared here with its reason**, and the gate fails if the declaration goes stale — i.e. if a
#      barb predicate appears in either file, the absence is over and this declaration must be updated or
#      removed. An allowlist that never expires is the "instrument that cannot report its own failure"
#      defect with a longer half-life; this one expires.
#
# WHAT IT DOES NOT CHECK. Whether the generic path works (the wallet's own tests and the pipeline are
# that), whether the two NativeToken functions are correct, and nothing about the *contracts'* manifests.
# It reads source text, so a barb predicate reached through a helper in a third file would not be seen
# — the declared absence is about these two files, which is where the selection logic lives.
#
# Usage: scripts/check-wallet-kernel.sh
# Exit status: 1 on a second bespoke path, or on the declared absence going stale; 0 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="/tmp/wallet-kernel.txt"
SRC=bin/dww/src

[ -d "$SRC" ] || { echo "check-wallet-kernel: missing $SRC" >&2; exit 2; }

python3 - "$SRC" "$OUT" <<'PY'
import re, sys, pathlib

src_dir, out_f = sys.argv[1], sys.argv[2]

# The one bespoke citizen, and the two functions that serve it. Declared rather than derived: §6.4 names
# NativeToken, and nothing about a name pattern can tell you *why* an exception is legitimate.
NATIVE_TOKEN_SCAN = {
    'scan_native_token_contract_calls': 'Path 1: the coinbase/transfer scan, §6.4\'s one bespoke citizen',
    'discover_native_token_outputs': 'Path 1: the discovery half, same citizen',
}

# Functions that scan a *block* rather than a contract — the generic machinery, not a bespoke path.
GENERIC_SCAN = {'scan_block', 'scan_blocks', 'scan_block_linear', 'scan_cache'}

# Rule 2's declaration: the write path's selection does not read a barb predicate. Each entry is
# `file: what is absent, and why that is the current state`.
DECLARED_ABSENT = {
    'bin/dww/src/dispatch.rs':
        'the write path\'s selection filters by asset id and contract; §6.4.1 invariant 2\'s barb half is '
        'not implemented here (measured 2026-09-24) — the §6.2 bullet in wallet.md says so too',
    'bin/dww/src/lib.rs':
        'same: `get_held_capabilities` returns the held set and the callers filter by asset id, with no '
        'barb predicate anywhere in the file',
}

BARB_MARKERS = ['Barb::', 'required_barbs', 'covers(']

errors, lines = [], []

# --- Rule 1: bespoke scan paths ---------------------------------------------------------------
bespoke = {}
for path in sorted(pathlib.Path(src_dir).rglob('*.rs')):
    for n, line in enumerate(path.read_text(encoding='utf-8', errors='replace').splitlines(), 1):
        m = re.search(r'\bfn\s+((?:scan|discover)_[a-z0-9_]+)', line)
        if not m:
            continue
        name = m.group(1)
        if name in GENERIC_SCAN or name in NATIVE_TOKEN_SCAN:
            continue
        bespoke.setdefault(name, []).append(f'{path}:{n}')

lines.append('bespoke scan paths (contract-specific functions under bin/dww/src):')
for name, why in sorted(NATIVE_TOKEN_SCAN.items()):
    lines.append(f'  declared   {name:<34} {why}')
lines.append(f'  (generic block-level machinery, not counted: {", ".join(sorted(GENERIC_SCAN))})')
if bespoke:
    lines.append('')
    lines.append('** AN UNDECLARED BESPOKE SCAN PATH — fails the gate **')
    for name, sites in sorted(bespoke.items()):
        lines.append(f'  {name:<34} {", ".join(sites)}')
    errors.extend(f'bespoke scan function `{name}` at {", ".join(sites)} is not a declared '
                  f'NativeToken path — either a second bespoke path (§6.4/§9 forbid it) or a rename of '
                  f'a declared one; the declaration in this script must be updated either way'
                  for name, sites in bespoke.items())

# --- Rule 2: the declared absence, and its expiry ----------------------------------------------
lines.append('')
lines.append('declared absences (each must still be absent):')
for rel, why in sorted(DECLARED_ABSENT.items()):
    path = pathlib.Path(rel)
    if not path.exists():
        errors.append(f'DECLARED_ABSENT names {rel}, which does not exist')
        lines.append(f'  MISSING  {rel} — the declaration is stale')
        continue
    text = path.read_text(encoding='utf-8', errors='replace')
    found = [mk for mk in BARB_MARKERS if mk in text]
    if found:
        errors.append(f'{rel} now contains {found}: the declared absence is over — update or remove '
                      f'the entry in scripts/check-wallet-kernel.sh')
        lines.append(f'  EXPIRED  {rel} now mentions {found} — declared absence is stale')
    else:
        lines.append(f'  absent   {rel}: {why}')

# --- Rule 1's other direction: a declared path that has gone -----------------------------------
for name, why in sorted(NATIVE_TOKEN_SCAN.items()):
    if not any(name in pathlib.Path(p).read_text(encoding='utf-8', errors='replace')
               for p in pathlib.Path(src_dir).rglob('*.rs')):
        errors.append(f'the declared NativeToken scan function `{name}` no longer exists — the '
                      f'declaration is stale')
        lines.append(f'  MISSING  {name} — declared as {why}, not found')

lines.append('')
lines.append(f'{len(errors)} violation(s)' if errors else 'no violations')
report = '\n'.join(lines) + '\n'
open(out_f, 'w').write(report)
print(report)
sys.exit(1 if errors else 0)
PY
status=$?
echo "check-wallet-kernel: written to $OUT" >&2
exit "$status"
