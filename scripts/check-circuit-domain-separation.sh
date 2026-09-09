#!/bin/bash
# HAZOP RC-D: Verify all ZK circuits have domain-separated poseidon_hash calls.
# V2 circuits prepend DOMAIN_* constants (witness_base(1..7)) to every hash.
# V1 circuits use bare poseidon_hash(inputs...) — this script catches them.
#
# The 2026-09 rewrite replaces the original per-LINE grep with a call-level
# scan: comment lines are skipped, and a call counts as compliant when its
# argument list (through the matching closing paren, across line breaks)
# contains DOMAIN_ or witness_base. The line-based version produced 44 false
# positives — 8 comment lines and 36 multi-line calls whose DOMAIN_* constant
# sits on the following line.
#
# Exit 0: all circuits have domain separation
# Exit 1: found undifferentiated poseidon_hash calls

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import glob, os, re, sys

repo = os.environ["REPO_ROOT"]
bad = []

for path in sorted(glob.glob(f"{repo}/src/contract/*/proof/*.zk")):
    src = open(path).read()
    # Blank comment lines in place so character offsets/line numbers survive:
    # zkas comments are full-line `#`; `//` handled defensively (unused today).
    lines = []
    for ln in src.split("\n"):
        ln = ln.split("//")[0]
        if ln.lstrip().startswith("#"):
            ln = ""
        lines.append(ln)
    scannable = "\n".join(lines)

    for m in re.finditer(r'poseidon_hash\s*\(', scannable):
        i = m.end() - 1  # index of the opening '('
        depth = 0
        j = i
        end = None
        while j < len(scannable):
            if scannable[j] == "(":
                depth += 1
            elif scannable[j] == ")":
                depth -= 1
                if depth == 0:
                    end = j
                    break
            j += 1
        if end is None:
            continue  # malformed call — not our corpus
        call_args = scannable[i + 1:end]
        if not re.search(r'DOMAIN_|witness_base', call_args):
            lineno = src[:m.start()].count("\n") + 1
            bad.append(f"{path}:{lineno}: poseidon_hash call without DOMAIN_/witness_base")

if not bad:
    print("PASS: All contract circuits have domain-separated poseidon_hash calls")
    sys.exit(0)

print("FAIL: Found undifferentiated poseidon_hash calls (missing DOMAIN_ prefix):")
for line in bad:
    print(line)
print("")
print("Fix: prepend the appropriate DOMAIN_ constant to each poseidon_hash call.")
print("  DOMAIN_NULLIFIER     = witness_base(1)")
print("  DOMAIN_TOKEN_COMMIT  = witness_base(2)  (some circuits: DOMAIN_TOK_COMMIT)")
print("  DOMAIN_TX_BINDING    = witness_base(3)")
print("  DOMAIN_COMMITMENT    = witness_base(4)  (dex: DOMAIN_CAP_COMMIT)")
print("  DOMAIN_MERKLE_LEAF   = witness_base(5)")
print("  DOMAIN_USER_DATA_ENC = witness_base(6)")
print("  DOMAIN_SIGNATURE_SECRET = witness_base(7)")
sys.exit(1)
PYEOF
