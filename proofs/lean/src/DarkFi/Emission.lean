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
`fixedPowDecay_le_one`, `decayedReward_le_initial`, `fpMul_le_left`. The schedule's own Rust test
covers a range; that test and the first-step theorem are what check the claim now.
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

    This is what `Axioms.reward_monotone` now assumes. It remains unproved — see that entry for
    the obstruction — but it is at least true, which its predecessor was not. -/
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
