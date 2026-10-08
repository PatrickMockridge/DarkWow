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

SELF_TEST=0
TARGETS=()
for arg in "$@"; do
    case "$arg" in
        --self-test) SELF_TEST=1 ;;
        *) TARGETS+=("$arg") ;;
    esac
done

# ── THE SELF-TEST ────────────────────────────────────────────────────────────────────────────
#
# R8: an instrument that cannot be shown to fail is not an instrument. Four rules in this gate
# each had to be *proven* rather than asserted, and one of them (`WARN(0/3)`) was silently wrong
# for thirteen circuits until 2026-10-04 precisely because nothing planted a defect for it. So
# this builds two corpora under a temporary `CONTRACT_ROOT` — one entirely conforming and one
# carrying three planted defects — and asserts on the CONTENT of each report, never on an exit
# code: a traceback also exits non-zero, so an exit-code assertion would pass while the checker
# was crashing, which is R8's own subject written into the check that exists to catch a missing
# check.
#
# The planted defects, one per rule, and the reasons they are these and not others:
#
#   badcontract/countshort  3 pushed values against 4 instances — the oldest rule in the gate.
#   badcontract/pairswap    counts AGREE at 4 and the pair sits at 3,4 in the circuit while the
#                           client's `to_vec` puts it at 2,3. This is the shape that cost the
#                           peer session a 907-second run: everything counts correctly and the
#                           proof is refused. It is invisible to a count-only checker.
#   badcontract/clientless  the arm pushes `params.zk_public_inputs` (not a literal) and there is
#                           no client file, so NO leg is readable. This one must be a FAIL when
#                           undeclared — that is the 2026-10-04 repair, and before it this site
#                           printed `WARN`, incremented `passes`, and was covered by a line
#                           reading "All 166 circuits … have matching metadata push counts".
#
# `okcontract` is the negative control, and it is not decoration: a checker that reported every
# circuit would pass all three assertions above.
if [ "$SELF_TEST" -eq 1 ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT

    mkdir -p "$TMP/a/src/contract/okcontract/proof" \
             "$TMP/a/src/contract/okcontract/src/client" \
             "$TMP/b/src/contract/badcontract/proof" \
             "$TMP/b/src/contract/badcontract/src/client"

    # ── corpus A: conforming ────────────────────────────────────────────────────────────────
    cat > "$TMP/a/src/contract/okcontract/proof/ok.zk" <<'ZKEOF'
k = 11; field = "pallas";
constant "OkV2" { }
witness "OkV2" { Base derived_x, Base tx_commitment, Base tx_nonce, Base tx_binding, }
circuit "OkV2" {
    derived_x = poseidon_hash(witness_base(4), tx_commitment);
    constrain_instance(derived_x);
    tx_binding = poseidon_hash(witness_base(3), tx_commitment, tx_nonce);
    constrain_instance(tx_binding);
    constrain_instance(tx_nonce);
}
ZKEOF
    cat > "$TMP/a/src/contract/okcontract/src/lib.rs" <<'RSEOF'
pub const OKC_ZKAS_OK_NS_V2: &str = "OkV2";
RSEOF
    cat > "$TMP/a/src/contract/okcontract/src/entrypoint.rs" <<'RSEOF'
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        OKC_ZKAS_OK_NS_V2.to_string(),
        vec![params.derived_x, params.tx_binding, params.tx_nonce],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    wasm::util::set_return_data(&metadata)
}
RSEOF
    cat > "$TMP/a/src/contract/okcontract/src/client/ok.rs" <<'RSEOF'
pub struct OkPublicInputs { pub derived_x: pallas::Base, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base }
impl OkPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_x, self.tx_binding, self.tx_nonce]
    }
}
RSEOF

    # ── corpus B: three planted defects ─────────────────────────────────────────────────────
    cat > "$TMP/b/src/contract/badcontract/proof/countshort.zk" <<'ZKEOF'
k = 11; field = "pallas";
constant "CountShortV2" { }
witness "CountShortV2" { Base derived_a, Base derived_b, }
circuit "CountShortV2" {
    derived_a = poseidon_hash(witness_base(4), derived_b);
    constrain_instance(derived_a);
    constrain_instance(derived_b);
    tx_binding = poseidon_hash(witness_base(3), derived_a, derived_b);
    constrain_instance(tx_binding);
    constrain_instance(tx_nonce);
}
ZKEOF
    cat > "$TMP/b/src/contract/badcontract/proof/pairswap.zk" <<'ZKEOF'
k = 11; field = "pallas";
constant "PairSwapV2" { }
witness "PairSwapV2" { Base derived_x, Base roll_hash, }
circuit "PairSwapV2" {
    derived_x = poseidon_hash(witness_base(4), roll_hash);
    constrain_instance(derived_x);
    constrain_instance(roll_hash);
    tx_binding = poseidon_hash(witness_base(3), derived_x, roll_hash);
    constrain_instance(tx_binding);
    constrain_instance(tx_nonce);
}
ZKEOF
    cat > "$TMP/b/src/contract/badcontract/proof/clientless.zk" <<'ZKEOF'
k = 11; field = "pallas";
constant "ClientlessV2" { }
witness "ClientlessV2" { Base derived_a, Base derived_b, Base derived_c, }
circuit "ClientlessV2" {
    derived_a = poseidon_hash(witness_base(4), derived_b);
    derived_b = poseidon_hash(witness_base(4), derived_c);
    derived_c = poseidon_hash(witness_base(4), derived_a);
    constrain_instance(derived_a);
    constrain_instance(derived_b);
    constrain_instance(derived_c);
    constrain_instance(derived_a);
    constrain_instance(derived_c);
}
ZKEOF
    cat > "$TMP/b/src/contract/badcontract/src/lib.rs" <<'RSEOF'
pub const BADC_ZKAS_COUNTSHORT_NS_V2: &str = "CountShortV2";
pub const BADC_ZKAS_PAIRSWAP_NS_V2: &str = "PairSwapV2";
pub const BADC_ZKAS_CLIENTLESS_NS_V2: &str = "ClientlessV2";
RSEOF
    cat > "$TMP/b/src/contract/badcontract/src/entrypoint.rs" <<'RSEOF'
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        BADC_ZKAS_COUNTSHORT_NS_V2.to_string(),
        vec![params.derived_a, params.derived_b, params.tx_binding],
    ));
    zk_public_inputs.push((
        BADC_ZKAS_PAIRSWAP_NS_V2.to_string(),
        vec![params.derived_x, params.roll_hash, params.tx_binding, params.tx_nonce],
    ));
    zk_public_inputs.push((
        BADC_ZKAS_CLIENTLESS_NS_V2.to_string(),
        params.zk_public_inputs,
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    wasm::util::set_return_data(&metadata)
}
RSEOF
    # `countshort`'s client is short too (3 against 4) and `pairswap`'s is the RIGHT length with
    # the pair in the WRONG place. No `clientless.rs`: the third defect is the absent one.
    cat > "$TMP/b/src/contract/badcontract/src/client/countshort.rs" <<'RSEOF'
pub struct CountShortPublicInputs { pub derived_a: pallas::Base, pub tx_binding: pallas::Base }
impl CountShortPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_a, self.tx_binding, self.tx_nonce]
    }
}
RSEOF
    cat > "$TMP/b/src/contract/badcontract/src/client/pairswap.rs" <<'RSEOF'
pub struct PairSwapPublicInputs { pub derived_x: pallas::Base, pub roll_hash: pallas::Base }
impl PairSwapPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_x, self.tx_binding, self.tx_nonce, self.roll_hash]
    }
}
RSEOF

    run_against() {
        CONTRACT_ROOT="$1" \
        METADATA_EXCEPTIONS=/dev/null \
        CLIENT_UNRESOLVED_EXCEPTIONS=/dev/null \
        REPO_ROOT="$REPO_ROOT" \
            bash "$0" 2>&1
    }

    FAILED=0

    A_OUT="$(run_against "$TMP/a/src/contract")" || true
    if ! printf '%s\n' "$A_OUT" | grep -qF "Passed: 1  Failed: 0"; then
        echo "SELF-TEST FAILED: the conforming corpus did not pass cleanly."
        FAILED=1
    fi
    if printf '%s\n' "$A_OUT" | grep -q '^FAIL'; then
        echo "SELF-TEST FAILED: reported a conforming corpus (okcontract/ok) as failing."
        FAILED=1
    fi

    B_OUT="$(run_against "$TMP/b/src/contract")" || true
    for want in \
        "4 constrain_instance vs 3 pushed values" \
        "the circuit instances \`tx_binding\` at position 3" \
        "NO leg was compared"
    do
        if ! printf '%s\n' "$B_OUT" | grep -qF "$want"; then
            echo "SELF-TEST FAILED: the planted defect was not reported: $want"
            FAILED=1
        fi
    done
    # The third defect must be attributed to the circuit it was planted in, not merely present:
    # a message with the right words and the wrong subject is the failure mode a substring test
    # alone cannot see.
    if ! printf '%s\n' "$B_OUT" | grep -qF "FAIL: badcontract/clientless"; then
        echo "SELF-TEST FAILED: the clientless circuit was not named as the failing subject."
        FAILED=1
    fi
    # `OBL-C124`: the summary must NAME each class it counted rather than lump them. The three
    # planted defects are of three different classes, so one `count mismatch(es)` label covering all
    # three is exactly the defect this asserts the absence of — and the assertion is on the three
    # *names*, not on the line's shape, so a summary that printed a count per class but not the
    # class would also fail.
    for want in "circuit-vs-metadata=" "element-order=" "no-leg-compared="
    do
        if ! printf '%s\n' "$B_OUT" | grep -qF "$want"; then
            echo "SELF-TEST FAILED: the summary did not name the class it counted: $want"
            FAILED=1
        fi
    done

    if [ "$FAILED" -ne 0 ]; then
        echo ""
        echo "--- conforming corpus ---"; printf '%s\n' "$A_OUT"
        echo "--- defective corpus ---"; printf '%s\n' "$B_OUT"
        exit 1
    fi
    echo "SELF-TEST OK: the conforming corpus passed; the short count, the swapped pair and the"
    echo "              clientless circuit were each reported by name."
    exit 0
fi

REPO_ROOT="$REPO_ROOT" python3 - "$@" <<'PYEOF'
import os, re, sys, glob

repo = os.environ["REPO_ROOT"]

# The contracts in scope, ENUMERATED rather than listed. A hardcoded allowlist
# here is what hid 21 of 32 contracts behind a PASS (OBL-C79); a contract is in
# scope if and only if it ships circuits. `CIRCUIT_FREE` is named in the output
# rather than omitted, so the gate's coverage is always on screen and an
# accidental shrink of the covered set cannot pass unremarked.
#
# OVERRIDABLE FOR THE SELF-TEST, and this is why: the four rules below can only be shown to
# work by planting a defect, and a defect cannot be planted in the real corpus. A path that can
# only point at `src/contract` is a path whose failure modes are asserted rather than
# demonstrated — the same argument `METADATA_EXCEPTIONS` and `CLIENT_UNRESOLVED_EXCEPTIONS`
# carry, applied to the corpus itself.
CONTRACT_ROOT = os.environ.get("CONTRACT_ROOT") or os.path.join(repo, "src", "contract")
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

# Overridable so the gate can be run against a deliberately malformed file — the
# negative control for the failure path below. A path that can only point at the
# real file cannot be tested against a planted defect.
METADATA_EXCEPTIONS = os.environ.get(
    "METADATA_EXCEPTIONS",
    os.path.join(repo, "script", "circuit_metadata_exceptions.txt"))

# Malformed lines in the exceptions file, collected by the loader below and folded
# into the exit status. A gate that prints FAIL and exits 0 has a verdict nobody
# can act on, which is what this list exists to prevent.
EXCEPTION_FILE_ERRORS = []

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
            # Recorded as well as printed. Until 2026-09-25 these branches printed
            # FAIL and `continue`d without touching `failures`, so a malformed
            # line was reported on stderr and the gate still exited 0 — a failure
            # it could see and could not act on. Sibling gates have a stale-entry
            # check; this one now has a failure path.
            EXCEPTION_FILE_ERRORS.append(
                f"{METADATA_EXCEPTIONS}:{lineno}: expected "
                f"`<contract>/<circuit> : <reason [OBL-…]>`")
            print(f"FAIL: {METADATA_EXCEPTIONS}:{lineno}: expected "
                  f"`<contract>/<circuit> : <reason [OBL-…]>`", file=sys.stderr)
            continue
        if "OBL-" not in parts[1]:
            EXCEPTION_FILE_ERRORS.append(
                f"{METADATA_EXCEPTIONS}:{lineno}: the reason cites no register ID")
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

    **A value maps to every constant that carries it**, not to the first one seen: a contract may
    declare an alias pair (`TENDER_CONTRACT_ZKAS_CREATE_TENDER_NS_V2` and
    `TENDER_CONTRACT_ZKAS_CREATE_NS_V2` are both `"CreateTenderV2"`, as are subscription's
    `VERIFY_ACCESS`/`VERIFY` pair and game_room's four). Keeping only the first reported
    `tender/create_tender` as "no metadata push carries ..." while its push was there and correct —
    the arm simply used the other name for the same string. A push matching *any* name for the
    circuit's identity is the push for that circuit.
    """
    table = {}
    for f in glob.glob(f"{contract_dir}/src/**/*.rs", recursive=True):
        try:
            src = strip_line_comments(open(f).read())
        except OSError:
            continue
        for m in re.finditer(r'pub const\s+([A-Z0-9_]+)\s*:\s*&str\s*=\s*"([^"]+)"\s*;', src):
            table.setdefault(m.group(2), []).append(m.group(1))
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
    # Odd ones out: a single client file carries every circuit of the contract, so the file name
    # names no circuit and the impl type is what distinguishes them.
    ("betting_stake", "init"): ("proof_gen.rs", "InitV1PublicInputs"),
    ("betting_stake", "stake"): ("proof_gen.rs", "StakeV1PublicInputs"),
    ("betting_stake", "unstake"): ("proof_gen.rs", "UnstakeV1PublicInputs"),
    ("betting_stake", "claim"): ("proof_gen.rs", "ClaimV1PublicInputs"),
    ("betting_stake", "update_risk"): ("proof_gen.rs", "UpdateRiskV1PublicInputs"),
    # A one-to-one remainder: each contract's circuits are named for their client file except one,
    # so the unmatched circuit is the unmatched file. Read, not inferred — `set_governance_config`
    # takes the file `update.rs`, `init` takes `initialize.rs`, `burn` takes `burn_stake.rs`, and
    # in each case the impl type is the circuit's own name plus the contract's suffix convention.
    ("stablecoin", "init"): ("initialize.rs", "InitV1PublicInputs"),
    ("dao_escrow", "set_governance_config"): ("update.rs", "UpdateV1PublicInputs"),
    ("bearer_bond", "burn"): ("burn_stake.rs", "BurnStakeRevealed"),
    # `native_token`'s `Mint_V2` is the transfer/spend *output* mint, and its revealed vector is
    # built in the transfer module — the file is named for neither the circuit nor the function.
    ("native_token", "mint"): ("transfer/proof.rs", "TransferMintRevealed"),
    # Verified element for element, not by count. `drain_protection`'s eight circuits all instance
    # `authority_pub_x, authority_pub_y, authority_nullifier, tx_binding, tx_nonce`, in that order,
    # and `AuthorityPublicInputs::to_vec` returns exactly those five in exactly that order — the
    # contract's governance endpoints share one authority proof, which is why eight circuits have
    # one client vector between them.
    ("drain_protection", "execute"): ("mod.rs", "AuthorityPublicInputs"),
    ("drain_protection", "initialize"): ("mod.rs", "AuthorityPublicInputs"),
    ("drain_protection", "lock"): ("mod.rs", "AuthorityPublicInputs"),
    ("drain_protection", "propose"): ("mod.rs", "AuthorityPublicInputs"),
    ("drain_protection", "transfer"): ("mod.rs", "AuthorityPublicInputs"),
    ("drain_protection", "unlock"): ("mod.rs", "AuthorityPublicInputs"),
    ("drain_protection", "update_config"): ("mod.rs", "AuthorityPublicInputs"),
    ("drain_protection", "vote"): ("mod.rs", "AuthorityPublicInputs"),
    # Same check, same result: both house endpoints instance `house_pub_x, house_pub_y,
    # house_nullifier, tx_binding, tx_nonce` and `HouseAuthPublicInputs::to_vec` returns exactly
    # that. The two circuits are the same shape, so one vector serves both.
    ("lottery", "draw_winners"): ("house_auth.rs", "HouseAuthPublicInputs"),
    ("lottery", "expire_lottery"): ("house_auth.rs", "HouseAuthPublicInputs"),
    # One shared identity vector for six circuits, and the sharing is stated in the source rather
    # than inferred: `src/client/identity_proof.rs`'s module doc says "withdraw, raise, call, fold,
    # close_pot, contribute_entropy share the same layout … instances (5): player_pub_x,
    # player_pub_y, player_nullifier, tx_binding, tx_nonce", and `IdentityPublicInputs::to_vec`
    # returns `[pub_x, pub_y, nullifier, tx_binding, tx_nonce]`. Both sides read: five against
    # five, same order. The harness confirms the linkage rather than the doc alone —
    # `harness/game_room.rs` calls `create_identity_proof` for all six (`:173` withdraw through
    # `:251` contribute_entropy). Without the alias all six had no located client, and five of them
    # had no readable metadata leg either, so they reached a verdict having compared nothing.
    ("game_room", "call"): ("identity_proof.rs", "IdentityPublicInputs"),
    ("game_room", "close_pot"): ("identity_proof.rs", "IdentityPublicInputs"),
    ("game_room", "contribute_entropy"): ("identity_proof.rs", "IdentityPublicInputs"),
    ("game_room", "fold"): ("identity_proof.rs", "IdentityPublicInputs"),
    ("game_room", "raise"): ("identity_proof.rs", "IdentityPublicInputs"),
    ("game_room", "withdraw"): ("identity_proof.rs", "IdentityPublicInputs"),
    # The same shape again in `darkbet_exchange`, named in the source the same way: `client/
    # auth_proof.rs`'s module doc says "cancel_order, match_orders, place_back, place_lay,
    # remove_liquidity, resolve_market all share the same witness/instance layout … instances (5):
    # pub_x, pub_y, nullifier, tx_binding, tx_nonce". The circuit variables are prefixed by role
    # (`user_pub_x` for the order endpoints, `provider_pub_x` for `remove_liquidity`,
    # `oracle_pub_x` for `resolve_market`) and the shared client vector is unprefixed — five
    # against five, same order, and the pair agrees under the hard rule. The harness confirms the
    # linkage rather than the doc alone: `create_auth_proof` is called for all six (`:347`
    # place_back … `:496` remove_liquidity).
    ("darkbet_exchange", "cancel_order"): ("auth_proof.rs", "AuthPublicInputs"),
    ("darkbet_exchange", "match_orders"): ("auth_proof.rs", "AuthPublicInputs"),
    ("darkbet_exchange", "place_back"): ("auth_proof.rs", "AuthPublicInputs"),
    ("darkbet_exchange", "place_lay"): ("auth_proof.rs", "AuthPublicInputs"),
    ("darkbet_exchange", "remove_liquidity"): ("auth_proof.rs", "AuthPublicInputs"),
    ("darkbet_exchange", "resolve_market"): ("auth_proof.rs", "AuthPublicInputs"),
}

# Files that carry more than one `to_vec` and no alias naming which one belongs to the circuit.
# Reported at the end rather than guessed: picking the vector whose *count* matches would make the
# check pass by construction, which is the one thing it must not do.
AMBIGUOUS = []

# Circuits whose client `to_vec` is not located, and which are therefore checked on two legs
# instead of three. A ratchet, not an adjudication: an entry says "the third leg is unchecked
# here", never "this site is sound". Format, one per line:
#
#   <contract>/<circuit> : <why the file is not named by CLIENT_ALIASES yet>
#
# Measured 2026-10-04: 48 of 166 circuits — 29% — and they were printed as a bare `OK` until
# this list existed, which is OBL-C79's shape (a green line covering a third of the tree) inside
# the check whose own header cites OBL-C79. An entry leaves this list when the circuit's client
# is either named in CLIENT_ALIASES or found by name; it does not leave by being excused.
CLIENT_UNRESOLVED = {}
# Overridable for the same reason `METADATA_EXCEPTIONS` is, and in the same idiom: a path that can
# only point at the real file cannot be run against a planted defect, and this list's failure path
# is the one that has to be controlled.
_unresolved_path = os.environ.get(
    "CLIENT_UNRESOLVED_EXCEPTIONS",
    os.path.join(repo, "script", "circuit_client_alignment_exceptions.txt"))
if os.path.isfile(_unresolved_path):
    for _raw in open(_unresolved_path, errors="replace").read().splitlines():
        _entry = _raw.split("#")[0].strip()
        if not _entry:
            continue
        _parts = [p.strip() for p in _entry.split(" : ", 1)]
        if len(_parts) != 2:
            print(f"ERROR: malformed line in script/circuit_client_alignment_exceptions.txt: "
                  f"{_entry!r}")
            print("       expected: <contract>/<circuit> : <why the client is not named yet>")
            sys.exit(2)
        CLIENT_UNRESOLVED[_parts[0]] = _parts[1]

# The `to_vec` signature, matched loosely on the return type and **exactly** on everything else.
#
# It used to require `-> Vec<pallas::Base>` character for character, which is the oldest and
# commonest spelling (51 impls) but not the only one: 10 return
# `Result<Vec<pallas::Base>, ContractError>` and 3 `GenericResult<Vec<pallas::Base>>`, and every
# one of those 13 was invisible — the resolver found no `to_vec` and the circuit's third leg went
# unchecked *silently*, which is the false negative this function's own docstring says the alias
# table was added to fix. Requiring `Vec<pallas::Base>` to appear in the return type and letting
# the wrapper around it vary is the fix; the body is still read by the balanced-`vec![` walk, so
# this narrows nothing about *what* is compared.
_TO_VEC = r'fn\s+to_vec\s*\(\s*&self\s*\)\s*->\s*[^{;]*Vec<pallas::Base>[^{;]*\{'


def client_to_vec_elements(contract_dir, circuit_name, contract_name=""):
    """The elements of the client's `to_vec` for this circuit, or None if not found.

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
        # Recursive by file *name* — the same rule as above, applied one directory deeper.
        # `native_token`'s transfer circuits keep their clients in `src/client/transfer/proof.rs`,
        # so a flat listing found nothing and two circuits went unchecked. It is still a name
        # match, so it stays unambiguous: the search that was tried and rejected searches the
        # sources for the *circuit's* name, which matches four files in `dex`.
        matches = []
        for root, _dirs, files in os.walk(f"{contract_dir}/src/client"):
            if rel in files:
                matches.append(os.path.join(root, rel))
        if len(matches) != 1:
            return None
        path = matches[0]
    src = strip_line_comments(open(path).read())
    start = 0
    if impl_type is not None:
        block = re.search(rf'impl\s+{impl_type}\s*\{{', src)
        if block is None:
            return None
        start = block.end()
    if impl_type is None:
        found = re.findall(_TO_VEC, src)
        if len(found) > 1:
            AMBIGUOUS.append((contract_name, circuit_name, rel, len(found)))
            return None
    m = re.search(_TO_VEC, src[start:])
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
    return split_top(src[i:j - 1])

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

# `OBL-C124`: every failure is counted under the class it belongs to, so the summary can NAME the
# classes rather than lump them. It printed `5 count mismatch(es) over 5 circuit(s)` while one of the
# five was a *missing metadata push* — an arm that pushed nothing at all, so there was no vector to
# compare counts against. The verdicts were right and the label was wrong, which is the failure mode
# worth guarding: a reader triaging by class (which is how this register's rows get worked) cannot
# find a class the summary says does not exist.
#
# The classes are the seven `fail(…)` call sites below, and each is the thing its own message says:
#   no-metadata-push      the circuit's namespace reaches no push at all
#   circuit-vs-metadata   the pushed vector is a literal and its length differs from the instances
#   circuit-vs-client     the client's `to_vec` length differs (whatever the metadata leg did)
#   no-leg-compared       neither the metadata vector nor the client's `to_vec` could be read
#   third-leg-unchecked   the metadata vector was read, the client's `to_vec` could not be
#   element-order         both legs were read and they disagree element for element
fail_kinds = {}


def fail(kind):
    """Count one failure under its class; `kind` is the name the summary prints."""
    global failures
    failures += 1
    fail_kinds[kind] = fail_kinds.get(kind, 0) + 1
# Circuits checked on TWO legs — circuit instance order and metadata push — because the client's
# `to_vec` could not be located. A *set*, because one circuit can be reached by more than one
# namespace push and the summary must not call 63 rows "63 circuits" — the same overstatement the
# count-mismatch summary below already guards against.
two_leg = set()
# Circuits checked on the CLIENT leg only, because the metadata vector is not a literal at the
# push site — either it is built inside a shared helper that takes the namespace as an argument
# (`game_room`'s `identity_get_metadata_v1`) or it rides the call data (`stablecoin`'s
# `params.zk_public_inputs`). These are NOT defects: in both shapes the vector and the client's
# `to_vec` are the same bytes. What they are is a leg this gate could not read, so they are
# counted on their own line rather than folded into `Passed`.
client_only = set()
# Circuits on which NO leg was compared — no readable metadata push AND no located client
# `to_vec`. A circuit in here is in scope and reached a verdict, but the verdict is "nothing was
# checked", which is exactly what the final PASS line must not claim. Before 2026-10-04 the
# thirteen circuits in this set were counted inside `Passed` and covered by a line that read
# "All 166 circuits … have matching metadata push counts" — the OBL-C79 shape, in the branch
# whose own comment says the count is not statically checkable.
no_leg = set()
# Every circuit in scope must reach a verdict line. This set is reconciled against the enumerated
# scope at the end, so a future `continue` cannot drop a circuit silently — the failure mode that
# let 21 contracts sit outside this check (OBL-C79).
seen = set()
failed_circuits = set()
for contract_name in COVERED:
    # Derived from `CONTRACT_ROOT`, never from `repo` — the self-test overrides the root, and a
    # walk that enumerates the overridden root while reading the real one would report every
    # planted circuit as "in scope but never reached a verdict". That is how it was found.
    contract_dir = os.path.join(CONTRACT_ROOT, contract_name)
    proof_dir = os.path.join(contract_dir, "proof")
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
        client_elems = client_to_vec_elements(contract_dir, circuit_name, contract_name)
        client_count = None if client_elems is None else len(client_elems)
        identity = circuit_identity(zk_src)

        if circuit_count == 0:
            print(f"WARN: {contract_name}/{circuit_name} — zero constrain_instance calls")
            no_leg.add((contract_name, circuit_name))
            continue
        if not entrypoint_files:
            print(f"SKIP: {contract_name}/{circuit_name} — no entrypoint source found")
            no_leg.add((contract_name, circuit_name))
            continue

        # Prefer the constant whose *value* is this circuit's identity string; fall back to the
        # file-name pattern when no constant resolves (the .zk name and the constant's value have
        # drifted, or the constant lives outside `src/`).
        ns_names = ns_table.get(identity) if identity else None
        if ns_names:
            matched = [(ns, elems) for (ns, elems) in pushes if ns in ns_names]
        else:
            pattern = re.compile(rf'ZKAS_{circuit_name.upper()}_NS(_V[0-9]+)?$')
            matched = [(ns, elems) for (ns, elems) in pushes if pattern.search(ns)]
        # EITHER THE METADATA LEG IS READABLE, OR IT IS NOT. When it is not, leg 2 is genuinely
        # absent and this says so — but the CLIENT leg does not depend on it, and until 2026-10-04
        # both branches below threw it away: each printed a WARN, did `passes += 1`, and `continue`d
        # past the pair rule. Thirteen circuits (game_room's five identity endpoints, stablecoin's
        # six caller-supplied ones, attestation's two) were counted inside `Passed` with NO
        # comparison made, on the gate whose header cites OBL-C79 as the incident that taught this
        # repository to print its coverage. A `WARN` covering a third of the invariant is the same
        # defect as a bare `OK` covering two legs of it.
        #
        # So: name the reason, then make the comparison that IS available. In both shapes the
        # metadata vector and the client's `to_vec` are the same bytes — `zk_public_inputs:
        # public_inputs.to_vec()` in the harness for stablecoin, and the arm pushes
        # `params.zk_public_inputs` — so client-vs-circuit is exactly the check that matters here.
        # Where even the client is unlocatable the circuit goes in `no_leg`, and the PASS line at
        # the end stops claiming it.
        unreadable = None
        label = None
        if not matched:
            # Distinguish "the namespace never appears" from "it appears, but the vector is built
            # somewhere this line cannot see". `game_room` passes five of its namespaces as an
            # ARGUMENT — `identity_get_metadata_v1(params.room_id, …, GAME_ROOM_ZKAS_FOLD_NS_V2)?`
            # (`lib.rs:270-290`) — so the vector is inside the shared helper and its count is no
            # more statically visible here than `stablecoin`'s caller-supplied
            # `params.zk_public_inputs`. Reporting those as defects would be five false findings
            # against correct code. A namespace that is referenced NOWHERE is a real absence — the
            # metadata function has no arm for the circuit at all.
            referenced = bool(ns_names) and any(
                re.search(rf'\b{re.escape(n)}\b', entrypoint_src) for n in (ns_names or []))
            label = ' | '.join(ns_names) if ns_names else 'ZKAS_' + circuit_name.upper() + '_NS'
            if not referenced:
                print(f"FAIL: {contract_name}/{circuit_name} — circuit has {circuit_count} "
                      f"constrain_instance but no metadata push carries {label}"
                      f"{'' if identity else ' (circuit declares no identity string)'}")
                fail("no-metadata-push")
                failed_circuits.add((contract_name, circuit_name))
                continue
            unreadable = (f"the vector for {label} is built elsewhere (the namespace is passed to "
                          f"a builder or supplied by the caller)")
        elif all(elems is None for _, elems in matched):
            label = matched[0][0]
            unreadable = (f"the metadata push for {label} is not a literal vector (the caller "
                          f"supplies it)")
        if unreadable is not None:
            if client_count is None:
                if f"{contract_name}/{circuit_name}" in CLIENT_UNRESOLVED:
                    print(f"WARN(0/3): {contract_name}/{circuit_name} — {circuit_count} "
                          f"constrain_instance; {unreadable}; and no client `to_vec` was located "
                          f"either, so NEITHER leg was compared (DECLARED)")
                    no_leg.add((contract_name, circuit_name))
                else:
                    # WORSE THAN THE TWO-LEG CASE, SO AT LEAST AS LOUD. A circuit declared in
                    # `circuit_client_alignment_exceptions.txt` says "one leg is unchecked here";
                    # an undeclared one that reaches this line had nothing compared at all, and
                    # must not be able to appear without someone writing it down.
                    print(f"FAIL: {contract_name}/{circuit_name} — {circuit_count} "
                          f"constrain_instance; {unreadable}; and the client's `to_vec` was not "
                          f"located either, so NO leg was compared. Name the file in "
                          f"CLIENT_ALIASES, or declare the circuit in "
                          f"script/circuit_client_alignment_exceptions.txt")
                    fail("no-leg-compared")
                    failed_circuits.add((contract_name, circuit_name))
            elif client_count == circuit_count:
                print(f"OK(client): {contract_name}/{circuit_name} — {circuit_count} "
                      f"constrain_instance, {client_count} client public inputs ({label}; "
                      f"{unreadable})")
                passes += 1
                client_only.add((contract_name, circuit_name))
            else:
                print(f"FAIL: {contract_name}/{circuit_name} — circuit {circuit_count} "
                      f"constrain_instance vs client to_vec {client_count}: the proof would be "
                      f"created over a different public-input vector than the verifier uses "
                      f"({label}; {unreadable})")
                fail("circuit-vs-client")
                failed_circuits.add((contract_name, circuit_name))
            # NO `continue`. The pair rule below compares the client against the CIRCUIT and does
            # not read the metadata leg at all, so it must still run for these circuits — they are
            # exactly the ones whose reordering would otherwise go unremarked.
            matched = []

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
                    fail("circuit-vs-metadata")
                    failed_circuits.add((contract_name, circuit_name))
            elif client_count is None:
                # The THIRD leg, and until 2026-10-04 this branch printed a bare `OK`: a green line
                # covering two sides of a three-way invariant, on 48 of 166 circuits. The output now
                # says which legs it read, and an *undeclared* unresolved client is a failure rather
                # than a pass — so the gap can be closed but not grown.
                if f"{contract_name}/{circuit_name}" in CLIENT_UNRESOLVED:
                    print(f"OK(2/3): {contract_name}/{circuit_name} — {circuit_count} "
                          f"constrain_instance, {n} metadata pushes ({ns}; client to_vec "
                          f"unresolved and DECLARED)")
                    two_leg.add((contract_name, circuit_name))
                else:
                    print(f"FAIL: {contract_name}/{circuit_name} — {circuit_count} constrain_instance, "
                          f"{n} metadata pushes ({ns}), and the client's `to_vec` for this circuit was "
                          f"not located, so the third leg is unchecked. Name the file in "
                          f"CLIENT_ALIASES, or declare the circuit in "
                          f"script/circuit_client_alignment_exceptions.txt")
                    fail("third-leg-unchecked")
                    failed_circuits.add((contract_name, circuit_name))
            elif client_count != circuit_count:
                print(f"FAIL: {contract_name}/{circuit_name} — circuit {circuit_count} "
                      f"constrain_instance vs client to_vec {client_count}: the proof would be "
                      f"created over a different public-input vector than the verifier uses")
                fail("circuit-vs-client")
                failed_circuits.add((contract_name, circuit_name))
            else:
                print(f"OK:   {contract_name}/{circuit_name} — {circuit_count} constrain_instance, "
                      f"{n} metadata pushes, {client_count} client public inputs ({ns})")
                passes += 1

        # THE TX PAIR'S POSITION IN THE CLIENT — a hard rule, and the one place in this file where
        # a name mapping is not a heuristic.
        #
        # Everywhere else the order comparison is advisory because mapping a circuit's variable to
        # a Rust expression has to guess. The pair is different: `tx_binding` and `tx_nonce` are
        # the two names, they appear under those names in both the circuit and the client, and the
        # client's element at the circuit's index either names its variable or it does not. So this
        # is compared and failed on, not warned about.
        #
        # It compares client against CIRCUIT, never against a convention — which is what makes it
        # safe to run today, with 31 circuits still un-reordered. A circuit whose pair has not
        # moved has a client whose pair has not moved either, and this rule is silent; it fires
        # only when the two DISAGREE. That disagreement is the defect that cost the peer session a
        # 907-second run: the circuit and the arm reordered, the client overlooked, everything
        # counting correctly and the proof refused.
        if client_elems is not None and len(client_elems) == circuit_count:
            pair_idx = [k for k, v in enumerate(instance_order) if v in ("tx_binding", "tx_nonce")]
            if len(pair_idx) == 2:
                for k in pair_idx:
                    want = instance_order[k]
                    # Case-insensitive: a client that computes the binding inline names the
                    # constant (`DRK_POSEIDON_DOMAIN_TX_BINDING`), not the circuit variable.
                    got = client_elems[k].lower()
                    if want.lower() not in got:
                        shown = client_elems[k].split("//")[0].strip()
                        shown = shown if len(shown) <= 58 else shown[:55] + "..."
                        print(f"FAIL: {contract_name}/{circuit_name} — the circuit instances `{want}` "
                              f"at position {k + 1} of {circuit_count}, and the client's `to_vec` "
                              f"supplies `{shown}` there. The two vectors disagree element for "
                              f"element; a proof built by this client is refused (OBL-C198).")
                        fail("element-order")
                        failed_circuits.add((contract_name, circuit_name))

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
# COVERAGE OF THE THIRD LEG, printed on its own line and never folded into `Passed`. A row that
# checked two of three sides is not the same claim as a row that checked three, and a single
# number covering both is how the 48 became invisible in the first place.
if two_leg:
    print(f"Third leg unchecked: {len(two_leg)} circuit(s) — the client `to_vec` was not located, "
          f"so only the circuit instance order and the metadata push were compared. Declared in "
          f"script/circuit_client_alignment_exceptions.txt; register OBL-C198.")
# THE MIDDLE LEG, the third coverage state and the one that was missing until 2026-10-04. These
# circuits are in `Passed` — the client was compared against the circuit, which is the leg that
# catches a proof built over a different vector — but leg 2 was not read, so the row is not the
# same claim as a three-leg row and is printed on its own line.
if client_only:
    print(f"Metadata leg unchecked (client compared instead): {len(client_only)} circuit(s) — the "
          f"metadata vector is not a literal at the push site, so leg 2 was not read. In these "
          f"shapes the same bytes reach both (`params.zk_public_inputs` is the client's `to_vec`), "
          f"so the comparison that matters was still made.")
# AND THE STATE THAT MATTERS MOST: nothing was compared. Named loudly, and `no_leg` is excluded
# from the PASS sentence below, because a circuit that reached no comparison has not been shown to
# agree with anything — it has been shown to be in scope.
if no_leg:
    print(f"UNCHECKED: {len(no_leg)} circuit(s) — neither the metadata leg nor the client leg "
          f"could be read, so NO comparison was made and these are NOT covered by the PASS below:")
    for _key in sorted(no_leg):
        print(f"  {_key[0]}/{_key[1]}")
# The other direction, so the list cannot outlive its sites: every declared circuit that the
# resolver DID reach this run has been repaired, and the declaration is now a line that lies by
# being present. Not a failure — a repair landing is not a defect — but it must be said, or the
# next reader counts 48 debts when there are fewer.
#
# ONLY ON A FULL RUN, and the guard is the whole point of this paragraph. `two_leg` holds the
# circuits this run actually walked; on `… <contract>` that is one contract's worth, so every
# declaration belonging to any *other* contract is absent from it and would be reported as
# repaired. Measured 2026-10-04: the first single-contract run after the six `multisig`/`purse`
# lines were read told the reader to delete twenty valid declarations, including
# `game_room/close_pot` from a run of `dex`. A NOTE that instructs a deletion is the kind of
# signal a reader obeys, so it is suppressed rather than softened when the coverage is partial.
# `no_leg` is excluded as well as `two_leg`: a circuit declared because NEITHER leg was readable
# is still a live debt, and reporting it as repaired would invert the declaration's meaning.
_stale_declarations = sorted(
    key for key in CLIENT_UNRESOLVED
    if tuple(key.split("/", 1)) not in two_leg and tuple(key.split("/", 1)) not in no_leg
) if len(COVERED) == len(FULL_COVERED) else []
if _stale_declarations:
    print("")
    print(f"NOTE: {len(_stale_declarations)} declared circuit(s) whose client was resolved this "
          f"run — the third leg IS checked for these, so remove their lines from "
          f"script/circuit_client_alignment_exceptions.txt:")
    for _key in _stale_declarations:
        print(f"  {_key}")

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

if failures == 0 and not literal_findings and not EXCEPTION_FILE_ERRORS:
    if no_leg:
        print(f"PASS(partial): {len(seen) - len(no_leg)} of {len(seen)} circuits across "
              f"{len(COVERED)} contracts have matching metadata push counts; {len(no_leg)} could "
              f"not be compared at all (named above)")
    else:
        print(f"PASS: All {len(seen)} circuits across {len(COVERED)} contracts have matching "
              f"metadata push counts")
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
        # `OBL-C124`: each class is named with its own count. Sorting by name keeps the line stable
        # between runs, so a diff of two reports shows what changed rather than what reordered.
        named = ", ".join(f"{k}={n}" for k, n in sorted(fail_kinds.items()))
        parts.append(f"{failures} failure(s) over {len(failed_circuits)} circuit(s) — {named}")
    if literal_findings:
        parts.append(f"{len(literal_findings)} literal-vs-value position(s) over "
                     f"{len(literal_circuits)} circuit(s)")
    if EXCEPTION_FILE_ERRORS:
        parts.append(f"{len(EXCEPTION_FILE_ERRORS)} malformed line(s) in {METADATA_EXCEPTIONS}")
    print(f"FAIL: {' and '.join(parts)}")
    print("")
    print("Root cause: a circuit's constrain_instance order must match the metadata")
    print("function's zk_inputs.push() order position-for-position (privacy.md §5.3).")
    print("A mismatch silently produces wrong proof verification.")
    sys.exit(1)
PYEOF
