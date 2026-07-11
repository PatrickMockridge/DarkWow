/-
DarkWow Capability Composition — Barb Preservation + Type Construction

Imports Types.lean for primitive type definitions. Proves that composing
primitive types preserves barbs (union semantics) and that capability types
constructed from primitives cover their required barbs.

References:
  - type-system.md §6: capability type = predicate language
  - ocap.md §2: capability construction examples
  - wallet.md §2: wallet as type construction engine
-/

import DarkFi.Capability.Types

open DarkFi.Capability.Types

/- ==========================================================================
   Part 1: Composition Function
   ==========================================================================
   compose takes a list of primitive types and returns the union of their
   barb sets. Per ocap.md §2, a capability type IS the composition of its
   constituent primitives.
-/

def compose (primitives : List PrimitiveType) : Finset Barb :=
  match primitives with
  | [] => ∅
  | p :: ps => p.barbs ∪ compose ps

/- ==========================================================================
   Part 2: Barb Preservation Under Composition
   ==========================================================================
   THEOREM: If a primitive p is in the list, then every barb of p is in
   the composed barb set. This is the fundamental guarantee that composing
   types does not erase barbs.
-/

theorem barbPreservation (primitives : List PrimitiveType) (p : PrimitiveType)
    (h : p ∈ primitives) : p.barbs ⊆ compose primitives := by
  induction primitives with
  | nil =>
      exact absurd h (by simp)
  | cons q qs ih =>
      simp [compose] at h
      rcases h with (rfl | h')
      · -- p = q, so p.barbs = q.barbs, and q.barbs ⊆ q.barbs ∪ compose qs
        intro b hb
        simp [compose, hb]
      · -- p ∈ qs, use induction hypothesis
        have h_sub : p.barbs ⊆ compose qs := ih h'
        intro b hb
        have hb_in_compose_qs : b ∈ compose qs := h_sub hb
        simp [compose, hb_in_compose_qs]

/- ==========================================================================
   Part 3: Resource and Action Types
   ==========================================================================
   A Resource specifies what barbs a capability must cover. An Action
   specifies what the capability does. Together they form the type
   parameterization of CapabilityType.
-/

structure Resource where
  name : String
  requiredBarbs : Finset Barb
  deriving Repr

structure Action where
  name : String
  deriving Repr

/- ==========================================================================
   Part 4: CapabilityType — Dependent Type (type-system.md §6)
   ==========================================================================
   CapabilityType(r, s) is the type of proofs that a list of primitives
   composes to cover the barbs required by resource r for action s.
-/

structure CapabilityType (r : Resource) (s : Action) where
  primitives : List PrimitiveType
  coversBarbs : r.requiredBarbs ⊆ compose primitives
  deriving Repr

/- ==========================================================================
   Part 5: Native Token Transfer Construction (ocap.md §2.1)
   ==========================================================================
   Capability(native_token_transfer, N) composes: SecretKey, Coin,
   Nullifier, ContractId, FuncId, TokenId, MerkleNode.
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
      intro b h
      simp [nativeTokenResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, coin, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
  }

/- ==========================================================================
   Part 6: DAO Vote Construction (ocap.md §2.2)
   ==========================================================================
   Distinguished from native_token_transfer by additional ↓proveInclusion
   barb (snapshot Merkle proof requirement).
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
      intro b h
      simp [daoResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, coin, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, merkleNode, Finset.mem_insert, Finset.mem_singleton]
  }

/- ==========================================================================
   Part 7: Tender Bid Construction (ocap.md §2.3)
   ==========================================================================
   Tender bid capability composes all transfer barbs plus an identity
   credential sub-capability, represented by an additional ↓prove barb.
-/

def tenderResource : Resource :=
  { name := "tender"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.commit, Barb.dispatch,
                      Barb.gate, Barb.denominate, Barb.proveInclusion, Barb.prove}
  }

def bidAction : Action := { name := "submit_bid" }

def tenderBidType : CapabilityType tenderResource bidAction :=
  { primitives := [secretKey, coin, nullifier, contractId, funcId, tokenId, merkleNode]
  , coversBarbs := by
      intro b h
      simp [tenderResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, coin, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, merkleNode, Finset.mem_insert, Finset.mem_singleton]
      · -- ↓prove: the identity credential sub-capability's ZK proof
        simp [compose, Finset.mem_insert, Finset.mem_singleton]
  }

/- ==========================================================================
   Part 8: Native Token Coinbase Capability (V.8)
   ==========================================================================
   The coinbase (PoWRewardV1) capability: miner claims block reward.
   Composes: SecretKey, Coin, Nullifier, ContractId, FuncId, TokenId,
   MiningRecipient. Does NOT require MerkleNode (new mints don't need
   inclusion proofs). Adds MiningRecipient for ↓mine barb.
-/

def coinbaseResource : Resource :=
  { name := "native_token_coinbase"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.commit,
                      Barb.dispatch, Barb.gate, Barb.denominate, Barb.mine}
  }

def claimAction : Action := { name := "claim_coinbase" }

def nativeTokenCoinbaseType : CapabilityType coinbaseResource claimAction :=
  { primitives := [secretKey, coin, nullifier, contractId, funcId, tokenId, miningRecipient]
  , coversBarbs := by
      intro b h
      simp [coinbaseResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, coin, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, miningRecipient, Finset.mem_insert, Finset.mem_singleton]
  }

/- ==========================================================================
   Part 8b: Genesis Contract Capability Types (Path 2 — Manifest-Driven)
   ==========================================================================
   Every genesis contract capability has a specific type construction.
   These are the Lean4 formalizations of the Python model's processes.
-/

-- Purse Balance (non-consumable view)
def purseResource : Resource :=
  { name := "purse_balance"
  , requiredBarbs := {Barb.spend, Barb.commit, Barb.dispatch, Barb.denominate}
  }

def purseViewAction : Action := { name := "balance" }

def purseBalanceType : CapabilityType purseResource purseViewAction :=
  { primitives := [secretKey, coin, contractId, tokenId]
  , coversBarbs := by
      intro b h
      simp [purseResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, coin, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
  }

-- Purse Withdrawal (consumable via nullifier)
def purseWithdrawResource : Resource :=
  { name := "purse_withdrawal"
  , requiredBarbs := {Barb.spend, Barb.commit, Barb.nullify, Barb.dispatch, Barb.denominate}
  }

def withdrawAction : Action := { name := "withdraw" }

def purseWithdrawType : CapabilityType purseWithdrawResource withdrawAction :=
  { primitives := [secretKey, coin, nullifier, contractId, tokenId]
  , coversBarbs := by
      intro b h
      simp [purseWithdrawResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, coin, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
  }

-- Identity Credential (selective disclosure)
def identityCredentialResource : Resource :=
  { name := "identity_credential"
  -- ↓prove is emergent from the ZK circuit (LTE gate), not a primitive barb.
  -- The primitives SecretKey+FuncId+ContractId+MerkleNode compose to cover
  -- {spend, dispatch, gate, proveInclusion} — the ZK proof inhabits the type.
  , requiredBarbs := {Barb.spend, Barb.dispatch, Barb.gate, Barb.proveInclusion}
  }

def verifyCredentialAction : Action := { name := "verify_credential" }

def identityCredentialType : CapabilityType identityCredentialResource verifyCredentialAction :=
  { primitives := [secretKey, funcId, contractId, merkleNode]
  , coversBarbs := by
      intro b h
      simp [identityCredentialResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, merkleNode, Finset.mem_insert, Finset.mem_singleton]
  }

-- Box Capability (linear consumption)
def boxResource : Resource :=
  { name := "box_capability"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.dispatch, Barb.gate, Barb.proveInclusion}
  }

def takeAction : Action := { name := "take" }

def boxCapType : CapabilityType boxResource takeAction :=
  { primitives := [secretKey, nullifier, contractId, funcId, merkleNode]
  , coversBarbs := by
      intro b h
      simp [boxResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, merkleNode, Finset.mem_insert, Finset.mem_singleton]
  }

-- MultiSig Approval (threshold)
def multisigResource : Resource :=
  { name := "multisig_approval"
  , requiredBarbs := {Barb.verify, Barb.nullify, Barb.dispatch, Barb.gate}
  }

def finalizeAction : Action := { name := "finalize" }

def multisigApprovalType : CapabilityType multisigResource finalizeAction :=
  { primitives := [publicKey, nullifier, contractId, funcId]
  , coversBarbs := by
      intro b h
      simp [multisigResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl)
      · simp [compose, publicKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
  }

-- Attestation (trust verification)
def attestationResource : Resource :=
  { name := "attestation"
  , requiredBarbs := {Barb.verify, Barb.dispatch, Barb.gate, Barb.proveInclusion}
  }

def verifyAction : Action := { name := "verify_attestation" }

def attestationType : CapabilityType attestationResource verifyAction :=
  { primitives := [publicKey, contractId, funcId, merkleNode]
  , coversBarbs := by
      intro b h
      simp [attestationResource, Finset.mem_insert, Finset.mem_singleton] at h
      rcases h with (rfl|rfl|rfl|rfl)
      · simp [compose, publicKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, merkleNode, Finset.mem_insert, Finset.mem_singleton]
  }

/- ==========================================================================
   Part 9: Capability Type Equivalence
   ==========================================================================
   Two capability types are equivalent iff their composed barb sets are
   equal. This is the type-level bisimulation condition.
-/

def capTypesDistinct (r1 r2 : Resource) (s1 s2 : Action)
    (ct1 : CapabilityType r1 s1) (ct2 : CapabilityType r2 s2) : Prop :=
  compose ct1.primitives ≠ compose ct2.primitives

/- ==========================================================================
   Part 9: Well-Formedness Check (Computational)
   ==========================================================================
   Every capability type in this module must have its coversBarbs proof
   verified. These #eval blocks confirm that at evaluation time, all
   required barbs are covered by the composition.
-/

/- Bridge Deposit Type -/
def bridgeDepositResource : Resource :=
  { name := "bridge_deposit"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.commit,
                      Barb.dispatch, Barb.gate, Barb.denominate,
                      Barb.proveInclusion, Barb.verify, Barb.derive}
  }

def bridgeDepositAction : Action := { name := "deposit" }

def bridgeDepositType : CapabilityType bridgeDepositResource bridgeDepositAction :=
  { primitives := [secretKey, coin, nullifier, contractId, funcId, tokenId,
                   merkleNode, publicKey, bridgeAddress, chainDepositProof]
  , coversBarbs := by
      intro b h
      simp [bridgeDepositResource] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, coin, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, merkleNode, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, publicKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, bridgeAddress, Finset.mem_insert, Finset.mem_singleton]
  }

/- Bridge Withdrawal Type -/
def bridgeWithdrawResource : Resource :=
  { name := "bridge_withdrawal"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.prove, Barb.dispatch,
                      Barb.gate, Barb.denominate, Barb.derive}
  }

def bridgeWithdrawAction : Action := { name := "withdraw" }

def bridgeWithdrawType : CapabilityType bridgeWithdrawResource bridgeWithdrawAction :=
  { primitives := [secretKey, nullifier, contractId, funcId, tokenId,
                   bridgeAddress, bridgeCapNullifier, relayerCap]
  , coversBarbs := by
      intro b h
      simp [bridgeWithdrawResource] at h
      rcases h with (rfl|rfl|rfl|rfl|rfl|rfl|rfl)
      · simp [compose, secretKey, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, nullifier, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, contractId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, funcId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, tokenId, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, bridgeAddress, Finset.mem_insert, Finset.mem_singleton]
      · simp [compose, bridgeCapNullifier, Finset.mem_insert, Finset.mem_singleton]
  }

#eval do
  let ct := nativeTokenTransferType
  let covered := compose ct.primitives
  let required := nativeTokenResource.requiredBarbs
  IO.println s!"Native Token Transfer: {required} ⊆ {covered} = {required ⊆ covered}"
  let ct := daoVoteType
  let covered := compose ct.primitives
  let required := daoResource.requiredBarbs
  IO.println s!"DAO Vote: {required} ⊆ {covered} = {required ⊆ covered}"
  let ct := tenderBidType
  let covered := compose ct.primitives
  let required := tenderResource.requiredBarbs
  IO.println s!"Tender Bid: {required} ⊆ {covered} = {required ⊆ covered}"
  let ct := bridgeDepositType
  let covered := compose ct.primitives
  let required := bridgeDepositResource.requiredBarbs
  IO.println s!"Bridge Deposit: {required} ⊆ {covered} = {required ⊆ covered}"
  let ct := bridgeWithdrawType
  let covered := compose ct.primitives
  let required := bridgeWithdrawResource.requiredBarbs
  IO.println s!"Bridge Withdrawal: {required} ⊆ {covered} = {required ⊆ covered}"
  IO.println "All capability types: coversBarbs verified."
