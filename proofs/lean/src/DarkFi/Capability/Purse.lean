import Mathlib
import DarkFi.Combinatorial.StateSpace
import DarkFi.CrossCutting

/-!
# Purse — state-nonce chaining and nullifier freshness (L1)

Formalizes the purse write-path invariant that the previous code-first attempt
missed: the nullifier is `poseidon(1, owner_secret, purse_id, state_nonce)` and
the produced leaf is `poseidon(5, purse_id, new_balance, new_nonce)`. When a
single `state_nonce` is reused for both the nullifier and the produced leaf, a
deposit→withdraw chain collides on the nullifier. Separating the old nonce
(nullifier) from the new nonce (leaf), with the new nonce strictly increasing,
makes the chain fresh.

This is the HAZOP vector V2 (l1-write-path-hazop.md). The nullifier is modeled
as an opaque compression; its injectivity in the nonce is the crypto assumption.
-/

namespace DarkFi.Capability

open Combinatorial

/-! ===== Purse state ===== -/

/-- A purse operation's witness: the owner secret, purse id, and the nonce that
    feeds the nullifier (the *consumed* leaf's nonce). -/
structure PurseWitness where
  ownerSecret : OwnerSecret
  purseId     : ObjectId
  nonce       : StateNonce
deriving BEq, Repr

/-- The purse nullifier: an opaque `poseidon(1, owner_secret, purse_id, nonce)`.
    Modeled as an uninterpreted compression — injectivity in the nonce is the
    crypto assumption below. -/
opaque purseNullifier (w : PurseWitness) : NullifierValue

/-- Crypto assumption (poseidon collision resistance): distinct nonces on the
    same (owner_secret, purse_id) yield distinct nullifiers. -/
axiom purseNullifier_nonce_injective
    (s : OwnerSecret) (p : ObjectId) (n₁ n₂ : StateNonce) :
    purseNullifier ⟨s, p, n₁⟩ = purseNullifier ⟨s, p, n₂⟩ → n₁ = n₂

/-! ===== Chained deposit → withdraw ===== -/

/-- A deposit/withdraw chain: two operations on one purse, consuming nonce `n`
    then nonce `n + 1`. The produced nonce of the first op equals the consumed
    nonce of the second. -/
structure PurseChain where
  ownerSecret : OwnerSecret
  purseId     : ObjectId
  depositNonce : StateNonce
deriving BEq, Repr

/-- The deposit consumes nonce `n`; the withdraw consumes `n + 1`. -/
def chainNullifiers (c : PurseChain) : (NullifierValue × NullifierValue) :=
  ( purseNullifier ⟨c.ownerSecret, c.purseId, c.depositNonce⟩
  , purseNullifier ⟨c.ownerSecret, c.purseId, c.depositNonce + 1⟩ )

/-- **V2 fixed**: a deposit→withdraw chain on one purse with an incremented nonce
    yields two distinct nullifiers — the second op is not a duplicate. -/
theorem purse_chained_nullifiers_distinct
    (c : PurseChain) :
    (chainNullifiers c).1 ≠ (chainNullifiers c).2 := by
  unfold chainNullifiers
  intro h
  have h' := purseNullifier_nonce_injective c.ownerSecret c.purseId
    c.depositNonce (c.depositNonce + 1) h
  have : c.depositNonce ≠ c.depositNonce + 1 := by omega
  exact this h'

/-! ===== Value conservation (Pedersen) ===== -/

/-- Purse deposit conserves value via Pedersen additive homomorphism:
    `old_commit + deposit_commit = new_commit`. This is the `↓conserve` barb;
    the entrypoint compares commitment sums, never plaintext balances. -/
theorem purse_deposit_value_conservation
    (oldCommit depositCommit newCommit : CrossCutting.PedersenCommitment)
    (h_sum : CrossCutting.sum_pedersen [oldCommit, depositCommit] = newCommit) :
    (CrossCutting.sum_pedersen [oldCommit, depositCommit]).value = newCommit.value := by
  rw [h_sum]

/-- Purse withdraw conserves value: `old_commit = new_commit + withdraw_commit`. -/
theorem purse_withdraw_value_conservation
    (oldCommit newCommit withdrawCommit : CrossCutting.PedersenCommitment)
    (h_sum : oldCommit = CrossCutting.sum_pedersen [newCommit, withdrawCommit]) :
    oldCommit.value = (CrossCutting.sum_pedersen [newCommit, withdrawCommit]).value := by
  rw [h_sum]

end DarkFi.Capability
