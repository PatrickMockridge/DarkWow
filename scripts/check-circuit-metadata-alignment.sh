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
# The second rewrite (BaseDiv Stage 3.1) adds the third voice. The invariant is
# THREE-way, not two-way, and the middle one was previously unchecked:
#
#   circuit `constrain_instance` order  ==  entrypoint metadata push order
#                                       ==  client `to_vec` order
#
# The client's `to_vec` is what `Proof::create(.., &public_inputs.to_vec(), ..)`
# commits the proof to, while the *verifier* takes its public inputs from the
# metadata function. A client that disagrees with the metadata produces proofs
# that never verify, and a circuit that disagrees with either is unsatisfiable.
# The three-way disagreement in `bearer_bond/prove_coverage` (circuit 2,
# metadata 1, client 3 — none of them equal) is what this check is for: every
# pairwise count matched nothing because no pair was ever compared.
#
# Contracts covered: the genesis set, plus the PN-issuing contracts from
# `doc/src/contract/` that carry the governance-ratio circuits (dex, stablecoin,
# bearer_bond). Entrypoints are discovered under `src/entrypoint.rs`,
# `src/entrypoint/mod.rs` and `src/entrypoint/*.rs` — the original glob found
# only the last, which is why `oracle` and `bearer_bond` were SKIPped.
#
# THE CLIENT SIDE, AND WHAT THIS CHECK CANNOT SEE. The client's vector is
# resolved by file name, or by CLIENT_ALIASES where the proof-building code
# lives elsewhere (`bearer_bond`'s `BlindOutput_V2` is built in
# `pay_interest.rs`); a client file carrying several `to_vec`s and no alias is
# reported AMBIGUOUS rather than guessed at, because choosing the vector whose
# count matches would make the check pass by construction.
#
# It compares COUNTS, not values. Two vectors of the same length can still
# disagree position-for-position — `bearer_bond/redeem` was reported OK as
# "8, 8, 8" while the circuit exposed `[coin, x, y, token_commit, value,
# tx_binding, tx_nonce, spend_hook]`, its metadata pushed a different 8 and its
# client a third order. That defect (`script/circuit_metadata_exceptions.txt`,
# OBL-Z15) is declared rather than detected.
#
# Exit 0: every circuit's counts agree, or the circuit is a declared exception
# Exit 1: mismatch found and undeclared

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import os, re, sys, glob

repo = os.environ["REPO_ROOT"]
# Genesis contracts (the ones whose circuits ship with the chain) plus the
# PN-issuing contracts whose circuits carry the governance ratio.
GENESIS = ["native_token", "box", "purse", "promissory_note", "identity",
           "attestation", "oracle", "multisig",
           "dex", "stablecoin", "bearer_bond"]

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
    """Every `push((<NS>.to_string(), <expr containing vec![...]>))` -> [(ns, value_count)].

    The public-input vector is not always written immediately after the NS: `oracle`'s
    `register_oracle` arm wraps it in a block that does the `xy()` extraction first
    (`entrypoint.rs:135-147`), so an adjacency-based match reported that circuit as having no
    push at all. This walks the whole `push(...)` call instead and takes the balanced
    `vec![...]` inside it.
    """
    results = []
    for m in re.finditer(r'push\(\s*\(\s*([A-Z0-9_]+)\s*\.to_string\(\s*\)\s*,', src):
        ns = m.group(1)
        # Walk to the end of the `push(` call: balance parentheses from the call's open paren.
        open_paren = src.index('(', m.start() + len('push'))
        depth = 0; j = open_paren
        while j < len(src):
            if src[j] == '(': depth += 1
            elif src[j] == ')':
                depth -= 1
                if depth == 0:
                    break
            j += 1
        call = src[open_paren:j]
        vec = re.search(r'vec!\[', call)
        if vec is None:
            # A push whose vector is not a literal — `stablecoin` carries
            # `params.zk_public_inputs` for eight of its operations, so the vector is supplied by
            # the caller and no static count exists. Recorded as `None` rather than dropped, so
            # the caller can WARN about it by name instead of reporting "no push at all".
            results.append((ns, None))
            continue
        i = vec.end()
        depth = 1; k = i
        while k < len(call) and depth > 0:
            if call[k] == "[": depth += 1
            elif call[k] == "]": depth -= 1
            k += 1
        if depth > 0:
            continue  # malformed vec literal — not our corpus
        results.append((ns, len(split_top(call[i:k - 1]))))
    return results

METADATA_EXCEPTIONS = os.path.join(repo, "script", "circuit_metadata_exceptions.txt")

def load_metadata_exceptions():
    """`<contract>/<circuit> : <reason citing an OBL- ID>`.

    For a circuit whose three vectors are *known* not to agree and whose repair is a piece of work
    rather than a line: the count check cannot express that, and silently skipping the circuit is
    what the gate did before. The reason must cite a register ID, so the exception is tracked.
    """
    entries = {}
    if not os.path.exists(METADATA_EXCEPTIONS):
        return entries
    for lineno, line in enumerate(open(METADATA_EXCEPTIONS, encoding="utf-8"), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        parts = [p.strip() for p in line.split(":", 1)]
        if len(parts) != 2 or not parts[1]:
            print(f"FAIL: {METADATA_EXCEPTIONS}:{lineno}: expected "
                  f"`<contract>/<circuit> : <reason [OBL-…]>`", file=sys.stderr)
            continue
        if "OBL-" not in parts[1]:
            print(f"FAIL: {METADATA_EXCEPTIONS}:{lineno}: the reason cites no register ID",
                  file=sys.stderr)
            continue
        entries[parts[0]] = parts[1]
    return entries

def circuit_identity(zk_src):
    """The circuit's own name, as declared in the .zk and recorded in the NS constant."""
    m = re.search(r'circuit\s+"([^"]+)"\s*\{', zk_src)
    return m.group(1) if m else None

def ns_constants(contract_dir):
    """{namespace string -> constant name} from `pub const NAME: &str = "VALUE";`.

    The circuit's identity string is what the verifier keys on (`zkas_db_set` /
    `load_zkbin`), so resolving the constant by its *value* is the exact mapping. Matching on
    the file name instead is a heuristic that fails where the two drift:
    `set_transparency_level.zk` declares `SetTransparencyLevelV2` whose constant is
    `DEX_CONTRACT_ZKAS_SET_TRANSPARENCY_NS_V2` — no `_LEVEL` — so the name-based pattern
    reported a missing push that is there.
    """
    table = {}
    for f in glob.glob(f"{contract_dir}/src/**/*.rs", recursive=True):
        try:
            src = strip_line_comments(open(f).read())
        except OSError:
            continue
        for m in re.finditer(r'pub const\s+([A-Z0-9_]+)\s*:\s*&str\s*=\s*"([^"]+)"\s*;', src):
            table.setdefault(m.group(2), m.group(1))
    return table

def strip_zk_comments(src):
    # zkas comments are full-line `#`; strip `//` defensively (unused today).
    out = []
    for line in src.splitlines():
        line = line.split("//")[0]
        if line.lstrip().startswith("#"):
            continue
        out.append(line)
    return "\n".join(out)

# Circuits whose proof-building code does not live in `src/client/<circuit>.rs`. Each entry was
# read, not inferred: the named file builds that circuit's public-input vector.
# (file, impl type) — the type matters: `unstake.rs` carries two `to_vec`s, `UnstakeBurnRevealed`
# (the Burn circuit, 10 values) and `UnstakeReceiptRevealed` (the Redeem circuit, 8).
CLIENT_ALIASES = {
    ("bearer_bond", "blind_output"): ("pay_interest.rs", "PayInterestRevealed"),
    ("bearer_bond", "redeem"): ("unstake.rs", "UnstakeReceiptRevealed"),
    # promissory_note's clients carry two revealed-vector types each, one per circuit that a
    # transfer or a redeem involves; the doc comments name which is which ("Order must match
    # Redeem_V1 circuit").
    ("promissory_note", "transfer"): ("transfer.rs", "TransferBlindOutputRevealed"),
    ("promissory_note", "redeem"): ("redeem.rs", "RedeemReceiptRevealed"),
}

# Files that carry more than one `to_vec` and no alias naming which one belongs to the circuit.
# Reported at the end rather than guessed: picking the vector whose *count* matches would make the
# check pass by construction, which is the one thing it must not do.
AMBIGUOUS = []

def client_to_vec_count(contract_dir, circuit_name, contract_name=""):
    """Element count of the client's `to_vec` for this circuit, or None if not found.

    Resolved by file name first — `src/client/<circuit>.rs` — then by `CLIENT_ALIASES`, because the
    file name is not the circuit's name in general: `bearer_bond`'s `BlindOutput_V2` public inputs
    are built in `pay_interest.rs` and `Redeem_V2`'s in `unstake.rs`. Without the alias, both were
    reported "no client to_vec found" and went unchecked, which is a false negative *inside* the
    three-way check that exists to catch exactly their kind of disagreement.

    (Resolving by searching the client sources for the circuit's name was tried and rejected: it is
    ambiguous — `dex/execute_swap` matches four files — and it misses `blind_output`, whose client
    never names the circuit.)

    `None` is not a failure: several circuits have no client module (box and purse are driven
    through generated calls), and a checker that demanded one would be red for reasons that are not
    defects. The count of circuits with no client found is reported, so the coverage of the check is
    visible rather than assumed.
    """
    rel, impl_type = CLIENT_ALIASES.get(
        (contract_name, circuit_name), (f"{circuit_name}.rs", None))
    path = f"{contract_dir}/src/client/{rel}"
    if not os.path.exists(path):
        return None
    src = strip_line_comments(open(path).read())
    start = 0
    if impl_type is not None:
        block = re.search(rf'impl\s+{impl_type}\s*\{{', src)
        if block is None:
            return None
        start = block.end()
    if impl_type is None:
        found = re.findall(r'fn\s+to_vec\s*\(\s*&self\s*\)\s*->\s*Vec<pallas::Base>\s*\{', src)
        if len(found) > 1:
            AMBIGUOUS.append((contract_name, circuit_name, rel, len(found)))
            return None
    m = re.search(r'fn\s+to_vec\s*\(\s*&self\s*\)\s*->\s*Vec<pallas::Base>\s*\{', src[start:])
    if not m:
        return None
    # The first `vec![` in the function body, balanced — the literal is not adjacent to the brace
    # in `bearer_bond`'s clients, which compute the value-commit coordinates on the line before.
    v = re.compile(r'vec!\s*\[').search(src, start + m.end())
    if not v:
        return None
    i = v.end()
    depth = 1; j = i
    while j < len(src) and depth > 0:
        if src[j] == "[": depth += 1
        elif src[j] == "]": depth -= 1
        j += 1
    if depth > 0:
        return None
    return len(split_top(src[i:j - 1]))

metadata_exceptions = load_metadata_exceptions()
excused = []

print("=== Circuit-Metadata Alignment Check ===")
print("")
passes = 0
failures = 0
for contract_name in GENESIS:
    proof_dir = f"{repo}/src/contract/{contract_name}/proof"
    if not os.path.isdir(proof_dir):
        continue
    contract_dir = f"{repo}/src/contract/{contract_name}"
    # `src/entrypoint.rs` (oracle), `src/entrypoint/mod.rs` and
    # `src/entrypoint/*.rs` (dex, bearer_bond). The original glob found only the
    # third form, so those contracts were SKIPped entirely.
    entrypoint_files = sorted(set(
        glob.glob(f"{contract_dir}/src/entrypoint.rs") +
        glob.glob(f"{contract_dir}/src/entrypoint/*.rs")))
    if not entrypoint_files:
        entrypoint_src = ""
    else:
        entrypoint_src = strip_line_comments(
            "\n".join(open(f).read() for f in entrypoint_files))
    pushes = extract_push_vecs(entrypoint_src) if entrypoint_src else []
    ns_table = ns_constants(contract_dir)

    for zk_file in sorted(glob.glob(f"{proof_dir}/*.zk")):
        circuit_name = os.path.basename(zk_file)[:-3]
        zk_src = strip_zk_comments(open(zk_file).read())
        circuit_count = len(re.findall(r'constrain_instance\(', zk_src))
        client_count = client_to_vec_count(contract_dir, circuit_name, contract_name)
        identity = circuit_identity(zk_src)

        if circuit_count == 0:
            print(f"WARN: {contract_name}/{circuit_name} — zero constrain_instance calls")
            continue
        if not entrypoint_files:
            print(f"SKIP: {contract_name}/{circuit_name} — no entrypoint source found")
            continue

        # Prefer the constant whose *value* is this circuit's identity string; fall back to the
        # file-name pattern when no constant resolves (the .zk name and the constant's value have
        # drifted, or the constant lives outside `src/`).
        ns_name = ns_table.get(identity) if identity else None
        if ns_name is not None:
            matched = [(ns, n) for (ns, n) in pushes if ns == ns_name]
        else:
            pattern = re.compile(rf'ZKAS_{circuit_name.upper()}_NS(_V[0-9]+)?$')
            matched = [(ns, n) for (ns, n) in pushes if pattern.search(ns)]
        if not matched:
            print(f"FAIL: {contract_name}/{circuit_name} — circuit has {circuit_count} "
                  f"constrain_instance but no metadata push carries "
                  f"{ns_name or 'ZKAS_' + circuit_name.upper() + '_NS'}"
                  f"{'' if identity else ' (circuit declares no identity string)'}")
            failures += 1
            continue
        if all(n is None for _, n in matched):
            print(f"WARN: {contract_name}/{circuit_name} — {circuit_count} constrain_instance, but "
                  f"the metadata push for {matched[0][0]} is not a literal vector (the caller "
                  f"supplies it), so the count is not statically checkable")
            passes += 1
            continue
        matched = [(ns, n) for ns, n in matched if n is not None]
        excuse = metadata_exceptions.get(f"{contract_name}/{circuit_name}")
        for ns, n in matched:
            if n < circuit_count:
                if excuse is not None:
                    excused.append((f"{contract_name}/{circuit_name}", circuit_count, n, excuse))
                else:
                    print(f"FAIL: {contract_name}/{circuit_name} — {circuit_count} constrain_instance "
                          f"vs {n} pushed values ({ns})")
                    failures += 1
            elif client_count is None:
                print(f"OK:   {contract_name}/{circuit_name} — {circuit_count} constrain_instance, "
                      f"{n} metadata pushes ({ns}; no client to_vec found)")
                passes += 1
            elif client_count != circuit_count:
                print(f"FAIL: {contract_name}/{circuit_name} — circuit {circuit_count} "
                      f"constrain_instance vs client to_vec {client_count}: the proof would be "
                      f"created over a different public-input vector than the verifier uses")
                failures += 1
            else:
                print(f"OK:   {contract_name}/{circuit_name} — {circuit_count} constrain_instance, "
                      f"{n} metadata pushes, {client_count} client public inputs ({ns})")
                passes += 1

if AMBIGUOUS:
    print("")
    for contract_name, circuit_name, rel, n in sorted(set(AMBIGUOUS)):
        print(f"WARN: {contract_name}/{circuit_name} — {rel} carries {n} `to_vec` impls and no "
              f"alias names the circuit's; the client vector is NOT checked. Add a CLIENT_ALIASES "
              f"entry (file, impl type).")
print("")
if excused:
    print(f"Declared exceptions ({len(excused)} row(s) over "
          f"{len(set(e[0] for e in excused))} circuit(s)) — counted, not hidden:")
    for label, circuit_count, pushed, reason in sorted(set(excused)):
        print(f"  {label}: circuit {circuit_count}, metadata {pushed} — {reason}")
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
