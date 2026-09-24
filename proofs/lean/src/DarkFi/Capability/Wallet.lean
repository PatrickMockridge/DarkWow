/-
DarkWow Capability Type System — Wallet Construction Soundness

Proves that the wallet's type construction (wallet.md §2) is sound and
complete with respect to the capability calculus.

  Soundness: if walletConstruct returns some ct, then ct is a valid
  CapabilityType (its coversBarbs proof holds).

  Completeness: if a CapabilityType r s exists, then walletConstruct
  with the appropriate primitives and manifest returns some ct.

References:
  - wallet.md §2: scan paths as type construction
  - wallet.md §7: wallet state as pure function
-/

import DarkFi.Capability.Types
import DarkFi.Capability.Composition
import DarkFi.AxiomBudget

open DarkFi.Capability.Types
open DarkFi.Capability.Composition

/- ==========================================================================
   Part 1: Wallet Type Construction Function
   ==========================================================================
   walletConstruct takes a list of discovered primitives and a resource
   manifest, and attempts to construct a CapabilityType. If the composed
   barbs cover the required barbs, construction succeeds. Otherwise, it
   returns none (the primitives don't form a capability for this resource).

   This matches the wallet's scan path: AEAD decrypt → discover primitives
   → read manifest → construct capability type.
-/

def walletConstruct (primitives : List PrimitiveType) (r : Resource) (s : Action)
    : Option (CapabilityType r s) :=
  if h : r.requiredBarbs ⊆ compose primitives then
    some { primitives := primitives, coversBarbs := h }
  else
    none

/- ==========================================================================
   Part 2: Soundness
   ==========================================================================
   THEOREM: If walletConstruct returns some ct, then ct is a valid
   CapabilityType (the coversBarbs proof is the h that was checked).

   Proof: walletConstruct only returns some when the subset check passes.
-/

/-- **What this proves, and what it does not.** `walletConstruct` is
    `if h : r.requiredBarbs ⊆ compose primitives then some { primitives, coversBarbs := h }
    else none`, so this theorem unfolds that `if` and hands back the branch's own hypothesis. Its
    content is the *definition*: it says the constructor stores the coverage proof it checked.
    That is worth having a name for — `wallet.md` §6.2's write path cites the read path's coverage
    guarantee by it — but it is not evidence that the wallet's construction is sound beyond barb
    coverage, because barb coverage is all a `CapabilityType` carries. The genuinely conditional
    statement is the negative one, `walletConstruct_rejects_emptyPrimitives` below. -/
@[axiom_budget 0]
theorem walletConstruct_sound (primitives : List PrimitiveType) (r : Resource) (s : Action)
    (ct : CapabilityType r s) (h_ret : walletConstruct primitives r s = some ct) :
    r.requiredBarbs ⊆ compose primitives := by
  unfold walletConstruct at h_ret
  split at h_ret
  · -- The split hypothesis IS the goal — `walletConstruct` only returns `some ct` in the branch
    -- where `requiredBarbs ⊆ compose primitives` holds, so no further work is needed.
    -- (`exact ct.coversBarbs` used to sit here and reported "no goals to be solved": the goal was
    -- already closed.)
    assumption
  · -- case ¬h: contradiction (walletConstruct would return none)
    injection h_ret

/- ==========================================================================
   Part 3: Completeness
   ==========================================================================
   THEOREM: If a CapabilityType r s exists with primitives p, then
   walletConstruct p r s returns some ct (not none).

   Proof: Since ct.coversBarbs proves requiredBarbs ⊆ compose primitives,
   the condition in walletConstruct evaluates to true, and it returns some.
-/

@[axiom_budget 0]
theorem walletConstruct_complete (primitives : List PrimitiveType) (r : Resource) (s : Action)
    (ct : CapabilityType r s) (h_prims : ct.primitives = primitives) :
    walletConstruct primitives r s ≠ none := by
  unfold walletConstruct
  have h_covers : r.requiredBarbs ⊆ compose primitives := by
    rw [← h_prims]
    exact ct.coversBarbs
  simp [h_covers]

/- ==========================================================================
   Part 4: Construction Preserves Primitives
   ==========================================================================
   THEOREM: The primitives returned by walletConstruct are exactly the
   primitives passed in (no loss, no modification).
-/

@[axiom_budget 0]
theorem walletConstruct_preservesPrimitives (primitives : List PrimitiveType)
    (r : Resource) (s : Action) (ct : CapabilityType r s)
    (h_ret : walletConstruct primitives r s = some ct) :
    ct.primitives = primitives := by
  unfold walletConstruct at h_ret
  split at h_ret
  · injection h_ret with h_inj
    -- `h_inj : { primitives := primitives, … } = ct`, and the goal is about `ct.primitives` — so the
    -- rewrite has to go right-to-left. `rw [h_inj]` looked for the anonymous-structure form in a
    -- goal that mentions `ct`, and found nothing.
    rw [← h_inj]
  · injection h_ret

/- ==========================================================================
   Part 5: Construction Is Deterministic
   ==========================================================================
   THEOREM: Given the same primitives and resource, walletConstruct
   always returns the same result (pure function property, per wallet.md §1).
-/

/-- **Not the wallet's purity property, whatever §7.4 calls it.** The statement is
    `f x = some ct1 → f x = some ct2 → ct1 = ct2` for a three-argument `def`: true of every
    function, and closed by rewriting with `h1` and injecting. `wallet.md` §7.4 presents it as "the
    type-level expression of the wallet's pure function property (§1)", and §1's property is not
    this one — `walletConstruct` takes neither a `Seed` nor any wallet state, so nothing here says
    an exercise is byte-deterministic. §1's `WalletState = f(AccountManager, ChainBlocks)`, §6.1's
    `Transaction = f(SelectedCapabilities, Action, Params, Secrets, Seed)` and §0.1.5's seed rule
    are the purity claims, and §7.8 records the latter two as un-discharged. -/
@[axiom_budget 0]
theorem walletConstruct_deterministic (primitives : List PrimitiveType)
    (r : Resource) (s : Action) (ct1 ct2 : CapabilityType r s)
    (h1 : walletConstruct primitives r s = some ct1)
    (h2 : walletConstruct primitives r s = some ct2) :
    ct1 = ct2 := by
  rw [h1] at h2
  injection h2
  -- CapabilityType equality follows from primitives equality
  -- (both are constructed from the same primitives with the same proof)

/- ==========================================================================
   Part 6: Idempotence — Wallet State as Pure Function
   ==========================================================================
   Per wallet.md §1: WalletState = f(AccountManager, ChainBlocks).
   The wallet is a pure function — given the same inputs, it produces
   byte-identical state.

   At the type level: repeated construction from the same primitives
   and manifest always yields the same CapabilityType.
-/

/-
## `walletConstruct_idempotent` — deleted

    theorem walletConstruct_idempotent (primitives : List PrimitiveType)
        (r : Resource) (s : Action) :
        walletConstruct primitives r s = walletConstruct primitives r s := by rfl

which is `x = x`. Two documents cite the name for content it did not have:
`doc/src/arch/wallet.md:1009` ("repeated construction from the same primitives and manifest always
yields the same CapabilityType") and `doc/src/arch/type-system.md:1701` ("constructing twice with
identical arguments"). Both are statements about determinism, and the *actual* determinism content
is that `walletConstruct` is a `def` — a function, hence equal on equal arguments by `rfl` for
every caller, and there is nothing to prove. The citations are corrected to say that. A theorem
whose whole proof is `rfl` on a definition adds no fact to the tree; leaving the name in place made
the two citations read as evidence.
-/

/- ==========================================================================
   Part 7: Construction Examples (Concrete Verification)
   ==========================================================================
   Verify that the three capability types defined in Composition.lean are
   constructible via walletConstruct from their respective primitives.
-/

@[axiom_budget 0]
theorem nativeTokenTransfer_constructible :
    walletConstruct [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode]
      nativeTokenResource transferAction ≠ none := by
  apply walletConstruct_complete
    [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode]
    nativeTokenResource transferAction nativeTokenTransferType rfl

@[axiom_budget 0]
theorem daoVote_constructible :
    walletConstruct [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode]
      daoResource voteAction ≠ none := by
  apply walletConstruct_complete
    [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode]
    daoResource voteAction daoVoteType rfl

/-- **Fixed: this was false.** It passed seven primitives for a resource that needs eight.

    `tenderResource.requiredBarbs` is `{↓spend, ↓nullify, ↓commit, ↓dispatch, ↓gate,
    ↓denominate, ↓prove-inclusion, ↓prove}`, and the seven-element list that used to be here
    supplies all of those *except* `↓prove` — which only `dleqProof` carries. So
    `walletConstruct` returned `none` on that list, and the claim `… ≠ none` was false; the
    `rfl` argument exposed it because `tenderBidType.primitives` is the eight-element list and
    was not defeq to the seven-element one. The list now matches `tenderBidType`. -/
@[axiom_budget 0]
theorem tenderBid_constructible :
    walletConstruct
      [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode, dleqProof]
      tenderResource bidAction ≠ none := by
  apply walletConstruct_complete
    [secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode, dleqProof]
    tenderResource bidAction tenderBidType rfl

@[axiom_budget 0]
theorem coinbaseClaim_constructible :
    walletConstruct [secretKey, commitment, nullifier, contractId, funcId, assetId, miningRecipient]
      coinbaseResource claimAction ≠ none := by
  apply walletConstruct_complete
    [secretKey, commitment, nullifier, contractId, funcId, assetId, miningRecipient]
    coinbaseResource claimAction nativeTokenCoinbaseType rfl

/- ==========================================================================
   Part 8: Failure Cases — When Construction Rightly Fails
   ==========================================================================
   If the primitives don't cover the required barbs, walletConstruct
   returns none. This is the correct behavior — the wallet cannot
   construct a capability type from insufficient primitives.
-/

@[axiom_budget 0]
theorem walletConstruct_rejects_emptyPrimitives (r : Resource) (s : Action)
    (h_req : r.requiredBarbs ≠ ∅) :
    walletConstruct [] r s = none := by
  unfold walletConstruct
  have h_no_cover : ¬ (r.requiredBarbs ⊆ compose []) := by
    simp only [compose, List.nil]
    -- `h_req : r.requiredBarbs ≠ ∅` and the goal is `¬ r.requiredBarbs ⊆ ∅`; `Finset.subset_empty`
    -- is the bridge. `exact h_req` reported a type mismatch because the two are equivalent but not
    -- syntactically the same proposition.
    intro hsub
    exact h_req (Finset.subset_empty.mp hsub)
  simp [h_no_cover]
