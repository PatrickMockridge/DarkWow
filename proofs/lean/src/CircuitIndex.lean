/-
# The `(r, s)` -> circuit index, generated

**GENERATED FILE — do not edit.** `scripts/gen_circuit_index.py` writes it from the manifests, the
`.zk` sources and the reviewed map in `script/circuit_index_map.txt`, and `scripts/run-all-tests.sh`
re-runs the generator in `--check` mode, so a stale copy is a gate failure. It reuses the model's rule
and the checker's own classifier rather than re-deriving either.

## What this is

`Axioms.NoFreeInstances (r : Resource) (s : Action)` is indexed by a resource/action pair, and five
places in the tree name the same missing half — `(r, s) ↦ the circuit source` "is still not in the
tree" (`Axioms.lean`, `proofs/lean/README.md`, `Capability/Inversion.lean`, `Circuits/Token.lean`,
`Circuits/All.lean`). This module is that half for the **12** pairs
`Capability/Composition.lean` instantiates. Each one names the contract and function the reviewed map
records, the circuit its manifest's `proof_circuit` resolves to, and the transcription's definitions
for it — with the verdict closed by `decide`, so the *kernel* is what establishes it.

## The verdict is the model's second predicate, because the strict one fails at every pair

Each theorem is `Circuits.InstanceDerivation.DisclosureRule` at that pair's transcribed statement list
— the strict `NoFreeInstance` plus the checker's `redundant` classification (a witness already inside
another exposed determination). Measured: **all 12 of these circuits fail the strict rule, every one
of them for a `redundant` exposure, and none of them has a `declared-free` instance** (0 declared
free across the 12). So the proofs rest on the structural rule with **no** external exception list
behind them, and that is a fact this generator checks rather than a claim it prints: it fails if any of
these circuits ever gains a declared-free instance, because `DisclosureRule` does not model that class
and could not carry it.

**What the rule does not say, and it matters more here than anywhere.** `redundant` buys that the
exposure adds no freedom — not that the witness is safe. The witness stays prover-chosen, and whether
that matters is a property of the contract's entrypoint and not of the statement list. That obligation
is `OBL-Z1`'s, and nothing in this module or the one it imports discharges it.

## The axiom is gone, and this module is what replaced it

`Axioms.NoFreeInstances` is **deleted** as of 2026-09-24. `Capability.Inversion.CircuitDerivable` now
carries the data that premise was about — the circuit's `held` names, its `stmts` — and a
`noFreeInstances` field that is a **computation** over them rather than an uninterpreted `Prop`. The
inhabitants below are that premise supplied rather than assumed at every pair this tree has a circuit
for, and the projection beside each one shows the bare rule is the same fact.

## What this is not

Nor is any of this a claim about the deployed circuits. The transcription is source-faithful *as data*,
and `script/circuit_instance_derivation.py`'s own caveats are inherited unchanged — it reads `.zk`
source rather than the `.zk.bin` that is deployed, and it does not know what the opcodes mean. See
`OBL-T7`.
-/

import DarkFi.Circuits.InstanceDerivation
import DarkFi.Capability.Inversion
import Transcribed

namespace CircuitIndex

open Circuits.InstanceDerivation
open DarkFi.Capability.Composition

/-! ===== One inhabitant and its projection, per `(r, s)` pair — 12 pairs ===== -/

/-- `daoVoteType` — (`dao_governance`, `vote`) maps to `dao_escrow`'s `vote_claim`, circuit `VoteClaimV2`,
    transcribed as `Circuits.Transcribed.dao_escrow_vote_claim`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def daoVoteType_circuitDerivable : CircuitDerivable daoResource voteAction :=
  { primitives := daoVoteType.primitives
   , coversBarbs := daoVoteType.coversBarbs
   , held := Circuits.Transcribed.dao_escrow_vote_claim_held
   , stmts := Circuits.Transcribed.dao_escrow_vote_claim_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem daoVoteType_disclosureRule :
    DisclosureRule Circuits.Transcribed.dao_escrow_vote_claim_held
      Circuits.Transcribed.dao_escrow_vote_claim_stmts :=
  daoVoteType_circuitDerivable.noFreeInstances

/-- `tenderBidType` — (`tender`, `submit_bid`) maps to `tender`'s `submit_bid`, circuit `SubmitBidV2`,
    transcribed as `Circuits.Transcribed.tender_submit_bid`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def tenderBidType_circuitDerivable : CircuitDerivable tenderResource bidAction :=
  { primitives := tenderBidType.primitives
   , coversBarbs := tenderBidType.coversBarbs
   , held := Circuits.Transcribed.tender_submit_bid_held
   , stmts := Circuits.Transcribed.tender_submit_bid_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem tenderBidType_disclosureRule :
    DisclosureRule Circuits.Transcribed.tender_submit_bid_held
      Circuits.Transcribed.tender_submit_bid_stmts :=
  tenderBidType_circuitDerivable.noFreeInstances

/-- `purseBalanceType` — (`purse_balance`, `balance`) maps to `purse`'s `balance`, circuit `Balance`,
    transcribed as `Circuits.Transcribed.purse_balance`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def purseBalanceType_circuitDerivable : CircuitDerivable purseResource purseViewAction :=
  { primitives := purseBalanceType.primitives
   , coversBarbs := purseBalanceType.coversBarbs
   , held := Circuits.Transcribed.purse_balance_held
   , stmts := Circuits.Transcribed.purse_balance_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem purseBalanceType_disclosureRule :
    DisclosureRule Circuits.Transcribed.purse_balance_held
      Circuits.Transcribed.purse_balance_stmts :=
  purseBalanceType_circuitDerivable.noFreeInstances

/-- `purseWithdrawType` — (`purse_withdrawal`, `withdraw`) maps to `purse`'s `withdraw`, circuit `Withdraw`,
    transcribed as `Circuits.Transcribed.purse_withdraw`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def purseWithdrawType_circuitDerivable : CircuitDerivable purseWithdrawResource withdrawAction :=
  { primitives := purseWithdrawType.primitives
   , coversBarbs := purseWithdrawType.coversBarbs
   , held := Circuits.Transcribed.purse_withdraw_held
   , stmts := Circuits.Transcribed.purse_withdraw_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem purseWithdrawType_disclosureRule :
    DisclosureRule Circuits.Transcribed.purse_withdraw_held
      Circuits.Transcribed.purse_withdraw_stmts :=
  purseWithdrawType_circuitDerivable.noFreeInstances

/-- `purseDepositType` — (`purse_deposit`, `deposit`) maps to `purse`'s `deposit`, circuit `Deposit`,
    transcribed as `Circuits.Transcribed.purse_deposit`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def purseDepositType_circuitDerivable : CircuitDerivable purseDepositResource depositAction :=
  { primitives := purseDepositType.primitives
   , coversBarbs := purseDepositType.coversBarbs
   , held := Circuits.Transcribed.purse_deposit_held
   , stmts := Circuits.Transcribed.purse_deposit_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem purseDepositType_disclosureRule :
    DisclosureRule Circuits.Transcribed.purse_deposit_held
      Circuits.Transcribed.purse_deposit_stmts :=
  purseDepositType_circuitDerivable.noFreeInstances

/-- `identityCredentialType` — (`identity_credential`, `verify_credential`) maps to `identity`'s `verify_capability`, circuit `VerifyCapabilityV2`,
    transcribed as `Circuits.Transcribed.identity_verify_capability`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def identityCredentialType_circuitDerivable : CircuitDerivable identityCredentialResource verifyCredentialAction :=
  { primitives := identityCredentialType.primitives
   , coversBarbs := identityCredentialType.coversBarbs
   , held := Circuits.Transcribed.identity_verify_capability_held
   , stmts := Circuits.Transcribed.identity_verify_capability_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem identityCredentialType_disclosureRule :
    DisclosureRule Circuits.Transcribed.identity_verify_capability_held
      Circuits.Transcribed.identity_verify_capability_stmts :=
  identityCredentialType_circuitDerivable.noFreeInstances

/-- `boxCapType` — (`box_capability`, `take`) maps to `box`'s `take`, circuit `Take`,
    transcribed as `Circuits.Transcribed.box_take`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def boxCapType_circuitDerivable : CircuitDerivable boxResource takeAction :=
  { primitives := boxCapType.primitives
   , coversBarbs := boxCapType.coversBarbs
   , held := Circuits.Transcribed.box_take_held
   , stmts := Circuits.Transcribed.box_take_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem boxCapType_disclosureRule :
    DisclosureRule Circuits.Transcribed.box_take_held
      Circuits.Transcribed.box_take_stmts :=
  boxCapType_circuitDerivable.noFreeInstances

/-- `multisigApprovalType` — (`multisig_approval`, `finalize`) maps to `multisig`'s `finalize`, circuit `FinalizeV2`,
    transcribed as `Circuits.Transcribed.multisig_finalize`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def multisigApprovalType_circuitDerivable : CircuitDerivable multisigResource finalizeAction :=
  { primitives := multisigApprovalType.primitives
   , coversBarbs := multisigApprovalType.coversBarbs
   , held := Circuits.Transcribed.multisig_finalize_held
   , stmts := Circuits.Transcribed.multisig_finalize_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem multisigApprovalType_disclosureRule :
    DisclosureRule Circuits.Transcribed.multisig_finalize_held
      Circuits.Transcribed.multisig_finalize_stmts :=
  multisigApprovalType_circuitDerivable.noFreeInstances

/-- `attestationType` — (`attestation`, `verify_attestation`) maps to `attestation`'s `verify_claim`, circuit `VerifyClaimV2`,
    transcribed as `Circuits.Transcribed.attestation_verify_claim`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def attestationType_circuitDerivable : CircuitDerivable attestationResource verifyAction :=
  { primitives := attestationType.primitives
   , coversBarbs := attestationType.coversBarbs
   , held := Circuits.Transcribed.attestation_verify_claim_held
   , stmts := Circuits.Transcribed.attestation_verify_claim_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem attestationType_disclosureRule :
    DisclosureRule Circuits.Transcribed.attestation_verify_claim_held
      Circuits.Transcribed.attestation_verify_claim_stmts :=
  attestationType_circuitDerivable.noFreeInstances

/-- `bridgeDepositType` — (`bridge_deposit`, `deposit`) maps to `bridge`'s `deposit`, circuit `DepositV2`,
    transcribed as `Circuits.Transcribed.bridge_deposit`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def bridgeDepositType_circuitDerivable : CircuitDerivable bridgeDepositResource bridgeDepositAction :=
  { primitives := bridgeDepositType.primitives
   , coversBarbs := bridgeDepositType.coversBarbs
   , held := Circuits.Transcribed.bridge_deposit_held
   , stmts := Circuits.Transcribed.bridge_deposit_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem bridgeDepositType_disclosureRule :
    DisclosureRule Circuits.Transcribed.bridge_deposit_held
      Circuits.Transcribed.bridge_deposit_stmts :=
  bridgeDepositType_circuitDerivable.noFreeInstances

/-- `bridgeWithdrawType` — (`bridge_withdrawal`, `withdraw`) maps to `bridge`'s `withdraw`, circuit `WithdrawV2`,
    transcribed as `Circuits.Transcribed.bridge_withdraw`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def bridgeWithdrawType_circuitDerivable : CircuitDerivable bridgeWithdrawResource bridgeWithdrawAction :=
  { primitives := bridgeWithdrawType.primitives
   , coversBarbs := bridgeWithdrawType.coversBarbs
   , held := Circuits.Transcribed.bridge_withdraw_held
   , stmts := Circuits.Transcribed.bridge_withdraw_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem bridgeWithdrawType_disclosureRule :
    DisclosureRule Circuits.Transcribed.bridge_withdraw_held
      Circuits.Transcribed.bridge_withdraw_stmts :=
  bridgeWithdrawType_circuitDerivable.noFreeInstances

/-- `oracleOperatorType` — (`oracle_operator`, `oracle_push_value`) maps to `oracle`'s `push_value`, circuit `PushValueV2`,
    transcribed as `Circuits.Transcribed.oracle_push_value`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **refuted**; the checker's class for its first undetermined exposure is
    `redundant`. Declared-free instances in this circuit: 0.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def oracleOperatorType_circuitDerivable : CircuitDerivable oracleResource pushValueAction :=
  { primitives := oracleOperatorType.primitives
   , coversBarbs := oracleOperatorType.coversBarbs
   , held := Circuits.Transcribed.oracle_push_value_held
   , stmts := Circuits.Transcribed.oracle_push_value_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem oracleOperatorType_disclosureRule :
    DisclosureRule Circuits.Transcribed.oracle_push_value_held
      Circuits.Transcribed.oracle_push_value_stmts :=
  oracleOperatorType_circuitDerivable.noFreeInstances

/-! ===== The pairs that resolve to no circuit, as data =====

**These are decisions rather than omissions, and a reader can tell which:** the
generator fails on any pair it cannot resolve, so the pairs named here are the only
ones it deliberately names no circuit for. A pair absent from the theorems above is a
failure of the generator, not a gap in this file.

* (`native_token`, `transfer`) maps to `native_token`'s `transfer`, whose manifest
declares no `proof_circuit`.
* (`native_token_coinbase`, `claim_coinbase`) maps to `native_token`'s `pow_reward`, whose manifest
declares no `proof_circuit`.

`native_token`'s manifest is the FYI document its own header says it is, and its
coinbase path is host-side: `bin/dwowd` verifies the proof of work, and the host
checks `effective_value.checked_add(total_pin) == Some(input.value)` at
`entrypoint/mod.rs:1000-1006`. `script/circuit_index_map.txt`'s `claim_coinbase` line
records the same decision with its citation.
-/

def circuitlessPairs : List (String × String) :=
  [("native_token", "transfer"), ("native_token_coinbase", "claim_coinbase")]

end CircuitIndex
