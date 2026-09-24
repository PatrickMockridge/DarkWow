/-
# Block-level Pedersen mass balance — the Python specification, mechanized

`contrib/model/proof_of_token_balance.py` specifies the rule that
`src/linear/src/proof_of_token_balance.rs` enforces at consensus level: for every block, the
non-coinbase calls satisfy

    Σ output_commits + Σ burn_input_commits + Σ fee_commits == Σ input_commits

with the coinbase excluded and checked separately against the emission schedule. This module
transcribes that equation and proves what the schedule's safety argument needs of it: reordering a
block's commitments cannot change whether it balances, the burn term constrains nothing *by
construction*, and both the balancing and the inflating cases are inhabited.

## Which level this models, and why

**The specification's level — commitments as `(value, blind)` pairs added componentwise — and not the
curve's.** That is not a shortcut, and the Python says why itself: "`sim/crypto.py`'s
`pedersen_commit()` hashes the blind to produce `r_part`, which breaks the simple additive property for
test construction. We use `PedersenCommitment(v, r)` directly instead." So what the spec models is the
*additive structure* and the value component, and a model of the spec is what this is.

`DarkFi/Pedersen.lean` models the other level: real Pallas over `ZMod PALLAS_MODULUS`, with the group
law and `pedersen_additive_homomorphism` **proved** rather than postulated. The bridge between the two
levels is the Pedersen assumptions, not a theorem here — reading an equation failure as "these *values*
differ" needs binding, which `Pedersen.lean`'s header records as not proved. So the equation below is
the specification's, and its value-level meaning is exactly where the assumptions sit.

**And because the level is integer pairs, every theorem in this module is `@[axiom_budget 0]`** — no
assumption and not even `Classical.choice`. That is what the level buys, and it is why the negative
controls are as cheap as the positive ones; the price is the sentence above, that the *meaning* of a
failed balance is not established here.

## Two things the specification does that the transcription preserves

* **The burn term is on both sides**, and the Python says why in a comment ("burned inputs are added to
  the output side so equation balances"). The consequence is that it constrains nothing —
  `burn_cancels` below is that fact as a theorem, and it is the honest reading of "burns are safe
  deflation": the rule permits any burn, because a burn cannot inflate.
* **The fee list is plaintext**: `mk(fee, 0)`, so the fee amounts enter the *value* sum and contribute
  zero to the blind sum (`values_fees`, `blinds_fees`).

## What is not here

The coinbase's own check against the emission schedule — that is `SupplyChain.lean` and `OBL-C5`'s
territory, and the specification excludes the coinbase from this equation deliberately. And the
*per-call* laws (`Capability/MultiProof.lean`'s transfer conservation) are not restated: this is the
block-level composition, and it should consume them rather than duplicate them.

**One thing worth saying about the two order-independence arguments in this layer**, because they are
different and look alike. Here, reordering commitments cannot change a *sum*, unconditionally —
`values_append` and `List.Perm.sum_eq` are all it takes. In `Semantics/Ledger.lean`, reordering *calls*
cannot change a *store* only when their write sets are disjoint, which is a real hypothesis and the
subject of `OBL-C100`. A block's balance surviving a reordering is therefore no evidence at all that its
state transitions do, and reading the first as the second is the mistake §9.2's argument is about.
-/

import Mathlib
import DarkFi.AxiomBudget

namespace Consensus.MassBalance

/-- A Pedersen commitment as the specification models one: a value component and a blind, added
    componentwise by the spec's `pedersen_add`. See the module note for why this is the spec's level
    rather than the curve's. -/
abbrev Commitment := Int × Int

/-- `commit v r`: the commitment to `v` under blind `r`. The specification's `mk`. -/
def commit (v r : Int) : Commitment := (v, r)

/-- The **value** components of a commitment list, summed. The specification's `total.v_part`. -/
def values (l : List Commitment) : Int := (l.map Prod.fst).sum

/-- The **blind** components, summed. The specification's `total.r_part`. -/
def blinds (l : List Commitment) : Int := (l.map Prod.snd).sum

/-- A block's commitment lists, in the specification's own categories. `BlockCommits` mirrors
    `verify_proof_of_token_balance`'s parameters one for one, the coinbase excepted. -/
structure BlockCommits where
  feeInputs : List Commitment
  feeOutputs : List Commitment
  burnInputs : List Commitment
  transferInputs : List Commitment
  transferOutputs : List Commitment
  spendInputs : List Commitment
  spendOutputs : List Commitment
  mintOutputs : List Commitment
  /-- The fee amounts, plaintext — the specification commits each with blind `0`. -/
  feeAmounts : List Int

/-- The specification's `total_outputs`: every output category, summed. -/
def BlockCommits.outs (b : BlockCommits) : List Commitment :=
  b.feeOutputs ++ b.transferOutputs ++ b.spendOutputs ++ b.mintOutputs

/-- The specification's `total_inputs` *without* the burn list, which is on both sides. -/
def BlockCommits.ins (b : BlockCommits) : List Commitment :=
  b.feeInputs ++ b.transferInputs ++ b.spendInputs

/-- The fee aggregate: each plaintext amount as a commitment with blind `0`. -/
def BlockCommits.fees (b : BlockCommits) : List Commitment :=
  b.feeAmounts.map (fun v => commit v 0)

/-- The specification's **left** side: outputs, then burn inputs, then the fee aggregate. -/
def BlockCommits.leftList (b : BlockCommits) : List Commitment :=
  b.outs ++ b.burnInputs ++ b.fees

/-- The specification's **right** side: the inputs, with the burn list in its place in the chain of
    concatenations — which is where the spec puts it, and it is why `burn_cancels` is a theorem about
    a term that looks load-bearing. -/
def BlockCommits.rightList (b : BlockCommits) : List Commitment :=
  b.ins ++ b.burnInputs

/-- **The mass-balance predicate**, as `verify_proof_of_token_balance` computes it: the point equality
    is a *pair* of equalities, so both components must agree. The blinds matter as much as the values —
    that is what makes the equation a commitment equality rather than a value equation, and it is the
    half `CrossCutting`'s no-wraparound bound is about. -/
def Balanced (b : BlockCommits) : Prop :=
  values b.leftList = values b.rightList ∧ blinds b.leftList = blinds b.rightList

/-! ===== The list laws the predicate needs ===== -/

/-- Permuting a commitment list cannot change its value sum. -/
@[axiom_budget 0]
theorem values_perm {l l' : List Commitment} (h : l.Perm l') : values l = values l' :=
  (h.map Prod.fst).sum_eq

/-- Nor its blind sum. -/
@[axiom_budget 0]
theorem blinds_perm {l l' : List Commitment} (h : l.Perm l') : blinds l = blinds l' :=
  (h.map Prod.snd).sum_eq

@[axiom_budget 0]
theorem values_append (l₁ l₂ : List Commitment) :
    values (l₁ ++ l₂) = values l₁ + values l₂ := by
  simp [values, List.map_append, List.sum_append]

@[axiom_budget 0]
theorem blinds_append (l₁ l₂ : List Commitment) :
    blinds (l₁ ++ l₂) = blinds l₁ + blinds l₂ := by
  simp [blinds, List.map_append, List.sum_append]

/-- The fee aggregate adds the plaintext amounts to the **value** sum. -/
@[axiom_budget 0]
theorem values_fees (b : BlockCommits) : values b.fees = b.feeAmounts.sum := by
  have h : (Prod.fst ∘ fun v : Int => commit v 0) = id := by funext v; rfl
  rw [BlockCommits.fees, values, List.map_map, h, List.map_id]

/-- And adds nothing to the **blind** sum, because the specification commits fees with blind `0`. -/
@[axiom_budget 0]
theorem blinds_fees (b : BlockCommits) : blinds b.fees = 0 := by
  have h : (Prod.snd ∘ fun v : Int => commit v 0) = fun _ => (0 : Int) := by funext v; rfl
  rw [BlockCommits.fees, blinds, List.map_map, h]
  induction b.feeAmounts with
  | nil => rfl
  | cons _ vs ih => simp [ih]

/-! ===== The rules of the predicate ===== -/

/-- **The burn term is vacuous.** The specification puts `burn_inputs` into *both* aggregates — "burned
    inputs are added to the output side so equation balances" — so a block balances exactly when it
    balances with the burn list empty, and the rule permits any burn.

    That is the right semantics (a burn cannot inflate) expressed as a term that cancels, and it is
    worth a theorem precisely because the term *looks* load-bearing on the page: a reader auditing the
    equation would otherwise have to cancel it by hand to see that it constrains nothing. -/
@[axiom_budget 0]
theorem burn_cancels (b : BlockCommits) :
    Balanced b ↔ Balanced { b with burnInputs := [] } := by
  simp only [Balanced, BlockCommits.leftList, BlockCommits.rightList, BlockCommits.outs,
    BlockCommits.ins, BlockCommits.fees, List.nil_append, values_append, blinds_append]
  constructor <;> intro h <;> exact ⟨by omega, by omega⟩

/-- **Balance is invariant under reordering a block's commitments**, which is the order-independence
    the block level has unconditionally — and the one the block level must not be confused with. The
    hypotheses are the four lists a reordering can permute; `b.outs` and `b.ins` are the spec's own
    concatenations, so permuting a *category* is a permutation of those.

    Contrast `Semantics/Ledger.lean`'s `exec_perm`, where the same sentence about *calls* needs the
    disjointness hypothesis: sums commute for free, stores do not. See the module note. -/
@[axiom_budget 0]
theorem balanced_of_perm {b b' : BlockCommits}
    (ho : b.outs.Perm b'.outs) (hi : b.ins.Perm b'.ins)
    (hburn : b.burnInputs.Perm b'.burnInputs) (hfee : b.feeAmounts.Perm b'.feeAmounts) :
    Balanced b ↔ Balanced b' := by
  have ho' := values_perm ho; have ho'' := blinds_perm ho
  have hi' := values_perm hi; have hi'' := blinds_perm hi
  have hb' := values_perm hburn; have hb'' := blinds_perm hburn
  have hf' := hfee.sum_eq
  simp only [Balanced, BlockCommits.leftList, BlockCommits.rightList, values_append,
    blinds_append, values_fees, blinds_fees]
  rw [ho', ho'', hi', hi'', hb', hb'', hf']

/-! ===== Non-vacuity: both sides of the predicate are inhabited =====

The specification's own tests are the shapes: a legal transfer, and a "hidden mint" — 100 in against
1,000,000 out with the *same* blind, which is the case its `test_illegal_hidden_mint` uses. The pair
below is that pair, and it is what keeps `Balanced` from being a predicate nothing satisfies (or
nothing refuses). -/

/-- **A block that balances.** One transfer, 100 in and 100 out, same blind. -/
@[axiom_budget 0]
theorem balanced_witness : ∃ b : BlockCommits, Balanced b :=
  ⟨{ feeInputs := [], feeOutputs := [], burnInputs := []
   , transferInputs := [commit 100 7], transferOutputs := [commit 100 7]
   , spendInputs := [], spendOutputs := [], mintOutputs := [], feeAmounts := [] }, by
    simp [Balanced, BlockCommits.leftList, BlockCommits.rightList, BlockCommits.outs,
      BlockCommits.ins, BlockCommits.fees, values, blinds, commit]⟩

/-- **And a block that does not**, which is the specification's negative control: 100 in, 1,000,000 out,
    the same blind. Stated so the predicate is *refutable* in the model and not merely satisfiable. -/
@[axiom_budget 0]
theorem inflation_witness : ∃ b : BlockCommits, ¬ Balanced b :=
  ⟨{ feeInputs := [], feeOutputs := [], burnInputs := []
   , transferInputs := [commit 100 7], transferOutputs := [commit 1000000 7]
   , spendInputs := [], spendOutputs := [], mintOutputs := [], feeAmounts := [] }, by
    simp [Balanced, BlockCommits.leftList, BlockCommits.rightList, BlockCommits.outs,
      BlockCommits.ins, BlockCommits.fees, values, blinds, commit]⟩

end Consensus.MassBalance
