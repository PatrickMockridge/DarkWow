/-
# The two trees, reconciled — a block's net creation is the schedule's reward

`OBL-C45`'s first clause is that the contracts tree and the supply-chain tree reconcile
*automatically*, and the register's own reading of it is exact: they are reconciled nowhere — not in
Rust, not in the script — while every *write* to the supply chain is step-verified (`OBL-C25`). What
the row does not have is a statement of the property. This module adds one, and it is a composition
rather than a new model: three models already carry the three legs.

* `Consensus/MassBalance.lean` — `Balanced`, the non-coinbase calls' Pedersen equation, at the
  specification's integer-pair level;
* `Consensus/CoinbaseSplit.lean` — `accepts`, the coinbase's five equations, whose conservation law
  `accepts_spendable_total` says the block's *spendable issuance* is the schedule's value, with the
  uncle pins funded from the same reward rather than beside it;
* `Consensus/FeeCollect.lean` — `rule`, the block's fee-collect shape.

## What it proves

A block's **net creation of commitment value** — its issuance plus what its non-coinbase calls create,
minus what they consume — is the emission schedule's value at that height, with the fee calls parking
nothing; and summed over a chain it is `Σ expected_reward(H)`, the sum the supply-chain tree's own
cumulative entry accumulates. It also proves the fee leg is **load-bearing rather than decorative**:
without it the identity fails by exactly the parked aggregate, and the witness is written out. So the
reconciliation of the two trees is not automatic — it is three of this layer's predicates and one
bridge, and the third leg is the weakest of them.

## What it does not prove

**It does not prove that anything checks it.** No code reads the contracts tree against the supply
chain, and `OBL-C45` stays `PARTLY` for that reason: what landed is the property a later Rust-side
reconciliation would be a check *of*. Three weaker things, stated rather than implied:

* the level is the specification's. `Commitment` is an integer pair, as in `MassBalance.lean`, so the
  identity is about values and blinds as integers — not about `pallas::Point`, and not a statement
  about the deployed curve;
* the fee leg rests on a **bridge no model enforces**. `FeeCollect`'s rule is written over FeeV3
  *call counts* and `MassBalance` carries the fee *amounts*, so the step from "this block parks an
  aggregate" to "this block collects" is a hypothesis `a_block_with_fees_collects` takes explicitly.
  That bridge is this unit's residue and its docstring says so;
* the pot's arithmetic — that the collection's value is the aggregate the fee calls parked — is a WASM
  check (`FeeCollect`'s note records the proxy), not modelled here. What is modelled is its
  *consequence*: the aggregate is released in the block that parked it.
-/

import Mathlib
import DarkFi.AxiomBudget
import DarkFi.Consensus.MassBalance
import DarkFi.Consensus.CoinbaseSplit
import DarkFi.Consensus.FeeCollect

namespace Consensus.SupplyReconciliation

open Consensus.MassBalance (BlockCommits values blinds commit values_append blinds_append values_fees)
open Consensus.CoinbaseSplit (Split accepts sumNotes)
open Consensus.FeeCollect (FeeShape rule)

/-! ==========================================================================
   Part 1 — the block, and its two numbers
   ========================================================================== -/

/-- The value a block's coinbase **issues**: the miner's spendable note plus every uncle-mint note.
    Both fields are `Split`'s, and `accepts_spendable_total` is what makes their *sum* the schedule's
    value — which is the composition this module is about, since an uncle's pin is a share of the same
    reward rather than a second issuance beside it. -/
def issuance (s : Split) : Int := (s.effective : Int) + (s.uncleNotes.sum : Int)

/-- **A block's net creation of commitment value**: what it issues, plus what its non-coinbase calls
    create, minus what they consume. The coinbase is the only creator in the non-coinbase-exempt sense
    `OBL-C118` marks, so this is the number the contracts tree grows by — and therefore the number the
    supply-chain tree's cumulative entry has to equal for the two trees to agree. -/
def netCreation (b : BlockCommits) (s : Split) : Int :=
  issuance s + values b.outs - values b.ins

/-- A block as the reconciliation reads it: the three modules' own structures, not copies of them,
    which is what makes the unit a composition rather than a fourth model. -/
structure Block where
  /-- The block's non-coinbase calls, in `MassBalance`'s categories. -/
  commits : BlockCommits
  /-- The coinbase's split, as the accept path holds it. -/
  split : Split
  /-- The block's fee-collect shape, as `FeeCollect`'s rule reads it. -/
  fees : FeeShape

/-- The block's net creation. -/
def Block.creation (bl : Block) : Int := netCreation bl.commits bl.split

/-- The schedule's value at the block's height — `Split.base`, which the accept path fills from
    `expected_reward(H)`, and which is the figure the supply chain's cumulative entry accumulates. -/
def Block.reward (bl : Block) : Int := (bl.split.base : Int)

/-- A block with no non-coinbase calls at all: the shape every witness below starts from, since it makes
    `Balanced` hold by definition rather than by arithmetic. -/
def noCalls : BlockCommits :=
  { feeInputs := [], feeOutputs := [], burnInputs := [], transferInputs := [], transferOutputs := []
  , spendInputs := [], spendOutputs := [], mintOutputs := [], feeAmounts := [] }

/-! ==========================================================================
   Part 2 — the block identity, from `Balanced` alone
   ========================================================================== -/

/-- **The non-coinbase calls park exactly the fee aggregate.** `Balanced` is
    `outs + burns + fees = ins + burns`; the burn term cancels on both sides (`burn_cancels`), and what
    is left says the inputs exceed the outputs by the fee aggregate. Read operationally: the fees are
    *parked*, not destroyed — which is why the two trees cannot be reconciled from this leg alone. -/
@[axiom_budget 0]
theorem non_coinbase_net_is_the_fee_aggregate (b : BlockCommits) (h : MassBalance.Balanced b) :
    values b.outs - values b.ins = -b.feeAmounts.sum := by
  have hl : values b.leftList = values b.outs + (values b.burnInputs + values b.fees) := by
    rw [BlockCommits.leftList, values_append, values_append]
    omega
  have hr : values b.rightList = values b.ins + values b.burnInputs := by
    rw [BlockCommits.rightList, values_append]
  have h1 : values b.leftList = values b.rightList := h.1
  rw [hl, hr, values_fees b] at h1
  omega

/-- **A balanced block's net creation is its issuance minus the parked aggregate.** No condition on the
    coinbase beyond this leg's, and none on the fees — the two fields appear separately, which is what
    makes the next theorem's hypothesis visibly the only thing that removes the fee term. -/
@[axiom_budget 0]
theorem creation_is_issuance_minus_fees (bl : Block) (h : MassBalance.Balanced bl.commits) :
    bl.creation = issuance bl.split - bl.commits.feeAmounts.sum := by
  have h1 := non_coinbase_net_is_the_fee_aggregate bl.commits h
  unfold Block.creation netCreation
  omega

/-- **The reconciliation, per block.** With the non-coinbase calls balanced, the split accepted and the
    block parking no fees, the contracts tree grows by exactly the schedule's value at that height.

    The second leg is `accepts_spendable_total` and it is where the coinbase's *shape* enters: the
    miner's note and the uncle notes together are the reward, so nothing is issued beside the schedule
    and nothing is stranded inside the split. -/
@[axiom_budget 0]
theorem creation_is_the_reward (bl : Block) (hbal : MassBalance.Balanced bl.commits) (hacc : accepts bl.split)
    (hrel : bl.commits.feeAmounts = []) : bl.creation = bl.reward := by
  have h1 := creation_is_issuance_minus_fees bl hbal
  have h2 : issuance bl.split = (bl.split.base : Int) := by
    have hnat : bl.split.effective + bl.split.uncleNotes.sum = bl.split.base :=
      Consensus.CoinbaseSplit.accepts_spendable_total bl.split hacc
    unfold issuance
    exact_mod_cast hnat
  rw [h1, h2, Block.reward, hrel]
  simp

/-- **The cumulative form**: over a chain of blocks that each balance, accept their split and park
    nothing, the contracts tree's net creation is `Σ expected_reward(H)` — the sum the supply chain's
    own definition accumulates, so this is the statement that the two trees carry the same number. -/
@[axiom_budget 0]
theorem cumulative_creation_is_the_cumulative_supply (l : List Block)
    (hbal : ∀ bl ∈ l, MassBalance.Balanced bl.commits) (hacc : ∀ bl ∈ l, accepts bl.split)
    (hrel : ∀ bl ∈ l, bl.commits.feeAmounts = []) :
    (l.map Block.creation).sum = (l.map Block.reward).sum := by
  induction l with
  | nil => rfl
  | cons bl tl ih =>
      have hb := hbal bl (by simp)
      have ha := hacc bl (by simp)
      have hr := hrel bl (by simp)
      have htl := ih (fun b hb' => hbal b (by simp [hb'])) (fun b hb' => hacc b (by simp [hb']))
        (fun b hb' => hrel b (by simp [hb']))
      simp only [List.map_cons, List.sum_cons]
      rw [creation_is_the_reward bl hb ha hr, htl]

/-! ==========================================================================
   Part 3 — the third leg is load-bearing, and the two witnesses
   ========================================================================== -/

/-- **A block that balances and accepts its split can still fail the identity — and this is it.** 150 in
    against 100 out with a 50-unit fee aggregate parked by the calls: every value check the two models
    have passes, and the contracts tree grows by `base - 50` where the schedule says `base`.

    So `creation_is_the_reward`'s `feeAmounts = []` is not decoration, and what usually makes it true is
    `FeeCollect.rule` — a block with fee calls must collect them — which is the third leg. -/
@[axiom_budget 0]
theorem parked_fees_break_the_identity :
    ∃ bl : Block, MassBalance.Balanced bl.commits ∧ accepts bl.split ∧ bl.creation ≠ bl.reward :=
  ⟨{ commits := { noCalls with transferInputs := [commit 150 0], transferOutputs := [commit 100 0]
                                , feeAmounts := [50] }
   , split := Consensus.CoinbaseSplit.legalSplit
   , fees := { collectPresent := false, collectIsLast := false, feeCalls := 1, collectCalls := 0 } }, by
    simp [MassBalance.Balanced, BlockCommits.leftList, BlockCommits.rightList, BlockCommits.outs,
      BlockCommits.ins, BlockCommits.fees, values, blinds, commit, noCalls],
   Consensus.CoinbaseSplit.legalSplit_accepts, by
    norm_num [Block.creation, Block.reward, netCreation, issuance, values, sumNotes,
      Consensus.CoinbaseSplit.legalSplit, BlockCommits.outs, BlockCommits.ins, noCalls, commit]⟩

/-- **And the identity holds on a concrete block**, so the law is not about an empty domain either. The
    coinbase side is `CoinbaseSplit.legalSplit` — that module's own accepted witness, reused rather than
    rebuilt — and the calls side is empty, which is the one case where the parked aggregate is zero by
    inspection. -/
@[axiom_budget 0]
theorem reconciliation_witness :
    MassBalance.Balanced noCalls ∧ accepts Consensus.CoinbaseSplit.legalSplit ∧
      netCreation noCalls Consensus.CoinbaseSplit.legalSplit
        = (Consensus.CoinbaseSplit.legalSplit.base : Int) :=
  ⟨by simp [MassBalance.Balanced, BlockCommits.leftList, BlockCommits.rightList, BlockCommits.outs,
      BlockCommits.ins, BlockCommits.fees, values, blinds, noCalls],
   Consensus.CoinbaseSplit.legalSplit_accepts, by
    norm_num [netCreation, issuance, values, sumNotes, Consensus.CoinbaseSplit.legalSplit,
      BlockCommits.outs, BlockCommits.ins, noCalls]⟩

/-- **The fee rule, read as the reconciliation's third leg — and the bridge it needs is taken here
    rather than assumed away.**

    `FeeCollect.rule` is written over FeeV3 *call counts*: "the collection is present iff the block has
    fee calls". `MassBalance` carries the fee *amounts*. So "this block parked an aggregate" does not
    by itself give "this block has fee calls > 0", and no code check ties the two — the count is the
    proxy `FeeCollect`'s own note records, not a reader of the commitments. The bridge is therefore a
    hypothesis, and it is this unit's residue: with it, a block whose calls park anything has a
    collection, which is what makes `creation_is_the_reward`'s hypothesis dischargeable in practice
    rather than only in the empty case. -/
@[axiom_budget 0]
theorem a_block_with_fees_collects (bl : Block) (h : rule bl.fees)
    (hbridge : bl.commits.feeAmounts ≠ [] → bl.fees.feeCalls > 0)
    (hf : bl.commits.feeAmounts ≠ []) : bl.fees.collectPresent = true :=
  h.2.1.mpr (hbridge hf)

end Consensus.SupplyReconciliation
