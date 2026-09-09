#!/bin/bash
# M-5 fix: Verify ZK circuit constrain_instance count <= metadata push count
# for genesis contracts.
#
# This CI gate catches mismatches where a circuit's public input count changes
# but the metadata function's zk_inputs vector isn't updated — the root cause of
# 3 prior CRITICAL findings (Box, Purse, PromissoryNote).
#
# Per-circuit: count constrain_instance in the .zk file, then find every
# metadata push carrying this circuit's namespace (ZKAS_<NAME>_NS) in the
# entrypoint and count the VALUES in its vec. Each per-circuit push count must
# be >= the constrain_instance count (metadata may push auxiliary values too).
# A circuit with no matching namespace push is a FAIL (the metadata fn is
# missing entirely).
#
# The 2026-09 rewrite fixes three false-positive bugs in the original
# line-grep metric: (1) push LINES were counted instead of pushed values (box/
# purse/multisig push a whole Vec in one call), (2) pushes were summed across
# every entrypoint .rs file and compared against each circuit individually,
# and (3) exec-side tuple pushes like input_sums.push((...)) matched the grep
# pattern and inflated counts.
#
# Exit 0: all circuits pass
# Exit 1: mismatch found

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import os, re, sys, glob

repo = os.environ["REPO_ROOT"]
# Genesis contracts only (8 contracts — the ones whose circuits ship with the
# chain; identity/attestation/oracle have no entrypoint dir and are SKIPped)
GENESIS = ["native_token", "box", "purse", "promissory_note", "identity",
           "attestation", "oracle", "multisig"]

def strip_line_comments(src):
    # Rust `//` comments. Mandatory: several vecs carry trailing inline
    # comments after the last element's comma, which a naive comma-split
    # would count as an extra value.
    return "\n".join(line.split("//")[0] for line in src.splitlines())

def split_top(s):
    """Split on commas at depth 0 (elements may contain nested calls)."""
    parts = []; depth = 0; cur = ""
    for ch in s:
        if ch in "([": depth += 1
        elif ch in ")]": depth -= 1
        if ch == "," and depth == 0:
            parts.append(cur.strip()); cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur.strip())
    return parts

def extract_push_vecs(src):
    """Every `push((<NS>.to_string(), vec![...]))` -> [(ns, value_count)]."""
    results = []
    for m in re.finditer(r'push\(\s*\(\s*([A-Z0-9_]+)\s*\.to_string\(\s*\)\s*,\s*vec!\[', src):
        ns = m.group(1)
        i = m.end()
        depth = 1; j = i
        while j < len(src) and depth > 0:
            if src[j] == "[": depth += 1
            elif src[j] == "]": depth -= 1
            j += 1
        if depth > 0:
            continue  # malformed vec literal — not our corpus
        results.append((ns, len(split_top(src[i:j - 1]))))
    return results

def strip_zk_comments(src):
    # zkas comments are full-line `#`; strip `//` defensively (unused today).
    out = []
    for line in src.splitlines():
        line = line.split("//")[0]
        if line.lstrip().startswith("#"):
            continue
        out.append(line)
    return "\n".join(out)

print("=== Circuit-Metadata Alignment Check ===")
print("")
passes = 0
failures = 0
for contract_name in GENESIS:
    proof_dir = f"{repo}/src/contract/{contract_name}/proof"
    if not os.path.isdir(proof_dir):
        continue
    contract_dir = f"{repo}/src/contract/{contract_name}"
    entrypoint_dir = f"{contract_dir}/src/entrypoint"
    entrypoint_files = glob.glob(f"{entrypoint_dir}/*.rs")
    if not entrypoint_files:
        entrypoint_src = ""
    else:
        entrypoint_src = strip_line_comments(
            "\n".join(open(f).read() for f in sorted(entrypoint_files)))
    pushes = extract_push_vecs(entrypoint_src) if entrypoint_src else []

    for zk_file in sorted(glob.glob(f"{proof_dir}/*.zk")):
        circuit_name = os.path.basename(zk_file)[:-3]
        zk_src = strip_zk_comments(open(zk_file).read())
        circuit_count = len(re.findall(r'constrain_instance\(', zk_src))

        if circuit_count == 0:
            print(f"WARN: {contract_name}/{circuit_name} — zero constrain_instance calls")
            continue
        if not entrypoint_files:
            print(f"SKIP: {contract_name}/{circuit_name} — no entrypoint/*.rs found")
            continue

        pattern = re.compile(rf'ZKAS_{circuit_name.upper()}_NS(_V2)?$')
        matched = [(ns, n) for (ns, n) in pushes if pattern.search(ns)]
        if not matched:
            print(f"FAIL: {contract_name}/{circuit_name} — circuit has {circuit_count} "
                  f"constrain_instance but no metadata push carries ZKAS_{circuit_name.upper()}_NS")
            failures += 1
            continue
        for ns, n in matched:
            if n < circuit_count:
                print(f"FAIL: {contract_name}/{circuit_name} — {circuit_count} constrain_instance "
                      f"vs {n} pushed values ({ns})")
                failures += 1
            else:
                print(f"OK:   {contract_name}/{circuit_name} — {circuit_count} constrain_instance, "
                      f"{n} metadata pushes ({ns})")
                passes += 1

print("")
print("---")
print(f"Passed: {passes}  Failed: {failures}")

if failures == 0:
    print("PASS: All circuits have matching metadata push counts")
    sys.exit(0)
else:
    print(f"FAIL: {failures} circuit(s) have insufficient metadata push counts")
    print("")
    print("Root cause: a circuit's constrain_instance order must match the metadata")
    print("function's zk_inputs.push() order position-for-position (privacy.md §5.3).")
    print("A mismatch silently produces wrong proof verification.")
    sys.exit(1)
PYEOF
