/-
DarkWow Capability Type System — Primitive Types and Barbs

Imports the existing DarkFi proof infrastructure to define the type system
from doc/src/arch/type-system.md §1-§8 as a calculus of constructions.

Every definition here is a theorem-free namespace of types. Proofs are in
the sibling modules: Pareto.lean, Distinction.lean, Composition.lean,
Inversion.lean, Wallet.lean.
-/

import Mathlib

/-! ## Namespace

This file declares into `DarkFi.Capability.Types`, which is the namespace every importer
already `open`s (`Composition.lean`, `Pareto.lean`, `Distinction.lean`, `Inversion.lean`,
`Wallet.lean`, `Net/Framing.lean`, `Net/Receive.lean`, `Main.lean`).

It did not, until now, and the declarations sat at top level instead. That was not merely
untidy: `Composition.lean` declares a `structure Action`, and at top level that name is
already taken by **Mathlib's categorical `Action`** — `(V : Type u) → [LargeCategory V] →
MonCat → Type u` — because this file imports `Mathlib`. So `Action` resolved to Mathlib's and
every use of the capability type system's own `Action` failed with `type expected, got
(Action : (V : Type u) → ...)`. That one collision, plus the failed `open` above, accounted
for 45 of the 103 compile errors in this tree. -/

namespace DarkFi.Capability.Types

/- ==========================================================================
   Part 1: Barbs — Observable Actions (type-system.md §1.1)
   ==========================================================================
   the normative alphabet. A barb is what a process can exhibit to an
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
  | concurrent     -- ↓concurrent: can execute in parallel with siblings
  | merge          -- ↓merge: can deterministically combine concurrent state diffs
  | syncBarrier    -- ↓sync-barrier: can block until a synchronization condition is met
  | broadcast      -- ↓broadcast: can publish to multiple subscribers simultaneously
  | rateLimit      -- ↓rate-limit: can constrain its own output rate
  | gossipForward  -- ↓gossip-forward: can relay to a subset of outbound peers
  | quorumQuery    -- ↓quorum-query: can query a threshold of peers and converge
  | dagParent      -- ↓dag-parent: can reference prior events in a partial-order
  -- Fee lifecycle (type-system.md §1.1 rows 23-24)
  | payFee         -- ↓pay-fee: can exercise FeeV2 — change + fee, fee accumulated in plaintext
  | collectFees    -- ↓collect-fees: can exercise FeeCollectV1 — pot equals the claim, miner paid
  -- Fee rejection (rows 25-28). These are the vocabulary in which "the validator rejects X" is an
  -- *observation* rather than an error path, which is what lets a soundness statement have a
  -- positive form instead of a list of absences. They are in the spec on purpose.
  | badFeeAmount   -- ↓bad-fee-amount: input value <= fee — rejected at the call builder
  | badMerkleRoot  -- ↓bad-merkle-root: Merkle root absent from commitment_roots_db
  | zeroClaim      -- ↓zero-claim: FeeCollectV1 with a zero pot — rejected as replay
  | badClaim       -- ↓bad-claim: claimed total does not match the plaintext pot
  -- Fee signalling (rows 29-32)
  | feeWindowOpen      -- ↓fee-window-open: window boundary reached, tiers recomputed
  | feeWindowAdvertise -- ↓fee-window-advertise: miner publishes the congestion direction
  | feeWindowEnforce   -- ↓fee-window-enforce: mempool applies tier prices to arrivals
  | feeWindowDiscover  -- ↓fee-window-discover: wallet reads the flags and sets its fee
  -- Shard identity (§1.1 row 33; §9.5 Emergent-Topology Sharding)
  --
  -- `↓denominate` names the *asset* a value belongs to; `↓shard` names the *name-space* it
  -- belongs to. §9.5 already writes the shards as ρ-processes — `S_1!(state_root_1, …)`,
  -- `S_2!(…)`, `CrossShardProof?(import_A_B)` — so an external chain (Ethereum, Monero,
  -- Zcash, Aztec, Litecoin) is a shard whose state lives off-network, and naming which shard
  -- a value came from is an observable action.
  --
  -- This barb is what makes *namespace convergence* visible. A bridge is not a primitive with a
  -- convergence barb of its own: convergence is what a *composition* of shard-naming and
  -- proof-bearing primitives exhibits, exactly as `ocap.md` says types emerge from composition.
  | shard          -- ↓shard: can name the shard (name-space) a value belongs to
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
  deriving BEq

/- ==========================================================================
   Part 2a: Concurrency Process Types (type-system.md §9)
   ==========================================================================
   Concurrency primitives map to process types with concurrency barbs.
   These types describe HOW a process executes, complementing the
   cryptographic types which describe WHAT a process can do.
-/

structure ConcurrentProcess where
  name : String
  authorizationBarbs : Finset Barb    -- cryptographic barbs (spend, nullify, etc.)
  concurrencyBarbs : Finset Barb      -- execution barbs (concurrent, merge, etc.)
  canConcurrent : Bool
  canMerge : Bool

def concurrentProcessBarbs (p : ConcurrentProcess) : Finset Barb :=
  p.authorizationBarbs ∪ p.concurrencyBarbs

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

def commitment : PrimitiveType :=
  { name := "Commitment"
  , barbs := {Barb.commit}
  , description := "Value commitment (public)"
  }

def contractId : PrimitiveType :=
  { name := "ContractId"
  , barbs := {Barb.dispatch}
  , description := "Contract routing (public)"
  }

def assetId : PrimitiveType :=
  { name := "AssetId"
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
  , description := "Intent-system nullifier (scoped to intent, not commitment)"
  }

def bridgeCapNullifier : PrimitiveType :=
  { name := "BridgeCapNullifier"
  , barbs := {Barb.nullify, Barb.dispatch}
  , description := "Bridge send-cap nullifier (2-field: hash + send_cap_hash)"
  }

/- Bridge primitives (type-system.md §8.6) -/

def bridgeAddress : PrimitiveType :=
  { name := "BridgeAddress"
  , barbs := {Barb.derive}
  , description := "Deterministic bridge address: poseidon_hash(recipient_pub, nonce)"
  }

def externalChain : PrimitiveType :=
  { name := "ExternalChain"
  -- Was `{Barb.dispatch}` — byte-identical to `contractId`, which made these two "distinct"
  -- primitive types the same behavioural type under `typesDistinct`. An external chain is a
  -- shard (§9.5), not a contract route, so it exhibits `↓shard`.
  , barbs := {Barb.shard}
  , description := "External chain as a shard (Ethereum, Monero, Zcash, Aztec, Litecoin)"
  }

def dleqProof : PrimitiveType :=
  { name := "DLEqProof"
  , barbs := {Barb.prove}
  , description := "Discrete log equality proof for cross-curve ownership verification"
  }

def chainDepositProof : PrimitiveType :=
  { name := "ChainDepositProof"
  , barbs := {Barb.proveInclusion, Barb.verify}
  , description := "Chain-specific deposit inclusion proof (Monero/Zcash/Aztec/Litecommitment)"
  }

def relayerCap : PrimitiveType :=
  { name := "RelayerCapability"
  , barbs := {Barb.spend, Barb.dispatch}
  , description := "Relayer execution authority for guaranteed withdrawals"
  }

/- ==========================================================================
   Part 3c: Complete Primitive Type List
   ==========================================================================
   All 17 primitive types in a single list for exhaustive checking.
-/

def allPrimitiveTypes : List PrimitiveType :=
  [secretKey, publicKey, nullifier, commitment, contractId, assetId, funcId,
   merkleNode, ownedSecretKey, miningRecipient, intentNullifier, bridgeCapNullifier,
   bridgeAddress, externalChain, dleqProof, chainDepositProof, relayerCap]

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

end DarkFi.Capability.Types
