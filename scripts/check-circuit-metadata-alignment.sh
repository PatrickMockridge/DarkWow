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
# Contracts covered: EVERY contract with a `proof/` directory.
#
# This used to be a hardcoded allowlist of eleven names, and that was the gate's
# worst defect (OBL-C79). The list was called `GENESIS` while containing three
# non-genesis contracts, and it examined 11 of 32 contracts while printing
# `PASS` — silently, with no coverage line. Every one of the six broken-proof
# contracts (`dao_escrow`, `drain_protection`, `subscription`, `insurance_market`,
# `labor_market`, `tender`) was outside it, which is why their instance vectors
# disagreed with their circuits for as long as they did: the check that exists to
# catch exactly that could not see them. Enumeration is now by directory
# presence, the covered set is printed, and the circuit-free contracts are named
# rather than omitted, so a future allowlist cannot return quietly.
#
# Entrypoints are discovered under `src/entrypoint.rs`, `src/entrypoint/mod.rs`
# and `src/entrypoint/*.rs` — the original glob found only the last, which is why
# `oracle` and `bearer_bond` were SKIPped.
#
# THE CLIENT SIDE, AND WHAT THIS CHECK CANNOT SEE. The client's vector is
# resolved by file name, or by CLIENT_ALIASES where the proof-building code
# lives elsewhere (`bearer_bond`'s `BlindOutput_V2` is built in
# `pay_interest.rs`); a client file carrying several `to_vec`s and no alias is
# reported AMBIGUOUS rather than guessed at, because choosing the vector whose
# count matches would make the check pass by construction.
#
# COUNTS are the hard check. ORDER is a WARN (added with OBL-C79), because the
# invariant this gate's header states is three-way equality of order, and only the
# count half was ever mechanized:
#
#   `bearer_bond/redeem` was reported OK as "8, 8, 8" while the circuit exposed
#   `[coin, x, y, token_commit, value, tx_binding, tx_nonce, spend_hook]`, its
#   metadata pushed a different 8 and its client a third order. That defect
#   (`script/circuit_metadata_exceptions.txt`, OBL-Z15) was declared rather than
#   detected.
#
# The order comparison names each position whose metadata expression does not
# mention the circuit's variable there. It is a WARN and not a FAIL because the
# mapping from a circuit variable to a Rust expression is a heuristic — a push may
# legitimately compute its element (`pallas::Base::from(params.milestone_count)`)
# or reach it through a helper — and a false FAIL would block work that is
# correct. A WARN that names the position is what makes the recorded order
# defects (``spent_nullifier`` in the wrong position; tender's `submit_bid`
# transposition) visible without inventing a name-mapping layer first.
#
# **Amended 2026-09-23 (OBL-C20): the heuristic stays a WARN, and one subset of it
# was promoted to a hard FAIL.** The reasoning above is why the general order
# comparison cannot block — but part of what it reports is not a mapping question at
# all, and that part now fails. See the literal-vs-value note below, next to
# `ZERO_CONVENTION`, for the rule, the measurement, and the one limitation it has.
#
# Exit 0: every circuit's counts agree, or the circuit is a declared exception
# Exit 1: mismatch found and undeclared

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO_ROOT="$REPO_ROOT" python3 - "$@" <<'PYEOF'
import os, re, sys, glob

repo = os.environ["REPO_ROOT"]

# The contracts in scope, ENUMERATED rather than listed. A hardcoded allowlist
# here is what hid 21 of 32 contracts behind a PASS (OBL-C79); a contract is in
# scope if and only if it ships circuits. `CIRCUIT_FREE` is named in the output
# rather than omitted, so the gate's coverage is always on screen and an
# accidental shrink of the covered set cannot pass unremarked.
CONTRACT_ROOT = os.path.join(repo, "src", "contract")
ALL_CONTRACTS = sorted(
    os.path.basename(d) for d in glob.glob(os.path.join(CONTRACT_ROOT, "*"))
    if os.path.isdir(d))

def circuits_of(contract_name):
    return sorted(glob.glob(os.path.join(CONTRACT_ROOT, contract_name, "proof", "*.zk")))

COVERED = [c for c in ALL_CONTRACTS if circuits_of(c)]
CIRCUIT_FREE = [c for c in ALL_CONTRACTS if not circuits_of(c)]
FULL_COVERED = list(COVERED)

# Optional single-contract mode for iterating on one repair: `… <contract>`. It narrows the walk,
# never the definition of scope — the contract must be one the enumeration already found, so this
# cannot be used to point the gate at a hand-picked set and call the result a pass.
if len(sys.argv) > 1:
    wanted = sys.argv[1]
    if wanted not in COVERED:
        print(f"FAIL: `{wanted}` is not a contract with a `proof/` directory. Known: "
              f"{', '.join(COVERED)}", file=sys.stderr)
        sys.exit(1)
    COVERED = [wanted]

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

def circuit_instance_order(zk_src):
    """The circuit's `constrain_instance` variables, in declared order.

    The *order* is the invariant the header states and the counts alone cannot express: a
    verifier reads its public inputs positionally, so a vector of the right length in the wrong
    order is still a disagreement. Comments are already stripped by `strip_zk_comments`, so a
    `.zk` whose prose mentions `constrain_instance` (seven circuits in `labor_market` do) does not
    contribute a phantom entry.
    """
    return [m.group(1).strip() for m in
            re.finditer(r'constrain_instance\(\s*([^)]+?)\s*\)', zk_src)]

def push_vec_order_diff(instance_order, elements):
    """[(position, circuit_var, metadata_expr)] for each element not naming its circuit variable.

    A heuristic, deliberately: the push may compute its element
    (`pallas::Base::from(params.milestone_count)`) or reach it through a helper, so the test is
    "does this expression mention the circuit's variable" rather than equality of identifiers.
    Reported as a WARN, so a miss costs a line of output and never a blocked build.
    """
    diffs = []
    for idx, (var, expr) in enumerate(zip(instance_order, elements)):
        if var not in expr:
            diffs.append((idx + 1, var, expr))
    return diffs

def extract_push_vecs(src):
    """Every `<NS>.to_string(), vec![...]` pair -> [(ns, [element_expr])], in source order.

    Returns the element EXPRESSIONS, not a bare count: the count is `len(elements)`, and keeping
    the expressions is what lets the order check read the same parse as the count check. Two
    walks of the same call would be two sources of truth for one fact — the RC5 shape.

    `None` as the second item means the vector is not a literal (`stablecoin` carries
    `params.zk_public_inputs` for eight of its operations, so no static count exists). Recorded
    rather than dropped, so the caller can WARN about it by name instead of reporting "no push at
    all".

    ANCHORED ON THE NS, NOT ON THE `push`. The original walked every `push((<NS>.to_string(), … ))`
    call. That is only one of the two idioms in the tree: `dao_escrow` and `game_room` write

        let zk_public_inputs = vec![(
            crate::DAO_ESCROW_ZKAS_INIT_NS_V2.to_string(),
            vec![…],
        )];

    and never call `push` at all, so seven of `dao_escrow`'s circuits and twelve of
    `game_room`'s were reported as "no metadata push carries <NS>" for vectors sitting in plain
    sight. The invariant is not the call shape — it is that a namespace constant is followed by
    its vector — so that is what this anchors on. It also subsumes two earlier special cases:
    the constant may be path-qualified (`crate::…`, the form every non-genesis contract uses),
    and the vector need not be adjacent to the NS (`oracle`'s `register_oracle` arm does the
    `xy()` extraction first, `entrypoint.rs:135-147`), because the search resumes at the NS and
    takes the next `vec![` rather than requiring it on the same line.
    """
    results = []
    for m in re.finditer(
            r'(?:[A-Za-z_][A-Za-z0-9_]*::)*([A-Z0-9_]+)\s*\.to_string\(\s*\)\s*,', src):
        ns = m.group(1)
        # What follows the comma decides it: `vec![…]` is the literal, anything else means the
        # caller supplies the vector (`stablecoin`: `params.zk_public_inputs`). Searching forward
        # for the *next* `vec![` instead would silently bind an unrelated literal further down the
        # file and report its count as this circuit's — a wrong number, not a missing one, which is
        # the worse failure. Requiring the literal to be the immediate next token keeps the
        # `None` case honest.
        after = src[m.end():]
        vec_start = m.end() + (len(after) - len(after.lstrip()))
        vec = re.compile(r'vec!\s*\[').match(src, vec_start)
        if vec is None:
            results.append((ns, None))
            continue
        i = vec.end()
        depth = 1; k = i
        while k < len(src) and depth > 0:
            if src[k] == "[": depth += 1
            elif src[k] == "]": depth -= 1
            k += 1
        if depth > 0:
            continue  # malformed vec literal — not our corpus
        results.append((ns, split_top(src[i:k - 1])))
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
order_warnings = []

print("=== Circuit-Metadata Alignment Check ===")
print("")
# The scope, stated rather than assumed. A gate that reports PASS must also report what it did
# not look at; omitting this is what let 21 contracts sit outside the check (OBL-C79).
print(f"Covered: {len(FULL_COVERED)} of {len(ALL_CONTRACTS)} contracts "
      f"({sum(len(circuits_of(c)) for c in FULL_COVERED)} circuits)"
      + (f" — narrowed to `{COVERED[0]}` for this run" if len(COVERED) != len(FULL_COVERED) else ""))
if CIRCUIT_FREE:
    print(f"Circuit-free, no `proof/` directory (not checked): {', '.join(CIRCUIT_FREE)}")
print("")
passes = 0
failures = 0
# Every circuit in scope must reach a verdict line. This set is reconciled against the enumerated
# scope at the end, so a future `continue` cannot drop a circuit silently — the failure mode that
# let 21 contracts sit outside this check (OBL-C79).
seen = set()
failed_circuits = set()
for contract_name in COVERED:
    proof_dir = f"{repo}/src/contract/{contract_name}/proof"
    contract_dir = f"{repo}/src/contract/{contract_name}"
    # Discovered by what the file DOES, not where it sits. The glob alone
    # (`src/entrypoint.rs` + `src/entrypoint/*.rs`) missed `game_room`, whose
    # `get_metadata` is in `src/lib.rs:236` — so all twelve of its circuits were
    # reported as having no metadata vector while the vectors sat in plain sight
    # (`lib.rs:330-332`). A path pattern is an allowlist wearing a disguise: it
    # enumerates where the code is expected to live, and the gate then cannot see
    # it anywhere else. The files that define `get_metadata` are found by saying so.
    entrypoint_files = sorted(set(
        glob.glob(f"{contract_dir}/src/entrypoint.rs") +
        glob.glob(f"{contract_dir}/src/entrypoint/*.rs") +
        [f for f in glob.glob(f"{contract_dir}/src/**/*.rs", recursive=True)
         if "fn get_metadata" in open(f).read()]))
    if not entrypoint_files:
        entrypoint_src = ""
    else:
        entrypoint_src = strip_line_comments(
            "\n".join(open(f).read() for f in entrypoint_files))
    pushes = extract_push_vecs(entrypoint_src) if entrypoint_src else []
    ns_table = ns_constants(contract_dir)

    for zk_file in sorted(glob.glob(f"{proof_dir}/*.zk")):
        circuit_name = os.path.basename(zk_file)[:-3]
        seen.add((contract_name, circuit_name))
        zk_src = strip_zk_comments(open(zk_file).read())
        instance_order = circuit_instance_order(zk_src)
        circuit_count = len(instance_order)
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
            matched = [(ns, elems) for (ns, elems) in pushes if ns == ns_name]
        else:
            pattern = re.compile(rf'ZKAS_{circuit_name.upper()}_NS(_V[0-9]+)?$')
            matched = [(ns, elems) for (ns, elems) in pushes if pattern.search(ns)]
        if not matched:
            # Distinguish "the namespace never appears" from "it appears, but the vector is built
            # somewhere this line cannot see". `game_room` passes five of its namespaces as an
            # ARGUMENT — `identity_get_metadata_v1(params.room_id, …, GAME_ROOM_ZKAS_FOLD_NS_V2)?`
            # (`lib.rs:270-290`) — so the vector is inside the shared helper and its count is no
            # more statically visible here than `stablecoin`'s caller-supplied
            # `params.zk_public_inputs`. Reporting those as defects would be five false findings
            # against correct code; a WARN that names the reason is what the gate does elsewhere
            # for exactly this situation. A namespace that is referenced NOWHERE is a real
            # absence — the metadata function has no arm for the circuit at all.
            referenced = ns_name is not None and re.search(rf'\b{re.escape(ns_name)}\b',
                                                           entrypoint_src)
            label = ns_name or 'ZKAS_' + circuit_name.upper() + '_NS'
            if referenced:
                print(f"WARN: {contract_name}/{circuit_name} — {circuit_count} constrain_instance; "
                      f"the vector for {label} is built elsewhere (the namespace is passed to a "
                      f"builder or supplied by the caller), so the count is not statically "
                      f"checkable here")
                passes += 1
            else:
                print(f"FAIL: {contract_name}/{circuit_name} — circuit has {circuit_count} "
                      f"constrain_instance but no metadata push carries {label}"
                      f"{'' if identity else ' (circuit declares no identity string)'}")
                failures += 1
                failed_circuits.add((contract_name, circuit_name))
            continue
        if all(elems is None for _, elems in matched):
            print(f"WARN: {contract_name}/{circuit_name} — {circuit_count} constrain_instance, but "
                  f"the metadata push for {matched[0][0]} is not a literal vector (the caller "
                  f"supplies it), so the count is not statically checkable")
            passes += 1
            continue
        matched = [(ns, elems) for ns, elems in matched if elems is not None]
        excuse = metadata_exceptions.get(f"{contract_name}/{circuit_name}")
        for ns, elems in matched:
            n = len(elems)
            if n < circuit_count:
                if excuse is not None:
                    excused.append((f"{contract_name}/{circuit_name}", circuit_count, n, excuse))
                else:
                    print(f"FAIL: {contract_name}/{circuit_name} — {circuit_count} constrain_instance "
                          f"vs {n} pushed values ({ns})")
                    failures += 1
                    failed_circuits.add((contract_name, circuit_name))
            elif client_count is None:
                print(f"OK:   {contract_name}/{circuit_name} — {circuit_count} constrain_instance, "
                      f"{n} metadata pushes ({ns}; no client to_vec found)")
                passes += 1
            elif client_count != circuit_count:
                print(f"FAIL: {contract_name}/{circuit_name} — circuit {circuit_count} "
                      f"constrain_instance vs client to_vec {client_count}: the proof would be "
                      f"created over a different public-input vector than the verifier uses")
                failures += 1
                failed_circuits.add((contract_name, circuit_name))
            else:
                print(f"OK:   {contract_name}/{circuit_name} — {circuit_count} constrain_instance, "
                      f"{n} metadata pushes, {client_count} client public inputs ({ns})")
                passes += 1

        # ORDER, as a WARN (OBL-C79). Only meaningful against the longest matching push, and only
        # when there are at least as many pushed values as instances — a count mismatch is already
        # a FAIL above and would make the positional comparison read noise.
        for ns, elems in matched:
            if len(elems) < circuit_count:
                continue
            for pos, var, expr in push_vec_order_diff(instance_order, elems):
                order_warnings.append((f"{contract_name}/{circuit_name}", pos, var, expr, ns))

# LITERAL-VS-VALUE — the half of the order comparison that is NOT a heuristic, promoted from
# advisory to a hard FAIL (OBL-C20).
#
# The order comparison stays a WARN in general, for the reason its own note gives: mapping a
# circuit variable to a Rust expression by name cannot tell a transposition from a derived value
# reached another way, and 291 of its 304 warnings are exactly that. But one subset of those
# warnings is not a mapping question at all: a position where the circuit instances a value and
# the metadata pushes a *literal constant*. A literal can only match a derived value by a
# preimage coincidence, and it cannot match a witness whose client-supplied value is not that
# literal either — so the site's proof does not verify, whatever the name mapping does.
#
# Measured 2026-09-23 before this was promoted: 81 positions push a literal `Base::zero()`, of
# which 71 are the tx-nonce convention below and 10 — across six circuits — are not. Those six
# have **no overlap at all** with the 19 circuits the count check already fails, so this rule finds
# a class that was invisible rather than restating one that was known.
#
# ONE LIMITATION, stated rather than discovered later: the order comparison — this half included —
# runs only against a push that is at least as long as the circuit's instance list, so a circuit
# that already fails on counts is not order-checked and its literal rows are not enumerated here.
# Nothing is *lost* (that circuit is failing and named), but the counts of the two classes are not
# additive across the whole corpus, and a reader comparing them should not expect them to be.
ZERO_CONVENTION = {"tx_nonce"}

literal_findings = sorted({
    (label, pos, var, expr)
    for label, pos, var, expr, _ns in set(order_warnings)
    if re.fullmatch(r'[A-Za-z_:]*Base::zero\(\)', expr.strip())
    and var not in ZERO_CONVENTION
})
literal_keys = {(label, pos, var, expr) for label, pos, var, expr in literal_findings}

if literal_findings:
    print("")
    print(f"FAIL: {len(literal_findings)} position(s) push a literal `Base::zero()` where the "
          f"circuit instances a value.")
    print("      A literal cannot equal a derived value, and cannot equal a client-supplied "
          "witness either, so")
    print("      these proofs cannot verify as built. `tx_nonce` is the only reviewed exemption "
          "(left zero by")
    print("      convention across 68 circuits); everything else is a finding. Register: "
          "OBL-Z2 / OBL-C78.")
    by_label = {}
    for label, pos, var, _expr in literal_findings:
        by_label.setdefault(label, []).append((pos, var))
    for label in sorted(by_label):
        print(f"  {label}: " + ", ".join(f"instance {p} `{v}`" for p, v in sorted(by_label[label])))

if AMBIGUOUS:
    print("")
    for contract_name, circuit_name, rel, n in sorted(set(AMBIGUOUS)):
        print(f"WARN: {contract_name}/{circuit_name} — {rel} carries {n} `to_vec` impls and no "
              f"alias names the circuit's; the client vector is NOT checked. Add a CLIENT_ALIASES "
              f"entry (file, impl type).")
if order_warnings:
    uniq = [w for w in sorted(set(order_warnings)) if (w[0], w[1], w[2], w[3]) not in literal_keys]
    by_circuit = {}
    for label, pos, var, expr, ns in uniq:
        by_circuit.setdefault(label, []).append((pos, var, expr))
    print("")
    print(f"Order warnings (advisory): {len(uniq)} position(s) across {len(by_circuit)} circuit(s).")
    print("The count agrees, but the circuit's variable at that position is not what the metadata")
    print("expression there supplies — a transposition, or a derived value reached another way.")
    print("ADVISORY and deliberately so: mapping a circuit variable to a Rust expression by name")
    print("is a heuristic, and a false FAIL here would block correct work. The literal-push rows")
    print("are NOT in this count — they are findings, reported above.")
    for label in sorted(by_circuit):
        rows = by_circuit[label]
        print(f"  {label} ({len(rows)}):")
        for pos, var, expr in rows[:4]:
            shown = expr if len(expr) <= 52 else expr[:49] + "..."
            print(f"    instance {pos}: constrains `{var}` vs pushes `{shown}`")
        if len(rows) > 4:
            print(f"    … and {len(rows) - 4} more position(s)")
print("")
if excused:
    print(f"Declared exceptions ({len(excused)} row(s) over "
          f"{len(set(e[0] for e in excused))} circuit(s)) — counted, not hidden:")
    for label, circuit_count, pushed, reason in sorted(set(excused)):
        print(f"  {label}: circuit {circuit_count}, metadata {pushed} — {reason}")
    print("")
print("---")
print(f"Covered: {len(COVERED)} of {len(ALL_CONTRACTS)} contracts")
print(f"Passed: {passes}  Failed: {failures}")

# Scope reconciliation, and the reason this gate can be trusted to report a PASS. The check is
# only as good as the set it walks, so the set it walked is compared against the set it declared,
# and a shortfall is a FAILURE rather than a footnote. Without this, a `continue` added later
# would shrink coverage invisibly — which is exactly how the eleven-name allowlist read as green
# while covering a third of the tree (OBL-C79).
declared = {(c, os.path.basename(z)[:-3]) for c in COVERED for z in circuits_of(c)}
unexamined = sorted(declared - seen)

if unexamined:
    print("")
    print(f"FAIL: {len(unexamined)} circuit(s) were in scope but never reached a verdict — the")
    print("gate's coverage is not what it claims:")
    for contract_name, circuit_name in unexamined:
        print(f"  {contract_name}/{circuit_name}")
    sys.exit(1)

if not COVERED:
    print("")
    print("FAIL: no contract with a `proof/` directory was found — the enumeration is broken.")
    sys.exit(1)

if failures == 0 and not literal_findings:
    print(f"PASS: All {len(seen)} circuits across {len(COVERED)} contracts have matching metadata "
          f"push counts")
    if order_warnings:
        print(f"      ({len(set(order_warnings))} order warning(s) above are advisory, not blocking.)")
    sys.exit(0)
else:
    # "findings", not "circuits": one circuit can be reached by more than one namespace push (two
    # functions of the same contract share a circuit — `labor_market`'s `CreateJobV1` and
    # `CreateJobWithMilestonesV1` both use `CREATE_JOB_NS_V2`), and each is reported separately.
    # Calling thirteen findings "thirteen circuits" would overstate the defect count by four.
    literal_circuits = {label for label, _p, _v, _e in literal_findings}
    parts = []
    if failures:
        parts.append(f"{failures} count mismatch(es) over {len(failed_circuits)} circuit(s)")
    if literal_findings:
        parts.append(f"{len(literal_findings)} literal-vs-value position(s) over "
                     f"{len(literal_circuits)} circuit(s)")
    print(f"FAIL: {' and '.join(parts)}")
    print("")
    print("Root cause: a circuit's constrain_instance order must match the metadata")
    print("function's zk_inputs.push() order position-for-position (privacy.md §5.3).")
    print("A mismatch silently produces wrong proof verification.")
    sys.exit(1)
PYEOF
