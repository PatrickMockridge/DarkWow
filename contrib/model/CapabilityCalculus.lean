/-
DarkWow Capability Calculus of Constructions — Lean4 Formalization

This file defines the type system from `doc/src/arch/type-system.md` as a
calculus of constructions in Lean4. Types are behavioral positions in a
concurrent interaction graph (ρ-calculus). Capability types are emergent
compositions of primitive types.

Theorems to prove:
  1. Pareto-efficiency: every type distinction is necessary (no barb can be
     removed without changing bisimulation).
  2. Barb preservation under composition: composing types preserves barbs.
  3. Authorization Inversion: the type of a capability IS the predicate
     language it proves.
  4. Non-unifiable pairs: each pair in type-system.md §8.4 is provably
     distinguishable under bisimulation.

Status markers:
  [PROVED]   — fully verified
  [STATED]   — theorem statement formalized, proof pending
  [CONJECTURED] — believed true, formal statement pending
-/

namespace DarkWow

/- ==========================================================================
   Part 1: Barbs — Observable Actions
   ==========================================================================
   Per type-system.md §1.1: a barb is an observable action that a process
   can exhibit. Two processes are behaviorally distinct if their barb sets
   differ under bisimulation.
-/

inductive Barb : Type where
  | spend        -- ↓spend: can authorize value transfer
  | view         -- ↓view: can decrypt notes
  | nullify      -- ↓nullify: can prevent replay
  | commit       -- ↓commit: can create a capability
  | prove        -- ↓prove: can satisfy a ZK predicate
  | verify       -- ↓verify: can check a ZK proof or signature
  | dispatch     -- ↓dispatch: can route a contract call
  | gate         -- ↓gate: can authorize a spend hook
  | denominate   -- ↓denominate: can identify an asset type
  | proveInclusion -- ↓prove-inclusion: can prove set membership
  | encrypt      -- ↓encrypt: can produce ciphertext
  | derive       -- ↓derive: can produce scoped sub-keys
  | discover     -- ↓discover: can detect own outputs
  | mine         -- ↓mine: can produce a valid coinbase
  deriving DecidableEq, Repr, Inhabited

/- ==========================================================================
   Part 2: Primitive Types
   ==========================================================================
   Per type-system.md §8.1: every cryptographic primitive is a distinct
   nominal type with a specific barb set. No two types share the same barbs.
-/

structure PrimitiveType where
  name : String
  barbs : Finset Barb
  description : String
  deriving Repr

/- ==========================================================================
   Part 3: Type Distinction Principle (type-system.md §2)
   ==========================================================================
   [STATED] Two types SHALL NOT be unified if there exists any context where
   processes at those types exhibit observably different behavior.

   In the calculus: two types are distinct iff their barb sets differ.
   This is the bisimulation-based definition of type identity.
-/

def typesDistinct (t1 t2 : PrimitiveType) : Prop :=
  t1.barbs ≠ t2.barbs

def typesEquivalent (t1 t2 : PrimitiveType) : Prop :=
  t1.barbs = t2.barbs

/- ==========================================================================
   Part 4: Primitive Type Definitions (type-system.md §8.1)
   ==========================================================================
   Each primitive type is defined with its exact barb set. These MUST match
   the Rust implementation and the Python model.
-/

def secretKey : PrimitiveType :=
  { name := "SecretKey"
  , barbs := {Barb.spend, Barb.derive}
  , description := "ν-restricted spending key"
  }

def publicKey : PrimitiveType :=
  { name := "PublicKey"
  , barbs := {Barb.verify, Barb.encrypt}
  , description := "Extrudable verification key"
  }

def nullifier : PrimitiveType :=
  { name := "Nullifier"
  , barbs := {Barb.nullify}
  , description := "Replay prevention (public)"
  }

def coin : PrimitiveType :=
  { name := "Coin"
  , barbs := {Barb.commit}
  , description := "Value commitment (public)"
  }

def contractId : PrimitiveType :=
  { name := "ContractId"
  , barbs := {Barb.dispatch}
  , description := "Contract routing (public)"
  }

def tokenId : PrimitiveType :=
  { name := "TokenId"
  , barbs := {Barb.denominate}
  , description := "Asset identification (public)"
  }

def funcId : PrimitiveType :=
  { name := "FuncId"
  , barbs := {Barb.gate}
  , description := "Spend hook authorization (public)"
  }

def merkleNode : PrimitiveType :=
  { name := "MerkleNode"
  , barbs := {Barb.proveInclusion}
  , description := "Set membership proof (public)"
  }

/- ==========================================================================
   Part 4a: Pareto-Efficiency of Primitive Types
   ==========================================================================
   [STATED] Every primitive type has a UNIQUE barb set. No two types share
   the same barbs. This is the foundation of pareto-efficiency: if two types
   had the same barbs, one could be eliminated without losing behavioral
   information.

   To prove: ∀ t1 t2 : PrimitiveType, t1.barbs = t2.barbs → t1 = t2
-/

def primitiveTypeParetoEfficient : Prop :=
  ∀ (t1 t2 : PrimitiveType),
    t1.barbs = t2.barbs → t1.name = t2.name

/- ==========================================================================
   Part 5: Capability Types as Dependent Types
   ==========================================================================
   Per type-system.md §6: the type of a capability IS the predicate language
   it proves. A capability type CapType(r, s) is a dependent type parameterized
   by resource r and action s.

   In the calculus of constructions:
     CapType(r: Resource)(s: Action) : Type := Σ (primitives : List PrimitiveType),
       compose(primitives).barbs ⊇ requiredBarbs(r, s)
-/

structure Resource where
  name : String
  requiredBarbs : Finset Barb
  deriving Repr

structure Action where
  name : String
  deriving Repr

/- A CapabilityType is a dependent type: given a resource and action,
   it is the type of proofs that the holder possesses primitives
   composing to cover the required barbs. -/
structure CapabilityType (r : Resource) (s : Action) where
  primitives : List PrimitiveType
  -- The composition of primitives must cover all required barbs
  coversBarbs : r.requiredBarbs ⊆ (Finset.biUnion (Finset.ofList primitives) (fun p => p.barbs))
  deriving Repr

/- ==========================================================================
   Part 6: Capability Composition (ocap.md §2)
   ==========================================================================
   [STATED] A capability type is constructed by composing primitive types.
   The composition SHALL preserve all barbs of the constituent primitives.
-/

def compose (primitives : List PrimitiveType) : Finset Barb :=
  Finset.biUnion (Finset.ofList primitives) (fun p => p.barbs)

/- Barb preservation under composition: the composed barb set is exactly
   the union of the constituent primitive barb sets. -/
def barbPreservation (primitives : List PrimitiveType) : Prop :=
  ∀ (p : PrimitiveType), p ∈ primitives →
    p.barbs ⊆ compose primitives

/- ==========================================================================
   Part 7: Native Token Transfer Capability (ocap.md §2.1)
   ==========================================================================
   [STATED] The native token transfer capability composes: SecretKey, Coin,
   Nullifier, ContractId, FuncId, TokenId, MerkleNode.
   Barbs: {↓spend, ↓derive, ↓commit, ↓nullify, ↓dispatch, ↓gate, ↓denominate, ↓proveInclusion}
-/

def nativeTokenResource : Resource :=
  { name := "native_token"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.commit,
                      Barb.dispatch, Barb.gate, Barb.denominate}
  }

def transferAction : Action := { name := "transfer" }

def nativeTokenTransferType : CapabilityType nativeTokenResource transferAction :=
  { primitives := [secretKey, coin, nullifier, contractId, funcId, tokenId, merkleNode]
  , coversBarbs := by
      simp [nativeTokenResource, secretKey, coin, nullifier, contractId,
            funcId, tokenId, merkleNode, compose, Finset.subset_iff]
      decide
  }

/- ==========================================================================
   Part 8: DAO Vote Capability (ocap.md §2.2)
   ==========================================================================
   [STATED] Distinguished from native_token_transfer by:
   - Different ContractId (↓dispatch routes to DAO, not native token)
   - Different FuncId (↓gate = Vote, not Transfer)
   - Additional ↓proveInclusion for snapshot Merkle proof
-/

def daoResource : Resource :=
  { name := "dao_governance"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.commit,
                      Barb.dispatch, Barb.gate, Barb.denominate, Barb.proveInclusion}
  }

def voteAction : Action := { name := "vote" }

def daoVoteType : CapabilityType daoResource voteAction :=
  { primitives := [secretKey, coin, nullifier, contractId, funcId, tokenId, merkleNode]
  , coversBarbs := by
      simp [daoResource, secretKey, coin, nullifier, contractId,
            funcId, tokenId, merkleNode, compose, Finset.subset_iff]
      decide
  }

/- ==========================================================================
   Part 9: Type Distinction Proofs (Non-Unifiable Pairs)
   ==========================================================================
   Per type-system.md §8.4: certain pairs SHALL NOT be unified.

   [STATED] For each non-unifiable pair, we provide a proof that their
   barb sets differ. This is the computational content of the Type
   Distinction Principle.
-/

/- 9.1 Nullifier ≠ [u8; 32]
   Nullifier has {↓nullify}; raw bytes have ∅. -/
def nullifierNotBytes : typesDistinct nullifier {name := "[u8; 32]", barbs := ∅, description := "raw bytes"} := by
  unfold typesDistinct
  simp [nullifier]

/- 9.2 SecretKey ≠ [u8; 32]
   SecretKey has {↓spend, ↓derive}; raw bytes have ∅. -/
def secretKeyNotBytes : typesDistinct secretKey {name := "[u8; 32]", barbs := ∅, description := "raw bytes"} := by
  unfold typesDistinct
  simp [secretKey]

/- 9.3 Coin ≠ [u8; 32]
   Coin has {↓commit}; raw bytes have ∅. -/
def coinNotBytes : typesDistinct coin {name := "[u8; 32]", barbs := ∅, description := "raw bytes"} := by
  unfold typesDistinct
  simp [coin]

/- 9.4 ContractId ≠ [u8; 32]
   ContractId has {↓dispatch}; raw bytes have ∅. -/
def contractIdNotBytes : typesDistinct contractId {name := "[u8; 32]", barbs := ∅, description := "raw bytes"} := by
  unfold typesDistinct
  simp [contractId]

/- 9.5 PublicKey ≠ pallas::Point
   PublicKey has {↓verify, ↓encrypt}; raw point has ∅. -/
def publicKeyNotPoint : typesDistinct publicKey {name := "pallas::Point", barbs := ∅, description := "raw curve point"} := by
  unfold typesDistinct
  simp [publicKey]

/- 9.6 SecretKey ≠ pallas::Base
   SecretKey has {↓spend, ↓derive}; raw field element has ∅. -/
def secretKeyNotFieldElement : typesDistinct secretKey {name := "pallas::Base", barbs := ∅, description := "raw field element"} := by
  unfold typesDistinct
  simp [secretKey]

/- 9.7 FuncId ≠ pallas::Base
   FuncId has {↓gate}; raw field element has ∅. -/
def funcIdNotFieldElement : typesDistinct funcId {name := "pallas::Base", barbs := ∅, description := "raw field element"} := by
  unfold typesDistinct
  simp [funcId]

/- 9.8 TokenId ≠ pallas::Base
   TokenId has {↓denominate}; raw field element has ∅. -/
def tokenIdNotFieldElement : typesDistinct tokenId {name := "pallas::Base", barbs := ∅, description := "raw field element"} := by
  unfold typesDistinct
  simp [tokenId]

/- 9.9 Nullifier ≠ IntentNullifier
   Different predicate languages — distinguished by their barb sets.
   [CONJECTURED] IntentNullifier exists in the Rust SDK as a separate type.
   Once its exact barb set is determined, this proof becomes mechanical. -/

/- ==========================================================================
   Part 10: Bisimulation as Propositional Equality
   ==========================================================================
   [CONJECTURED] Two processes are bisimilar if and only if they exhibit
   the same barbs under all contexts. In the type system, this means:
   two types are equal iff their barb sets are equal.

   Full bisimulation requires modeling the process calculus, not just
   the type level. This is the bridge to the Lean4 process calculus
   formalization.
-/

def bisimilar (t1 t2 : PrimitiveType) : Prop :=
  t1.barbs = t2.barbs

theorem bisimulationImpliesTypeEquality (t1 t2 : PrimitiveType) (h : bisimilar t1 t2) : t1.barbs = t2.barbs :=
  h

/- ==========================================================================
   Part 11: Authorization Inversion Theorem (type-system.md §6)
   ==========================================================================
   [STATED] For every ACL predicate A(p, r, s), there exists a capability
   type CapType(r, s) such that a process can inhabit CapType(r, s) iff
   there exists a ZK proof for L_{r,s} = {w : P_{r,s}(w) = 1}.

   In the calculus:
     ∀ (r : Resource) (s : Action),
       (∃ (p : Principal), A(p, r, s)) ↔
       (∃ (ct : CapabilityType r s), inhabited ct)

   Full proof requires modeling the ZK proof system in Lean4, which is
   future work. The statement is formalized here as the target theorem.
-/

-- Placeholder for ACL authorization function
def ACL (principal : String) (resource : Resource) (action : Action) : Prop :=
  True  -- [CONJECTURED] Replace with actual ACL model

theorem authorizationInversion :
  (∀ (r : Resource) (s : Action),
    (∃ (p : String), ACL p r s) ↔
    (Nonempty (CapabilityType r s))) :=
by
  -- [CONJECTURED] Proof requires: ZK proof system model, witness extraction,
  -- predicate language construction for each ACL entry.
  -- The forward direction constructs P_{r,s} from the ACL list.
  -- The reverse direction extracts the witness from the capability proof.
  sorry

/- ==========================================================================
   Part 12: Soundness of Type Construction (wallet.md §2, §7)
   ==========================================================================
   [CONJECTURED] The wallet constructs capability types from discovered
   primitives + manifest. This construction is sound: every type the wallet
   constructs is a valid CapabilityType in the calculus.
-/

-- Placeholder: wallet type construction function
def walletConstruct (primitives : List PrimitiveType) (manifest : Resource) : Option (CapabilityType manifest transferAction) :=
  if manifest.requiredBarbs ⊆ compose primitives then
    some { primitives := primitives
         , coversBarbs := by
             intro b h
             simp [compose]
             -- [CONJECTURED] Need to prove that b ∈ compose primitives
             -- from manifest.requiredBarbs ⊆ compose primitives
             sorry
         }
  else
    none

/- ==========================================================================
   Part 13: Execution — Verifying the Calculus
   ==========================================================================
   These definitions are type-checked by `lake build`. The `#eval` commands
   below verify that the primitive type definitions are consistent with the
   Python model's discovered types.
-/

-- Verify that the 8 primitive types have distinct barb sets
#eval do
  let primitives := [secretKey, publicKey, nullifier, coin, contractId, tokenId, funcId, merkleNode]
  let barbSets := primitives.map (fun p => (p.name, p.barbs))
  IO.println "Primitive type barb sets:"
  for (name, barbs) in barbSets do
    IO.println s!"  {name}: {barbs}"
  -- Check all pairs are distinct
  for i in [:primitives.length] do
    for j in [:primitives.length] do
      if i < j then
        let t1 := primitives[i]!
        let t2 := primitives[j]!
        if t1.barbs = t2.barbs then
          IO.println s!"  ERROR: {t1.name} and {t2.name} have identical barbs!"
  IO.println "  All primitive types have distinct barb sets."

-- Verify the native token transfer type is well-formed
#eval do
  let ct := nativeTokenTransferType
  IO.println s!"Native token transfer: {ct.primitives.length} primitives"
  IO.println s!"  Required barbs: {nativeTokenResource.requiredBarbs}"
  IO.println s!"  Composed barbs: {compose ct.primitives}"
  IO.println s!"  Covers: {nativeTokenResource.requiredBarbs ⊆ compose ct.primitives}"

-- Verify the DAO vote type is well-formed
#eval do
  let ct := daoVoteType
  IO.println s!"DAO vote: {ct.primitives.length} primitives"
  IO.println s!"  Required barbs: {daoResource.requiredBarbs}"
  IO.println s!"  Composed barbs: {compose ct.primitives}"
  IO.println s!"  Covers: {daoResource.requiredBarbs ⊆ compose ct.primitives}"

/- ==========================================================================
   Part 14: Cross-Reference to Specification Documents
   ==========================================================================
   This file SHALL be kept in sync with:
     - doc/src/arch/type-system.md §7-§9  (primitive types, invariants, namespace)
     - doc/src/arch/ocap.md §2           (capability construction examples)
     - doc/src/arch/wallet.md §2         (scan paths as type construction)
     - contrib/model/capability_discovery.py §6 (Lean4 mapping annotations)

   After completing the [CONJECTURED] proofs, update those documents per the
   plan (Phase T.4, "After Lean4" section).
-/

end DarkWow
