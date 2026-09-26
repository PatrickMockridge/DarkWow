import DarkFi.AxiomBudget

/-!
# L1 Combinatorial State Space Types

Defines the formal types for modeling L1 vs L2 contract state spaces.
L1: anonymous encrypted objects in a Merkle tree (only nullifiers + roots visible).
L2: singleton object with known identity (deterministic KV lookup).

The key insight: in L1, an external observer sees only nullifiers and Merkle
roots. The object identities (box_id, purse_id) and owner secrets are
witness-only — they exist in the ZK witness but are NEVER public inputs.

This module defines the abstract types. Transitions and theorems are in the
sibling modules Transitions.lean, ComplexityJump.lean, etc.

References:
  - doc/src/arch/privacy.md §5 (four-component architecture, consume+create model)
  - doc/src/contract/box.md (Box L1 spec)
  - doc/src/contract/purse.md (Purse L1 spec)
-/

namespace Combinatorial

/-! ==========================================================================
   Part 1: Core Cryptographic Primitives (abstract)
   ==========================================================================
   These are abstract representations of Poseidon hashes, Merkle roots, and
   nullifiers. We model them as Nat for combinatorial counting — the actual
   field arithmetic (pallas::Base, poseidon_hash, merkle_root) is opaque.
   The combinatorial bounds hold regardless of the specific hash function.

   Using `abbrev` (not `def`) so that typeclass instances like BEq are
   inherited from Nat.
-/

/-- A Merkle tree leaf commitment: poseidon_hash(domain, args...) --/
abbrev LeafCommitment := Nat

/-- A nullifier: poseidon_hash(DOMAIN_NULLIFIER, owner_secret, object_id, nonce) --/
abbrev NullifierValue := Nat

/-- A Merkle root: Sinsemilla-based MerkleCRH of depth-32 tree --/
abbrev MerkleRoot := Nat

/-- An owner secret: the spending key (witness-only, never public) --/
abbrev OwnerSecret := Nat

/-- An object identifier: box_id or purse_id (witness-only, never public) --/
abbrev ObjectId := Nat

/-- A state nonce: sequential counter per object (witness-only) --/
abbrev StateNonce := Nat

/-! ==========================================================================
   Part 2: Public State — What an External Observer Sees
   ==========================================================================
   In L1, the public state consists of:
   - The current Merkle root (all objects committed)
   - The set of spent nullifiers (prevents double-spend)
   - Historical Merkle roots (for inclusion proofs against past states)

   Object identities, owner secrets, and state nonces are NOT visible.
-/

structure PublicState where
  merkleRoot      : MerkleRoot
  spentNullifiers : List NullifierValue
  historicalRoots : List MerkleRoot
  /-- The recognized commitments — the Create face of an exercise, where `spentNullifiers` is the
      Consume face. `Capability/Exercise.lean`'s `applyExercise` appends its `outputs` here.

      This is the *recognized set*, not the tree: `merkleRoot` is the root that commits to it, and
      recomputing the root from this list is the per-contract tree's business
      (`Capability/PerContractTree.lean`, `HashOps`), not modelled here. The field exists because
      `Exercise.outputs` was otherwise dead — `applyExercise` ignored it, so the module named for
      Exercise+Consume modelled Consume only. -/
  recognizedCommitments : List LeafCommitment
  deriving BEq, Repr

/-! ==========================================================================
   Part 3: Witness State — What the Prover Knows (Hidden)
   ==========================================================================
   These values exist in the ZK witness but are NEVER public inputs.
   They are the "anonymous" part of the anonymous object.

   For Box:
     objectId = box_id
     contentsCommit = poseidon_hash(DOMAIN_MERKLE_LEAF, box_id, contents, nonce, owner_pub)

   For Purse:
     objectId = purse_id
     contentsCommit = poseidon_hash(DOMAIN_MERKLE_LEAF, purse_id, balance, nonce, owner_pub)

   **The fourth `owner_pub` argument is the owner-binding fix, and it is not decoration.** Both
   commitments were `poseidon_hash(DOMAIN_MERKLE_LEAF, <id>, <contents-or-balance>, <nonce>)` — no owner
   term — which meant the spender's secret was bound only to the nullifier and never to the leaf. Anyone
   who knew a leaf's preimage (the call params publish it in plaintext) could consume the leaf with a
   secret of their own choosing, and one leaf admitted one nullifier *per distinct secret*, so the
   "exercised exactly once" property was not circuit-enforced and the chain's nullifier de-duplication
   could not see it. The leaf now commits to `owner_pub = poseidon_hash(DOMAIN_SIGNATURE_SECRET,
   owner_secret)`, so a second spend of the same leaf needs the same secret and therefore the same
   nullifier. Register row `OBL-C81` and its box counterpart.
-/

structure WitnessState where
  objectId       : ObjectId
  ownerSecret    : OwnerSecret
  stateNonce     : StateNonce
  contentsCommit : LeafCommitment
  deriving BEq, Repr

/-! ==========================================================================
   Part 4: L1 Anonymity Set — N Anonymous Objects in a Merkle Tree
   ==========================================================================
   The full L1 state: a Merkle tree of depth `depth` containing `objects`
   concurrent anonymous objects, each with hidden witness data. The public
   state (roots + nullifiers) is all an external observer sees.

   Key invariant (enforced by consume+create model):
     |objects| = active (unspent) objects
     |spentNullifiers| = objects that have been consumed
     No object appears in both sets simultaneously.
-/

structure L1AnonymitySet where
  depth   : Nat
  objects : List WitnessState
  public  : PublicState
  deriving BEq, Repr

/-! ==========================================================================
   Part 5: L2 Singleton State — Known Identity, Deterministic
   ==========================================================================
   In L2, there is exactly one object with a known public identifier.
   Operations always target "the box" or "the purse" — no anonymity set,
   no target selection, no combinatorial state space.
-/

structure L2SingletonState where
  object     : WitnessState
  publicHash : ObjectId  -- known identifier, visible to all
  deriving BEq, Repr

/-! ==========================================================================
   Part 6: Factory Functions
   ==========================================================================
   Construct L1 and L2 states for testing and theorem statements.
-/

/-- Create an L1 anonymity set with N distinct anonymous objects --/
def mkL1State (depth : Nat) (objectCount : Nat) : L1AnonymitySet :=
  let objects := List.range objectCount |>.map λ i =>
    { objectId := i
    , ownerSecret := i + 1000
    , stateNonce := 0
    , contentsCommit := i + 2000
    : WitnessState }
  { depth := depth
  , objects := objects
  , public := { merkleRoot := 0
              , spentNullifiers := []
              , historicalRoots := [0]
              , recognizedCommitments := objects.map (fun o => o.contentsCommit)
              : PublicState }
  }

/-- Create an L2 singleton state --/
def mkL2State : L2SingletonState :=
  { object := { objectId := 0
              , ownerSecret := 1000
              , stateNonce := 0
              , contentsCommit := 2000
              : WitnessState }
  , publicHash := 0
  }

/-! ==========================================================================
   Part 5: The Anonymity Premise, Stated
   ==========================================================================
   `Transitions.l1_exceeds_l2` counts `N ^ K` valid trajectories for K sequential operations over N
   concurrent objects, and `privacy.md` §2.4 reads that N as the anonymity set. **The count is only
   an anonymity set if an observer cannot tell which of the N an operation touched** — a premise the
   arithmetic assumes and which, until 2026-09-26, nothing in this tree stated. It is stated here
   because Box's and Purse's call data carried the object identity, the nonces and the balances in
   plaintext, and a transcript that carries the object id determines the object: N = 1, where
   `1 ^ K = 1` is the L2 (singleton) count rather than the L1 one.

   The model below is the smallest one that separates the two wires, and the separation is
   *computed*: `anonymitySet` is 1 for a wire that publishes the id and 3 for one whose observation
   does not depend on the object. The residual 10 slots declared in
   `scripts/check-l1-wire-conformance.sh` are what a real contract would have to shed to move from
   the first wire to the second.
-/

/-- What one operation's wire exposes, as a function of the object it operated on: everything an
    observer can see of the call — the public inputs plus `Call.data`, which the transaction hash
    commits to byte-for-byte. A `Nat` is enough for the distinction this Part is about, and keeping
    it one is what makes both instances below `decide`-able. -/
abbrev Obs := ObjectId → Nat

/-- The transcript determines the object when two objects that produce the same observation are the
    same object. Where this holds, N is 1. -/
def DeterminesObject (obs : Obs) (objs : List ObjectId) : Prop :=
  ∀ o₁ ∈ objs, ∀ o₂ ∈ objs, obs o₁ = obs o₂ → o₁ = o₂

/-- The anonymity set of one observation: the objects in `objs` that would have produced it. -/
def anonymitySet (obs : Obs) (objs : List ObjectId) (o : ObjectId) : Nat :=
  (objs.filter (fun x => obs x == obs o)).length

/-- A wire that publishes the object id: the pre-2026-09-26 Box and Purse, where `box_id` and
    `purse_id` were call params. -/
def wirePublishingTheId : Obs := fun o => o

/-- A wire whose observation does not depend on the object: what an AEAD note looks like from
    outside, since the chain carries the same shape whatever object moved. -/
def wireCarryingANote : Obs := fun _ => 0

/-- The publishing wire's anonymity set is a singleton. -/
@[axiom_budget 0]
theorem the_publishing_wire_collapses_the_anonymity_set :
    anonymitySet wirePublishingTheId [1, 2, 3] 2 = 1 := by decide

/-- The note-carrying wire's is the whole set: every object could have produced the observation. -/
@[axiom_budget 0]
theorem the_note_wire_keeps_the_anonymity_set :
    anonymitySet wireCarryingANote [1, 2, 3] 2 = 3 := by decide

/-- The premise separates the two wires, computed rather than argued: the publishing wire satisfies
    it (which is what makes N = 1) and the note-carrying wire does not. -/
@[axiom_budget 0]
theorem the_premise_separates_the_two_wires :
    DeterminesObject wirePublishingTheId [1, 2, 3] ∧
      ¬ DeterminesObject wireCarryingANote [1, 2, 3] := by
  refine ⟨fun o₁ _ o₂ _ h => h, fun h => absurd (h 1 (by decide) 2 (by decide) rfl) (by decide)⟩

end Combinatorial
