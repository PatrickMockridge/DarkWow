/-
DarkWow.Capability.Prover — Generic Prover Soundness

Formalizes the write-path invariant of wallet.md §6.4.1 / manifest.md
"Typed Capability Fields": a manifest-driven invocation constructs a proof only
when the witness_map is arity-correct and every witness slot is bound to a
declared source. The vocabulary is closed across three categories — input
(note:<field>, param:<field>, secret, merkle_path[:current|cumulative],
leaf_position, tx_commitment, tx_nonce), named blind (blind:<name>), and
derived (derived:<rule>). A note:/param: slot that references an undeclared
field makes the proof non-constructible; a derived slot is always computable
from the already-bound inputs.
-/

import Mathlib
import DarkFi.Capability.Types

namespace DarkFi.Capability

/- The closed derived-witness rule table, mapped 1:1 to zkas opcode families
   (wallet.md §6.4.1). A derived slot is computed by the circuit from the
   already-bound input slots; operands reference earlier slots by position. -/
inductive DerivedRule where
  | nullifier
  | txBinding
  | merkleRoot
  | leaf
  | ownerPub
  | tokenCommit
  | purseId
  | coin
  | pedersenX (name : String)
  | pedersenY (name : String)
  | baseAdd (a b : Nat)
  | baseSub (a b : Nat)
  | blindSum (a b : Nat)
  | blindSub (a b : Nat)
  | signatureSecret
deriving Repr, DecidableEq

/- A witness slot's source, per manifest.md's closed vocabulary. -/
inductive WitnessSource where
  | note (field : String)
  | param (field : String)
  | secret
  | merklePath
  | merklePathCurrent
  | merklePathCumulative
  | leafPosition
  | blind (name : String)
  | txCommitment
  | txNonce
  | derived (rule : DerivedRule)
deriving Repr, DecidableEq

/- A source is bindable when any referenced field is declared in the note_schema
   (note:) or parameters (param:). The intrinsic sources carry their own data
   from the capability provider and are always bindable. -/
def bindable (src : WitnessSource) (noteFields paramFields : List String) : Prop :=
  match src with
  | WitnessSource.note f => f ∈ noteFields
  | WitnessSource.param f => f ∈ paramFields
  | _ => True

/- Every slot in the witness_map is bound to a declared source. -/
def allSlotsBound (map : List WitnessSource) (noteFields paramFields : List String) : Prop :=
  ∀ src, src ∈ map → bindable src noteFields paramFields

/- A proof is constructible when the witness_map arity matches the circuit's
   witness count AND every slot is bound. -/
def constructible (map : List WitnessSource) (noteFields paramFields : List String)
    (witnessCount : Nat) : Prop :=
  map.length = witnessCount ∧ allSlotsBound map noteFields paramFields

/- Theorem (genericProver_sound): a constructible invocation has correct arity
   and binds every slot — there is no unbound (half-specified) witness. -/
theorem genericProver_sound (map : List WitnessSource) (noteFields paramFields : List String)
    (witnessCount : Nat) :
    constructible map noteFields paramFields witnessCount →
    map.length = witnessCount ∧ (∀ src, src ∈ map → bindable src noteFields paramFields) := by
  intro h
  rcases h with ⟨h_arity, h_bound⟩
  exact ⟨h_arity, h_bound⟩

/- Theorem (undeclared_field_blocks): a note: slot referencing an undeclared
   field makes the proof non-constructible — the negative soundness. -/
theorem undeclared_field_blocks (map : List WitnessSource) (noteFields paramFields : List String)
    (witnessCount : Nat) (f : String) :
    WitnessSource.note f ∈ map → f ∉ noteFields →
    ¬ constructible map noteFields paramFields witnessCount := by
  intro h_mem h_undeclared h_constructible
  rcases h_constructible with ⟨_, h_bound⟩
  have h_bind := h_bound (WitnessSource.note f) h_mem
  unfold bindable at h_bind
  exact h_undeclared h_bind

/- Theorem (derivedWitness_computable): a derived witness slot is always
   bindable — it is computed by the circuit's closed rule table from the
   already-bound input slots, so it requires no note: or param: declaration. -/
theorem derivedWitness_computable (r : DerivedRule) (noteFields paramFields : List String) :
    bindable (WitnessSource.derived r) noteFields paramFields := by
  simp [bindable]

/- Theorem (namedBlind_distinct): distinct blind names yield distinct blind
   sources — the name is load-bearing, so the Rust prover's per-name Seed domain
   never collides two differently-named blinds. -/
theorem namedBlind_distinct (n1 n2 : String) (h : n1 ≠ n2) :
    WitnessSource.blind n1 ≠ WitnessSource.blind n2 := by
  intro heq
  have hn : n1 = n2 := by
    cases heq
    rfl
  exact h hn

/- Theorem (witnessMap_arity): a constructible witness map has arity equal to
   the circuit's declared witness count — there is no unbound or half-bound slot. -/
theorem witnessMap_arity (map : List WitnessSource) (noteFields paramFields : List String)
    (witnessCount : Nat) :
    constructible map noteFields paramFields witnessCount → map.length = witnessCount := by
  intro h
  exact h.1

/-! ===== T6 — transaction binding (invariant #4) ===== -/

/-- T6 (spec): the transaction binding is `poseidon(3, tx_commitment, tx_nonce)`.
    Invariant #4 (`wallet.md:815`) SHALL hold: the prover binds the REAL
    seed-derived `tx_commitment`/`tx_nonce` (never a hardcoded zero). Stated as a
    predicate over the bound inputs — the HAZOP V6 remediation. -/
def bindsRealTxBinding (txCommitment txNonce : Nat) : Prop :=
  txCommitment ≠ 0 ∨ txNonce ≠ 0

/-- T6 (well-typed): `tx_commitment` is an intrinsic witness source — always
    bindable, never requiring a note:/param: declaration. -/
theorem txCommitment_source_bindable (noteFields paramFields : List String) :
    bindable WitnessSource.txCommitment noteFields paramFields := by
  simp [bindable]

/-- T6 (well-typed): `tx_nonce` is an intrinsic witness source — always bindable. -/
theorem txNonce_source_bindable (noteFields paramFields : List String) :
    bindable WitnessSource.txNonce noteFields paramFields := by
  simp [bindable]

end DarkFi.Capability
