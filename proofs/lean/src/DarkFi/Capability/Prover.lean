/-
DarkWow.Capability.Prover — Generic Prover witness binding

Formalizes the write-path invariant of wallet.md §6.4.1 / manifest.md
"Typed Capability Fields": a manifest-driven invocation constructs a proof only
when the witness_map is arity-correct and every witness slot is bound to a
source whose declaration it satisfies.

## What this file models, and what it does not

It models **binding requirements** — which source declarations a `witness_map`
entry consults, and which consult nothing. It mirrors
`dwow_sdk::prover::WitnessSource` (`src/sdk/src/prover.rs:144-171`) and the
failure modes of `bin/dww/src/prover_impl.rs`'s `bind_slot` (`:301-368`).

Four sources can fail to bind, and they are exactly the four whose
`CapabilityProvider` accessor returns `Option`/`Result`: `note:<field>`
(`note_value`, `:312`), `param:<field>` (`param_value`, `:317`),
`secret:<name>` (`named_secret`, `:331`) and the three `merkle_path` forms
(`merkle_path_array`, `:569`, which rejects a path longer than
`MERKLE_DEPTH_ORCHARD` and *pads* a shorter one). The other seven cannot fail:
`bind_slot` reads `provider.secret()`, `merkle_root()`, `leaf_position()`,
`tx_commitment()`, `tx_nonce()` — all total — and `blind:<name>` is
`derive_blind(seed, name)` (`:587`), which accepts any name. This is stated as a
theorem (`bindable_requirements_are_exhaustive`) rather than as a comment,
because the split is what the Rust's totality actually is.

Two things it does **not** model, both recorded rather than implied:

* **Closure of the vocabulary.** That an unknown source is a parse error is a
  property of `parse_source` (`src/sdk/src/prover.rs:237-263`), which this file
  has no counterpart for. The claim is in the Rust; it is not mechanized here.
* **The derived-rule table.** This file's `DerivedRule` is not the Rust's
  `DerivedRule` (`src/sdk/src/prover.rs`): the Rust carries `Increment` and
  `LeafIncrement`, which have no constructor here, so the two tables differ in
  both directions. `derived:<rule>` is modelled only to the extent that
  `Capability/DerivedChain.lean` models its operand ordering, which is
  separately and independently checked.

`manifest.md:416-422`'s list of input sources omits `merkle_root`, which
`parse_source` accepts (`:243`) and which is a variant of the Rust enum. The
list is corrected in this unit; the Lean here carries all thirteen variants.
-/

import Mathlib
import DarkFi.Capability.Types
import DarkFi.AxiomBudget

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

/- A witness slot's source, per manifest.md's closed vocabulary. All thirteen variants of
   `dwow_sdk::prover::WitnessSource` (`src/sdk/src/prover.rs:144-171`), in its order. -/
inductive WitnessSource where
  | note (field : String)
  | param (field : String)
  | secret
  | secretNamed (name : String)
  | merklePath
  | merklePathCurrent
  | merklePathCumulative
  | merkleRoot
  | leafPosition
  | blind (name : String)
  | txCommitment
  | txNonce
  | derived (rule : DerivedRule)
deriving Repr, DecidableEq

/- What a binding has to consult: the manifest declarations a `note:`/`param:` slot names, the
   provider's named secrets, and whether the provider's Merkle path is within the tree depth.

   `pathWithinDepth` stands for the one check `merkle_path_array` performs — `path.len() >
   MERKLE_DEPTH_ORCHARD` is an error (`bin/dww/src/prover_impl.rs:572-577`). A *shorter* path is
   padded with Sinsemilla empty roots rather than rejected, so `true` here means "not rejected
   for length" and says nothing about the path being the right one for the leaf. -/
structure Declarations where
  noteFields : List String
  paramFields : List String
  namedSecrets : List String
  pathWithinDepth : Bool
deriving Repr

/- A source is bindable when the declaration it names is present. Exactly the four sources whose
   accessor can fail consult anything; the other seven are `True`. -/
def bindable (src : WitnessSource) (d : Declarations) : Prop :=
  match src with
  | WitnessSource.note f => f ∈ d.noteFields
  | WitnessSource.param f => f ∈ d.paramFields
  | WitnessSource.secretNamed n => n ∈ d.namedSecrets
  | WitnessSource.merklePath => d.pathWithinDepth = true
  | WitnessSource.merklePathCurrent => d.pathWithinDepth = true
  | WitnessSource.merklePathCumulative => d.pathWithinDepth = true
  | _ => True

/- Every slot in the witness_map is bound to a source it satisfies. -/
def allSlotsBound (map : List WitnessSource) (d : Declarations) : Prop :=
  ∀ src, src ∈ map → bindable src d

/- A proof is constructible when the witness_map arity matches the circuit's
   witness count AND every slot is bound. -/
def constructible (map : List WitnessSource) (d : Declarations) (witnessCount : Nat) : Prop :=
  map.length = witnessCount ∧ allSlotsBound map d

/- ==========================================================================
   The split: four sources can fail to bind, seven cannot
   ==========================================================================
   Stated as one theorem with both halves, because the unconditional half is
   `∀ d, True` on its own — the shape the tautology arm refuses — and it says
   something only beside the half where binding *is* conditional. Each of the
   four conditional cases is a witness that the requirement can fail, so none of
   them is decoration.
   ========================================================================== -/

@[axiom_budget 0]
theorem bindable_requirements_are_exhaustive :
    -- the four that consult a declaration, each falsifiable
    (∃ (d : Declarations) (f : String), ¬ bindable (WitnessSource.note f) d) ∧
    (∃ (d : Declarations) (f : String), ¬ bindable (WitnessSource.param f) d) ∧
    (∃ (d : Declarations) (n : String), ¬ bindable (WitnessSource.secretNamed n) d) ∧
    (∃ d : Declarations, ¬ bindable WitnessSource.merklePath d) ∧
    -- the seven that cannot, whatever the declarations say
    (∀ (d : Declarations) (r : DerivedRule), bindable (WitnessSource.derived r) d) ∧
    (∀ (d : Declarations) (n : String), bindable (WitnessSource.blind n) d) ∧
    (∀ d : Declarations, bindable WitnessSource.secret d) ∧
    (∀ d : Declarations, bindable WitnessSource.merkleRoot d) ∧
    (∀ d : Declarations, bindable WitnessSource.leafPosition d) ∧
    (∀ d : Declarations, bindable WitnessSource.txCommitment d) ∧
    (∀ d : Declarations, bindable WitnessSource.txNonce d) := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> simp [bindable, Declarations.mk.injEq]
  · exact ⟨{ noteFields := [], paramFields := [], namedSecrets := [], pathWithinDepth := true },
           "undeclared", by simp [bindable]⟩
  · exact ⟨{ noteFields := [], paramFields := [], namedSecrets := [], pathWithinDepth := true },
           "undeclared", by simp [bindable]⟩
  · exact ⟨{ noteFields := [], paramFields := [], namedSecrets := [], pathWithinDepth := true },
           "undeclared", by simp [bindable]⟩
  · exact ⟨{ noteFields := [], paramFields := [], namedSecrets := [], pathWithinDepth := false },
           by simp [bindable]⟩

/- ==========================================================================
   The negative side: each conditional source's failure blocks construction
   ==========================================================================
   One lemma, because `constructible`'s binding half is `allSlotsBound` and the
   four cases below differ only in *which* source fails. The named theorems are
   kept rather than replaced by it: each names the Rust failure it mirrors, which
   a call to a general lemma does not.
   ========================================================================== -/

/- A slot whose source is not bindable blocks the whole construction. -/
@[axiom_budget 0]
theorem unbound_source_blocks (map : List WitnessSource) (d : Declarations)
    (witnessCount : Nat) (src : WitnessSource) :
    src ∈ map → ¬ bindable src d → ¬ constructible map d witnessCount := by
  intro h_mem h_unbound h_constructible
  rcases h_constructible with ⟨_, h_bound⟩
  exact h_unbound (h_bound src h_mem)

/- Theorem (undeclared_field_blocks): a `note:` slot referencing an undeclared
   field makes the proof non-constructible — `bind_slot`'s
   `note field '{field}' not found` (`prover_impl.rs:312-313`). -/
@[axiom_budget 0]
theorem undeclared_field_blocks (map : List WitnessSource) (d : Declarations)
    (witnessCount : Nat) (f : String) :
    WitnessSource.note f ∈ map → f ∉ d.noteFields →
    ¬ constructible map d witnessCount :=
  fun h_mem h_undeclared =>
    unbound_source_blocks map d witnessCount _ h_mem (by simpa [bindable] using h_undeclared)

/- The same for `param:<field>` (`prover_impl.rs:317`). -/
@[axiom_budget 0]
theorem undeclared_param_blocks (map : List WitnessSource) (d : Declarations)
    (witnessCount : Nat) (f : String) :
    WitnessSource.param f ∈ map → f ∉ d.paramFields →
    ¬ constructible map d witnessCount :=
  fun h_mem h_undeclared =>
    unbound_source_blocks map d witnessCount _ h_mem (by simpa [bindable] using h_undeclared)

/- The same for `secret:<name>` (`prover_impl.rs:331`): a named secret the provider does not
   hold makes the proof non-constructible. -/
@[axiom_budget 0]
theorem undeclared_named_secret_blocks (map : List WitnessSource) (d : Declarations)
    (witnessCount : Nat) (n : String) :
    WitnessSource.secretNamed n ∈ map → n ∉ d.namedSecrets →
    ¬ constructible map d witnessCount :=
  fun h_mem h_undeclared =>
    unbound_source_blocks map d witnessCount _ h_mem (by simpa [bindable] using h_undeclared)

/- The same for a Merkle path longer than the tree depth (`prover_impl.rs:572-577`). -/
@[axiom_budget 0]
theorem path_too_long_blocks (map : List WitnessSource) (d : Declarations) (witnessCount : Nat) :
    WitnessSource.merklePath ∈ map → d.pathWithinDepth = false →
    ¬ constructible map d witnessCount :=
  fun h_mem h_too_long =>
    unbound_source_blocks map d witnessCount _ h_mem (by simp [bindable, h_too_long])

/- ==========================================================================
   Named blinds: what the name is load-bearing for
   ==========================================================================
   `bindable_requirements_are_exhaustive` records that `blind:<name>` consults
   no declaration — `derive_blind(seed, name)` accepts any name
   (`prover_impl.rs:587-589`). So the name's only role is to keep two
   differently-named blinds from being the same *source*, which is constructor
   injectivity and nothing more.

   **The docstring here used to claim more.** It said the theorem was why "the
   Rust prover's per-name Seed domain never collides two differently-named
   blinds" — but a statement about `WitnessSource.blind`'s injectivity says
   nothing about `hash_to_base(b"darkwow-blind", &[name, seed])`, whose
   collision-freedom is `Axioms.poseidon_collision_resistance`'s business and is
   not this theorem's. The name is load-bearing for *distinguishing sources*;
   the domain separation itself is the hash's.
   ========================================================================== -/

@[axiom_budget 0]
theorem namedBlind_distinct (n1 n2 : String) (h : n1 ≠ n2) :
    WitnessSource.blind n1 ≠ WitnessSource.blind n2 := by
  intro heq
  have hn : n1 = n2 := by
    cases heq
    rfl
  exact h hn

/-! ===== T6 — transaction binding (invariant #4) ===== -/

/-- T6 (spec): the transaction binding is `poseidon(3, tx_commitment, tx_nonce)`.
    Invariant #4 (`wallet.md:815`) SHALL hold: the prover binds the REAL
    seed-derived `tx_commitment`/`tx_nonce` (never a hardcoded zero). Stated as a
    predicate over the bound inputs — the HAZOP V6 remediation.

    **Unconsumed, and that is measured rather than assumed**: a grep across
    `.lean`/`.rs`/`.md`/`.py` finds no reference to this name outside its own
    line. It is the predicate the SDK's accessors make unreachable —
    `CapabilityProvider::tx_commitment`/`tx_nonce` default to
    `pallas::Base::zero()` (`src/sdk/src/prover.rs:353-360`, "zero if unbound")
    and `bind_slot` binds whatever they return with no failure path
    (`bin/dww/src/prover_impl.rs:358-366`) — so the trait compiles an
    implementation that binds exactly the value this predicate forbids. The
    wallet's `ResolvedCapProvider` overrides both with real fields (`:198-204`),
    so no live defect is claimed; what is claimed is that the default is a trap
    and this predicate is the statement it violates. -/
def bindsRealTxBinding (txCommitment txNonce : Nat) : Prop :=
  txCommitment ≠ 0 ∨ txNonce ≠ 0

/- ==========================================================================
   Removed in this unit: two definitional restatements
   ==========================================================================
   `genericProver_sound` and `witnessMap_arity` stood here. `constructible` **is**
   `map.length = witnessCount ∧ allSlotsBound map d`, so the first proved
   `constructible … → map.length = witnessCount ∧ (∀ src ∈ map, bindable src d)` —
   unfold and re-conjoin — and the second was that conjunction's first projection.
   Neither added a fact to the definition, and neither is cited anywhere in the
   tree (`grep` over `.md`/`.lean`/`.rs`/`.py`). Their claim is carried instead by
   the two halves that have content: `bindable_requirements_are_exhaustive` (what
   each source requires, and that seven require nothing) and the four
   `*_blocks` theorems above (what a missing declaration does).

   The sentence `genericProver_sound` carried, "there is no unbound
   (half-specified) witness", was also **false of the model it was attached to**:
   `bindable`'s `_ => True` catch-all sent eight of the thirteen constructors to
   `True`, so a `blind`, `merkle_path`, `leaf_position`, `tx_commitment` or
   `tx_nonce` slot was bound by the model without consulting anything. That is
   now true rather than false — but the reason is that the Rust's accessors are
   total for those sources, not that nothing is checked, and
   `bindable_requirements_are_exhaustive` states the difference.
   ========================================================================== -/

end DarkFi.Capability
