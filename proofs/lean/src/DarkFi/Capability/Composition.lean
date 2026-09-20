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
import DarkFi.AxiomBudget

open DarkFi.Capability.Types

/-! ## Namespace

Declared into `DarkFi.Capability.Composition`, which `Inversion.lean`, `Wallet.lean` and
`Main.lean` already `open`.

At top level this file's `Action` collided with **Mathlib's categorical `Action`**
(`(V : Type u) → [LargeCategory V] → MonCat → Type u`), and since Mathlib is imported
transitively, every reference to the capability type system's own `Action` resolved to
Mathlib's — 45 compile errors from one shadowed name. `Resource` and `CapabilityType` were not
colliding, but they belong in the same namespace as the types they are used with. -/

namespace DarkFi.Capability.Composition

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

@[axiom_budget 0]
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

/- ==========================================================================
   Part 5: Native Token Transfer Construction (ocap.md §2.1)
   ==========================================================================
   Capability(native_token_transfer, N) composes: SecretKey, Commitment,
   Nullifier, ContractId, FuncId, AssetId, MerkleNode.
-/

def nativeTokenResource : Resource :=
  { name := "native_token"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.commit,
                      Barb.dispatch, Barb.gate, Barb.denominate}
  }

def transferAction : Action := { name := "transfer" }

def nativeTokenTransferType : CapabilityType nativeTokenResource transferAction :=
  { primitives := [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode]
  , coversBarbs := by decide
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
  { primitives := [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode]
  , coversBarbs := by decide
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
  { primitives := [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode, dleqProof]
  , coversBarbs := by decide
  }

/- ==========================================================================
   Part 8: Native Token Coinbase Capability (V.8)
   ==========================================================================
   The coinbase (PoWRewardV1) capability: miner claims block reward.
   Composes: SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId,
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
  { primitives := [secretKey, commitment, nullifier, contractId, funcId, assetId, miningRecipient]
  , coversBarbs := by decide
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
  { primitives := [secretKey, commitment, contractId, assetId]
  , coversBarbs := by decide
  }

-- Purse Withdrawal (consumable via nullifier)
def purseWithdrawResource : Resource :=
  { name := "purse_withdrawal"
  , requiredBarbs := {Barb.spend, Barb.commit, Barb.nullify, Barb.dispatch, Barb.denominate}
  }

def withdrawAction : Action := { name := "withdraw" }

def purseWithdrawType : CapabilityType purseWithdrawResource withdrawAction :=
  { primitives := [secretKey, commitment, nullifier, contractId, assetId]
  , coversBarbs := by decide
  }

-- Purse Deposit (consumable via nullifier, identical barbs to Withdraw)
def purseDepositResource : Resource :=
  { name := "purse_deposit"
  , requiredBarbs := {Barb.spend, Barb.commit, Barb.nullify, Barb.dispatch, Barb.denominate}
  }

def depositAction : Action := { name := "deposit" }

def purseDepositType : CapabilityType purseDepositResource depositAction :=
  { primitives := [secretKey, commitment, nullifier, contractId, assetId]
  , coversBarbs := by decide
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
  , coversBarbs := by decide
  }

-- Box Capability (linear consumption)
def boxResource : Resource :=
  { name := "box_capability"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.dispatch, Barb.gate, Barb.proveInclusion}
  }

def takeAction : Action := { name := "take" }

def boxCapType : CapabilityType boxResource takeAction :=
  { primitives := [secretKey, nullifier, contractId, funcId, merkleNode]
  , coversBarbs := by decide
  }

-- MultiSig Approval (threshold)
def multisigResource : Resource :=
  { name := "multisig_approval"
  , requiredBarbs := {Barb.verify, Barb.nullify, Barb.dispatch, Barb.gate}
  }

def finalizeAction : Action := { name := "finalize" }

def multisigApprovalType : CapabilityType multisigResource finalizeAction :=
  { primitives := [publicKey, nullifier, contractId, funcId]
  , coversBarbs := by decide
  }

-- Attestation (trust verification)
def attestationResource : Resource :=
  { name := "attestation"
  , requiredBarbs := {Barb.verify, Barb.dispatch, Barb.gate, Barb.proveInclusion}
  }

def verifyAction : Action := { name := "verify_attestation" }

def attestationType : CapabilityType attestationResource verifyAction :=
  { primitives := [publicKey, contractId, funcId, merkleNode]
  , coversBarbs := by decide
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
  { primitives := [secretKey, commitment, nullifier, contractId, funcId, assetId,
                   merkleNode, publicKey, bridgeAddress, chainDepositProof]
  , coversBarbs := by decide
  }

/- Bridge Withdrawal Type -/
def bridgeWithdrawResource : Resource :=
  { name := "bridge_withdrawal"
  , requiredBarbs := {Barb.spend, Barb.nullify, Barb.prove, Barb.dispatch,
                      Barb.gate, Barb.denominate, Barb.derive}
  }

def bridgeWithdrawAction : Action := { name := "withdraw" }

def bridgeWithdrawType : CapabilityType bridgeWithdrawResource bridgeWithdrawAction :=
  { primitives := [secretKey, nullifier, contractId, funcId, assetId,
                   bridgeAddress, bridgeCapNullifier, dleqProof]
  , coversBarbs := by decide
  }

/- ==========================================================================
   The `#eval` well-formedness check — removed
   ==========================================================================
   A `#eval` block used to run here, printing five lines of the form

     Native Token Transfer: {spend, nullify, …} ⊆ {…} = true
     …
     All capability types: coversBarbs verified.

   `proofs/lean/README.md` cited that output as evidence, and `src/Main.lean`
   printed the same lines again.

   It is removed because it is no longer evidence of anything. Every one of the
   twelve capability types now carries `coversBarbs := by decide`, which is a
   *kernel-checked proof* of exactly the proposition the block was computing at
   run time — so the printout stated a weaker thing (a `Bool` that could have
   been `false`) than the term beside it, and a reader had to trust the `#eval`
   output instead of the type checker. The block also never worked as written:
   interpolating a `Finset Barb` needs `ToString (Finset Barb)`, which `Barb`
   does not provide, so it was 15 of this file's 45 compile errors.

   `Main.lean` still prints the same summary. That is tracked separately: a
   hand-printed summary is a claim, and these are now proofs.
   ========================================================================== -/

/- ==========================================================================
   Part 7: The oracle operator's capability — commitment + nullifier
   ==========================================================================
   `oracle/proof/{push_value,attest_value,push_value_commitment,aggregate}.zk` authorize nobody,
   and the reason is a vocabulary failure before it is a circuit failure. Each circuit does

       oracle_pub    = ec_mul_base(oracle_secret, NULLIFIER_K);
       derived_pub_x = ec_get_x(oracle_pub);
       constrain_equal_base(derived_pub_x, oracle_pub_x);   -- witness == witness
       constrain_instance(oracle_id);                        -- the key is never exposed

   so the equality holds for *any* secret and the proof asserts only "the prover knows some curve
   secret". The obvious repair — expose the key so the host can compare it to the registered one —
   is **wrong for this project**: addresses are cycled per transaction
   (`derive_instance(secret, contract_id, instance)`, `src/sdk/src/crypto/keypair.rs:202-222`) and a
   static address is the anti-pattern, so disclosing a registered key in every operation would
   trade an authorization failure for a correlation failure.

   The chosen remedy is **commitment + nullifier**, expressed here as a capability type so the type
   system's coverage rule carries it:

   * the oracle registers a *hiding commitment* to its secret rather than a public key — nothing
     static is disclosed, and a non-operator cannot open it;
   * every push or attestation proves knowledge of the opening in-circuit and consumes a
     per-operation nullifier that the host checks unspent, exactly as `native_token/proof/burn.zk`
     does for a coin;
   * the barbs below are what the operation exhibits, so the same `coversBarbs` obligation the other
     twelve types carry applies to this one.

   This is the *type-level* half. The circuit and entrypoint changes it describes are not made. -/

/-- The oracle operator's capability: commit to the operator secret at registration, prove the
    opening and consume a nullifier on each operation, and route the call to the oracle contract. -/
def oracleResource : Resource :=
  { name := "oracle_operator"
  , requiredBarbs := {Barb.commit, Barb.nullify, Barb.prove, Barb.dispatch}
  }

def pushValueAction : Action := { name := "oracle_push_value" }

/-- `commitment` = `↓commit`, `nullifier` = `↓nullify`, `dleqProof` = `↓prove`,
    `contractId` = `↓dispatch` — the four barbs the resource requires, and no others. -/
def oracleOperatorType : CapabilityType oracleResource pushValueAction :=
  { primitives := [commitment, nullifier, dleqProof, contractId]
  , coversBarbs := by decide
  }

end DarkFi.Capability.Composition
