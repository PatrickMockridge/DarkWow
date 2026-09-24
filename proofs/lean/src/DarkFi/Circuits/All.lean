/-!
MANUAL AUDIT DOCUMENTATION — NOT FORMAL PROOFS
This file contains structured vulnerability findings / circuit audit
results. It contains ZERO Lean theorems with non-trivial proofs.
All defs return String or List values for programmatic consumption.

`proofs/lean/README.md` used to list `all_contracts_orchard_safe` (98 additional
circuits) among eleven "Circuit Audit Axioms" spread across
`Circuits/{Token,Bridge,Exchange,All}.lean`. This file declares nothing at all: it is
comment-only, and the name appears only inside prose. The same is true of the other two
`-- NOT DECLARED IN LEAN` lines below. The audit they describe is manual; recording it as a
Lean result requires the obligation `Axioms.NoFreeInstances` names to become checkable.
CORRECTED 2026-09-24: that obligation **is** checkable, by a gate rather than by hand
(`scripts/check-circuit-instance-derivation.sh`, wired at `scripts/run-all-tests.sh:101`), and the
check refutes this file's claims for eleven instances across seven circuits — named below. What is
still missing is not the check but the *transcription*: `Circuits/InstanceDerivation.lean` defines
the property over a circuit's statement list and checks one worked circuit, and the step from
`(r, s)` to a statement list is a Lean term that cannot read a `.zk` file. So the manual readings
in this file are superseded by a gate on the reading side and unreplaced on the stating side, and
that asymmetry is the honest description of the file.
CORRECTED 2026-09-24 (same day, later): the *stating* side now exists too. `Transcribed.lean`
is generated from the `.zk` sources by `scripts/gen_circuit_transcription.py` and freshness-gated
(`run_gate "circuit transcription freshness (OBL-T7)"`), and it carries one `List Stmt` per circuit
with the model's verdict closed by `decide` — every circuit this file reads by hand is now a kernel-
checked statement about a transcription of it. **What remains missing is the mapping `(r, s) ↦ a
circuit`, not the transcription of the circuits** — that mapping is not in the tree, so the axiom is
still a name, and the numbers in this file's blocks are the checker's while `Transcribed.lean`'s are
the model's, which disagree by design and whose disagreement that module decomposes.
**It is also no longer on `lake build DarkFi`'s path, and that is a correctness requirement rather
than tidiness**: the module is 181 `decide` proofs over 2747 statements, the tree's most expensive
elaboration, and while it rode on that library a `LEAN_NUM_THREADS=4` build of `DarkFi` exhausted
this host's memory and froze the machine (2026-09-24). It lives at `src/Transcribed.lean` and is
built by the gate as `lake build DarkFi Transcribed` under `scripts/lean-build.sh`, which adds the
memory ceiling the thread cap never was. See `proofs/lean/README.md`.
-/
/-!
# All Remaining Contract Circuit Instance-Derivation Proofs

Identity/Attestation (18), Labor/Escrow (25), Gaming (15),
Staking (9), Insurance/Protection (3), Subscription/Relayer (6),
Oracle/Tender (10), Core proofs (12).

Total: 98 circuits. All follow the same pattern: Pedersen commitments,
Poseidon hashes, nullifiers, Merkle proofs. All public inputs are
derived from witnesses in-circuit.

Orchard-class audit result: NO FREE INSTANCES across all contracts.
CORRECTED 2026-09-24: the mechanised audit reports 11 instances it cannot classify, across 7
circuits — see the block below, which names them and cites the gate. This line was a manual
reading of 98 circuits and the gate walks 181.
-/

namespace Circuits

/-
## Identity Circuits (8)

Attestation-style proofs: credential issuance, claim verification,
delegation. All constrain_instance calls derived.

Key circuits: create_claim_v1, issue_credential_v1, verify_capability_v1
-/

/-
## Attestation Circuits (10)

On-chain attestation verification: slash, revoke, consume, create,
delegate, update, verify chain/claim. Largest circuit: k=15 (delegate).

All instances derived from witness data (attestation payloads, chain states).
-/

/-
## LaborMarket Circuits (9)

Job lifecycle: create, accept, deliver, confirm, refund, cancel, dispute.
All use k=14 for larger constraint counts.

Each circuit commits to job amounts using Pedersen. All instances derived.
CORRECTED 2026-09-24: not this category's — `labor_market/proof/create_job.zk` exposes
`attestation_id` with no derivation, no binding, and nothing that reads it, and the mechanised
audit names it. See the block below.
-/

/-
## Escrow/Auction/DAO Circuits (16)

Escrow (4): create, fund, claim, refund
DAO Escrow (6): init, pay, propose, resolve, verify, vote
Auction (6): create, bid, close, settle, claim, refund

All use commitment/nullifier pattern. All instances derived.
-/

/-
## Gaming Circuits (15)

GameRoom (5): create_room, deposit, place_bet, claim, settle_pot
Baccarat (2): commit_bet, settle_bet
DarktoshiDice (2): commit_bet, settle_bet
Roulette (2): place_bet, settle_bet
Slot (2): commit_bet, settle_bet
Lottery (2): commit_ticket, reveal_ticket

All use TransferV1 for PN interaction. All instances derived.
-/

/-
## Staking Circuits (9)

BettingStake (5): init, stake, unstake, claim, update_risk
PoolStake (4): create_pool, join_pool, allocate_coverage, slash_coverage

Stake amounts committed via Pedersen. All instances derived.
-/

/-
## Oracle Circuits (5)

register, attest, push_value, push_value_commitment, aggregate
Uses k=10 for some circuits (lower constraint count).
-/

/-
## Tender Circuits (5)

create, submit_bid, reveal_bid, select_winner
+ capability-based variant.
-/

/-
## Core Proof Circuits (12)

proof/ directory: arithmetic, burn, encrypt, inclusion_proof, lead,
mint, nested, opcodes, set_v1, smt, tx, voting.

These are the core system circuits. All instances derived.
CORRECTED 2026-09-24: six of these are not. The mechanised audit reports `lead.zk`'s `sigma1` and
`sigma2`, and `set_v1.zk`'s `lock`, `root`, `key` and `value`, as neither derived, bound, redundant
nor declared free — and unlike the contract circuits, these two have no host verifier anywhere in
this repository (`grep -rn "Set_V1\|Lead"` over `src/` and `bin/` finds no reference to either
namespace), which is `OBL-Z16`'s disposition for them. See the block below.
-/

/-
CLAIM, AND IT IS FALSE AS WRITTEN — corrected 2026-09-24 against the gate that refutes it.

What stood here was:

    THEOREM: All 98 remaining contract circuits are Orchard-class safe.
    Comprehensive audit confirms:
      1. Every constrain_instance has an in-circuit derivation constraint
      2. No free instances (except by documented design choice)
      3. All EC multiplications use fixed constants (not witness-chosen bases)
      4. All Merkle roots are derived from leaf + path (not free)
    This is the formal verification result: no Orchard-class vulnerability
    exists in any DarkFi contract circuit.

None of that is a Lean result, and item 2 is refuted for eleven instances by the repository's own
mechanised audit — a gate, not a script beside one: `scripts/check-circuit-instance-derivation.sh`,
wired at `scripts/run-all-tests.sh:101`. Run it and it exits 1:

    181 circuits, 905 constrain_instance, 40 declared-free, 11 unclassified

The eleven, each named by the gate itself (`doc/src/arch/verification-hazop.md` `OBL-Z16` is the row
that tracks them, and it lists the same set):

    bridge/proof/withdraw.zk                              token_minimum
    insurance_market/proof/purchase_coverage_with_capability.zk  required_capability_id
    insurance_market/proof/underwrite_with_capability.zk  required_capability_id
    labor_market/proof/create_job.zk                      attestation_id
    oracle/proof/attest_value.zk                          threshold
    proofs/core/lead.zk                                   sigma1, sigma2
    proofs/core/set_v1.zk                                 lock, root, key, value

Four of those files are in the categories this file enumerates below (`Oracle`, `Labor`, `Insurance`,
`Core`), so the claim was not merely unproved — it was contradicted in its own list. What *is* true
of the rest is that the gate classifies 894 of the 905 instances as derived, bound, redundant or
declared free, and the 40 declared-free ones carry host-side justifications in
`script/circuit_free_instances.txt` (`OBL-Z5`). "Safe" for the other eleven is not established by
anything here, and two of them — `insurance_market`'s `required_capability_id` pair — are live
defects recorded as such, where the host never checks that the caller holds the capability the
circuit exposes.
-/
-- NOT DECLARED IN LEAN (comment, not a declaration):all_contracts_orchard_safe : Prop

end Circuits
