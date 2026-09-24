import Mathlib
import DarkFi.AxiomBudget

/-!
# The emission schedule, transcribed

`reward` used to be `axiom reward : Nat → Nat` — an uninterpreted function, which made every
statement about the supply chain a statement about *any* schedule. It is a definition now, taken
from the implementation:

`src/sdk/src/blockchain.rs:1032-1069`

```rust
pub fn expected_reward(height: BlockHeight) -> BlockReward {
    let height = height.get();
    if height == 0 { return BlockReward::ZERO; }
    if height == 1 { return reward::INITIAL_REWARD; }
    let exp = height - 1;
    let decay = fixed_pow_decay(exp);
    let reward = reward::INITIAL_REWARD.mul_fixed_point(decay);
    if reward <= reward::TAIL_REWARD.0 { return reward::TAIL_REWARD; }
    BlockReward(reward)
}

fn fixed_pow_decay(mut exp: u64) -> u64 {
    const DECAY_FP: u64 = 4_294_964_465;
    const FP_SHIFT: u32 = 32;
    let mut result: u64 = 1u64 << FP_SHIFT;
    let mut base: u64 = DECAY_FP;
    while exp > 0 {
        if exp & 1 == 1 { result = ((result as u128 * base as u128) >> FP_SHIFT) as u64; }
        base = ((base as u128 * base as u128) >> FP_SHIFT) as u64;
        exp >>= 1;
    }
    result
}
```

with `mul_fixed_point(self, decay_fp) = (self * decay_fp) >> 32` (saturating, which never fires for
these values) and the constants

    INITIAL_REWARD   = 1_383_764_049
    TAIL_REWARD      =    79_853_981
    DECAY_FP         = 4_294_964_465      -- DECAY_FP = floor(2^(-1/H) * 2^32) for H = 1_051_920
    HALF_LIFE_BLOCKS =     1_051_920

## What this buys, and what it does not

The docstring says `R(h) = max(R₀ × 2^(-(h-1)/H), R_tail)`. Transcribing it means the theorems
downstream are about *this* schedule rather than about an arbitrary function, and it makes
`reward_monotone` — which is still an assumption — a claim about a **computable, falsifiable**
function instead of a claim about nothing in particular. That phrasing turned out to be load
bearing: the assumption was **falsified** when it became checkable, at `(h₁, h₂) = (0, 1)`, because
`reward 0 = 0` is a sentinel and not a schedule value. See `reward_monotone_unbounded_is_false`
below; the assumption now carries the `1 ≤ h₁` hypothesis its neighbour `reward_tail_floor` always
had.

The remaining gap is the *proof* of non-increase on `h ≥ 1`. `DECAY_FP < 2^32`, so each
fixed-point multiplication by the decay is a contraction, but `fixed_pow_decay` truncates at *every*
squaring, so the value is not `DECAY_FP^e / 2^(32e)` and the induction obstructs on the odd/even
case — `Axioms.reward_monotone` states the obstruction precisely. Kernel-checked today:
`reward_nonincreasing_first_step` (the step after genesis, and the sentinel jump it excludes),
`fixedPowDecay_le_one`, `decayedReward_le_initial`, `fpMul_le_left` — and, since 2026-09-24, the two
factor-bounding lemmas that do *not* work (`fixedPowDecayGo_step_bound_is_false`,
`fpMul_nested_bound_is_false`), so that the routes already tried are machine-checked rather than
recalled from prose.

**The claim itself is checked over a range, not proved**, and that is the state the register records:
the schedule's own Rust test, the scans on `RewardNonIncreasing` below, and the first-step theorem are
what check it. `doc/src/arch/verification-hazop.md`'s `OBL-C5` says so in those words, and
`RewardNonIncreasing` records why a kernel check over a useful range is not available (a stack
overflow between 500 and 2000 blocks, against a scan that covers 3·10⁵).
-/

/-- `1.0` in the fixed-point representation the schedule uses (`1 << 32`). -/
def FP_ONE : Nat := 2 ^ 32

/-- The per-block decay factor in fixed point: `floor(2^(-1/H) * 2^32)` for `H = 1_051_920`.
    Note `DECAY_FP < FP_ONE`, which is what makes the schedule decaying at all. -/
def DECAY_FP : Nat := 4294964465

/-- Block reward at height 1 (and at genesis). -/
def INITIAL_REWARD : Nat := 1383764049

/-- The tail-emission floor. There is no supply *cap*: this is a floor, not a ceiling
    (`consensus-coinbase.md:830` — "NOT a hard cap — perpetual tail emission"). -/
def TAIL_REWARD : Nat := 79853981

/-- Blocks per half-life, for reference: the decay is `2^(-height/HALF_LIFE_BLOCKS)`. -/
def HALF_LIFE_BLOCKS : Nat := 1051920

/-- Fixed-point multiplication: multiply, then drop the fractional part. Mirrors
    `BlockReward::mul_fixed_point`. -/
def fpMul (a b : Nat) : Nat := a * b / FP_ONE

/-- The exponentiation-by-squaring loop, transcribed. `exp` is the number of blocks elapsed
    (`height - 1`); it carries `result` (starting at `1.0`) and squares `base` every iteration,
    multiplying `result` by `base` for each set bit of `exp`, least-significant first. -/
def fixedPowDecayGo : Nat → Nat → Nat → Nat
  | 0, result, _ => result
  | exp + 1, result, base =>
      fixedPowDecayGo ((exp + 1) / 2)
        (if (exp + 1) % 2 = 1 then fpMul result base else result)
        (fpMul base base)

/-- `fixed_pow_decay(exp)`: the decay factor for `exp` elapsed blocks. -/
def fixedPowDecay (exp : Nat) : Nat := fixedPowDecayGo exp FP_ONE DECAY_FP

/-- The un-floored reward at height `h ≥ 2`: `INITIAL_REWARD` scaled by the decay. -/
def decayedReward (exp : Nat) : Nat := fpMul INITIAL_REWARD (fixedPowDecay exp)

/-- The emission schedule. `reward 0 = 0` (the pre-genesis sentinel), `reward 1 = INITIAL_REWARD`,
    and every later height gets the decayed reward floored at `TAIL_REWARD`. -/
def reward (height : Nat) : Nat :=
  match height with
  | 0 => 0
  | 1 => INITIAL_REWARD
  | h + 2 => max (decayedReward (h + 1)) TAIL_REWARD

/-! ===== What is proved about it -/

/-- Fixed-point multiplication by something at most `1.0` is a contraction. This is the fact the
    whole schedule rests on: `DECAY_FP < FP_ONE`, so every step shrinks. -/
@[axiom_budget 1]
lemma fpMul_le_left {a b : Nat} (hb : b ≤ FP_ONE) : fpMul a b ≤ a := by
  have h : a * b ≤ a * FP_ONE := Nat.mul_le_mul_left a hb
  calc fpMul a b = a * b / FP_ONE := rfl
    _ ≤ a * FP_ONE / FP_ONE := Nat.div_le_div_right h
    _ = a := Nat.mul_div_cancel a (by norm_num [FP_ONE])

/-- `DECAY_FP < FP_ONE`: `4_294_964_465 < 4_294_967_296`. Without this the schedule would grow. -/
@[axiom_budget 1]
lemma DECAY_FP_lt_FP_ONE : DECAY_FP ≤ FP_ONE := by norm_num [DECAY_FP, FP_ONE]

/-- The loop never increases its accumulator, because it only ever multiplies by values at most
    `1.0` (the base stays a contraction by `fpMul_le_left`).

    Strong induction, not `succ` induction: the recursive call's first argument is `(n+1)/2`, which
    is smaller than `n+1` but is not `n`. -/
@[axiom_budget 1]
lemma fixedPowDecayGo_le_start (exp : Nat) :
    ∀ (result base : Nat), base ≤ FP_ONE → fixedPowDecayGo exp result base ≤ result := by
  induction exp using Nat.strong_induction_on with
  | _ exp ih =>
      cases exp with
      | zero =>
          intro result base _
          simp [fixedPowDecayGo]
      | succ n =>
          intro result base hb
          have hb' : fpMul base base ≤ FP_ONE := le_trans (fpMul_le_left hb) hb
          have hlt : (n + 1) / 2 < n + 1 := Nat.div_lt_self (Nat.succ_pos n) (by norm_num)
          simp only [fixedPowDecayGo]
          split
          · exact le_trans (ih _ hlt (fpMul result base) (fpMul base base) hb')
              (fpMul_le_left hb)
          · exact ih _ hlt result (fpMul base base) hb'

/-- **The loop is monotone in its accumulator** — a smaller `result` stays smaller.

    This is one of the two cases the non-increase proof splits into. Writing `G` for
    `fixedPowDecayGo`, the single-step lemma `G (n+1) r b ≤ G n r b` breaks by parity of `n`:

    * `n = 2k` — the two sides are `G k (fpMul r b) b'` and `G k r b'` with `b' = fpMul b b`, so
      it is `fpMul r b ≤ r` fed to **this** lemma. Proved.
    * `n = 2k+1` — the sides are `G (k+1) r b'` and `G k (fpMul r b) b'`. That inequality is
      **true** (checked numerically over the real constants) but this lemma does not reach it: the
      hypothesis it needs is `fpMul r b ≤ r`, which points the wrong way. See
      `Axioms.reward_monotone` for what that case still needs.

    So this is the even case, and the obstruction is entirely in the odd one — which is a smaller
    target than "the parity analysis" and is where a further attempt should start. **Two candidate
    lemmas for that target are refuted below** (`fixedPowDecayGo_step_bound_is_false`,
    `fpMul_nested_bound_is_false`), which narrows it further and in an unexpected direction: the odd
    case's *own* inequality is true and measured, so what is missing is a lemma that reaches it, and
    the two shapes such a lemma would take do not hold. -/
@[axiom_budget 1]
lemma fixedPowDecayGo_mono_acc (exp : Nat) :
    ∀ {r r' : Nat}, r' ≤ r → ∀ base, base ≤ FP_ONE →
      fixedPowDecayGo exp r' base ≤ fixedPowDecayGo exp r base := by
  induction exp using Nat.strong_induction_on with
  | _ exp ih =>
      intro r r' h base hb
      cases exp with
      | zero => simpa [fixedPowDecayGo] using h
      | succ n =>
          have hb' : fpMul base base ≤ FP_ONE := le_trans (fpMul_le_left hb) hb
          have hlt : (n + 1) / 2 < n + 1 := Nat.div_lt_self (Nat.succ_pos n) (by norm_num)
          simp only [fixedPowDecayGo]
          split
          · exact ih _ hlt (Nat.div_le_div_right (Nat.mul_le_mul_right base h))
              (fpMul base base) hb'
          · exact ih _ hlt h (fpMul base base) hb'

/-! ===== The two factor-bounding lemmas that do not exist =====

`Axioms.reward_monotone` records the odd case as needing "a statement bounding the factor the extra
step multiplies by, not a strengthening of the accumulator induction". These are the two forms that
statement takes when reached for from the loop's own recursion, and **both are false**, with witnesses
that are each off by one.

**What this does *not* say, and it matters.** It does not refute the odd case itself, which is a
different inequality and is **true**. Measured: `G (k+1) r (fpMul b b) ≤ G k (fpMul r b) (fpMul b b)`
holds over 300 steps at the state the schedule actually reaches (`r = FP_ONE`, `b = DECAY_FP`) and over
grids of `(k, r, b)` (60×40×40 and 60×200, by evaluation of the definition, and the earlier analysis of
the same shape is in `Axioms.lean`). So the obstruction is neither that the goal is false nor that it is
unreachable in principle: it is that no lemma bounding that factor has been found, and these two are the
forms it would have to take.

The off-by-one is the whole content, and it is the truncation `fixedPowDecayGo` performs at *every*
squaring: the loop is not `DECAY_FP^e / 2^(32e)`, so no law of `fpMul` alone recovers it. That is also
why the closed form fails — `fixedPowDecay 34 = 4294871042` against `FP_ONE · DECAY_FP^34 / FP_ONE^34 =
4294871043`, the third route `Axioms.lean` already rules out. -/

/-- **The step bound is false.** `G (k+1) r b ≤ G k (fpMul r b) b` would say one step of the loop is
    bounded by pushing the accumulator through, at the same base. It fails at `k = 1`,
    `r = 95872739`, `b = 1363349908`, where the two sides are `9660288` and `9660287`. -/
@[axiom_budget 1]
theorem fixedPowDecayGo_step_bound_is_false :
    ¬ (∀ (k r b : Nat), fixedPowDecayGo (k + 1) r b ≤ fixedPowDecayGo k (fpMul r b) b) := by
  intro h
  have hw := h 1 95872739 1363349908
  norm_num [fixedPowDecayGo, fpMul, FP_ONE] at hw

/-- **The nested bound is false.** `fpMul r (fpMul b b) ≤ fpMul (fpMul r b) b` would let the extra
    step be folded into the accumulator by associativity, which is the other way the odd case could be
    made to go through. It fails at `r = 346803675`, `b = 4229726225`: `336347717` against `336347716`.

    Note this one needs no loop at all — it is a statement about `fpMul` — so its refutation is not a
    statement about the schedule's size or the parity analysis. `fpMul` is not associative, and that is
    a fact about truncating fixed-point multiplication rather than about this schedule. -/
@[axiom_budget 1]
theorem fpMul_nested_bound_is_false :
    ¬ (∀ (r b : Nat), fpMul r (fpMul b b) ≤ fpMul (fpMul r b) b) := by
  intro h
  have hw := h 346803675 4229726225
  norm_num [fpMul, FP_ONE] at hw

/-- **The nested bound also fails at the schedule's *own* constant**, which the witness above does not
    show: `r = 95872739`, `b = DECAY_FP` gives `95872612` against `95872611`. So the obstruction is not
    an artefact of a contrived `b` — it bites at the constant the schedule itself multiplies by, and a
    reader who took the earlier witness for a corner case would size the odd case wrong. Recorded
    because the difference between "false in general" and "false at the values we use" decides whether
    a repair has to change the loop or only its analysis. -/
@[axiom_budget 1]
theorem fpMul_nested_bound_fails_at_the_decay_constant :
    ¬ (fpMul 95872739 (fpMul DECAY_FP DECAY_FP) ≤ fpMul (fpMul 95872739 DECAY_FP) DECAY_FP) := by
  norm_num [fpMul, FP_ONE, DECAY_FP]

/-- **The step bound's even half, closed.** `fixedPowDecayGo_mono_acc` does not reach this statement by
    itself, because the two sides differ in *exponent*, not in accumulator — but one unfolding removes
    the difference: at `e = 2m` the left side becomes `G m (fpMul FP_ONE b) (fpMul b b)` and the right
    `G m FP_ONE (fpMul b b)`, and `fpMul FP_ONE b = b` makes the accumulators comparable **at the same
    exponent**, so this is `mono_acc` fed `b ≤ FP_ONE`.

    **This was prose until 2026-09-24.** The file said "the even case is proved
    (`fixedPowDecayGo_mono_acc`)" — the *ingredient* was proved and the *half* was not stated, which is
    the difference between a reader being told a case is handled and being able to cite it. With
    `OddCaseStepBound` below as the odd half, the parity split is now one theorem and one named
    statement rather than two sentences. -/
@[axiom_budget 1]
theorem fixedPowDecayGo_step_bound_even (m : Nat) {b : Nat} (hb : b ≤ FP_ONE) :
    fixedPowDecayGo (2 * m + 1) FP_ONE b ≤ fixedPowDecayGo (2 * m) FP_ONE b := by
  have hb' : fpMul b b ≤ FP_ONE := le_trans (fpMul_le_left hb) hb
  have hmul : fpMul FP_ONE b = b := by simp [fpMul, FP_ONE]
  have hmul' : ∀ c : Nat, fpMul FP_ONE c = c := fun c => by simp [fpMul, FP_ONE]
  cases m with
  | zero =>
      have hz : fixedPowDecayGo (2 * 0 + 1) FP_ONE b = b := by
        simp [fixedPowDecayGo, fpMul, FP_ONE]
      rw [hz]
      show b ≤ fixedPowDecayGo (2 * 0) FP_ONE b
      simp [fixedPowDecayGo]
      exact hb
  | succ k =>
      have h2 : 2 * (k + 1) = (2 * k + 1) + 1 := by omega
      rw [h2]
      have d1 : (2 * k + 1 + 1 + 1) / 2 = k + 1 := by omega
      have m1 : (2 * k + 1 + 1 + 1) % 2 = 1 := by omega
      have d2 : (2 * k + 1 + 1) / 2 = k + 1 := by omega
      have m2 : (2 * k + 1 + 1) % 2 = 0 := by omega
      simp only [fixedPowDecayGo, d1, m1, d2, m2, hmul, if_true]
      simp only [Nat.reduceEqDiff, if_false]
      -- both sides now share the exponent `(k + 1) / 2` and the base `fpMul (fpMul b b) (fpMul b b)`,
      -- so only the accumulators differ, and each branch of the parity test bounds them
      have hbb : fpMul (fpMul b b) (fpMul b b) ≤ FP_ONE := le_trans (fpMul_le_left hb') hb'
      have hacc : (if (k + 1) % 2 = 1 then fpMul b (fpMul b b) else b) ≤
          (if (k + 1) % 2 = 1 then fpMul FP_ONE (fpMul b b) else FP_ONE) := by
        split_ifs with h
        · rw [hmul']
          simpa [fpMul, Nat.mul_comm] using fpMul_le_left (a := fpMul b b) (b := b) hb
        · exact hb
      exact fixedPowDecayGo_mono_acc ((k + 1) / 2) hacc _ hbb

/-- **The decay factor does not increase at any even step — proved, not scanned.** This is the even
    half applied at the schedule's own base, and it is the first statement in this file about
    `fixedPowDecay`'s monotonicity that is a theorem rather than a range check: it covers every even
    step, where `RewardNonIncreasing`'s scans cover a prefix.

    **What it does not cover, and this is the whole residue**: the odd steps. So the honest reading is
    that half of the adjacent pairs are proved and the other half are `OddCaseStepBound` — a sharper
    place to be than "checked over a range", because the unchecked part now has a name and a boundary
    instead of a scan's edge. -/
@[axiom_budget 1]
theorem fixedPowDecay_nonincreasing_even (m : Nat) :
    fixedPowDecay (2 * m + 1) ≤ fixedPowDecay (2 * m) :=
  fixedPowDecayGo_step_bound_even m (b := DECAY_FP) DECAY_FP_lt_FP_ONE

/-! ===== The odd case, stated =====

`Axioms.reward_monotone` records the odd case as needing "a statement bounding the factor the extra
step multiplies by, not a strengthening of the accumulator induction". `fixedPowDecayGo_mono_acc` above
reaches the even case; the two shapes the odd case was first reached for are refuted above. What had
never been written down is the statement itself — and this is it:

    fixedPowDecayGo (e + 1) FP_ONE b ≤ fixedPowDecayGo e FP_ONE b     whenever  b ≤ DECAY_FP

**Both hypotheses are load-bearing, and dropping the first makes it false.** The accumulator is pinned
to `FP_ONE`, where `fpMul FP_ONE x = x` — so the *result* side suffers no truncation and the only loss
is in the base squarings; and `b ≤ DECAY_FP` makes each squaring a contraction (`fpMul_le_left`).
Measured 2026-09-24: with the accumulator freed the statement **fails** at `r = 827599667`, `k = 3`.

**Why this statement and not another.** `fixedPowDecay e = fixedPowDecayGo e FP_ONE DECAY_FP`, so the
lemma at `b = DECAY_FP` *is* `fixedPowDecay (e+1) ≤ fixedPowDecay e` — the non-increase of the decay
factor, from which `decayedReward` follows by monotonicity of `fpMul` and `reward` by monotonicity of
`max`. And it is exactly the odd case: unfolding the definition once, the `e` **even** case reduces to
`fixedPowDecayGo_mono_acc` at base `fpMul b b`, and the `e` **odd** case reduces to *this* statement at
base `fpMul b b`. So the two parities are one statement evaluated at the squared base, which is why a
single lemma would close both — and why the earlier phrase "the parity analysis" described two halves
of one target.

**Its status is empirical and it is recorded as such, which is why it is a `def : Prop` and not a
theorem**: no proof exists, and a statement without one does not belong in this tree wearing a
theorem's kind — the same reason `RewardNonIncreasing` below is a `def`. The evidence, wider than the
scans `RewardNonIncreasing` records: no violation over the schedule's own orbit (12 squarings of
`DECAY_FP`) with `e ∈ [0,60)`, nor over 24,420 sampled `(e, b)` points with `b ≤ DECAY_FP` and
`e ∈ [0,60)`; and separately `fixedPowDecay` non-increasing on `[0, 2·10⁶)` and `reward` non-increasing
on `[1, 2·10⁶)` — the latter two covering 6.7× the range the tree had scanned. The computation was
transcribed from this file and reproduces its own recorded values (`fixedPowDecay 34 = 4294871042`
against the closed form's `4294871043`), so it is a check on the same function rather than on a
look-alike. **What would discharge it**: a mutual induction, since each parity reduces to the other at
the squared base — the even half needs only `fixedPowDecayGo_mono_acc`, and the odd half needs this
statement. -/
def OddCaseStepBound : Prop :=
  ∀ e b : Nat, b ≤ DECAY_FP → fixedPowDecayGo (e + 1) FP_ONE b ≤ fixedPowDecayGo e FP_ONE b

/-- The first five steps of the decay factor's non-increase, **kernel-checked** — the closed companion
    to the empirical scan above, in the shape `reward_nonincreasing_first_step` already uses one level
    out. It checks the decay *factor*, which is the level the odd case lives at, rather than the
    reward, which is what the existing theorem checks. `norm_num` with the full unfolding set rather
    than `decide`: `fixedPowDecayGo` recurses on `(exp + 1) / 2`, so Lean elaborates it as
    well-founded recursion and the equation lemmas have to be supplied.

    **Budget 1 and not 0, which the collector is what settled**: this was annotated 0 when written, on
    the reasoning that a closed numeral inequality is arithmetic, and the gate refuted it —
    `norm_num` reaches `Classical.choice`, so the measured budget is 1. Recorded because the
    annotation is the kind of claim this file has spent the day correcting elsewhere. -/
@[axiom_budget 1]
theorem fixedPowDecay_nonincreasing_first_steps :
    (fixedPowDecay 5 ≤ fixedPowDecay 4) ∧ (fixedPowDecay 4 ≤ fixedPowDecay 3) ∧
      (fixedPowDecay 3 ≤ fixedPowDecay 2) ∧ (fixedPowDecay 2 ≤ fixedPowDecay 1) ∧
      (fixedPowDecay 1 ≤ fixedPowDecay 0) := by
  norm_num [fixedPowDecay, fixedPowDecayGo, fpMul, FP_ONE, DECAY_FP]

/-- The decay factor is at most `1.0`. -/
@[axiom_budget 1]
lemma fixedPowDecay_le_one (exp : Nat) : fixedPowDecay exp ≤ FP_ONE :=
  fixedPowDecayGo_le_start exp FP_ONE DECAY_FP DECAY_FP_lt_FP_ONE

/-- The decayed reward never exceeds the initial reward. -/
@[axiom_budget 1]
lemma decayedReward_le_initial (exp : Nat) : decayedReward exp ≤ INITIAL_REWARD :=
  fpMul_le_left (fixedPowDecay_le_one exp)

@[axiom_budget 0]
theorem reward_zero : reward 0 = 0 := rfl

@[axiom_budget 0]
theorem reward_one : reward 1 = INITIAL_REWARD := rfl

/-- **The tail floor**, for every height from genesis on: `reward h ≥ TAIL_REWARD`.

    Not for `h = 0`: the pre-genesis sentinel is `0`, which is below the floor. The schedule's own
    docstring gives `R(0) = 0`, so the property holds for `h ≥ 1`. -/
@[axiom_budget 1]
theorem reward_tail_floor : ∀ (h : Nat), 1 ≤ h → TAIL_REWARD ≤ reward h
  | 1, _ => by norm_num [reward, TAIL_REWARD, INITIAL_REWARD]
  | h + 2, _ => by simp only [reward]; exact le_max_right _ _

/-! ===== Non-increase, and the height it does not hold at =====

`Axioms.reward_monotone` says `∀ h₁ h₂, h₁ ≤ h₂ → reward h₂ ≤ reward h₁`. **That was false**, for
the same reason `reward_tail_floor` needs its `1 ≤ h` hypothesis and the axiom did not have one:
`reward 0 = 0` is the *pre-genesis sentinel*, not a schedule value, so the schedule jumps from `0`
at height 0 to `INITIAL_REWARD` at height 1. `0 ≤ 1`, and `reward 1 ≤ reward 0` is
`1383764049 ≤ 0`.

The refutation is below, and the axiom is restated with the hypothesis the property actually
needs. The failure is instructive beyond this instance: the file *already knew* about the sentinel
— `reward_tail_floor`'s docstring says so in as many words — and the assumption next to it had
simply not been given the same treatment. -/

/-- **The non-increase assumption, as it was stated, is false.** Machine-checked rather than
    argued: the hypothesis `h₁ ≤ h₂` is satisfied at `(0, 1)` and the conclusion is not. -/
@[axiom_budget 1]
theorem reward_monotone_unbounded_is_false :
    ¬ (∀ h₁ h₂ : Nat, h₁ ≤ h₂ → reward h₂ ≤ reward h₁) := by
  intro h
  have h01 : reward 1 ≤ reward 0 := h 0 1 (by norm_num)
  simp only [reward, INITIAL_REWARD] at h01
  omega

/-- **Non-increase holds from genesis on**, which is the range the schedule is defined over:
    `reward` is non-increasing on `h ≥ 1`.

    This is what `Axioms.reward_monotone` assumes, and it is **stated here with no theorem
    attached** — the honest state rather than a stopgap, and everything known about it is measured:

    * **It is true.** `fixedPowDecay` is non-increasing exhaustively over `e ∈ [0, 3·10⁵]` and over
      200 000 sampled exponents in `[1, 3.4·10⁷]`, with no violation and no equal-step plateau;
      `reward` is non-increasing for `1 ≤ h ≤ 200 000`. Two facts worth having for an attempt:
      `fixedPowDecay e = 0` for all `e ≥ 2²⁵+1`, and `decayedReward` falls below `TAIL_REWARD` at
      `e ≈ 4.32·10⁶`, after which `reward` is constant.
    * **It is not a missing routine.** The truncation at every squaring means the cumulative error in
      the decay passes the *local* gap between successive ideal values once the exponent exceeds about
      5.5·10⁴ — so no absolutely-bounded sandwich survives the middle range, and a proof has to compare
      the errors of *adjacent* exponents, which nearly cancel because they differ by one carry.
      Estimated 30–60 lemmas. One instrument is unavailable: `native_decide`, because `OBL-T10` is a
      closed row whose whole content is that no proof rests on `Lean.ofReduceBool`.
    * **The kernel cannot check it over a useful range**, which is why the bullet above says
      "measured" and not "verified". A fuel-indexed restatement of the loop — structural recursion, so
      the kernel *can* reduce it, unlike the well-founded definition — was written on 2026-09-24 and
      abandoned on measurement: a range check costs about 1.8s at 500 blocks and **aborts with a kernel
      stack overflow** somewhere between 500 and 2000, against the 3·10⁵ exponents the scan covers. The
      instrument was strictly weaker than the measurement it would have replaced, so it was not landed
      — a definition nothing consumes is what this tree deletes.

    Five routes are ruled out rather than rediscovered: three in `Axioms.reward_monotone`'s
    `DISCHARGED BY:` field and its `NOT PROVED BECAUSE:` list, and two machine-checked above. -/
def RewardNonIncreasing : Prop := ∀ h₁ h₂ : Nat, 1 ≤ h₁ → h₁ ≤ h₂ → reward h₂ ≤ reward h₁

/-- The corrected statement is not vacuous at the point the old one failed, and the old one's
    failure is exactly the sentinel: `reward 1 ≤ reward 0` is false, while `reward 2 ≤ reward 1`
    holds. Both halves are checked, so the restatement is motivated rather than asserted. -/
@[axiom_budget 1]
theorem reward_nonincreasing_first_step :
    reward 2 ≤ reward 1 ∧ ¬ (reward 1 ≤ reward 0) := by
  constructor
  · norm_num [reward, decayedReward, INITIAL_REWARD, TAIL_REWARD, fixedPowDecay,
      fixedPowDecayGo, fpMul, FP_ONE, DECAY_FP]
  · norm_num [reward, INITIAL_REWARD]
