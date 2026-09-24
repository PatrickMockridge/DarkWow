/-
# The nullifier lifecycle — claim, maturity, spend, and the replay refusal

Three pieces of this mechanism were already in the tree and none of them was connected to the others:
`Combinatorial/NullifierStorage.lean` has the store and its laws, `Capability/NativeToken.lean` has the
maturity gate (`COINBASE_MATURITY := 100`, with `immature_coinbase_rejected` and
`mature_coinbase_accepted`), and the consensus-side replay rule — the thing
`chain_state.rs`'s `connect_block` enforces when it refuses a duplicate spend — had no model at all.
This module is the connection, and it is a small one because the pieces are right: a **claim** recorded
with the height it was created at, a spend **allowed only once the claim is mature**, and a **record of
the spend** that refuses a second one.

## The state is two maps, and that is forced rather than chosen

The lifecycle is one mechanism, but its *state* cannot be one store: a spend must not destroy the
height the maturity rule reads. `spend_preserves_claims` below is that fact as a theorem — with the
claim store and the spend set separate, marking a spend leaves the height intact; collapsed into one
`nullifier → value` map it would overwrite it, and the claim's age would become unreadable the moment it
was used.

The Rust has the same split, and for the same reason: in memory `nullifier_set` is a
`BTreeMap<Nullifier, BlockHeight>` and `spent_nullifiers` is a `BTreeSet<Nullifier>`
(`chain_state.rs`), and on disk both live in one sled tree **tagged by kind** —
`src/linear/src/store.rs`'s comment is explicit that the value is `[kind] ++ height.to_le_bytes()`, kind
`0` for a claim and `1` for a spend. So the tag is exactly what buys back the distinction that one
untagged map cannot hold, and the Lean model mirrors the in-memory shape rather than the on-disk one.

## What is connected, and what is deliberately not

* **Connected**: `Capability/maturityGate` is *used*, not restated — `matureAt` feeds it the
  `CoinbaseClaim` that the store's height implies, which is the gap the map recorded ("its `createdAt`
  is supplied rather than derived from the commitment set"). The gate is now supplied.
* **Not connected**: `Capability/Exercise.lean`'s `validExercise`/`consume_is_single_use` is the *same
  rule* over a different data shape — `PublicState.spentNullifiers`, a `List`, on the contract side —
  and this module is the consensus side over the store. Bridging a `List` and a predicate is a separate
  unit and nothing consumes it yet, so the relation is stated here rather than mechanized.
* **Not here**: the coinbase's *value* (that the claim is worth what the emission schedule says). That
  is `SupplyChain.lean` and `OBL-C5`'s territory, and its non-increase clause is checked over a range
  rather than proved — see that row.
-/

import DarkFi.Capability.NativeToken
import DarkFi.Combinatorial.NullifierStorage
import DarkFi.AxiomBudget

namespace Consensus.NullifierLifecycle

open Combinatorial

/-- The claim store: a nullifier's **creation height**, or `none` if nothing was ever claimed under it.
    The height is what the maturity rule needs and what a spend must not destroy. -/
abbrev ClaimStore := NullifierValue → Option Nat

/-- The spend store: the nullifiers already spent. A predicate rather than a list, so that the replay
    rule is a *refusal* and not a search. -/
abbrev SpendStore := NullifierValue → Prop

/-- The consensus-side state of the nullifier lifecycle: the claims and the spends. Two maps, for the
    reason the module note gives. -/
structure State where
  claims : ClaimStore
  spends : SpendStore

/-- The state before anything happened. -/
def empty : State := ⟨fun _ => none, fun _ => False⟩

/-- `claimed σ n`: a claim under `n` exists, at some height. -/
def claimed (σ : State) (n : NullifierValue) : Prop := ∃ h, σ.claims n = some h

/-- **Maturity, read off the store rather than supplied.** The height comes from the claim record and
    the gate is `Capability`'s own — so this is the connection between the maturity rule and the
    nullifier state, and the claim's age is a fact about the store rather than a parameter. -/
def matureAt (σ : State) (current : Nat) (n : NullifierValue) : Prop :=
  ∃ h, σ.claims n = some h ∧ Capability.maturityGate current { nullifier := n, createdAt := h }

/-- **A spend is allowed exactly when the claim is mature and unspent.** The two refusals the consensus
    rule can give, as one predicate: the maturity gate's, and the replay gate's. -/
def canSpendAt (σ : State) (current : Nat) (n : NullifierValue) : Prop :=
  matureAt σ current n ∧ ¬ σ.spends n

/-- Record a claim at height `h`. -/
def record (σ : State) (n : NullifierValue) (h : Nat) : State :=
  ⟨fun m => if m = n then some h else σ.claims m, σ.spends⟩

/-- Spend: record the nullifier as spent. The check is `canSpendAt`, carried by the caller — a
    definition that cannot be called without one is the shape this layer prefers, and it keeps the
    refusal itself out of the data. -/
def spend (σ : State) (n : NullifierValue) : State :=
  ⟨σ.claims, fun m => m = n ∨ σ.spends m⟩

/-! ===== The lifecycle's laws ===== -/

/-- **An immature claim cannot be spent**, and this is the maturity gate doing the refusing: the
    hypothesis is exactly the gate's own negation. -/
@[axiom_budget 0]
theorem immature_not_spendable (σ : State) (current : Nat) (n : NullifierValue)
    (h : ¬ matureAt σ current n) : ¬ canSpendAt σ current n :=
  fun hc => h hc.1

/-- **A spend is recorded.** The replay gate's input, and the reason `canSpendAt` can refuse
    afterwards. -/
@[axiom_budget 0]
theorem spend_records (σ : State) (n : NullifierValue) : (spend σ n).spends n :=
  Or.inl rfl

/-- **A second spend is refused**, by the replay gate rather than the maturity one — the two refusals
    are distinct and this is the one `chain_state.rs`'s duplicate check is. -/
@[axiom_budget 0]
theorem respend_refused (σ : State) (current : Nat) (n : NullifierValue) :
    ¬ canSpendAt (spend σ n) current n :=
  fun hc => hc.2 (spend_records σ n)

/-- **A spend does not destroy the claim's height**, which is why the state is two maps. Collapsed into
    one `nullifier → value` store this would be false, and the maturity rule would lose its input at
    exactly the moment the claim became usable. -/
@[axiom_budget 0]
theorem spend_preserves_claims (σ : State) (n : NullifierValue) :
    (spend σ n).claims = σ.claims := rfl

/-- **Maturity is monotone in the height**, so a claim that can be spent now can be spent later: the
    gate's condition only gets easier. -/
@[axiom_budget 0]
theorem maturity_monotone (σ : State) (n : NullifierValue) {current later : Nat}
    (h : current ≤ later) (hm : matureAt σ current n) : matureAt σ later n := by
  obtain ⟨h₀, hc, hg⟩ := hm
  exact ⟨h₀, hc, by unfold Capability.maturityGate Capability.coinbaseMature at hg ⊢; omega⟩

/-- One nullifier's spend does not touch another's. -/
@[axiom_budget 0]
theorem other_spends_preserved (σ : State) (n m : NullifierValue) (h : σ.spends m) :
    (spend σ n).spends m :=
  Or.inr h

/-! ===== Non-vacuity: the four-step lifecycle, at concrete heights =====

The map's witness, and the reason it is stated rather than described: each step of the lifecycle is a
different *refusal or permission*, and a model in which they collapse is a model nothing constrains.
The claim is made at height `0` and the block is at `50` (immature: `0 + 100 > 50`), then `100`
(mature), then spent, then refused. -/

@[axiom_budget 0]
theorem lifecycle_witness :
    ∃ n : NullifierValue,
      ¬ canSpendAt (record empty n 0) 50 n ∧
      canSpendAt (record empty n 0) 100 n ∧
      (spend (record empty n 0) n).spends n ∧
      ¬ canSpendAt (spend (record empty n 0) n) 100 n := by
  refine ⟨0, ?_, ?_, ?_, ?_⟩
  · intro hc
    obtain ⟨h, hc', hg⟩ := hc.1
    simp only [record, empty, if_true, Option.some.injEq] at hc'
    subst hc'
    simp only [Capability.maturityGate, Capability.coinbaseMature,
      Capability.COINBASE_MATURITY] at hg
    omega
  · refine ⟨⟨0, ?_, ?_⟩, ?_⟩
    · simp [record, empty]
    · simp [Capability.maturityGate, Capability.coinbaseMature, Capability.COINBASE_MATURITY]
    · simp [record, empty]
  · exact spend_records _ _
  · intro hc
    obtain ⟨_, hspent⟩ := hc
    exact hspent (Or.inl rfl)

end Consensus.NullifierLifecycle
