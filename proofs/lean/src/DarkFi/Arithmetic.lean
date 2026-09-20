/-
# DarkFi Field Arithmetic Soundness Proofs

base_add (0x30), base_mul (0x31), base_sub (0x32) operate on
Pallas base field elements (F_p where p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1).

Key property: field arithmetic wraps around at p. For inputs in
the range [0, 2^253), field operations and integer operations coincide.
-/

import Mathlib
import DarkFi.AxiomBudget

-- The `2^65 < PALLAS_PRIME` and `2^128 < PALLAS_PRIME` comparisons evaluate numerals up to
-- `2^254`, past the default elaborator recursion depth. Raised for this file only; `norm_num`
-- still yields a kernel-checked proof, unlike the `native_decide` calls it replaces.
set_option maxRecDepth 10000

namespace Arithmetic

def PALLAS_PRIME : Int := 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1

/-
## Field Addition (0x30): c = a + b (mod p)

Constraint: c = a + b in the field (wraps at p).
For inputs in [0, 2^253), this is identical to integer addition
since a + b < 2^254 < p + 2^253.
-/
def base_add (a b : Int) : Int := (a + b) % PALLAS_PRIME

/-
## THEOREM: Base Addition Correctness

For inputs a, b in range [0, 2^64) (as constrained by range_check(64)),
integer addition and field addition produce the same result.
-/
@[axiom_budget 1]
theorem base_add_correctness (a b : Int) (ha : 0 ≤ a ∧ a < 2^64) (hb : 0 ≤ b ∧ b < 2^64) :
  base_add a b = a + b := by
  rcases ha with ⟨ha_low, ha_high⟩
  rcases hb with ⟨hb_low, hb_high⟩
  have hsum : a + b < PALLAS_PRIME := by
    -- a < 2^64, b < 2^64, so a + b < 2^65
    -- PALLAS_PRIME ≈ 2^254, so a + b ≪ p
    have hmax : a + b < 2^64 + 2^64 := by
      apply add_lt_add ha_high hb_high
    have h2_65 : (2 : Int)^64 + (2 : Int)^64 = (2 : Int)^65 := by
      ring
    have hp_gt : (2 : Int)^65 < PALLAS_PRIME := by
      -- PALLAS_PRIME ≈ 2^254 ≫ 2^65
      rw [PALLAS_PRIME]; norm_num
    calc
      a + b < (2 : Int)^64 + (2 : Int)^64 := hmax
      _ = (2 : Int)^65 := h2_65
      _ < PALLAS_PRIME := hp_gt
  rw [base_add]
  -- `Int.emod_eq_of_lt` takes two arguments (`0 ≤ a`, `a < b`), not a conjunction; the
  -- `constructor` that used to sit here was aimed at its first goal and found no constructor.
  exact Int.emod_eq_of_lt (add_nonneg ha_low hb_low) hsum

/-
## Field Subtraction (0x32): c = a - b (mod p)

Constraint: c = a - b in the field.
If a ≥ b, integer subtraction and field subtraction coincide.
If a < b, the result wraps around: c = p + (a - b).
-/
def base_sub (a b : Int) : Int := (a - b) % PALLAS_PRIME

/-
## THEOREM: Base Subtraction Correctness (a ≥ b case)

When a ≥ b, field subtraction equals integer subtraction.
-/
@[axiom_budget 1]
theorem base_sub_ge_case (a b : Int) (ha_ge_b : a ≥ b) (hb_nonneg : 0 ≤ b) (ha_lt_p : a < PALLAS_PRIME) :
  base_sub a b = a - b := by
  rw [base_sub]
  have h_nonneg : 0 ≤ a - b := sub_nonneg.mpr ha_ge_b
  have h_lt_p : a - b < PALLAS_PRIME := by
    -- a - b ≤ a (since b ≥ 0) and a < p, so a - b < p
    have hle : a - b ≤ a := by linarith
    exact lt_of_le_of_lt hle ha_lt_p
  -- Two arguments, not a conjunction: `exact ⟨…⟩` was building a pair where a `Prop` was
  -- expected, which is why this reported "expected type must be an inductive type with only
  -- one constructor" (and named `ℕ`, from the `≤` unfolding).
  exact Int.emod_eq_of_lt h_nonneg h_lt_p

/-
## Field Multiplication (0x31): c = a * b (mod p)

Constraint: c = a * b in the field.
For inputs in range [0, 2^64), a * b < 2^128 < p, so
integer and field multiplication coincide.
-/
def base_mul (a b : Int) : Int := (a * b) % PALLAS_PRIME

/-
## THEOREM: Base Multiplication Correctness (bounded inputs)

For inputs in [0, 2^64), integer product < 2^128 < p,
so no modular reduction needed.
-/
@[axiom_budget 1]
theorem base_mul_correctness_bounded (a b : Int)
  (ha : 0 ≤ a ∧ a < 2^64) (hb : 0 ≤ b ∧ b < 2^64) :
  base_mul a b = a * b := by
  rcases ha with ⟨ha_low, ha_high⟩
  rcases hb with ⟨hb_low, hb_high⟩
  have hprod : a * b < PALLAS_PRIME := by
    -- `nlinarith` from the four bounds. The previous `mul_lt_mul ha_high hb_high hb_low ha_low`
    -- did not match the lemma's argument order in this mathlib (it wanted `≤` where `<` was
    -- given, and `0 < b` where `0 ≤ b` was given).
    have hmax : a * b < (2^64 : Int) * (2^64 : Int) := by nlinarith
    have h128 : (2^64 : Int) * (2^64 : Int) = (2^128 : Int) := by ring
    have hp_gt : (2^128 : Int) < PALLAS_PRIME := by
      -- PALLAS_PRIME ≈ 2^254 ≫ 2^128
      rw [PALLAS_PRIME]; norm_num
    calc
      a * b < (2^64 : Int) * (2^64 : Int) := hmax
      _ = (2^128 : Int) := h128
      _ < PALLAS_PRIME := hp_gt
  rw [base_mul]
  exact Int.emod_eq_of_lt (mul_nonneg ha_low hb_low) hprod

/-
## Modular Arithmetic Congruence — a tautology deleted

    @[axiom_budget 0]
    theorem base_ops_are_congruent (a b : Int) (op : Int → Int → Int) :
      (op a b) % PALLAS_PRIME = (op a b) % PALLAS_PRIME := by rfl

both sides of which are the same expression. The heading above it claims "for inputs that MAY exceed
the field prime, the result is always congruent to the integer result modulo p" — that is a claim
about `base_add`/`base_sub`/`base_mul`, and it involves `% PALLAS_PRIME` once, not twice. The
statement as written says nothing about any opcode, because `op` is an arbitrary function parameter
and both sides are `op a b % PALLAS_PRIME`. Deleted rather than restated: the real content is
`Arithmetic.base_add_correctness` and its siblings below, which relate a `base_*` opcode's output to
its integer arithmetic rather than to itself. Recorded so the name is not re-added for its title.
-/

/-
## BaseDiv (0x58): Field Division via Fermat's Little Theorem

a / b = a * b^{p-2} mod p

Cost: ~254 squarings + ~251 multiplications (~505 constraints).

CORRESPONDENCE: src/zk/vm.rs:1503-1557 — BaseDiv computes a * b^{p-2}
via 253 squaring iterations (Fermat exponentiation).
-/

/-
## Division Correctness (Fermat) — assumption moved

For b ≠ 0 in F_p: (a * b^{p-2}) * b ≡ a (mod p).

This is no longer declared here. It lives in `DarkFi/Axioms.lean` as
`Arithmetic.base_div_mul_cancel`, under the same name, together with the four-field
annotation that states what is assumed, what would discharge it, and what breaks if it is
false. `Axioms.lean` is the only file in `proofs/lean/` permitted to contain an `axiom`, and
`script/check_lean_axioms.py` enforces that.

The comment that used to sit here claimed the reason for the assumption was that the project
"depends on core Lean 4 without Mathlib". That was false, and the corrected reason is in
`Axioms.lean`: the blocker is not Fermat's little theorem (mathlib has it) but the
un-mechanised primality of `PALLAS_PRIME`.
-/

/-
## THEOREM: Division by Zero Convention

Division by zero returns 0 in DarkFi, consistent with
the field convention in the Halo2 implementation.

When b = 0, BaseDiv returns 0 because the binary exponentiation
b^(p-2) computes 0^(p-2) = 0 for p > 2 (PALLAS_PRIME ≈ 2^254).
Then (a * 0) % p = 0. We prove this computationally since
PALLAS_PRIME is a concrete constant.
-/
/-- Division by zero returns 0, matching the Halo2 field convention: the binary exponentiation
    `b^(p-2)` gives `0^(p-2) = 0` for `p > 2`, so `(a * 0) % p = 0`.

    The exponent is a parameter of type `Nat`, not `PALLAS_PRIME - 2`. `PALLAS_PRIME : Int`, so
    `0 ^ (PALLAS_PRIME - 2)` would need `HPow Int Int _`, which does not exist — the previous
    statement could not elaborate at all, and the `native_decide` under it was deciding a
    malformed term. `hn : n ≠ 0` is exactly the hypothesis the argument uses. -/
@[axiom_budget 1]
theorem base_div_by_zero (a : Int) (n : Nat) (hn : n ≠ 0) :
    (a * (0 : Int) ^ n) % PALLAS_PRIME = 0 := by
  rw [zero_pow hn, mul_zero]
  exact Int.emod_eq_of_lt le_rfl (by rw [PALLAS_PRIME]; norm_num)

end Arithmetic
