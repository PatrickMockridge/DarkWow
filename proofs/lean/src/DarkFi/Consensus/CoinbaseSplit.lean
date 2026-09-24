/-
# The coinbase split — five quantities, five equations, and which one is load-bearing

A block that includes uncles does not pay one reward, it pays a *split*: the canonical miner's
spendable note is reduced by the pins the block pays to its uncles, and the sum has to come out to the
emission schedule exactly. This module models that as an arithmetic system and asks the question no
single file in the tree can ask — **which of the checks doing the work is actually load-bearing.**

## The five equations, and the four sites that enforce them

Measured by reading, 2026-09-24. `accepts` below is the conjunction over the enforcement sites, not a
function in the tree — that is the point of the model, and it is stated here rather than implied.

| | equation | enforced at |
|---|---|---|
| 1 | `total_reward + Σ pin == expected_reward(H)` | `block_acceptor.rs` (the host's reward check) |
| 2 | `total_pin == Σ pin` | `block_acceptor.rs` (the "reward theft" check) |
| 3 | `effective_value + Σ pin == expected_reward(H)` | `block_acceptor.rs` (the spendable-note over-mint check) |
| 4 | `effective_value + total_pin == input.value` | `validation.rs` (the coinbase's own self-consistency) |
| 5 | `Σ uncle_note_value == Σ pin` | `block_acceptor.rs` (the uncle-note sum check) |

and the `pow_reward_v1` guard adds the *schedule* — `input.value == expected_reward(H)` — which
register row `OBL-C4` records as checked "only in WASM", leaving the host's validator holding the split
without the schedule. That reading is corrected below, by theorem.

## The finding: the spec already had it, and this is the mechanization

Register row `OBL-C4` carries `effective_value + total_pin == expected_reward(H)` as its proposition.
The interesting question is not whether it holds — it is *why the five checks are five*, and which of
them could be dropped. The answer is stated in `contrib/model/chain_model.py`, in the specification's
own words, before this module existed:

> The value-level `verify_uncle_split` is **NOT sufficient**: it checks the header and the declared
> pins, not the SPENDABLE NOTE. A producer can declare a compliant split and still commit the coinbase
> note to the full base while emitting the uncle notes, so total spendable = base + Σ pin while `S_H`
> and `TOTAL_SUPPLY` advance by only base — the Pedersen supply audit cannot see the difference.

`note_sum_is_load_bearing` is that sentence as a theorem, and it is deliberately *stronger* than a bare
witness: it says there is no proof of the conservation law from the other four equations **and** the
schedule — i.e. no weakening of the hypothesis set rescues it. So of the five checks, the uncle-note
sum is the one that cannot be replaced.

This is the first unit of the campaign whose headline is that the specification was **right**. The two
preceding units (`OBL-C109`, `OBL-C110`) were about a specification that misdescribed the code; here the
specification contained the load-bearing analysis, and what was missing was the machine check.

## And the three value checks are mutually redundant, which is a second finding

`declaredPin_eq_of_others`, `split_eq_of_others` and `note_split_eq_of_others` say that given the
schedule check, equations 2, 3 and 4 are *over-determined*: each follows from the other two. So the host
is not missing the schedule rule — `accepts_implies_schedule` proves the host's three equations **imply**
it, which is what `OBL-C4`'s caveat needed and did not have. Defence in depth that is provably
equivalent is a different claim from defence in depth that is three independent rules.

## The schedule, and why the split check is not made redundant by the structural bounds

`split_for_uncle` computes the pin as `base / 2^depth`, and the pin schedule's own total over the
window the code allows is bounded by the base reward (`halvings_window_le`) — so for a set of *distinct*
depths the canonical miner is always left something. But nothing in `validation.rs`'s uncle rules forces
the depths to be distinct: `MAX_UNCLE_COUNT` and `MAX_UNCLE_DEPTH` are both 6, and **three uncles at
depth 1** satisfy both and pay `3·(base/2)`, which exceeds the base reward
(`structural_bounds_do_not_bound_the_pin`). So the split check is load-bearing arithmetic, not a
formality — `compute_reward`'s `checked_sub` and `verify_uncle_split` are the enforcement, and they
are the only thing standing between a 3-at-depth-1 block and a negative canonical reward.

One measurement about the schedule's shape, recorded rather than papered over: the *same* sum written as
a fold over `List.range` (`Σ_{d=1}^{6} base / 2^d ≤ base`) does not close — `omega` runs without
terminating on the six-term form, and the two-term form it *does* close (`base/2 + base/4 ≤ base`) does
not compose into it. The recursion above is the shape that closes, and it closes at budget 0. So the
schedule is bounded by an induction on the depth rather than by arithmetic over the written-out sum, and
that is a fact about the proof environment, not about the schedule.

`split_for_uncle` also carries the fix for a build-profile-dependent bug, and the model pins both
sides: the *unbounded* shift behind it is masked mod 64 in release, so `depth = 64` wraps to a shift by
0 and pays the **full** reward, which `shift_wrap_pays_full_reward` states. The deployed form bounds
the divisor in the arithmetic instead (`split_for_uncle_le_base`).

## What this module does not model, stated rather than implied

* **The per-note key binding.** The host requires each uncle-mint call to match an included uncle by
  `(miner, pin)` — a *per-element* rule. Only the **sum** is modelled here (equation 5), and the sum is
  necessary but not sufficient for the binding: a block could pay the right total to the wrong keys.
  That is a separate unit.
* **Overflow as a mechanism.** `total_accepted_pin` sums with `checked_add` and fails closed. The model
  shows *why that branch is not the interesting one* — `accepts_pins_bounded` derives `Σ pin ≤ base`
  from equation 3, so an accepted block's pin sum never needs more width than the base reward — but it
  does not model the sum's own arithmetic.
* **WASM.** `pow_reward_v1` re-derives the commitment and re-checks the split and the schedule. The
  model treats it as the schedule equation's source, not as a second enforcement site.
* **The commitment equation.** Register row `OBL-C85` records that the spec's per-uncle Pedersen audit
  commitments are materialised nowhere and that the split is enforced as **integer** arithmetic. This
  model is that integer arithmetic, so it mechanizes the row's reading rather than the spec's equation.
* **Nothing here is a claim about the Rust.** The five equations are transcribed from the sites named
  above; a model of a rule is not a proof that the code implements the rule. -/

import Mathlib
import DarkFi.AxiomBudget

namespace Consensus.CoinbaseSplit

/-! ===== Part 0 — the pin schedule, and the bug its arithmetic carries =====

`BlockReward::split_for_uncle` splits the base reward by depth. Its docstring records that a
`debug_assert!(depth <= 6)` used to guard it, and that a shift by 64 or more is masked mod 64 in
release — so an out-of-range depth returned the *full* reward, and the reward depended on the build
profile. The bound is in the arithmetic now. Both halves are below. -/

/-- **The pin at a depth**, in the deployed form: `base / 2^depth`, with a depth past the last
    representable divisor yielding zero — which is where `base / 2^63` already sits. -/
def splitForUncle (base depth : Nat) : Nat := if 64 ≤ depth then 0 else base / 2 ^ depth

/-- **The pre-fix behaviour**, kept because it is what the current arithmetic rules out: a shift that
    wraps mod 64 returns `base / 2^(depth % 64)`, so `depth = 64` is a shift by zero. -/
def splitForUncleWrapped (base depth : Nat) : Nat := base / 2 ^ (depth % 64)

/-- The deployed form never pays more than the reward. This is the bound the `debug_assert` used to be
    the only guard for, now a property of the definition. -/
@[axiom_budget 0]
theorem splitForUncle_le_base (base depth : Nat) : splitForUncle base depth ≤ base := by
  unfold splitForUncle
  split
  · exact Nat.zero_le _
  · exact Nat.div_le_self _ _

/-- **And the wrapped form does**: at depth 64 the masked shift is a shift by zero, so an out-of-range
    uncle is paid the whole reward rather than a sixty-fourth of it. This is the bug's falsity as a
    theorem, not a reading of the comment. -/
@[axiom_budget 0]
theorem shift_wrap_pays_full_reward (base : Nat) : splitForUncleWrapped base 64 = base := by
  unfold splitForUncleWrapped
  norm_num

/-- The pin schedule's own total for depths `1..k`: `b/2 + (b/2)/2 + …`, the halving chain the
    geometric split implies. Recursive on the *depth*, with the base halved on each step, because that
    is the shape the induction needs — the same sum written as a `List.range` fold does not close (see
    the module note's measurements). -/
def halvings : Nat → Nat → Nat
  | _, 0 => 0
  | b, k + 1 => b / 2 + halvings (b / 2) k

/-- **The schedule is bounded by the reward it splits**: the shares of any distinct depth set inside
    the allowed window sum to at most `base`, so the canonical miner is left something. A *sufficient*
    condition, and — as Part 4 shows — one the code's own structural bounds do not impose. -/
@[axiom_budget 0]
theorem halvings_le (base k : Nat) : halvings base k ≤ base := by
  induction k generalizing base with
  | zero => simp [halvings]
  | succ n ih =>
    rw [halvings]
    have := ih (base / 2)
    omega

/-- The code's depth window: `MAX_UNCLE_DEPTH`. Named here the way `MEDIAN_BLOCK_COUNT` is in
    `Consensus/BlockTimestamp.lean`, so the schedule's window is a constant rather than a literal. -/
def MAX_UNCLE_DEPTH : Nat := 6

/-- The code's per-block uncle limit: `MAX_UNCLE_COUNT`. -/
def MAX_UNCLE_COUNT : Nat := 6

/-- The schedule's total over the window the code actually allows. -/
@[axiom_budget 0]
theorem halvings_window_le (base : Nat) : halvings base MAX_UNCLE_DEPTH ≤ base := by
  unfold MAX_UNCLE_DEPTH
  exact halvings_le base 6

/-! ===== Part 1 — the accept predicate =====

The five equations, subtraction-free. Every one of them is written as an equality between sums rather
than as a subtraction, for the reason `Consensus/CommitmentSet.lean` records: `Nat` truncated
subtraction needs a case split at every step, and `omega` decides the addition forms directly. Here it
also matters for a second reason — the code's own quantities are *checked* sums, and stating the rule as
`a + b = c` is stating the check the code performs. -/

/-- The coinbase split as the accept path holds it: the five scalars the rules constrain, and the two
    lists the sums range over. `unclePins` is the **accepted** uncles' pins — `total_accepted_pin`'s
    filter, not all uncles — and `uncleNotes` is the uncle-mint calls' declared values. -/
structure Split where
  /-- `expected_reward(H)`, the emission schedule's value at the referencing height. -/
  base : Nat
  /-- `block.header.total_reward` — what the block says the canonical miner gets. -/
  totalReward : Nat
  /-- `pow_params.total_pin` — the pin figure the coinbase *declares*. -/
  declaredPin : Nat
  /-- `pow_params.effective_value` — the spendable value the coinbase note commits. -/
  effective : Nat
  /-- `pow_params.input.value` — the coinbase call's own input value. -/
  inputValue : Nat
  /-- The accepted uncles' `pin_confirmed` values. -/
  unclePins : List Nat
  /-- The uncle-mint calls' `input.value` values. -/
  uncleNotes : List Nat

/-- Σ of the pins payable to the block's uncles. -/
def sumPins (s : Split) : Nat := s.unclePins.sum

/-- Σ of the values the block's uncle-mint notes declare. -/
def sumNotes (s : Split) : Nat := s.uncleNotes.sum

/-- **The accept predicate**: the five equations the four enforcement sites impose, as one conjunction.
    A block whose split does not satisfy all five is refused; which site refuses it is a diagnostic,
    not a difference in the rule. -/
def accepts (s : Split) : Prop :=
  s.totalReward + sumPins s = s.base ∧
  s.declaredPin = sumPins s ∧
  s.effective + sumPins s = s.base ∧
  s.effective + s.declaredPin = s.inputValue ∧
  sumNotes s = sumPins s

/-! ===== Part 2 — what the predicate delivers ===== -/

/-- **The conservation law, and the reason the checks exist.** The block's total *spendable* issuance —
    the canonical note plus every uncle note — is exactly the emission schedule's value. Nothing is
    created and nothing is stranded. -/
@[axiom_budget 0]
theorem accepts_spendable_total (s : Split) (h : accepts s) :
    s.effective + sumNotes s = s.base := by
  obtain ⟨_, _, h3, _, h5⟩ := h
  omega

/-- The header's reward figure and the note's spendable value agree: `block_acceptor`'s equation 1 and
    equation 3 pin the same quantity from two directions. -/
@[axiom_budget 0]
theorem accepts_total_reward_eq_effective (s : Split) (h : accepts s) :
    s.totalReward = s.effective := by
  obtain ⟨h1, _, h3, _, _⟩ := h
  omega

/-- **The host implies the schedule check.** `OBL-C4` records that `input.value == expected_reward(H)`
    is checked only in WASM, so "a reader who looks only at the host's validator would find the split
    and not the schedule". The host's three equations put the schedule *back*: they are sufficient for
    it. So the WASM guard is defence in depth, not the sole home of the rule. -/
@[axiom_budget 0]
theorem accepts_implies_schedule (s : Split) (h : accepts s) : s.inputValue = s.base := by
  obtain ⟨_, h2, h3, h4, _⟩ := h
  omega

/-- **The pin sum is bounded by the reward it splits.** This is why `total_accepted_pin`'s `checked_add`
    is an error branch rather than a wrap in the interesting direction: an *accepted* block's pin sum
    cannot exceed `base` at all, so nothing is silently clamped into a value the guard would agree
    with. -/
@[axiom_budget 0]
theorem accepts_pins_bounded (s : Split) (h : accepts s) : sumPins s ≤ s.base := by
  obtain ⟨_, _, h3, _, _⟩ := h
  omega

/-! ===== Part 3 — which check is load-bearing, and which are redundant ===== -/

/-- The **over-mint witness**: every value-side check compliant, the schedule check holding, the pins
    declared and accounted for — and the uncle notes emitting nothing. Named because it is what
    `note_sum_is_load_bearing` refutes with, and because the attack it encodes is the one
    `chain_model.py` describes in prose. -/
def overMintSplit : Split :=
  { base := 100, totalReward := 70, declaredPin := 30, effective := 70,
    inputValue := 100, unclePins := [30], uncleNotes := [0, 0] }

/-- **The uncle-note sum is the load-bearing check, and no weakening rescues it.** The statement is a
    refutation of the *universal*: there is no proof of the conservation law from the other four
    equations, with the schedule implied. The witness satisfies everything except the note sum — a
    compliant split, a compliant header, the schedule check, and uncle notes that emit nothing — and
    the law fails on it. This is `chain_model.py`'s own analysis, mechanized. -/
@[axiom_budget 0]
theorem note_sum_is_load_bearing :
    ¬ (∀ s : Split, s.totalReward + sumPins s = s.base →
        s.declaredPin = sumPins s →
        s.effective + sumPins s = s.base →
        s.effective + s.declaredPin = s.inputValue →
        s.effective + sumNotes s = s.base) := by
  intro h
  have hw := h overMintSplit
  norm_num [overMintSplit, sumPins, sumNotes] at hw

/-- **And the three value checks are over-determined.** Given the schedule, equation 4 plus equation 3
    give equation 2 — so the "reward theft" check is implied rather than independent. -/
@[axiom_budget 0]
theorem declaredPin_eq_of_others (s : Split)
    (hsched : s.inputValue = s.base)
    (h3 : s.effective + sumPins s = s.base)
    (h4 : s.effective + s.declaredPin = s.inputValue) :
    s.declaredPin = sumPins s := by
  omega

/-- Given the schedule, equation 2 plus equation 4 give equation 3. -/
@[axiom_budget 0]
theorem split_eq_of_others (s : Split)
    (hsched : s.inputValue = s.base)
    (h2 : s.declaredPin = sumPins s)
    (h4 : s.effective + s.declaredPin = s.inputValue) :
    s.effective + sumPins s = s.base := by
  omega

/-- Given the schedule, equation 2 plus equation 3 give equation 4 — which is the direction that says
    the host's own equations imply `validation.rs`'s check rather than needing it. -/
@[axiom_budget 0]
theorem note_split_eq_of_others (s : Split)
    (hsched : s.inputValue = s.base)
    (h2 : s.declaredPin = sumPins s)
    (h3 : s.effective + sumPins s = s.base) :
    s.effective + s.declaredPin = s.inputValue := by
  omega

/-! ===== Part 4 — non-vacuity: a legal split, and the structural bounds' limit =====

Both sides of the predicate are inhabited. `legalSplit` is a block that pays one uncle at depth 1 half
the base reward, with every quantity in agreement — so `accepts` is satisfiable and the conservation law
is not true of nothing, and `legal_split_spends_exactly_the_schedule` is the law *applied* to it rather
than restated. `overMintSplit` is the other side: fully compliant values with uncle notes that emit
nothing. And `structural_bounds_do_not_bound_the_pin` is the counterexample that makes
`accepts_pins_bounded` and the split check *content* rather than formality: the code's own structural
bounds admit an uncle set whose pins exceed the reward. -/

/-- A block paying one accepted uncle at depth 1: `pin = base/2`, canonical note `base - base/2`,
    header reward equal to the note's value, coinbase input value equal to the schedule. -/
def legalSplit : Split :=
  { base := 100, totalReward := 50, declaredPin := 50, effective := 50,
    inputValue := 100, unclePins := [50], uncleNotes := [50] }

/-- The split is accepted — so the predicate is satisfiable, and the laws of Part 2 are not laws about
    an empty domain. -/
@[axiom_budget 0]
theorem legalSplit_accepts : accepts legalSplit := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> norm_num [legalSplit, sumPins, sumNotes]

/-- **And the conservation law holds on it** — the law *applied*, not restated, so this is the
    non-vacuity witness for Part 2 rather than a second copy of it. -/
@[axiom_budget 0]
theorem legal_split_spends_exactly_the_schedule :
    legalSplit.effective + sumNotes legalSplit = legalSplit.base :=
  accepts_spendable_total legalSplit legalSplit_accepts

/-- **The structural bounds do not bound the pin, and this is the counterexample made about them.**
    There is an uncle *count* within `MAX_UNCLE_COUNT` whose pins at depth 1 exceed the reward — every
    structural rule `validation.rs` enforces is satisfied by it. So the split check is load-bearing
    arithmetic rather than a formality, and `compute_reward`'s `checked_sub` is the enforcement.

    **Budget 1, and the annotation was first written 0.** These two witnesses are the module's only
    declarations whose stated budget measurement corrected — `norm_num` over an existential with a
    multiplication reaches `Classical.choice` where the closed-form bounds above do not. The
    docstrings record what the collector measured, not what the arithmetic looks like. -/
@[axiom_budget 1]
theorem structural_bounds_do_not_bound_the_pin :
    ∃ n : Nat, n ≤ MAX_UNCLE_COUNT ∧ splitForUncle 100 1 * n > 100 :=
  ⟨3, by norm_num [MAX_UNCLE_COUNT, splitForUncle]⟩

/-- The two halves together, so neither half is asserted of nothing: the schedule's own total is
    bounded, and the code's structural bounds do not impose the distinctness that bound needs. -/
@[axiom_budget 1]
theorem schedule_bound_needs_distinctness :
    halvings 100 MAX_UNCLE_DEPTH ≤ 100 ∧
    (∃ n : Nat, n ≤ MAX_UNCLE_COUNT ∧ splitForUncle 100 1 * n > 100) :=
  ⟨halvings_window_le 100, structural_bounds_do_not_bound_the_pin⟩

end Consensus.CoinbaseSplit
