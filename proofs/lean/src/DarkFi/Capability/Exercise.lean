import DarkFi.Combinatorial.StateSpace
import DarkFi.AxiomBudget

/-!
# Capability Exercise — the Consume+Create Transition

The "transfer" of a capability is its **Exercise + Consume** phase (ocap.md §6,
wallet.md §6): the consumed capabilities publish their nullifiers (single-use
consumption evidence) and the produced capabilities carry fresh commitments.

This module models the transition on capability *instances*. A capability
instance is `{ commitment, nullifier }`: the commitment is the Create face
(ocap.md §6.2 Create), the nullifier is the Consume face (ocap.md §6.2 Consume).
The type of a capability — the composition of primitives whose barbs cover the
action's required barbs — is formalized in `Capability.Types` / `Composition`;
this module operates one level down, on the instance.

The public state is `Combinatorial.PublicState`: the current Merkle root, the
spent-nullifier set, the historical roots, and the recognized-commitment set
(StateSpace.lean). The nullifier-freshness gate reuses
`Combinatorial.NullifierStorage` (the Representation Faithfulness Law).

## Both faces are modelled, and the module's name was not true of it until 2026-09-24

`Exercise` carries `outputs : List LeafCommitment` and the header said "the
produced capabilities carry fresh commitments" — but `applyExercise` ignored
`outputs` entirely and no theorem mentioned it, so the field was **dead** and the
module named for Exercise+Consume modelled Consume alone. It now appends the
outputs to `PublicState.recognizedCommitments` and the create side has its own
theorems: `create_completeness` (the dual of `nullifier_completeness`),
`recognized_monotone`, `consume_and_create_are_independent`, and
`outputs_are_load_bearing` — the last being a witness that two exercises
differing only in their outputs give different states, so the field is not
decorative.

## What is still not modelled

The **Merkle root**. `applyExercise` records the created commitments and leaves
`merkleRoot` and `historicalRoots` untouched: recomputing the root is an ordered
append into the contract's tree, which `Capability/PerContractTree.lean` and
`HashOps.lean` own. So "the created commitment is recognized" here means
"present in the recognized set", not "provably included under a root" — those are
different claims and this file only makes the first.
-/

namespace Capability

open Combinatorial

/-! ===== Capability instance ===== -/

/-- A capability instance: its commitment (the Create face, a Merkle leaf in
    the recognized set) and its nullifier (the Consume face, single-use
    consumption evidence). -/
structure Cap where
  commitment : LeafCommitment
  nullifier : NullifierValue
deriving BEq, Repr

/-! ===== Exercise (consume+create) ===== -/

/-- An exercise ("transfer"): consume `inputs` (their nullifiers are published)
    and create `outputs` (fresh commitments). This is Exercise+Consume of
    ocap.md §6. -/
structure Exercise where
  inputs : List Cap
  outputs : List LeafCommitment
deriving BEq, Repr

/-- Consume: a capability is consumed iff its nullifier is in the spent set. -/
def consumed (state : PublicState) (c : Cap) : Prop :=
  c.nullifier ∈ state.spentNullifiers

/-- The Consume single-use gate: an exercise is valid wrt `state` iff every
    input's nullifier is fresh (not already spent). This is the nullifier
    freshness check every contract's exec performs (`db_contains_key`). -/
def validExercise (state : PublicState) (e : Exercise) : Prop :=
  ∀ c ∈ e.inputs, c.nullifier ∉ state.spentNullifiers

/-- Apply: consume the inputs (publish their nullifiers) and create the outputs
    (recognize their commitments). The created commitments are recorded in
    `recognizedCommitments`; the *root* that commits to them is the contract's
    tree and is not recomputed here — see the module note. -/
def applyExercise (state : PublicState) (e : Exercise) : PublicState :=
  { merkleRoot := state.merkleRoot
  , spentNullifiers := state.spentNullifiers ++ (e.inputs.map (fun c => c.nullifier))
  , historicalRoots := state.historicalRoots
  , recognizedCommitments := state.recognizedCommitments ++ e.outputs
  : PublicState }

/-! ===== Theorems: Consume ===== -/

/-- Consume is single-use: after applying an exercise, re-exercising the same
    input is invalid — its nullifier is now spent (double-spend rejection). -/
@[axiom_budget 0]
theorem consume_is_single_use
    (state : PublicState) (e : Exercise) (c : Cap)
    (h_in : c ∈ e.inputs) :
    ¬ validExercise (applyExercise state e) e := by
  intro h
  unfold validExercise at h
  have hc : c.nullifier ∉ (applyExercise state e).spentNullifiers := h c h_in
  unfold applyExercise at hc
  have hspent : c.nullifier ∈ state.spentNullifiers ++ (e.inputs.map (fun x => x.nullifier)) := by
    rw [List.mem_append]
    right
    rw [List.mem_map]
    exact ⟨c, h_in, rfl⟩
  exact hc hspent

/-- Nullifier completeness (wallet.md §7.8): every consumed input's nullifier
    is published in the post-apply spent set. This is the property the mempool
    relies on for double-spend detection. -/
@[axiom_budget 0]
theorem nullifier_completeness
    (state : PublicState) (e : Exercise) (c : Cap)
    (h_in : c ∈ e.inputs) :
    c.nullifier ∈ (applyExercise state e).spentNullifiers := by
  unfold applyExercise
  rw [List.mem_append]
  right
  rw [List.mem_map]
  exact ⟨c, h_in, rfl⟩

/-- The spent set only grows under exercise — consumed nullifiers are never
    removed. -/
@[axiom_budget 0]
theorem exercise_nullifiers_monotone
    (state : PublicState) (e : Exercise) (n : NullifierValue) :
    n ∈ state.spentNullifiers → n ∈ (applyExercise state e).spentNullifiers := by
  intro h
  unfold applyExercise
  simp [h]

/-! ===== Theorems: Create — the duals the module was missing =====
    `nullifier_completeness` publishes what is consumed; `create_completeness`
    recognizes what is created. They are the two halves of "an exercise is
    consume *and* create", and only the first was here. -/

/-- Create completeness: every output commitment of an exercise is in the
    post-apply recognized set. The dual of `nullifier_completeness`. -/
@[axiom_budget 0]
theorem create_completeness
    (state : PublicState) (e : Exercise) (o : LeafCommitment)
    (h_out : o ∈ e.outputs) :
    o ∈ (applyExercise state e).recognizedCommitments := by
  unfold applyExercise
  rw [List.mem_append]
  exact Or.inr h_out

/-- The recognized set only grows under exercise — a commitment once recognized
    is never un-recognized by a later exercise. The dual of
    `exercise_nullifiers_monotone`. -/
@[axiom_budget 0]
theorem recognized_monotone
    (state : PublicState) (e : Exercise) (o : LeafCommitment) :
    o ∈ state.recognizedCommitments → o ∈ (applyExercise state e).recognizedCommitments := by
  intro h
  unfold applyExercise
  simp [h]

/-- **The two faces do not interfere.** Two exercises with the same inputs spend
    the same nullifiers whatever their outputs are, and two with the same outputs
    recognize the same commitments whatever their inputs are. So the Consume
    theorems above are about the inputs alone and the Create theorems about the
    outputs alone — which is what makes `applyExercise` a pair of independent
    updates rather than one entangled transition. -/
@[axiom_budget 0]
theorem consume_and_create_are_independent
    (state : PublicState) (e₁ e₂ : Exercise) :
    (e₁.inputs = e₂.inputs →
        (applyExercise state e₁).spentNullifiers = (applyExercise state e₂).spentNullifiers) ∧
    (e₁.outputs = e₂.outputs →
        (applyExercise state e₁).recognizedCommitments
          = (applyExercise state e₂).recognizedCommitments) := by
  refine ⟨?_, ?_⟩ <;> intro h <;> unfold applyExercise <;> simp [h]

/-- **`outputs` is load-bearing, and that is a measurement.** Two exercises that
    differ only in their outputs give different states — so the create side is
    modelled rather than ignored, which is what `applyExercise` did before
    2026-09-24 when it read neither `e.outputs` nor anything derived from it.
    Without this the two `create` theorems above would be consistent with a
    `recognizedCommitments` field that nothing ever writes. -/
@[axiom_budget 0]
theorem outputs_are_load_bearing (state : PublicState) :
    ∃ e₁ e₂ : Exercise,
      (applyExercise state e₁).recognizedCommitments
        ≠ (applyExercise state e₂).recognizedCommitments := by
  refine ⟨{ inputs := [], outputs := [] }, { inputs := [], outputs := [1] }, ?_⟩
  unfold applyExercise
  simp

end Capability
