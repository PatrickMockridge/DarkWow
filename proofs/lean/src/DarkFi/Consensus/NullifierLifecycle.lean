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
* **Connected, and this paragraph used to say otherwise**: `Capability/Exercise.lean`'s
  `validExercise`/`consume_is_single_use` is the *same rule* over a different data shape —
  `PublicState.spentNullifiers`, a `List`, on the contract side — and this module is the consensus side
  over the store. It read "*Bridging a `List` and a predicate is a separate unit and nothing consumes it
  yet, so the relation is stated here rather than mechanized*" until 2026-09-24, when the bridging
  section at the foot of this file landed it: `spentSet` is the view between the two vocabularies, and
  the theorems there say the stronger thing — that the two transitions build the same set *and* that the
  agreement between the two states is preserved by them, so it is a relation between the models rather
  than a resemblance between two predicates.
* **Not here**: the coinbase's *value* (that the claim is worth what the emission schedule says). That
  is `SupplyChain.lean` and `OBL-C5`'s territory, and its non-increase clause is checked over a range
  rather than proved — see that row.
-/

import DarkFi.Capability.NativeToken
import DarkFi.Capability.Exercise
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

/- ==========================================================================
   The bridge to the capability side: one rule, two shapes
   ==========================================================================
   `Capability/Exercise.lean` keeps its spent set as a `List NullifierValue` and this module keeps it as
   a `Prop`. The module note above called the relation between them *stated rather than mechanized*.
   These are the two facts that were missing, and the second is the one worth having: not that the two
   predicates look alike, but that the **agreement between the two states is preserved by both
   transitions** — an exercise appends and a spend extends, and they stay the same set as the state
   moves. A change to either rule therefore shows up as a failure here rather than as two documents
   drifting apart.
   ========================================================================== -/

/-- The capability side's spent set as this module's predicate: `PublicState.spentNullifiers` is a
    `List`, `State.spends` is a `Prop`, and this is the function between the two vocabularies. -/
def spentSet (l : List NullifierValue) : SpendStore := fun n => n ∈ l

/-- **The predicate a fold of `spend` builds, read at one nullifier.** Stated for an arbitrary starting
    predicate rather than for `spentSet l`, because the induction that uses it has to generalise the
    accumulator — and that is the whole reason the append theorem below is provable. -/
@[axiom_budget 0]
theorem foldl_spend_iff (p : SpendStore) (ns : List NullifierValue) (m : NullifierValue) :
    (ns.foldl (fun p n => fun m => m = n ∨ p m) p) m ↔ m ∈ ns ∨ p m := by
  induction ns generalizing p with
  | nil => simp
  | cons n t ih =>
    -- `rw` first so the induction hypothesis applies to the accumulator it was stated for, then
    -- normalise the `∨` on the propositional goal that is left — the two in one `simp` set would
    -- reorient the fold's own function and stop `ih` matching (which is how this failed first time).
    rw [List.foldl_cons, ih]
    simp only [List.mem_cons, or_comm, or_assoc, or_left_comm]

/-- **The two constructions of a spend are one function.** `Exercise.applyExercise` *appends* the
    inputs' nullifiers to the list; this module's `spend` *extends* the predicate by one name. Folding
    `spend` over the same nullifiers reproduces the append exactly — which is what "the same rule over a
    different data shape" means once it is mechanized rather than asserted. -/
@[axiom_budget 0]
theorem spentSet_append_foldl (l ns : List NullifierValue) :
    spentSet (l ++ ns) = ns.foldl (fun p n => fun m => m = n ∨ p m) (spentSet l) := by
  funext m
  exact propext (by
    rw [foldl_spend_iff]
    simp only [spentSet, List.mem_append, or_comm])

/-- **The agreement is preserved by both transitions.** If the consensus state's `spends` is the
    capability state's list read through `spentSet`, then folding `spend` over an exercise's inputs
    lands on `applyExercise`'s result — read through `spentSet` again. This is the relation the module
    note said was stated rather than mechanized, and it is stronger than the resemblance it replaces:
    the two models do not merely use similar predicates, they denote the same set *as each transition
    is taken*, so neither can be changed without the other failing here. -/
@[axiom_budget 0]
theorem spend_fold_agrees_with_applyExercise
    (σ : State) (state : PublicState) (h : σ.spends = spentSet state.spentNullifiers)
    (e : Capability.Exercise) :
    (e.inputs.map (fun c => c.nullifier)).foldl (fun p n => fun m => m = n ∨ p m) σ.spends
      = spentSet (Capability.applyExercise state e).spentNullifiers := by
  -- expose the append, fold the list side, then transport the hypothesis in the direction it is
  -- stated (`h` rewrites `σ.spends` into the list's denotation, which is the side to keep)
  simp only [Capability.applyExercise]
  rw [spentSet_append_foldl, h]

/-- **And the two refusals are one fact, in the direction the capability side states it.** After the
    exercise, the consensus side records the input's nullifier as spent — `spend_records` read through
    the bridge — which is the positive form of `Exercise.consume_is_single_use`'s refusal. The
    capability side's theorem is the negative of this, so a reader can now see the two as one rule
    rather than take the resemblance on trust. -/
@[axiom_budget 0]
theorem the_consensus_side_records_the_exercised_nullifier
    (σ : State) (state : PublicState) (h : σ.spends = spentSet state.spentNullifiers)
    (e : Capability.Exercise) (c : Capability.Cap) (h_in : c ∈ e.inputs) :
    spentSet (Capability.applyExercise state e).spentNullifiers c.nullifier := by
  rw [← spend_fold_agrees_with_applyExercise σ state h e]
  exact (foldl_spend_iff _ _ _).mpr
    (Or.inl (List.mem_map_of_mem (f := fun c => c.nullifier) h_in))

end Consensus.NullifierLifecycle
