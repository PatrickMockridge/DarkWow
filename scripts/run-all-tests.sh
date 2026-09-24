#!/bin/bash
# DarkWow unified integration test umbrella.
#
# Two tiers, sequential. Every gate runs and the summary at the tail reports all of them; the
# script's exit code is non-zero iff at least one gate failed. `--fail-fast` restores the older
# behaviour of stopping at the first failure.
#
# WHY EVERY GATE RUNS. It used to `exit 1` at the first failure, and the third of the eleven gates
# was red — so the other eight, including `make test`, `lake build DarkFi` and the axiom checker,
# had not run since that gate was wired in, and the tail's "umbrella summary" was unreachable on
# any failure. A gate that fails is information; eleven gates behind a curtain is not.
#
# Usage:
#   ./scripts/run-all-tests.sh            # Tier 1 (fast, hermetic)
#   ./scripts/run-all-tests.sh --tier 1   # same
#   ./scripts/run-all-tests.sh --tier 2   # Tier 1 + Docker pipeline
#   ./scripts/run-all-tests.sh --fail-fast  # stop at the first failing gate
#
# Tier 1 — fast + hermetic (seconds to minutes, no Docker, no network)
# Tier 2 — heavyweight E2E (Docker devnet, minutes to hours)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"
export RUST_MIN_STACK="${RUST_MIN_STACK:-67108864}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASSED=0
FAILED=0
FAILED_GATES=()
FAIL_FAST=0
GATE_RESULTS=()

run_gate() {
    local label="$1"; shift
    echo ""
    echo -e "=== ${label} ==="
    local status=0
    "$@" || status=$?
    if [ "$status" -eq 0 ]; then
        echo -e "${GREEN}PASS:${NC} ${label}"
        PASSED=$((PASSED + 1))
        GATE_RESULTS+=("${GREEN}PASS${NC}  ${label}")
    else
        echo -e "${RED}FAIL:${NC} ${label} (exit ${status})"
        FAILED=$((FAILED + 1))
        FAILED_GATES+=("${label}")
        GATE_RESULTS+=("${RED}FAIL${NC}  ${label} (exit ${status})")
        if [ "${FAIL_FAST}" -eq 1 ]; then
            echo -e "${YELLOW}--fail-fast: stopping at the first failing gate${NC}"
            exit 1
        fi
    fi
}

TIER="${1:-}${2:-}"  # allows "--tier 2" as two args: $1=--tier $2=2
for arg in "$@"; do
    [ "$arg" = "--fail-fast" ] && FAIL_FAST=1
done

# Static circuit audits first — they are seconds, and they need no build.
#
# Artifact freshness goes before everything that builds, deliberately. `make test` has
# `contracts` as a prerequisite, so a stale .source_hash fails inside the build with a
# bare "WARNING: ... is stale" naming one contract and no indication of how many others
# share the cause — and because src/sdk/** is in every contract's SOURCE_MANIFEST, one
# sdk edit makes all 32 stale. This gate names all of them in one command.
run_gate "contract artifact freshness"    bash "$SCRIPT_DIR/check-artifact-freshness.sh"
run_gate "circuit metadata alignment"     bash "$SCRIPT_DIR/check-circuit-metadata-alignment.sh"
run_gate "circuit domain separation"      bash "$SCRIPT_DIR/check-circuit-domain-separation.sh"
# OBL-Z1: the Orchard-class rule. The other two circuit gates are structural (counts, prefix
# presence); this is the only one that asks whether an exposed public input is *determined*.
# It is currently RED, and it is BLOCKING: re-measured 2026-09-24, it exits 1 on **5** unclassified
# instances over **178** circuits / **880** `constrain_instance` sites — insurance_market's two
# `required_capability_id`, bridge/withdraw's `token_minimum`, labor_market's `attestation_id`, and
# oracle/attest_value's `threshold`.
# This paragraph read "15 over 181 circuits / 897 sites" until 2026-09-24, and named sites that had
# since been adjudicated (oracle/aggregate's min_result and max_result, labor_market's
# milestone_payment_amount, roulette/settle_bet's payout) or **deleted** — `proofs/core/lead.zk` and
# `set_v1.zk` were removed that day rather than repaired: they were the two with no host anywhere in
# this repository, so no binding existed and no free-instance entry was permitted, and cutting them
# took the count from 11 to 5. Before that the number read "33". The gate is the authority and the
# register's OBL-Z16 agrees with it; a count here is stale prose the moment a circuit moves. Because
# `run_gate` has no allowlist, a red gate means no full-gate run can be green until its sites are
# adjudicated — each either derived, bound, or declared free with a reason in
# script/circuit_free_instances.txt. See doc/src/arch/verification-hazop.md, OBL-Z1 and OBL-Z16.
#
# TWO gates are red, not one — measured 2026-09-22 rather than inferred from this comment. The other
# is `circuit metadata alignment` (gate 2 above): re-measured 2026-09-23 it exits 1 with **19 count
# mismatches over 19 circuits and 10 literal-vs-value positions over 6 more**, and the register's
# OBL-Z2 still records that gate as passing on 68 circuit/site pairs, which is stale. The second
# class is new: a literal-vs-value rule was promoted from the advisory order comparison to a hard
# FAIL that day (OBL-C20), because a position where the circuit instances a value and the metadata
# pushes a literal constant is not a name-mapping question. That class is the other session's
# OBL-C78/OBL-C79, so it is named here and not duplicated. A "the full gate is green" claim is
# therefore two obligations away, not one: **5 unclassified instances and 5 metadata findings,
# re-measured 2026-09-24**. This sentence said 15 and 29, then 11 and 5, both stale — the numbers belong to the gates,
# and both gates print their own on every run.
run_gate "circuit instance derivation"    bash "$SCRIPT_DIR/check-circuit-instance-derivation.sh"
# The same sources, transcribed into Lean as data. `Transcribed.lean` is a *generated*
# artefact, so it is checked the way this tree checks generated artefacts — by re-running the
# generator: `--check` regenerates in memory and compares byte for byte, so an edited `.zk` source, an
# edited lexer, or a hand-edit to the module all fail here rather than leaving a module that describes
# circuits no longer in the tree. Seconds, no build, so it sits with the other circuit audits.
#
# WHAT A PASS MEANS, and it is narrower than "the circuits are safe": that the committed module is
# what the generator produces today. The verdicts inside it are the *model's*
# (`Circuits/InstanceDerivation`'s rule, one theorem per circuit closed by `decide`), and its header
# decomposes where those verdicts differ from this gate's checker — 167 circuits fail the model's
# strict rule, of which 155 are the checker's `redundant` class, 11 `declared-free`, 3 the checker's
# own `OBL-Z16` failures, and 1 the model's declared-constant boundary. It is the *data* side of
# `OBL-T7`'s `(r, s) ↦ circuit` bridge and not the bridge, which the module says it does not supply.
run_gate "circuit transcription freshness (OBL-T7)" \
                                          python3 "$REPO_ROOT/scripts/gen_circuit_transcription.py" --check
# OBL-C72/C73: the exec/apply phase rule — apply writes blindly, exec does not write. This gate
# existed and was invoked by NOTHING until now, which is why the class it guards drifted: seven
# live `db_set` calls in an exec phase, every one of them a call the host refuses at runtime
# (§B.2.2 `CALLER_ACCESS_DENIED`), so `drain_protection` is inert as shipped. It is wired here as
# a ratchet rather than a blocker: a site whose defect is understood and scheduled is named in
# `script/phase_host_function_exceptions.txt` and prints as EXCEPTED, while a *new* violation
# fails. Negative-controlled before wiring, both ways — with the exception file removed the gate
# exits 1 with all seven, and with one entry dropped it exits 1 naming exactly that site.
#
# The list is now EMPTY, because OBL-C73's repair landed: the seven writes moved into apply
# functions and their updates were widened to carry the values (apply may not read). The gate grew
# a stale-entry check at the same time, so an exception whose site no longer matches is reported
# rather than silently admitting the next finding of the same shape — that check is what showed the
# list was empty in one run instead of leaving seven dead entries in it.
run_gate "exec/apply phase rule"          bash "$SCRIPT_DIR/check-phase-host-functions.sh"
# OBL-Z18: the vacuous-binding rule. Promoted from report-only to a ratchet on 2026-09-23, when
# the sweep finished — all 57 candidates were read, circuit and host together, and each is listed
# in `script/circuit_pubkey_binding_exceptions.txt` with either the mechanism that makes it sound
# or the register row that schedules its repair (15 are genuine defects: OBL-C81, C82, C83, C84 and
# OBL-C75). A new vacuous binding anywhere fails this gate, and `hooks/pre-commit` now blocks on one
# too. The detector itself was left deliberately shallow — see that file's header for why a cleverer
# classifier was rejected.
run_gate "circuit pubkey binding"         bash "$SCRIPT_DIR/check-pubkey-binding.sh"
# OBL-C99: a client builder's params are the params the contract decodes — the wallet's call is the
# contract's call. A client that declares its own params type is a second encoder for one function,
# and the two drift: `drain_protection`'s `initialize`, `execute` and `transfer` builders returned
# four-to-five-field types where the contract decodes eight to ten, so a wallet following the builder
# API built a call the entrypoint refused as truncated — and **nothing read both sides**, which is
# why it survived. The three types are gone (removed in the same commit that added this gate), so
# rule (B) — a client-declared params codec — is a clean ratchet with no exception list. Rule (A) —
# a client type whose *name* is a model type's minus its version suffix — is name-based and therefore
# shallow, stated as such in the script's header; `dao_escrow`'s two sites are declared in
# `script/client_params_alignment_exceptions.txt` (OBL-C103) because those client files are another
# session's in-flight rewrite, not because the sites are sound. Negative-controlled both ways before
# wiring: with the exception list removed the gate exits 1 naming both dao_escrow sites, and with a
# client codec injected into an unrelated contract it exits 1 naming that struct.
run_gate "client params alignment"        bash "$SCRIPT_DIR/check-client-params-alignment.sh"
# OBL-C76: the tests that do not run in their crate's DEFAULT configuration are enumerated and
# declared. `make test` runs `--release --all-features --workspace`, so this gate's subject is not the gate run — it is the ad-hoc `cargo test -p <crate>` one reaches
# for to check a single crate, and the claim that follows it: 172 tests where `--all-features` lists
# 263, with the 91 that differ being the consensus surface. Four mechanisms hide a test here —
# `#[ignore]`, a feature-gated test fn, a feature-gated test module, and a module gated in its
# PARENT (which is why walking files alone finds 20 of the 29) — and each site must be declared in
# `script/hidden_test_exceptions.txt` with a reason and the command that runs it.
run_gate "hidden tests declared (OBL-C76)" bash "$SCRIPT_DIR/check-hidden-tests.sh"
# The documentation index. Also seconds, also needs no build: it checks that every
# doc is listed and every citation resolves, both directions. The 2026-09 docs
# clean-up removed 25 documents and repointed ~30 referrers by hand; this is what
# keeps that from silently un-happening.
run_gate "documentation index"            bash "$SCRIPT_DIR/check-doc-index.sh"
# A register row that cites a path is making a claim about the tree, and this is the gate that holds it
# to one: every in-scope cited path must resolve against HEAD, with any deliberate exception declared in
# `script/register_artifact_exceptions.txt` — and an exception whose path now resolves, or that its row
# no longer cites, is reported **stale** rather than kept quietly. It was written for exactly the failure
# it then caught and nobody saw: three OBL-T7 exemptions covering the transcription's artefacts while
# they were untracked, which went stale the moment those artefacts landed here on 2026-09-24
# (`3e5dc703ed`, `e5ef7c1c96`). It ran nowhere, so the signal was never delivered — the tree's own
# "gate that cannot fail" class, in the shape of a gate that cannot *report*. Wired now; 0.23 s.
run_gate "register artifacts resolve (OBL-T7)" bash "$SCRIPT_DIR/check-register-artifacts.sh"
# The barb alphabet, in five representations: the Lean `inductive Barb`, the core `BarbId`, the sdk
# `Barb`, the Python model, and `type-system.md` §1.1. `contrib/barb_alphabet_diff.sh` extracts and
# diffs the four sets mechanically; `contrib/primitive_barbs_diff.sh` does the same for the *type→barb
# mapping*, which is the table the type-system properties are stated over and which exists twice.
# Both were written for exactly this purpose and **invoked by no runner** until 2026-09-24 — which the
# register records as OBL-T9's coverage gap, and it is the `OBL-C76` class: agreement checked by a
# script nobody runs is agreement nothing checks. They take no arguments, need no build, write their
# reports under `/tmp` rather than into the tree, and exit non-zero on disagreement.
#
# This is what closes that gap, and it closes it more strongly than the Rust test that was going to:
# `src/sdk/src/capability.rs`'s `test_all_primitives_have_distinct_barb_sets` covered **10 of 17**
# primitives, and raising it to 17 would have edited `src/sdk/**` — which is in *every* contract's
# `SOURCE_MANIFEST`, so it would have invalidated all 32 recorded artifacts and failed the freshness
# gate above until they were rebuilt. The diff script checks all 17, names the seven the model has and
# the Rust does not, and needs no rebuild.
run_gate "barb alphabet agreement (OBL-T3/T9)" bash "$REPO_ROOT/contrib/barb_alphabet_diff.sh"
run_gate "primitive barb mapping (OBL-T3)"     bash "$REPO_ROOT/contrib/primitive_barbs_diff.sh"
# The coinbase classifier, at every accept-path site. `first_call_is_pow_reward` matches `data[0] ==
# 0x05` against **any** contract and ~20 contracts use 0x05 as a real function code, so it answers
# "coinbase" for transactions that are not the coinbase. Where the exemption *skips a check* that is a
# soundness bug: the L2 witness loop in `execution.rs` — the only place a transaction's proofs are
# verified at block acceptance — exempted such transactions entirely, so a fabricated proof rode in.
# Corrected at four sites on 2026-09-24 (`execution.rs`, the miner's assembly filter, and both RPC
# predicates), and this gate is what keeps them corrected. It is a source gate rather than an
# end-to-end test for the reason in its header: the fixtures here top out at selector 0x04.
run_gate "coinbase classifier (shared, not selector-only)" bash "$SCRIPT_DIR/check-coinbase-classifier.sh"
# OBL-C77: an empty get_metadata arm is the host's *rejection* signal, not "no public inputs".
# `execution.rs` decodes the first encoded vector out of the metadata and fails the call at
# `metadata-decode-zkp` on an empty buffer, before exec runs — so an arm whose success path returns
# `vec![]` makes the instruction impossible to call, which is how `relayer_endowment`'s eight
# plaintext instructions were unreachable until 2026-09-23. The gate is a shape check (the arm's
# value is a literal empty vector) and every finding must be declared in
# `script/metadata_plaintext_exceptions.txt` with the reason it must reject; a stale entry is
# reported. Negative-controlled both ways: dropping one entry fails naming exactly that site, and a
# fake entry is reported stale while the run still passes.
run_gate "empty metadata arms declared"   bash "$SCRIPT_DIR/check-metadata-arms.sh"
# The register's own convention, guarded. Four times the register has been wrong about its own
# bookkeeping and only a human reading a diff noticed — including three markers written on 2026-09-22
# with words the register does not use ("STALE", "CONFIRMED") or with no status word at all. This is
# the mechanical check for that class: a row whose last cell opens with an ALL-CAPS word must open it
# with a status from the register's stated vocabulary. Narrow by construction (last cell only, bold
# span only, ALL-CAPS only) so it cannot cry wolf, which is why it is green today while ten rows still
# carry no marker at all — it guards the form of a marker, not the existence of one.
# Negative-controlled before wiring, per the OBL-Z18 lesson: a copy of the register with a coined
# marker appended as OBL-C23's last cell fails the check; the register itself passes.
run_gate "register status markers"        bash "$SCRIPT_DIR/register-status.sh" --check

run_gate "build contract ZK circuits"     "$SCRIPT_DIR/build-contract-zk.sh"
# OBL-Z8: the compiled artefacts are structurally valid. This is the only check that looks at the
# `.zk.bin` files themselves rather than at the sources — `validate_zk_bins.sh` runs the compiler's
# own `validate` over every one of them — and it ran nowhere until 2026-09-23. It belongs here,
# immediately after the build that produces them, because `.zk.bin` is a build artefact: **zero are
# tracked in git** against 224 tracked `.zk`, so there is no committed binary to be stale. Freshness
# against the source is already covered twice over — the Makefiles declare
# `proof/%.zk.bin: proof/%.zk`, and `ZK_SRC := $(wildcard proof/*.zk)` is in every contract's
# `SOURCE_MANIFEST`, so an edited circuit makes the contract stale and `contract artifact freshness`
# (gate 1) names it. What was missing is exactly this: nobody checked the *output* is well-formed.
run_gate "ZK binaries well-formed"        bash "$SCRIPT_DIR/validate_zk_bins.sh"
run_gate "pre-build guard (dwowd + wallet + 32 contracts→wasm32)" \
                                          "$SCRIPT_DIR/check_pipeline_build.sh"
run_gate "Rust tests (make test)"          make test

# Lake requires its own working directory.
#
# NOTE: the target is `DarkFi`, not a bare `lake build`. A bare `lake build` builds "the default
# facet of the root package" — which for this package is nothing at all: it exits 0 without
# compiling a single module. The gate below said `lake build` and therefore passed for as long
# as it existed while 24 of 50 modules did not compile. `lake build DarkFi` is what actually
# type-checks the proofs. See proofs/lean/README.md.
#
# GUARDED BY `scripts/lean-build.sh`, which is the 2026-09-24 correction to the paragraph that stood
# here. `LEAN_NUM_THREADS=4` was this file's whole guardrail and it is not sufficient: the cap bounds
# how many `lean` processes run, not how much memory any one of them uses. On 2026-09-24 **this exact
# gate** — `LEAN_NUM_THREADS=4 lake build DarkFi` — exhausted this host's memory and froze the
# machine, taking every open window with it; the failed scope `run-rdfa5ca85…` still in
# `systemctl --user list-units` is that command. The wrapper keeps the thread cap and adds a cgroup
# `MemoryMax` ceiling, so a runaway elaboration is killed with the exit explained and the desktop
# intact rather than taking the box down. See proofs/lean/README.md.
#
# The paragraph that stood here said the cap covered "both invocations" — the build, and the
# collector inside `script/check_lean_axioms.py`. It covered one: the collector runs its own
# `lake env lean --run` as a separate process, and `env LEAN_NUM_THREADS=4` on the line below was
# scoped to this gate alone. It is guarded now too, through the wrapper's `--stream` mode.
#
# TWO TARGETS, and the second is load-bearing. The transcription is a library of its own so that
# `lake build DarkFi` does not elaborate it — a generated module of one `decide` proof per circuit,
# 178 circuits over 2677 statements as this line is written (the generator now *derives* both counts
# into the module's own header; it used to hold them as literals and contradict itself, so read the
# header rather than this comment). That separation was made on 2026-09-24
# while the artefact could not build at all: it exceeded 24 GiB in one `lean` process and took this
# host down. **What caused it is not established, and this comment used to state a cause as settled
# fact.** It read: "The cause turned out to be an inline `if` in the model's `boundWalk` whose
# branches the kernel's reduction duplicated, compounding per statement." That explanation is
# **retracted** — `bindAssign`'s docstring in `DarkFi/Circuits/InstanceDerivation.lean` records it as
# "not proved, and it cannot be the whole story" (a refuted circuit still forces the walk over every
# statement before its first undetermined exposure, and `purse/withdraw` forces a deeper chain than
# `burn` did), and `proofs/lean/README.md` says the mechanism is deliberately not asserted. What *is*
# measured is the extraction's effect: extracting `bindAssign` removed the blow-up and the artefact
# builds whole in 743 MB. This line said "78 s" where the other four records that quote a time say
# "~71 s"; the disagreement is left visible rather than resolved by picking one, because nobody has
# re-measured it since. The separation is kept
# for the reason it is still owed: `CheckAxioms.lean`
# imports this module by name, so its `.olean` must exist before the collector can run — and with
# `--require-collector` that is a hard failure rather than a silent gap, which is why both targets are
# named here rather than left for the axiom gate to discover.
run_gate "Lean proofs (lake build DarkFi + Transcribed)" \
                                          "$SCRIPT_DIR/lean-build.sh" build DarkFi Transcribed

# The assumption boundary. Runs after the build it depends on: the budget check walks the
# compiled environment, so it needs `lake build DarkFi` to have succeeded. `--require-collector`
# makes a failure to run the collector fatal, so a red build cannot present itself as a clean
# boundary.
run_gate "Lean assumption boundary (axioms/budgets)" \
                                          python3 script/check_lean_axioms.py --require-collector

# The boundary gate's own detector, exercised. `check_lean_axioms.py` now fails when the collector's
# rows do not account for exactly the names it was fed — the check that catches a row renamed in
# flight, which a row *count* cannot. Measured 2026-09-24: a 4096-byte stdout flush boundary put the
# collector's stderr summary inside a row, `supply_chain_invariant` came back as `nvariant`, its budget
# was never checked, and the count was right (see `--stream` in `scripts/lean-build.sh`). A detector
# that has never been shown to fail is a claim rather than a check, so its negative control is its own
# gate — hermetic, no Lean, no files, seconds.
run_gate "Axiom gate's record channel (negative control)" \
                                          python3 script/check_lean_axioms.py --self-test

run_gate "Python: pipeline model"          python3 contrib/model/pipeline_model.py
run_gate "Python: supply chain model"      python3 contrib/model/supply_chain_model.py

# The rest of `contrib/model/`. Twenty-two models existed and three were gated; of the ungated ones,
# `chain_validation_model.py` caught its own `AssertionError`s per test, counted them, and fell off
# the end of its `__main__` with exit 0 whether they passed or failed — so wiring it as it stood would
# have added a gate that *cannot* fail, which is worse than leaving it ungated: an ungated model is
# invisible, a vacuous gate is a false assurance counted in the tally. It was given a failure-sensitive
# exit and falsified by injecting a failure through its own error path rather than by flipping an
# assertion — flipping one is a misleading control, because a model that swallows test failures still
# exits non-zero when its first `assert` sits outside the `try`. `test_oracle.py`, which could not be
# run at all, was repaired the same way.
#
# **Two of the models below are wired with a caveat rather than a clean contract, and an independent
# reader is why this says so rather than asserting they were all falsified.** `vm_state_model.py`
# *demonstrates* known crash paths, so its printed `FAIL — …` lines are its expected output: it can
# still fail (through the assertions on its mitigations), but its summary reads like a red gate, and
# nothing ties its exit status to the findings it prints. `dockernet_model.py` likewise counts its own
# `SKIP`/`FAIL` lines as passes under its runner. Both belong in the register as rows rather than being
# rewritten here: what they should assert is a question about the models, not about the wiring.
#
# Measured aggregate ≈4.5 minutes, dominated by `chain_validation_model` (98 s) and
# `wallet_simulation` (97 s); the other sixteen are 0–29 s. Two of the twenty-two are excluded, with
# their reasons: `capability_discovery.py` is a report with no assertions and no exit, and
# `generate_wallet_fixture.py` is a generator that needs an `--out`.
run_gate "Python: chain model"              python3 contrib/model/chain_model.py
run_gate "Python: chain validation model"   python3 contrib/model/chain_validation_model.py
run_gate "Python: dex lock model"           python3 contrib/model/dex_lock_model.py
run_gate "Python: dockernet model"          python3 contrib/model/dockernet_model.py
run_gate "Python: fee model"                python3 contrib/model/fee_model.py
run_gate "Python: fee window model"         python3 contrib/model/fee_window_model.py
run_gate "Python: halo2 math"               python3 contrib/model/halo2_math.py
run_gate "Python: key management"           python3 contrib/model/key_management.py
run_gate "Python: merge mining spec"        python3 contrib/model/merge_mining_model.py
run_gate "Python: nullifier lifecycle"      python3 contrib/model/nullifier_lifecycle.py
run_gate "Python: proof of token balance"   python3 contrib/model/proof_of_token_balance.py
run_gate "Python: sync model"               python3 contrib/model/sync_model.py
run_gate "Python: test oracle"              python3 contrib/model/test_oracle.py
run_gate "Python: transaction lifecycle"    python3 contrib/model/transaction_lifecycle.py
run_gate "Python: uncle fork model"         python3 contrib/model/uncle_fork_model.py
run_gate "Python: vm state model"           python3 contrib/model/vm_state_model.py
run_gate "Python: wallet model"             python3 contrib/model/wallet_model.py
run_gate "Python: wallet simulation"        python3 contrib/model/wallet_simulation.py
# The three-chain merge-mining model, whose "CARIBINA rejects the attacker fork" table
# `doc/src/arch/caribina.md` quotes. It ran nowhere until 2026-09-22, so the table's numbers sat in
# the docs unchecked while the model's Caribina settlement was measured on the very chain under
# attack (see OBL-C66 and the finality-widget campaign). It self-verifies: a failing assertion exits
# 1, so it is a gate and not just a printout.
run_gate "Python: merge mining model"      python3 contrib/docker/darkwow-testnet/merge_mining_model.py

if [ "$TIER" = "--tier 2" ]; then
    run_gate "Docker test pipeline (native, 2 wallets)" \
        "$REPO_ROOT/contrib/docker/darkwow-testnet/test_pipeline.sh" --mode native --with-wallet 2
fi

echo ""
echo "========================================"
echo -e "Umbrella summary: ${GREEN}${PASSED} passed${NC}, ${RED}${FAILED} failed${NC}"
echo ""
for result in "${GATE_RESULTS[@]}"; do
    echo -e "  ${result}"
done
if [ "$FAILED" -gt 0 ]; then
    echo ""
    echo -e "Failed gates: ${RED}${FAILED_GATES[*]}${NC}"
fi
echo "========================================"

[ "$FAILED" -eq 0 ] || exit 1
exit 0
