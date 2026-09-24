/-
# The write set, the overlay diff, and why disjoint calls commute

`Capability/Concurrency.lean` deleted a parallel-composition layer whose central record was
`KeyDisjoint`, carrying `writeSetDisjoint : Bool` — a field nothing read and nothing constrained. It
left a forward reference rather than a silence: the replacement is `writeSet : Key → Prop` with the
disjointness lemma *proved* rather than assumed, and it names this module as where that lives. This
is that module. `type-system.md` §9.2 states the theorem it is for.

## What is mechanized

A key is a byte string and a store is a partial map from keys to byte strings, so `Store` is what
`sled-overlay`'s `SledTreeOverlayState` is: `none` means the tree does not have this key and `some b`
is the cached value — which keeps `some []` distinct from `none`. A `Diff` is one call's overlay
delta, with the same three cases the Rust's two-field state has: untouched, removed, written. A
`CallJob` is a call's *declared* write set together with its diff and an invariant tying them:
everything the diff touches is declared.

The chain, and each link's consumer:

* `diff_apply_other` and `diff_apply_self` — touching a key the diff does not mention leaves the store
  alone; touching one it does mention yields the recorded value. This is
  `Combinatorial/NullifierStorage.lean`'s `mark_other`/`mark_self` generalised from one key to a whole
  write set, and it is the reason that module's template is the one this file follows.
* `Diff.apply_comm` — two diffs commute on a store when their touched sets are disjoint. The case
  analysis is pointwise and splits only on `Option` constructors: the `none` branches close by `getD`'s
  reduction and the both-`some` branch closes by the disjointness, never by comparing values.
* `CallJob.act_comm` — the same for calls, through `writeSet_contains`, which is that invariant's
  consumer. A `Bool` with no reader became a `Prop` with a proof obligation, and this is the proof that
  uses it.
* `exec_perm` — every execution order of a list of pairwise-disjoint calls yields the same store. And
  `no_duplicate_of_pairwiseDisjoint` — the duplicate-key check cannot observe a collision under the
  same hypothesis, which is §9.2's "if a key collision is detected, the parallel composition is NOT
  bisimilar to sequential execution, and the block SHALL be rejected", read as an obligation about the
  check rather than about the composition.

Sets are predicates throughout, as in `Combinatorial/NullifierStorage.lean`: `Disjoint A B` is
`∀ k, A k → B k → False`, with no `Finset`, `Multiset` or `Set` anywhere. Every theorem carries
budget 0, and the module's only import is `AxiomBudget`, so no project axiom is reachable from here.

## What §9.2 asks for, and what this module does not prove

§9.2 states `parallel_execute(calls) ≈ sequential_execute(calls)` under `pairwise_disjoint_keys`. That
exact statement is **not** mechanized, for three separate reasons, and each is a divergence between the
document and the code rather than a gap in the proof.

**`parallel_execute` has no Rust counterpart.** `src/linear/src/execution.rs` computes
`ExecutionSchedule::build(&per_call_keys)` and the comment on it reads "This does not change execution
order — it logs what COULD be parallel when wasmer supports concurrent Runtime instances (Tier 6)".
The schedule's only consumers are that `info!` line and its own accessors. So calls in a block execute
sequentially today, and what this module proves is *order-independence of the merged per-call updates*
— the property that would make a parallel path safe. "Parallel equals sequential" is the reading of
that result, not a theorem about a path that exists.

**`ExecutionSchedule::build` is not mechanized.** The wave partition is a diagnostic, and its
mathematical content is the hypothesis `exec_perm` consumes: `src/linear/src/schedule.rs`'s own note
says the algorithm is "conservative: if two calls share ANY sled key, they are serialized. This is the
bisimulation safety condition from type-system.md §9.2". Mechanizing the partition would restate that
property, not establish it.

**The Rust's write set is a delta, not a set, and it misses the case that matters.** `execution.rs`
snapshots `before := cache.keys() ∪ removed` and then computes
`call_keys := (cache.keys() ∪ removed) \ before` — the keys a call newly *touches*, where touched
means "has a record in `cache` or in `removed`". A key that is already present and is overwritten
with a *different value* is touched before and touched after, so it lands in neither set: two calls
overwriting the same live key record as disjoint and do not commute.

That is a theorem here and not a remark — Part 7's `presence_diff_is_not_sufficient`, with the
witness written out so a reader can evaluate it: at a store where every key holds `[9]`, one call
writes `[1]` and the other `[2]` to the same key, both recorded write sets are empty, and the two
orders disagree at that key. It is the reason §9.2's safety argument should not lean on the schedule
as built. This model's `Diff.dom` is the value-sensitive notion instead, and `diff_dom_of_apply_ne`
is the bridge that makes writing a different value a write. The obligation is registered as
`OBL-C100` in `doc/src/arch/verification-hazop.md`, which is where this project keeps the ones it has
not discharged.

**§9.2's cited witness is stale.** It names `src/linear/src/execution.rs:398-405`
(`written_keys.insert(key)`) as the bisimulation witness. Those lines are `Runtime::new`'s failure
branch — a `revert_to_checkpoint` and "canonical call runtime creation failed" — and `written_keys`
does not occur there. The real insert checks are `:677` (uncle against uncle) and `:741` (Deployooor
against already-written state, seeded from the canonical overlay diff at `:651-654`).
`no_duplicate_of_pairwiseDisjoint` is the obligation both sites share.

## What is out of scope

The empty-value-as-absent marker defect is `Combinatorial/NullifierStorage.lean`'s subject and is not
re-modelled here, because `Store` keeps `some []` distinct from `none`. The duplicate check's
*completeness* — that every real conflict is detected — is not claimed; `DuplicateKey` is used
negatively only. And there is no `WeakBisim` bridge: this module does not import `Semantics/LTS.lean`
and so cannot state `≈`, deliberately, so that nothing here reads as modelling a parallel path the
Rust does not have.

## A note on two names

`CallJob.dom` is the set a call *declares* and `Diff.dom` the set it *touches*; `writeSet_contains` is
the only bridge between them, and the split is why `Capability/Concurrency.lean`'s replacement row
`Disjoint δ.dom ε.dom` can be read literally while commutation still gets the stronger hypothesis it
needs. §9.2's `pairwise_disjoint_keys` is about the declared sets, because that is what a schedule is
built from.
-/

import DarkFi.AxiomBudget

namespace DarkFi.Semantics

/-! ==========================================================================
   Part 1 — Carriers, and the model
   ========================================================================== -/

/-- `Bytes`: a byte string. The Rust's `Vec<u8>`, which is both what `SledKey` is and what an overlay
    value is (`sled-overlay`'s `cache: BTreeMap<IVec, IVec>`), so one carrier names both.

    `List Nat` rather than `List UInt8` so that `Key` has a computable `DecidableEq` in core Lean
    without a `deriving` on an opaque wrapper — the equality test is what `Diff.single` builds on, and
    a split on it must not reach `Classical.choice`. -/
abbrev Bytes := List Nat

/-- `Key`: `src/linear/src/schedule.rs`'s `pub type SledKey = Vec<u8>`. -/
abbrev Key := Bytes

/-- `Store`: the overlay's state, as a total map. `sled-overlay`'s `SledTreeOverlayState` has two
    fields, `cache` and `removed`; totalised, `none` is "not present" (either removed or never
    written) and `some b` is the cached value.

    `some []` and `none` are different states here, which `Combinatorial/NullifierStorage.lean`'s `ε`
    does not distinguish. That difference is deliberate — see the module note. -/
abbrev Store := Key → Option Bytes

/-- `Diff`: one call's overlay delta, in the shape the Rust state has. `none` — the call did not touch
    this key; `some none` — it removed it; `some (some b)` — it wrote `b`. -/
structure Diff where
  val : Key → Option (Option Bytes)

/-- `Diff.dom d`: the keys `d` touches, i.e. the ones its delta mentions at all.

    **The *touched* set**, not the declared one — `CallJob.dom` is the declared set, and
    `CallJob.writeSet_contains` is the bridge. Commutation needs this one; §9.2's hypothesis is about
    the other. -/
def Diff.dom (d : Diff) : Key → Prop := fun k => d.val k ≠ none

/-- `Diff.apply d s`: `s` with `d`'s delta applied. A key `d` does not touch keeps its value; a key it
    removes becomes absent; a key it writes takes the written value. -/
def Diff.apply (d : Diff) (s : Store) : Store := fun k => (d.val k).getD (s k)

/-- `Disjoint A B`: no key is in both. The predicate-level replacement for the deleted `KeyDisjoint`'s
    `writeSetDisjoint : Bool` — a `Prop` that a proof can consume, which the `Bool` never was.

    Named `Disjoint` so that `Capability/Concurrency.lean`'s replacement row `Disjoint δ.dom ε.dom`
    reads literally, which is why `CallJob.dom` exists as a projection of the `writeSet` field. -/
def Disjoint (A B : Key → Prop) : Prop := ∀ k, A k → B k → False

/-- `CallJob`: one call in a block, as the schedule and the overlay see it.

    `writeSet` is the call's **declared** write set — the Rust's `write_sets[i]`, the `HashSet<SledKey>`
    that `ExecutionSchedule::build` partitions into waves. `diff` is what the call actually did to the
    overlay. `writeSet_contains` is the invariant that relates them, and it exists because a declared
    set that excludes something the call writes is exactly the situation the schedule's disjointness
    argument cannot survive: without it, two calls could be placed in one wave and still collide.

    That field is the whole difference from the deleted `KeyDisjoint`. A `Bool` field is a claim
    nothing can read; this one is a hypothesis, and `disjoint_diff_of_disjoint_dom` below is its
    consumer. -/
structure CallJob where
  writeSet : Key → Prop
  diff : Diff
  writeSet_contains : ∀ k, diff.dom k → writeSet k

/-- `CallJob.dom c`: the set a call *declares* it writes. A projection of the field, so that
    `Disjoint δ.dom ε.dom` typechecks and says what §9.2's hypothesis says. -/
def CallJob.dom (c : CallJob) : Key → Prop := c.writeSet

/-- `CallJob.act c s`: running `c` against the store. -/
def CallJob.act (c : CallJob) (s : Store) : Store := c.diff.apply s

/-- The canonical instantiation: a call whose declared set *is* its touched set.

    This is the Rust's `per_call_keys` case, where `write_sets[i]` is computed from the observed
    overlay diff. Its `writeSet_contains` is `fun _ h => h`, which is why the invariant is not a dead
    field: it is inhabited, and inhabited by the case the Rust actually produces. -/
def CallJob.ofDiff (d : Diff) : CallJob where
  writeSet := d.dom
  diff := d
  writeSet_contains := fun _ h => h

/-- `Diff.single k v`: the delta of `db_set(k, v)` — write one key, touch nothing else. -/
def Diff.single (k : Key) (v : Bytes) : Diff where
  val := fun k' => if k' = k then some (some v) else none

/-- `CallJob.single k v`: the smallest call that writes — one key, one value. -/
def CallJob.single (k : Key) (v : Bytes) : CallJob where
  writeSet := fun k' => k' = k
  diff := Diff.single k v
  writeSet_contains := by
    intro k' h
    by_cases hk : k' = k
    · exact hk
    · exact absurd h (by simp [Diff.dom, Diff.single, hk])

/-! ==========================================================================
   Part 2 — The store laws
   ========================================================================== -/

/-- **A diff's own key yields its own value.** `NullifierStorage.lean`'s `mark_self`, generalised
    from one key to a whole delta: `d` may touch any number of keys, and each of them takes the value
    `d` recorded for it. -/
@[axiom_budget 0]
theorem diff_apply_self (d : Diff) (s : Store) (k : Key) (ov : Option Bytes)
    (h : d.val k = some ov) : d.apply s k = ov := by
  simp [Diff.apply, h]

/-- **A key a diff does not touch keeps its value.** `NullifierStorage.lean`'s `mark_other`,
    generalised: this is the law the whole module rests on, because it is what makes two diffs on
    disjoint key sets independent of each other. -/
@[axiom_budget 0]
theorem diff_apply_other (d : Diff) (s : Store) (k : Key) (h : d.val k = none) :
    d.apply s k = s k := by
  simp [Diff.apply, h]

/-- **Writing a different value is a write.** The converse of `apply_other`, and the bridge from the
    Rust's *presence*-sensitive write set to this module's value-sensitive one: if applying a diff
    changed the store at `k`, then `k` is in the diff's touched set.

    This is the lemma that shows the Rust's `call_keys` is too small to be the safety condition §9.2
    calls it — a key already present and overwritten with a different value is absent from that set and
    present in this one. See the module note. -/
@[axiom_budget 0]
theorem diff_dom_of_apply_ne (d : Diff) (s : Store) (k : Key) (h : d.apply s k ≠ s k) :
    d.dom k := by
  cases hv : d.val k with
  | none => exact absurd (by simp [Diff.apply, hv]) h
  | some ov => simp [Diff.dom, hv]

/-- One-key instance of `diff_apply_self`: `db_set(k, v)` reads back as `v`. -/
@[axiom_budget 0]
theorem diff_single_apply_self (k : Key) (v : Bytes) (s : Store) :
    (Diff.single k v).apply s k = some v := by
  simp [Diff.apply, Diff.single]

/-- One-key instance of `diff_apply_other`: `db_set(k, v)` leaves every other key alone. -/
@[axiom_budget 0]
theorem diff_single_apply_other (k k' : Key) (v : Bytes) (s : Store) (h : k' ≠ k) :
    (Diff.single k v).apply s k' = s k' := by
  simp [Diff.apply, Diff.single, h]

/-! ==========================================================================
   Part 3 — Per-call updates commute
   ========================================================================== -/

/-- **`Disjoint` is symmetric.** Not an artefact to be avoided but a fact the argument needs: the case
    analysis in `diff_apply_comm` reaches the disjointness in whatever order the two diffs' constructors
    are split, and this is what makes the order irrelevant. -/
@[axiom_budget 0]
theorem disjoint_comm {A B : Key → Prop} (h : Disjoint A B) : Disjoint B A := by
  intro k hB hA
  exact h k hA hB

/-- **Two diffs on disjoint key sets commute.** §9.2's safety property for a pair of *diffs*, and the
    engine of everything below.

    The case analysis splits only on `Option` constructors, which is what keeps this at budget 0 and is
    a design constraint rather than a coincidence: the two `none` branches close by `getD`'s reduction
    on both sides, and the `some`/`some` branch closes by the disjointness — never by comparing the two
    recorded values, which are independent. No split is on a `Prop`, so `Classical.choice` is not
    reachable. -/
@[axiom_budget 0]
theorem diff_apply_comm (d₁ d₂ : Diff) (h : Disjoint d₁.dom d₂.dom) (s : Store) :
    d₁.apply (d₂.apply s) = d₂.apply (d₁.apply s) := by
  funext k
  cases h₁ : d₁.val k with
  | none => simp [Diff.apply, h₁]
  | some ov₁ =>
    cases h₂ : d₂.val k with
    | none => simp [Diff.apply, h₂]
    | some ov₂ =>
      exact False.elim (h k (by simp [Diff.dom, h₁]) (by simp [Diff.dom, h₂]))

/-- **Declared disjointness implies touched disjointness**, and this is `writeSet_contains`'s consumer.

    The step that makes the invariant a load-bearing field rather than a comment. Disjointness is a
    hypothesis about the sets calls *declare* — that is what a schedule is built from — while
    commutation is a fact about the sets they *touch*. Without this bridge the two never meet, and
    `calljob_act_comm` could not be stated at all. -/
@[axiom_budget 0]
theorem disjoint_diff_of_disjoint_dom (c₁ c₂ : CallJob) (h : Disjoint c₁.dom c₂.dom) :
    Disjoint c₁.diff.dom c₂.diff.dom := by
  intro k h₁ h₂
  exact h k (c₁.writeSet_contains k h₁) (c₂.writeSet_contains k h₂)

/-- **Two calls on disjoint declared write sets commute.** This is what the schedule's wave partition
    assumes when it puts two calls in one wave: the order they run in cannot change the store.

    Worth naming the contrast with the record it replaces. The deleted `KeyDisjoint` carried
    `writeSetDisjoint : Bool`; nothing read it, so nothing about parallel execution followed from it.
    Here the corresponding statement is a theorem whose proof consumes the invariant, and the
    hypothesis is about the *declared* sets precisely because those are what the schedule sees. -/
@[axiom_budget 0]
theorem calljob_act_comm (c₁ c₂ : CallJob) (h : Disjoint c₁.dom c₂.dom) (s : Store) :
    c₁.act (c₂.act s) = c₂.act (c₁.act s) := by
  exact diff_apply_comm c₁.diff c₂.diff (disjoint_diff_of_disjoint_dom c₁ c₂ h) s

/-! ==========================================================================
   Part 4 — Every execution order gives the same store
   ========================================================================== -/

/-- `PairwiseDisjoint l`: any two calls in `l` have disjoint declared write sets. §9.2's
    `pairwise_disjoint_keys`.

    Stated with a `d₁ = d₂ ∨ …` disjunct rather than a `d₁ ≠ d₂` guard, and that is a design
    constraint rather than a convenience. `CallJob` has function fields, so there is no
    `Decidable (c₁ = c₂)` to split on: a guarded form would force `Classical.choice` into `exec_perm`
    below and cost it budget 0. The disjunct supplies the reflexive case instead — which is also what
    makes the hypothesis *inhabited*, proved in Part 6 rather than asserted, so the capstone is not a
    theorem true of nothing. -/
def PairwiseDisjoint (l : List CallJob) : Prop :=
  ∀ d₁ ∈ l, ∀ d₂ ∈ l, d₁ = d₂ ∨ Disjoint d₁.dom d₂.dom

/-- `DuplicateKey l`: two different calls in `l` both touch the same key. The situation the Rust
    detects when `written_keys.insert(k)` returns `false`, and the reason §9.2 says the block is
    rejected. -/
def DuplicateKey (l : List CallJob) : Prop :=
  ∃ k : Key, ∃ d₁ ∈ l, ∃ d₂ ∈ l, d₁ ≠ d₂ ∧ d₁.dom k ∧ d₂.dom k

/-- `exec l s`: run the calls in `l` in order against `s`. The "sequential execution" side of §9.2.

    There is deliberately no second function for parallel execution. Under `PairwiseDisjoint` the
    order does not matter — that is `exec_perm` — and a `parallel_exec` would suggest the Rust has
    one. It does not: see the module note. -/
def exec : List CallJob → Store → Store
  | [], s => s
  | c :: cs, s => exec cs (c.act s)

/-- **`PairwiseDisjoint` is a property of the multiset, not of the order.** No induction: it is a
    membership statement, so a permutation only moves the membership witnesses. -/
@[axiom_budget 0]
theorem pairwise_disjoint_perm {l₁ l₂ : List CallJob} (h : l₁.Perm l₂)
    (hd : PairwiseDisjoint l₁) : PairwiseDisjoint l₂ := by
  intro d₁ h₁ d₂ h₂
  exact hd d₁ ((List.Perm.mem_iff h).mpr h₁) d₂ ((List.Perm.mem_iff h).mpr h₂)

/-- **Adjacent transposition.** Swapping two neighbouring calls leaves the store alone, when their
    declared write sets are disjoint — which is `calljob_act_comm` read through `exec`'s two
    unfolding steps.

    Stated separately from `exec_perm` because it is the only step of the permutation induction that
    does any work, and because it is the statement a reader can check against §9.2 directly: one
    transposition, no order changed but this one. -/
@[axiom_budget 0]
theorem exec_swap (c₁ c₂ : CallJob) (l : List CallJob) (h : Disjoint c₁.dom c₂.dom) (s : Store) :
    exec (c₁ :: c₂ :: l) s = exec (c₂ :: c₁ :: l) s := by
  show exec l (c₂.act (c₁.act s)) = exec l (c₁.act (c₂.act s))
  rw [calljob_act_comm c₁ c₂ h s]

/-- **Every execution order of a list of pairwise-disjoint calls gives the same store.**

    This is what §9.2's `parallel_execute(calls) ≈ sequential_execute(calls)` is reaching for, minus
    the `≈`: a schedule may run a wave in any order it likes, and this says the store it produces is
    the same one sequential execution produces. The proof is the four constructors of `List.Perm`,
    and `hd` is reverted before the induction because the motive mentions it — without that the
    induction is not well typed.

    The `swap` case is where the `∨ d₁ = d₂` in `PairwiseDisjoint` earns its place: `hd` supplies the
    reflexive case as a *disjunct*, so `swap` needs no decision procedure for equality of calls. -/
@[axiom_budget 0]
theorem exec_perm {l₁ l₂ : List CallJob} (h : l₁.Perm l₂) (hd : PairwiseDisjoint l₁) :
    ∀ s : Store, exec l₁ s = exec l₂ s := by
  revert hd
  induction h with
  | nil => intro _ s; rfl
  | cons c _ ih =>
      intro hd s
      exact ih (fun d₁ h₁ d₂ h₂ => hd d₁ (by simp [h₁]) d₂ (by simp [h₂])) (c.act s)
  | swap c₁ c₂ l =>
      intro hd s
      rcases hd c₂ (by simp) c₁ (by simp) with heq | hdisj
      · subst heq; rfl
      · exact exec_swap c₂ c₁ l hdisj s
  | trans h₁ _ ih₁ ih₂ =>
      intro hd s
      exact (ih₁ hd s).trans (ih₂ (pairwise_disjoint_perm h₁ hd) s)

/-! ==========================================================================
   Part 5 — The duplicate-key check cannot fire
   ========================================================================== -/

/-- **No pair of calls in a pairwise-disjoint list collides on a key.**

    The obligation §9.2's two Rust sites share: `execution.rs:677` for uncle against uncle and
    `:741` for Deployooor against already-written state both return `DuplicateKeyConflict` when an
    `insert` returns `false`, and this says that under the schedule's disjointness hypothesis the
    branch is unreachable.

    Only the negative direction is claimed. *Completeness* — that every real conflict is detected —
    is a property of the Rust's insertion loop, not of this model, and is not stated here. -/
@[axiom_budget 0]
theorem no_duplicate_of_pairwise_disjoint (l : List CallJob) (hd : PairwiseDisjoint l) :
    ¬ DuplicateKey l := by
  intro hdup
  rcases hdup with ⟨k, d₁, h₁, d₂, h₂, hne, hk₁, hk₂⟩
  rcases hd d₁ h₁ d₂ h₂ with heq | hdisj
  · exact hne heq
  · exact hdisj k hk₁ hk₂

/-! ==========================================================================
   Part 6 — The smallest instance, and why the hypothesis is not vacuous
   ========================================================================== -/

/-- **Two disjoint calls satisfy `PairwiseDisjoint`.** The witness that §9.2's hypothesis is
    inhabited, which is the thing a `∀ … → …` capstone cannot establish about itself: `exec_perm`
    would be worth nothing if no list ever satisfied `PairwiseDisjoint`, and this says one does, for
    the case the Rust actually produces — a block of calls that touch different keys.

    Deliberately two calls rather than one. `PairwiseDisjoint []` and `PairwiseDisjoint [c]` are both
    true and both trivial — the first because every quantifier is vacuous, the second because the
    disjunct `d₁ = d₂` carries it — and a witness of that shape would satisfy the letter of
    non-vacuity while showing nothing. Two calls force the `Disjoint` disjunct to be used in one of
    the four cases, which is the case `exec_swap` needs. -/
@[axiom_budget 0]
theorem pairwise_disjoint_pair {c₁ c₂ : CallJob} (h : Disjoint c₁.dom c₂.dom) :
    PairwiseDisjoint [c₁, c₂] := by
  intro d₁ h₁ d₂ h₂
  simp at h₁ h₂
  rcases h₁ with rfl | rfl
  · rcases h₂ with rfl | rfl
    · exact Or.inl rfl
    · exact Or.inr h
  · rcases h₂ with rfl | rfl
    · exact Or.inr (disjoint_comm h)
    · exact Or.inl rfl

/-- Two-call instance of `exec_perm`: the adjacent transposition, with the hypothesis discharged by
    `pairwise_disjoint_pair` rather than assumed. -/
@[axiom_budget 0]
theorem exec_pair_swap {c₁ c₂ : CallJob} (h : Disjoint c₁.dom c₂.dom) (s : Store) :
    exec [c₁, c₂] s = exec [c₂, c₁] s := by
  exact exec_swap c₁ c₂ [] h s

/-- **Two single-key writes to different keys are disjoint** — the case where the whole chain is
    concrete: `db_set` at one key and `db_set` at another, which is what the Rust's
    `written_keys.insert` sees as two different `SledKey`s. -/
@[axiom_budget 0]
theorem singles_disjoint {k₁ k₂ : Key} {v₁ v₂ : Bytes} (h : k₁ ≠ k₂) :
    Disjoint (CallJob.single k₁ v₁).dom (CallJob.single k₂ v₂).dom := by
  intro k h₁ h₂
  dsimp [CallJob.dom, CallJob.single] at h₁ h₂
  exact h (h₁.symm.trans h₂)

/-- **The concrete capstone**: two `db_set`s on different keys commute.

    Every abstraction the module introduces is instantiated here — a `Key`, a one-key `Diff`, a
    `CallJob` whose declared set is the singleton, `PairwiseDisjoint`, `exec` — so the chain is
    anchored to something a reader can check without believing any of the definitions were chosen to
    make it work. -/
@[axiom_budget 0]
theorem exec_two_singles_swap {k₁ k₂ : Key} {v₁ v₂ : Bytes} (h : k₁ ≠ k₂) (s : Store) :
    exec [CallJob.single k₁ v₁, CallJob.single k₂ v₂] s
      = exec [CallJob.single k₂ v₂, CallJob.single k₁ v₁] s := by
  exact exec_pair_swap (singles_disjoint h) s

/-! ==========================================================================
   Part 7 — Why the Rust's write set is not the safety condition it is called
   ========================================================================== -/

/-- `Present s k`: the store holds a value at `k` — the Rust's `k ∈ cache.keys()`, as opposed to `k`
    being in `removed` or never having been written at all.

    `Store` cannot separate those last two; `none` is both. That conflation is `Present`'s whole
    subject, and Part 7 says where it matters below. -/
def Present (s : Store) (k : Key) : Prop := ∃ b : Bytes, s k = some b

/-- `presenceDiff d s`: the keys whose *presence* in the store applying `d` changes. This is the
    shape of the per-call write set `src/linear/src/execution.rs` computes — a delta taken over an
    overlay-key set before the call and subtracted from the same set after it.

    What the Rust deltas is its **touched** keys (`cache.keys() ∪ removed`), not its present ones, and
    `Store` cannot tell those apart. The two notions agree on the witness
    `presence_diff_is_not_sufficient` uses — the key there is present throughout, so it is touched
    throughout too — which is why that refutation lands on the Rust's set as actually computed rather
    than on a substitute for it. Where the two differ in general is **no longer prose**: a key removed
    after being written is a change of *touch* and not of presence, and `rustPerCallKeys_is_strict`
    below is that case as a theorem, with `rustPerCallKeys_subset_touched` giving the direction.

    Written as an explicit disjunction rather than as `Present s k ≠ Present (d.apply s) k`, so that
    the proof below stays inside `Prop` without appealing to `propext` for the negation. -/
def presenceDiff (d : Diff) (s : Store) : Key → Prop :=
  fun k => (Present s k ∧ ¬ Present (d.apply s) k) ∨ (¬ Present s k ∧ Present (d.apply s) k)

/-! #### The Rust's write set, in the vocabulary that can state it

`presenceDiff` above is the Rust's delta *shape* over a `Store`, and the note says why that is not the
Rust's notion: `Store` cannot tell "a key the call removed" from "a key nobody mentioned". The overlay
delta can — `Diff.val` has the third case, `some none` — and `Diff.dom` is already the predicate that
names it. So the Rust's `per_call_keys` needs no new modelling, only the right vocabulary, which is
the whole reason this was prose: the gap was a **vocabulary** gap rather than a modelling one.

`execution.rs` computes the set as `(cache.keys() ∪ removed) \ before`, which is "keys the overlay has
an opinion about *after* that it had no opinion about *before*". That is `rustPerCallKeys` below.
What the schedule's safety argument needs is "keys whose overlay entry *changed*", which is
`Diff.dom` of the call's delta. The two are compared by the pair of theorems here, and the relation
is one-directional — which is the register's `OBL-C100` in one sentence. -/

/-- `rustPerCallKeys before after`: the keys the Rust's `per_call_keys` records for a call — the
    overlay's opinion after the call, minus its opinion before. `before` and `after` are the overlay
    state at the two snapshot points (`execution.rs:381-386` and `:547-551`), and "has an opinion" is
    membership of `cache.keys() ∪ removed`, which is what `Diff.val _ ≠ none` is.

    Stated with `= none` on the left rather than `¬ … ≠ none` so the subset proof below stays inside
    `Prop` — `¬¬(a = b) → a = b` is where the classical choice would otherwise be spent. -/
def rustPerCallKeys (before after : Diff) : Key → Prop :=
  fun k => before.val k = none ∧ after.val k ≠ none

/-- **What the Rust records, the model also counts.** Every key the per-call delta names is a key whose
    overlay entry changed, so `per_call_keys` is a *subset* of the notion `Diff.dom` names and
    `exec_perm` is stated over.

    The direction is the point, and it is the unsafe one: a partition built on the Rust's set is
    *permissive*, because the set can miss a key the call changed. A superset relation in the other
    direction would have been a safety argument; this is the statement that there is none. -/
@[axiom_budget 0]
theorem rustPerCallKeys_subset_touched (before after : Diff) :
    ∀ k, rustPerCallKeys before after k → before.val k ≠ after.val k := by
  intro k h
  rw [h.1]
  exact fun hc => h.2 hc.symm

/-- **And the subset is strict.** There is a pair of overlay states and a key where the entry changed
    and the Rust's delta is empty — a key the overlay already had an opinion about, whose opinion
    *changed*: written and then removed.

    This is the case the `presenceDiff` note called prose. It is the same shape as the register's
    witness for the value-safe case, one level down: there a key was present throughout, here it is
    *touched* throughout, and in both the delta is empty while the effect is not. The Rust's set
    cannot be a safety condition for either reason. -/
@[axiom_budget 0]
theorem rustPerCallKeys_is_strict :
    ∃ before after : Diff, ∃ k : Key,
      before.val k ≠ after.val k ∧ ¬ rustPerCallKeys before after k := by
  refine ⟨Diff.single [] [9], ⟨fun k => if k = ([] : Key) then some none else none⟩, [], ?_, ?_⟩
  · intro hc
    simp [Diff.single] at hc
  · intro h
    simp [rustPerCallKeys, Diff.single] at h

/-- **A one-key diff's touched set contains its key.** The contrast the refutation below turns on: the
    value-sensitive notion sees the write, and the presence-delta does not. -/
@[axiom_budget 0]
theorem diff_single_dom_self (k : Key) (v : Bytes) : (Diff.single k v).dom k := by
  simp [Diff.dom, Diff.single]

/-- **A write set computed as a presence delta is not a sufficient safety condition.**

    The tempting statement is that disjointness of the deltas — which is what
    `ExecutionSchedule::build` partitions waves by — makes two calls commute. It is false, and the
    witness is the case the Rust's own `per_call_keys` cannot see: both calls write the *same*,
    already-present key, so neither delta changes any key's presence, and the two are disjoint while
    failing to commute.

    Concretely, at `s` where every key holds `[9]`: `d₁` writes `[1]` to `k` and `d₂` writes `[2]` to
    the same `k`. Both presence-diffs are empty, so `Disjoint` holds vacuously; and
    `d₁.apply (d₂.apply s) k = some [1]` while `d₂.apply (d₁.apply s) k = some [2]`, which is the
    whole failure — the two orders disagree, so no safety argument can be read off the deltas.

    This is the theorem the module note's third divergence and `type-system.md` §9.2 point at. It is
    a fact about the Rust as it is written, not about this model: the witness's key is present, hence
    touched, hence in `before_keys` on both calls, so the Rust records both write sets as empty too.
    `diff_single_dom_self` is the other half — the notion this module uses does catch it. -/
@[axiom_budget 0]
theorem presence_diff_is_not_sufficient :
    ¬ (∀ (d₁ d₂ : Diff) (s : Store),
        Disjoint (presenceDiff d₁ s) (presenceDiff d₂ s) →
        d₁.apply (d₂.apply s) = d₂.apply (d₁.apply s)) := by
  intro h
  -- the witness is written out rather than bound with `let`: a local definition is opaque to `simp`,
  -- so every step below would need an unfolding lemma named for it, and the point of a refutation is
  -- that a reader can evaluate it.
  have hd₁ : ∀ k' : Key, ¬ presenceDiff (Diff.single [0] [1]) (fun _ => some [9]) k' := by
    intro k' hpd
    simp only [presenceDiff] at hpd
    rcases hpd with ⟨_, hn⟩ | ⟨hn, _⟩
    · exact hn (by
        by_cases hk : k' = [0]
        · subst hk
          exact ⟨[1], diff_single_apply_self [0] [1] (fun _ => some [9])⟩
        · exact ⟨[9], by simp [Diff.apply, Diff.single, hk]⟩)
    · exact hn ⟨[9], rfl⟩
  have hd₂ : ∀ k' : Key, ¬ presenceDiff (Diff.single [0] [2]) (fun _ => some [9]) k' := by
    intro k' hpd
    simp only [presenceDiff] at hpd
    rcases hpd with ⟨_, hn⟩ | ⟨hn, _⟩
    · exact hn (by
        by_cases hk : k' = [0]
        · subst hk
          exact ⟨[2], diff_single_apply_self [0] [2] (fun _ => some [9])⟩
        · exact ⟨[9], by simp [Diff.apply, Diff.single, hk]⟩)
    · exact hn ⟨[9], rfl⟩
  -- Both calls' recorded write sets are empty. Only the first is needed for `Disjoint`, which is
  -- vacuous when either side is empty — but the finding is that *both* are, which is what makes the
  -- schedule's test pass rather than merely be inapplicable, so both are proved and bound.
  have hempty : (∀ k' : Key, ¬ presenceDiff (Diff.single [0] [1]) (fun _ => some [9]) k')
      ∧ (∀ k' : Key, ¬ presenceDiff (Diff.single [0] [2]) (fun _ => some [9]) k') := ⟨hd₁, hd₂⟩
  have hdisj : Disjoint (presenceDiff (Diff.single [0] [1]) (fun _ => some [9]))
      (presenceDiff (Diff.single [0] [2]) (fun _ => some [9])) := by
    intro k' h₁ _
    exact hempty.1 k' h₁
  have heq := h (Diff.single [0] [1]) (Diff.single [0] [2]) (fun _ => some [9]) hdisj
  have h₁ : (Diff.single [0] [1]).apply ((Diff.single [0] [2]).apply (fun _ => some [9])) [0]
      = some ([1] : Bytes) :=
    diff_single_apply_self [0] [1] _
  have h₂ : (Diff.single [0] [2]).apply ((Diff.single [0] [1]).apply (fun _ => some [9])) [0]
      = some ([2] : Bytes) :=
    diff_single_apply_self [0] [2] _
  have hbad : some ([1] : Bytes) = some ([2] : Bytes) := by
    rw [← h₁, heq, h₂]
  exact absurd hbad (by decide)

end DarkFi.Semantics
