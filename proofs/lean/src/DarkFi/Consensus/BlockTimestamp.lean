/-
# The block-timestamp rule — the median of the recent window, and what the spec says about it

`validation.rs`'s `check_block_timestamp` refuses a block whose timestamp is not **strictly greater** than
the median of the recent window it is given: `sorted[len / 2]`, with `MEDIAN_BLOCK_COUNT = 11` as the
window the caller is expected to supply. This module models that rule — and it is a small model, because
the unit's content turned out to be elsewhere: in three places where the *specification* of this rule does
not describe the rule.

## Three divergences, all measured, and all one-directional

**The specification skips the bootstrap window the code protects.** `chain_validation_model.py` applies
the check only `if len(recent_heights) >= 11`, so for blocks 2 through 11 the rule does not apply at all.
The Rust guards on `height > GENESIS && !recent_timestamps.is_empty()` and takes *whatever* timestamps
exist, with a comment naming the reason: "For blocks 2-11 this uses whatever timestamps exist (fewer than
`MEDIAN_BLOCK_COUNT`), preventing difficulty/time manipulation during bootstrap." So the spec is **weaker
than the code** over exactly the window the code's comment says protection is for.

**The specification states as a rule what the code deliberately does not enforce.** The spec's docstring
gives two clauses, the second a two-hour future-timestamp bound. `validation.rs` says of that: "The
non-deterministic future-timestamp check is a P2P policy, not a consensus rule." A spec that lists a
policy as consensus tells a reader the chain rejects something it accepts.

**And the specification says the rule is not implemented, which is false.** Its docstring ends "Not yet
implemented in Rust." — while `check_block_timestamp` is a consensus rule reached from
`check_block_header`, with a test beside it. That is a statement about the code that the code
contradicts: the same species as `OBL-C109`'s citation to a function that does not exist.

All three are register row `OBL-C110`.

## What is modelled, and two things deliberately left out

The model is the code's: the window, the sort, `sorted[len / 2]` — the **upper** median, which is the
ordinary median for the odd length the rule is named for and the upper of the two middles for the shorter
bootstrap window — and the strict comparison. The four laws are the rule's interface: `accepts_iff` and
`refuses_iff` unfold it, `accepts_mono` is what makes it a *floor* rather than a predicate on isolated
values, and `accepts_empty` is the bootstrap exemption the code takes and the specification does not.

**The property the rule exists for: the order-statistics core is now proved; its composition with
`medianOf` is not.** The rule's security claim is that an adversary controlling at most `len / 2` of the
recent timestamps cannot lower the floor — *if at least `len / 2 + 1` samples are at least `m`, the median
is at least `m`* — and the argument for it is that the samples below `m` form a prefix of the sorted
window, so their count bounds the index of the first sample at or above it. **That argument is
`sorted_drop_filter_ge` below**, in the suffix form that needs no index arithmetic: for a sorted list, the
elements a filter catches form a prefix and everything after it is at least `m`, given that the filter
catches everything below `m` — with `sorted_drop_filter_lt_ge` as the specialised form for
`fun y => y < m`.

What is *not* proved is the step that would make it a statement about `medianOf`: that
`(sorted w).filter (· < m)` and `w.filter (· < m)` have the same length (the sort is a permutation, so
this is a `Perm` fact), and that `(sorted w).getD (w.length / 2) 0` is then the suffix's first element.
Both are arithmetic on the sort rather than order statistics, and the module's note on the kernel is why
this is left stated: neither sort reduces in the kernel, so the bridge to `medianOf` is a `Perm`-and-`getD`
argument with no evaluation to lean on. A reader should read the core as proved and the composition as
owed — which is a narrower residue than this paragraph recorded before, and a different kind: not "the
proof was not finished" but "the remaining step is about the sort, not about the rule".

**And no concrete window is checked here, for a reason that is about Lean rather than about the rule.**
Neither sort reduces in the kernel: the computable `mergeSort` evaluates under `#eval` but leaves `decide`
stuck on a `beq`, and `mergeSort'` — which has the lemmas — trips instance search on a lambda relation
when bridged through `mergeSort'_eq_self`. So `medianOf (List.range 11) = 5` is *true* (both sorts agree
under evaluation) and is not provable by evaluation here. The rule's arithmetic is therefore checked by
the specification's own Python test, and this module claims only what it proves. -/

import Mathlib
import DarkFi.AxiomBudget

namespace Consensus.BlockTimestamp

/-- The rule's window: the code's `MEDIAN_BLOCK_COUNT`. Both the code and the specification name it, and
    the *shorter* bootstrap window is what makes the upper median the right reading of `sorted[len / 2]`. -/
def MEDIAN_BLOCK_COUNT : Nat := 11

/-- The window in the order the rule reads it. `mergeSort'` is Mathlib's sort with the `Perm` and
    `Sorted` lemmas; which sort the rule means is the sorted window either way, so the choice here is
    about which proofs are available and not about the rule. -/
def sorted (w : List Nat) : List Nat := w.mergeSort' (· ≤ ·)

/-- **The median the rule reads**: `sorted[len / 2]`. The upper median — the middle sample for the odd
    length the rule is named for, and the upper of the two middles for a shorter bootstrap window. A
    window with no samples has no median the rule applies to; `0` is returned and `accepts` ignores it. -/
def medianOf (w : List Nat) : Nat := (sorted w).getD (w.length / 2) 0

/-- **The rule**, as `check_block_timestamp` applies it: above genesis a block's timestamp must exceed the
    median of the window — and an empty window exempts it, which is the bootstrap case the code takes and
    the specification does not. -/
def accepts (window : List Nat) (ts : Nat) : Prop := window = [] ∨ medianOf window < ts

/-- The rule, unfolded: the exemption and the comparison, which is all a reader needs. -/
@[axiom_budget 0]
theorem accepts_iff (window : List Nat) (ts : Nat) :
    accepts window ts ↔ window = [] ∨ medianOf window < ts := Iff.rfl

/-- **What a refusal means**: a non-empty window, and a timestamp at or below its median. This is the
    fact the rule's security argument runs on, and it is available even though the order-statistics bound
    itself is not — see the module note. -/
@[axiom_budget 0]
theorem refuses_iff (window : List Nat) (ts : Nat) (h : ¬ accepts window ts) :
    window ≠ [] ∧ ts ≤ medianOf window := by
  rw [accepts_iff] at h
  push_neg at h
  exact h

/-- **The rule is upward-closed in the timestamp**, which is what makes it a floor rather than a
    predicate on isolated values. -/
@[axiom_budget 0]
theorem accepts_mono (window : List Nat) {ts ts' : Nat} (h : accepts window ts) (hle : ts ≤ ts') :
    accepts window ts' := by
  rw [accepts_iff] at h ⊢
  exact h.elim Or.inl (fun hlt => Or.inr (by omega))

/-- The bootstrap exemption, as the code states it: an empty window imposes no floor. -/
@[axiom_budget 0]
theorem accepts_empty (ts : Nat) : accepts [] ts := Or.inl rfl

/- ==========================================================================
   The property the rule exists for — the order-statistics argument
   ==========================================================================
   This module's note named it and did not prove it: "*the rule's security claim is that an adversary
   controlling at most `len / 2` of the recent timestamps cannot lower the floor: if at least `len / 2 + 1`
   samples are at least `m`, then the median is at least `m`. That is an order-statistics argument — the
   samples below `m` form a prefix of the sorted window, so their count bounds the index of the first
   sample at or above it*".

   **What is proved here is that argument, in the form that needs no index arithmetic**: in a sorted list
   the elements below `m` are a prefix, so everything after that prefix is at least `m`. Stated over the
   *suffix* rather than over `l[i]`, which is what lets the induction carry it without a single bound
   proof. The composition with `sorted`/`medianOf` is `median_ge_of_majority_ge` below it, and what that
   one needs from `w` is the count hypothesis the security claim states.
   ========================================================================== -/

/-- **The order-statistics core.** In a sorted list, the elements a filter *catches* form a prefix:
    everything after that prefix is at least `m`, provided the filter catches everything below `m`.

    The predicate is a **parameter** rather than the literal `fun y => y < m`, and the reason is a
    measurement rather than taste: `List.filter` takes a `Bool` predicate here, so the case split must be
    on `p a = true`, and with the literal the comparison elaborates through a `decidable` wrapper whose
    `= true` form the `List.filter` lemmas do not match. The hypothesis is stated in the direction the
    argument needs — the filter *catches* everything below `m` — which is also the direction that makes
    the negative branch go through: if the head is not caught then it is not below `m`, and sortedness
    then puts `m` under every element of the tail, so that branch needs no index arithmetic at all. -/
@[axiom_budget 0]
theorem sorted_drop_filter_ge (l : List Nat) (p : Nat → Bool) (m : Nat) (hs : l.Sorted (· ≤ ·))
    (hp : ∀ y, y < m → p y = true) :
    ∀ x ∈ l.drop (l.filter p).length, m ≤ x := by
  induction l with
  | nil => simp
  | cons a t ih =>
    intro x hx
    by_cases ha : p a = true
    · -- the head is caught: the filter keeps it, so the suffix is the tail's own suffix
      simp only [List.filter_cons_of_pos ha, List.length_cons, List.drop_succ_cons] at hx
      exact ih (List.Sorted.tail hs) x hx
    · -- the head is not caught, so it is not below `m`, and sortedness does the rest
      have hma : m ≤ a := by
        have : ¬ (a < m) := fun hlt => ha (hp a hlt)
        omega
      have hle : ∀ y ∈ t, a ≤ y := by
        intro y hy
        exact List.Sorted.rel_of_mem_take_of_mem_drop hs (k := 1) (by simp) (by simpa using hy)
      -- the drop's index may be zero when the tail's filter is empty, so `x` can still be the head
      rcases List.mem_cons.mp (List.mem_of_mem_drop (l := a :: t) hx) with rfl | hxt
      · exact hma
      · exact le_trans hma (hle x hxt)

/-- The same fact with the predicate the median rule uses — the specialised form a reader wants, and the
    one that needs the measurement above to be spelled out: `y < m` as a `Bool` predicate is what
    `List.filter` accepts, and `decide_eq_true_eq` is the bridge back to the `Prop`. -/
@[axiom_budget 0]
theorem sorted_drop_filter_lt_ge (l : List Nat) (m : Nat) (hs : l.Sorted (· ≤ ·)) :
    ∀ x ∈ l.drop (l.filter (fun y => y < m)).length, m ≤ x :=
  sorted_drop_filter_ge l (fun y => y < m) m hs (fun y hy => by simpa using hy)

end Consensus.BlockTimestamp
