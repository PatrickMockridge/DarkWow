/-
DarkWow Capability Type System — Primitive Types and Barbs

Imports the existing DarkFi proof infrastructure to define the type system
from doc/src/arch/type-system.md §1-§8 as a calculus of constructions.

Every definition here is a theorem-free namespace of types. Proofs are in
the sibling modules: Pareto.lean, Distinction.lean, Composition.lean,
Inversion.lean, Wallet.lean.
-/

/- ==========================================================================
   Part 1: Barbs — Observable Actions (type-system.md §1.1)
   ==========================================================================
   14 observable actions. A barb is what a process can exhibit to an
   external observer. Two processes are behaviorally distinct iff their
   barb sets differ under bisimulation.
-/

inductive Barb : Type where
  | spend          -- ↓spend: can authorize value transfer
  | view           -- ↓view: can decrypt notes
  | nullify        -- ↓nullify: can prevent replay
  | commit         -- ↓commit: can create a capability
  | prove          -- ↓prove: can satisfy a ZK predicate
  | verify         -- ↓verify: can check a ZK proof or signature
  | dispatch       -- ↓dispatch: can route a contract call
  | gate           -- ↓gate: can authorize a spend hook
  | denominate     -- ↓denominate: can identify an asset type
  | proveInclusion -- ↓prove-inclusion: can prove set membership
  | encrypt        -- ↓encrypt: can produce ciphertext
  | derive         -- ↓derive: can produce scoped sub-keys
  | discover       -- ↓discover: can detect own outputs
  | mine           -- ↓mine: can produce a valid coinbase
  deriving DecidableEq, Repr, Inhabited

/- ==========================================================================
   Part 2: Primitive Types (type-system.md §8.1)
   ==========================================================================
   Every cryptographic primitive is a distinct nominal type with a specific
   barb set. Per §2: two types SHALL NOT be unified if their barbs differ.
-/

structure PrimitiveType where
  name : String
  barbs : Finset Barb
  description : String
  deriving Repr, BEq

/- ==========================================================================
   Part 3: Primitive Type Definitions
   ==========================================================================
   Each type is defined with its EXACT barb set per the specification.
   These MUST match the Rust implementation and the Python model.
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
   Part 3a: Authority Types (type-system.md §8.3)
   ==========================================================================
   These types carry the ν-restriction property: they SHALL NOT be
   constructed from raw bytes or random field elements. Only the
   authorized constructor paths may produce them.
-/

def ownedSecretKey : PrimitiveType :=
  { name := "OwnedSecretKey"
  , barbs := {Barb.spend}
  , description := "Declared spending key (no ::random constructor)"
  }

def miningRecipient : PrimitiveType :=
  { name := "MiningRecipient"
  , barbs := {Barb.spend, Barb.mine}
  , description := "Coinbase recipient (from_account only)"
  }

/- ==========================================================================
   Part 3b: Structural Types (type-system.md §8.2)
   ==========================================================================
   These types are compositions of primitives that form the blockchain
   data structures.
-/

def intentNullifier : PrimitiveType :=
  { name := "IntentNullifier"
  , barbs := {Barb.nullify, Barb.gate}
  , description := "Intent-system nullifier (scoped to intent, not coin)"
  }

def bridgeCapNullifier : PrimitiveType :=
  { name := "BridgeCapNullifier"
  , barbs := {Barb.nullify, Barb.dispatch}
  , description := "Bridge send-cap nullifier (2-field: hash + send_cap_hash)"
  }

/- ==========================================================================
   Part 3c: Complete Primitive Type List
   ==========================================================================
   All 13 primitive types in a single list for exhaustive checking.
-/

def allPrimitiveTypes : List PrimitiveType :=
  [secretKey, publicKey, nullifier, coin, contractId, tokenId, funcId,
   merkleNode, ownedSecretKey, miningRecipient, intentNullifier, bridgeCapNullifier]

/- ==========================================================================
   Part 4: Type Distinction (type-system.md §2)
   ==========================================================================
   Two types are distinct iff their barb sets differ. This is the
   bisimulation-based definition of type identity.
-/

def typesDistinct (t1 t2 : PrimitiveType) : Prop :=
  t1.barbs ≠ t2.barbs

def typesEquivalent (t1 t2 : PrimitiveType) : Prop :=
  t1.barbs = t2.barbs

/- ==========================================================================
   Part 5: Raw Byte Comparison Types
   ==========================================================================
   These are NOT valid DarkWow types. They exist solely to prove the
   non-unifiable pairs in Distinction.lean. [u8; 32] has NO barbs —
   it is an opaque byte container with zero behavioral constraints.
-/

def rawBytes : PrimitiveType :=
  { name := "[u8; 32]"
  , barbs := ∅
  , description := "Opaque byte container (no barbs — not a valid type)"
  }

def rawFieldElement : PrimitiveType :=
  { name := "pallas::Base"
  , barbs := ∅
  , description := "Raw field element (no barbs — not a valid type)"
  }

def rawCurvePoint : PrimitiveType :=
  { name := "pallas::Point"
  , barbs := ∅
  , description := "Raw curve point (no barbs — not a valid type)"
  }
