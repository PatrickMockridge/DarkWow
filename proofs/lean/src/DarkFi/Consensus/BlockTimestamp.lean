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

**The property worth proving next is the one the rule exists for, and it is *not* proved here.** The
rule's security claim is that an adversary controlling at most `len / 2` of the recent timestamps cannot
lower the floor: if at least `len / 2 + 1` samples are at least `m`, then the median is at least `m`. That
is an order-statistics argument — the samples below `m` form a prefix of the sorted window, so their count
bounds the index of the first sample at or above it — and the lemmas it needs do exist
(`List.perm_mergeSort'`, `List.sorted_mergeSort'`, `List.mergeSort'_eq_self`). It is absent because the
proof was not finished, which is a different statement from "it cannot be", and it is recorded rather than
stated because a claim whose proof is missing is the thing this repository deletes.

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

end Consensus.BlockTimestamp
