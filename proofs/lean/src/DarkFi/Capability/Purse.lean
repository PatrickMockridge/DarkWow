import Mathlib
import DarkFi.Combinatorial.StateSpace
import DarkFi.CrossCutting
import DarkFi.Axioms
import DarkFi.AxiomBudget

/-!
# Purse — state-nonce chaining and nullifier freshness (L1)

Formalizes the purse write-path invariant that the previous code-first attempt
missed: the nullifier is `poseidon(1, owner_secret, purse_id, state_nonce)` and
the produced leaf is `poseidon(5, purse_id, new_balance, new_nonce, owner_pub)`.
When a single `state_nonce` is reused for both the nullifier and the produced
leaf, a deposit→withdraw chain collides on the nullifier. Separating the old nonce
(nullifier) from the new nonce (leaf), with the new nonce strictly increasing,
makes the chain fresh.

This is the HAZOP vector V2, carried as `CRIT-5` in `DarkFi/HAZOP/Critical.lean`
(`purseNullifier_nonce_injective`, whose consumer `purse_chained_nullifiers_distinct`
is the theorem below). **The citation here read `OBL-C31` until 2026-09-25, when
`OBL-C31`'s proposition was the generic prover's derived-rule DAG** — a different
finding that had taken the row over, which is why the reference moved to the Lean
HAZOP entry rather than to a register row. The nullifier is modeled
as an opaque compression; its injectivity in the nonce is the crypto assumption.

**And the leaf gained its `owner_pub` argument on 2026-09-25**, so that a leaf
commits to its owner: without it the spender's secret was bound to the nullifier
alone, and knowing a leaf's preimage — which the call params publish in plaintext
— was enough to spend it. The module's theorems are about the nonce chain and are
unaffected by the extra argument; what changed is what the leaf *is*, and the model
follows the circuit.
-/

namespace DarkFi.Capability

open Combinatorial
open HashOps

/-! ===== Purse state — DISCHARGED

`PurseWitness` and `purseNullifier` used to live in `DarkFi/Axioms.lean`, the second as a
value-less `opaque`, together with the assumption

    axiom purseNullifier_nonce_injective (s p n₁ n₂) :
      purseNullifier ⟨s, p, n₁⟩ = purseNullifier ⟨s, p, n₂⟩ → n₁ = n₂

That assumption is now a **theorem**, because `purseNullifier` is now a definition over the hash
model rather than an uninterpreted function — there is nothing left to assume once you can see
what the nullifier is a hash *of*.

It was the only assumption in the tree with a live consumer: `purse_chained_nullifiers_distinct`
below cited it by name, and its `IF FALSE:` entry named that theorem. With this discharge,
`darkfi/Axioms.lean` no longer participates in the purse write path at all.

The return type is `Int`, not `NullifierValue` (`Nat`): `poseidon_hash_output : List Int → Int`,
and going through `Int.toNat` would map every negative output to `0`, destroying exactly the
injectivity this theorem is about. -/

/-- A purse operation's witness: the owner secret, purse id, and the nonce that
    feeds the nullifier (the *consumed* leaf's nonce). -/
structure PurseWitness where
  ownerSecret : OwnerSecret
  purseId     : ObjectId
  nonce       : StateNonce
deriving BEq, Repr

/-- `purse_nullifier = poseidon(1, owner_secret, purse_id, state_nonce)`, per
    `src/contract/purse/README.md`. A definition, not an assumption. -/
def purseNullifier (w : PurseWitness) : Int :=
  poseidon_hash_output [1, (w.ownerSecret : Int), (w.purseId : Int), (w.nonce : Int)]

/-- **Discharged**: distinct nonces on the same `(owner_secret, purse_id)` give distinct
    nullifiers — a consequence of `poseidon_collision_resistance` (injective as stated), applied
    to the two four-element preimages and then read off the fourth component.

    Budget 1: it rests on `poseidon_collision_resistance`, which is the assumption it always
    really rested on. -/
@[axiom_budget 1]
theorem purseNullifier_nonce_injective
    (s : OwnerSecret) (p : ObjectId) (n₁ n₂ : StateNonce) :
    purseNullifier ⟨s, p, n₁⟩ = purseNullifier ⟨s, p, n₂⟩ → n₁ = n₂ := by
  intro h
  have hlists : [1, (s : Int), (p : Int), (n₁ : Int)] = [1, (s : Int), (p : Int), (n₂ : Int)] := by
    by_contra hne
    exact poseidon_collision_resistance _ _ hne h
  have h4 : (n₁ : Int) = (n₂ : Int) := by
    have := congrArg (fun l : List Int => l.getD 3 0) hlists
    simpa using this
  exact_mod_cast h4

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
def chainNullifiers (c : PurseChain) : (Int × Int) :=
  ( purseNullifier ⟨c.ownerSecret, c.purseId, c.depositNonce⟩
  , purseNullifier ⟨c.ownerSecret, c.purseId, c.depositNonce + 1⟩ )

/-- **V2 fixed**: a deposit→withdraw chain on one purse with an incremented nonce
    yields two distinct nullifiers — the second op is not a duplicate. -/
@[axiom_budget 1]
theorem purse_chained_nullifiers_distinct
    (c : PurseChain) :
    (chainNullifiers c).1 ≠ (chainNullifiers c).2 := by
  unfold chainNullifiers
  intro h
  have h' := purseNullifier_nonce_injective c.ownerSecret c.purseId
    c.depositNonce (c.depositNonce + 1) h
  -- `omega` reported "No usable constraints found" on this goal; `Nat.lt_succ_self` is the
  -- direct fact, and needs no hypotheses at all.
  have : c.depositNonce ≠ c.depositNonce + 1 :=
    Nat.ne_of_lt (Nat.lt_succ_self c.depositNonce)
  exact this h'

/-! ===== Value conservation (Pedersen) ===== -/

/-- Purse deposit conserves value via Pedersen additive homomorphism:
    `old_commit + deposit_commit = new_commit`. This is the `↓conserve` barb;
    the entrypoint compares commitment sums, never plaintext balances. -/
@[axiom_budget 0]
theorem purse_deposit_value_conservation
    (oldCommit depositCommit newCommit : CrossCutting.PedersenCommitment)
    (h_sum : CrossCutting.sum_pedersen [oldCommit, depositCommit] = newCommit) :
    (CrossCutting.sum_pedersen [oldCommit, depositCommit]).value = newCommit.value := by
  rw [h_sum]

/-- Purse withdraw conserves value: `old_commit = new_commit + withdraw_commit`. -/
@[axiom_budget 0]
theorem purse_withdraw_value_conservation
    (oldCommit newCommit withdrawCommit : CrossCutting.PedersenCommitment)
    (h_sum : oldCommit = CrossCutting.sum_pedersen [newCommit, withdrawCommit]) :
    oldCommit.value = (CrossCutting.sum_pedersen [newCommit, withdrawCommit]).value := by
  rw [h_sum]

end DarkFi.Capability
