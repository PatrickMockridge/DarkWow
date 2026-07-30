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
  deriving BEq, Repr

/-! ==========================================================================
   Part 3: Witness State — What the Prover Knows (Hidden)
   ==========================================================================
   These values exist in the ZK witness but are NEVER public inputs.
   They are the "anonymous" part of the anonymous object.

   For Box:
     objectId = box_id
     contentsCommit = poseidon_hash(DOMAIN_MERKLE_LEAF, box_id, contents, nonce)

   For Purse:
     objectId = purse_id
     contentsCommit = poseidon_hash(DOMAIN_MERKLE_LEAF, purse_id, balance, nonce)
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

end Combinatorial
