import DarkFi.Combinatorial.StateSpace

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
spent-nullifier set, and the historical roots (StateSpace.lean). The
nullifier-freshness gate reuses `Combinatorial.NullifierStorage` (the
Representation Faithfulness Law).
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

/-- Apply: consume the inputs (publish their nullifiers). The created
    commitments are appended to the recognized set by the contract's own
    `merkle_add`; that tree growth is a per-contract concern and is not
    re-derived here. -/
def applyExercise (state : PublicState) (e : Exercise) : PublicState :=
  { merkleRoot := state.merkleRoot
  , spentNullifiers := state.spentNullifiers ++ (e.inputs.map (fun c => c.nullifier))
  , historicalRoots := state.historicalRoots
  : PublicState }

/-! ===== Theorems ===== -/

/-- Consume is single-use: after applying an exercise, re-exercising the same
    input is invalid — its nullifier is now spent (double-spend rejection). -/
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
theorem exercise_nullifiers_monotone
    (state : PublicState) (e : Exercise) (n : NullifierValue) :
    n ∈ state.spentNullifiers → n ∈ (applyExercise state e).spentNullifiers := by
  intro h
  unfold applyExercise
  simp [h]

end Capability
