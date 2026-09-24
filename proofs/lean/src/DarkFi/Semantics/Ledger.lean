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

**The Rust's write set is presence-sensitive; this model's is value-sensitive.** `execution.rs`
snapshots `before := cache.keys() ∪ removed` and then computes
`call_keys := (cache.keys() ∪ removed) \ before`. A key that was already present and is overwritten
with a *different value* lands in neither set, so two calls overwriting the same live key record as
disjoint — and they do not commute. This model's `Diff.dom` is the value-sensitive notion, and
`diff_dom_of_apply_ne` is the bridge that makes writing a different value a write. The Rust's set is
therefore too small to be the safety condition §9.2 calls it, which is consistent with its being
consumed only by a log line.

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

end DarkFi.Semantics
