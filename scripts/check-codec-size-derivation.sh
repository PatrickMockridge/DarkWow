#!/bin/bash
# OBL-C150, second instance: a fixed-size composite's `ENCODED_SIZE` is derived, not restated.
#
# The class this exists for is a **number**, and numbers drift silently. `PayInterestUpdateV1`
# carried `ENCODED_SIZE = 625` and sliced 272 bytes per `BondCommitment` while `encode` wrote
# `BondCommitment::ENCODED_SIZE` each — so `encode` produced 753 bytes and `decode` demanded 625
# and refused its own output. It was live for as long as `BondCommitment` had been anything but
# 272 (i.e. always), and nothing saw it: a contract `Err` crossing the wasm boundary carries a
# **code**, not a message, so the failure arrived as the catch-all `IoError("Unknown")` with
# neither the type nor the length named. It surfaced only when the endpoint behind it became
# reachable — the first run ever to reach `apply_pay_interest`.
#
# The repair was not to correct 625 to 753. It was to make the constant **derive** from the
# things it summarises: `336 + 336 + 32 + 8 + 41`. A constant that restates a size it could
# compute is a second copy of a fact, and two copies disagree eventually — which is the whole
# lesson `OBL-C150` and its sibling `OBL-C199` keep teaching.
#
# WHAT IT FLAGS. A type whose `encode` is **fixed-size** (it uses neither `SerializedLen` nor a
# length-dependent capacity, so its output length is a compile-time constant) and **composite**
# (it concatenates at least one nested `.encode()`), whose `ENCODED_SIZE` is a **single bare
# integer**. That lone integer is exactly the shape that went stale: it names a total while the
# parts it totals are free to move. A variable-length type's `ENCODED_SIZE` is its fixed *prefix*
# (e.g. `UnlockUpdateV1 = 8` before a length-prefixed fund) and a bare integer is correct there.
#
# WHAT IT DOES NOT SEE, stated because a reader will otherwise assume more: a **sum of literals**
# (`336 + 336 + 32 + 8 + 41`) is accepted here and is a weaker state than naming the nested types'
# `ENCODED_SIZE` — if `BondCommitment`'s size moves, those 336s go stale again. The ideal is
# `2 * BondCommitment::ENCODED_SIZE + 32 + 8 + RequestedClaim::ENCODED_SIZE`, and this gate's
# parenthetical is that a term standing for a nested wire type should name that type. Enforcing it
# as the rule rather than a note would also flag the one site already repaired this way, so it is
# recorded rather than required. Nor does it verify that an accepted sum is *arithmetically*
# equal to what `encode` writes — only that it is not a lone restatement. Equal-but-derived is
# the point; this gate cannot compute the sum, and does not claim to.
#
# REPORT-ONLY ON THE FIRST RUN, following `check-store-key-agreement.sh`'s precedent: the census
# is a measurement, not a clean bill. Populate `script/codec_size_derivation_exceptions.txt` with
# the sites that are intended to stay as they are to turn this into a ratchet; `--strict` fails on
# any undeclared site today.
#
# Exit 0: no undeclared site (or report-only). Exit 1: `--strict` and an undeclared site.
# Exit 2: `--self-test` failed, or the exceptions file is malformed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

STRICT=0
SELF_TEST=0
for arg in "$@"; do
    case "$arg" in
        --strict) STRICT=1 ;;
        --self-test) SELF_TEST=1 ;;
        --report-only) STRICT=0 ;;
        *) echo "usage: $0 [--strict] [--report-only] [--self-test]" >&2; exit 2 ;;
    esac
done

STRICT="$STRICT" SELF_TEST="$SELF_TEST" REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import glob, os, re, sys

repo = os.environ["REPO_ROOT"]
strict = os.environ["STRICT"] == "1"
self_test = os.environ["SELF_TEST"] == "1"

IMPL_START = re.compile(r"^\s*impl\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>]*>)?\s*\{")
SIZE_DECL = re.compile(r"pub\s+const\s+ENCODED_SIZE\s*:\s*usize\s*=\s*([^;]+);")
BARE_INT = re.compile(r"^\s*\d+\s*$")

def impls(text):
    """Yield (type_name, body_text) per `impl <Type> { ... }` block.

    The end of an impl is a line whose first character is `}` at column zero — the layout the
    tree uses for a top-level impl. A single-line `impl X { ... }` is captured whole.
    """
    out = []
    cur = None
    buf = []
    for line in text.split("\n"):
        if cur is None:
            m = IMPL_START.match(line)
            if m:
                cur = m.group(1)
                rest = line[m.end():]
                buf = [rest]
                if rest.rstrip().endswith("}") and rest.count("}") > rest.count("{"):
                    out.append((cur, "\n".join(buf)))
                    cur = None
            continue
        buf.append(line)
        if line.startswith("}"):
            out.append((cur, "\n".join(buf)))
            cur = None
    if cur is not None:
        out.append((cur, "\n".join(buf)))
    return out

def findings_for(path):
    """Return [(type, expr)] for fixed-size composites with a single bare ENCODED_SIZE."""
    try:
        text = open(path, encoding="utf-8").read()
    except OSError:
        return []
    found = []
    for ty, body in impls(text):
        m = SIZE_DECL.search(body)
        if not m:
            continue
        expr = m.group(1).strip()
        if not BARE_INT.match(expr):
            continue
        enc = body.split("fn decode", 1)[0]
        composite = ".encode()" in enc
        variable = ("SerializedLen" in enc) or (".len()" in enc)
        if composite and not variable:
            found.append((ty, expr))
    return found

# --- self-test -----------------------------------------------------------------------------------
SELF_BARE = '''
impl FooV1 {
    pub const ENCODED_SIZE: usize = 81;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(81);
        b.extend_from_slice(&self.bond_commitment.to_repr());
        b.extend_from_slice(&self.claim_block.to_le_bytes());
        b.extend_from_slice(&self.claim.encode());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 81 { return Err(e) } Ok(()) }
}
'''
SELF_DERIVED = '''
impl FooV1 {
    pub const ENCODED_SIZE: usize = 32 + 8 + RequestedClaim::ENCODED_SIZE;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.bond_commitment.to_repr());
        b.extend_from_slice(&self.claim_block.to_le_bytes());
        b.extend_from_slice(&self.claim.encode());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(e) } Ok(()) }
}
'''
# A variable-length type: a bare ENCODED_SIZE is the fixed prefix, and it is correct.
SELF_VARIABLE = '''
impl FooV1 {
    pub const ENCODED_SIZE: usize = 8;
    pub fn encode(&self) -> Vec<u8> {
        let fund = self.fund.encode()?;
        let len = SerializedLen::try_from_len(fund.len())?;
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE + 4 + fund.len());
        buf.extend_from_slice(&self.unlocked_at.to_le_bytes());
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&fund);
        Ok(buf)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::ENCODED_SIZE + 4 { return Err(e) } Ok(()) }
}
'''

if self_test:
    import tempfile
    ok = True
    cases = (("bare.rs", SELF_BARE, True),
             ("derived.rs", SELF_DERIVED, False),
             ("variable.rs", SELF_VARIABLE, False))
    with tempfile.TemporaryDirectory() as d:
        for name, body, expect in cases:
            p = os.path.join(d, name)
            open(p, "w").write(body)
            got = findings_for(p)
            if bool(got) != expect:
                print(f"FAIL --self-test: {name} gave {got!r}, expected finding={expect}", file=sys.stderr)
                ok = False
    if not ok:
        sys.exit(2)
    print("PASS --self-test: a lone literal on a fixed-size composite is reported; the same made to "
          "derive is not; and a variable-length type's fixed prefix is left alone")
    sys.exit(0)

# --- the corpus ----------------------------------------------------------------------------------
paths = sorted(glob.glob(os.path.join(repo, "src/contract/*/src/model/mod.rs")))
findings = []
for p in paths:
    rel = os.path.relpath(p, repo)
    for ty, expr in findings_for(p):
        findings.append((rel, ty, expr))

print(f"COVERAGE: scanned {len(paths)} contract model file(s).")

EXC = os.path.join(repo, "script/codec_size_derivation_exceptions.txt")
declared = set()
if os.path.exists(EXC):
    for n, line in enumerate(open(EXC, encoding="utf-8"), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = [p.strip() for p in line.split(":", 2)]
        if len(parts) < 3:
            print(f"FAIL: {EXC}:{n} is malformed — expected `path : type : reason`", file=sys.stderr)
            sys.exit(2)
        declared.add((parts[0], parts[1]))

seen = set()
undeclared = []
for rel, ty, expr in findings:
    if (rel, ty) in declared:
        seen.add((rel, ty))
    else:
        undeclared.append((rel, ty, expr))

stale = sorted(declared - seen)
if stale:
    print("NOTE: declared but no longer a finding — remove these lines:")
    for rel, ty in stale:
        print(f"  {rel} : {ty}")
    print()

if not undeclared:
    print(f"PASS: {len(findings)} site(s), all {len(declared)} declared — every fixed-size composite")
    print("      either derives its ENCODED_SIZE or is adjudicated as intended.")
    sys.exit(0)

print(f"REPORT: {len(undeclared)} fixed-size composite(s) whose ENCODED_SIZE is a lone integer:\n")
for rel, ty, expr in undeclared:
    print(f"  {rel}\n    {ty}::ENCODED_SIZE = {expr}\n")

print("A lone integer names a total whose parts are free to move — the shape `PayInterestUpdateV1`")
print("went stale on. The repair is to derive it (`32 + 8 + RequestedClaim::ENCODED_SIZE`), naming")
print(f"the nested types' constants. Declare intended exceptions in {os.path.relpath(EXC, repo)};")
print("--strict fails on any undeclared site today.")
sys.exit(1 if strict else 0)
PYEOF
