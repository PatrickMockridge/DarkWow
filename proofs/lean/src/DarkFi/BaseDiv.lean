import Mathlib
import DarkFi.Axioms
import DarkFi.Arithmetic

/-!
# `base_div_mul_cancel`, discharged

`Arithmetic.base_div_mul_cancel` was an **axiom** until 2026-09-20:

    axiom base_div_mul_cancel (a b : Int) (hb : b % PALLAS_PRIME ≠ 0) :
      ((a * (b ^ (PALLAS_PRIME.toNat - 2))) % PALLAS_PRIME * b) % PALLAS_PRIME = a % PALLAS_PRIME

with `NOT PROVED BECAUSE:` naming the blocker as the *un-mechanised primality certificate* rather
than Fermat's little theorem, "which mathlib does have". That diagnosis was right, and it is why
this is a theorem now and not an axiom:

* **the arithmetic is Fermat's little theorem**, which mathlib has;
* **what it needs is `Fact (Nat.Prime PALLAS_MODULUS)`**, which `Axioms.lean` supplies as an
  instance derived from `pallasPrime`;
* so the axiom was never carrying arithmetic content — it was carrying the primality assumption,
  stated a second time in `Int` clothing.

It now rests on `pallasPrime` like everything else over the Pallas field, and the budget table says
so. Two things follow that are worth stating:

* **The assumption count drops by one.** `base_div_mul_cancel` had no consumers, so no theorem's
  budget moves, but the boundary is smaller.
* **A document claim becomes true.** `doc/src/philosophy/philosophy.md:39` and
  `doc/src/arch/quantum-os.md:64` describe `BaseDiv` as "Lean4-verified". `security-analysis.md:517`
  correctly listed `base_div_mul_cancel` among the assumptions rather than the theorems — that was
  the accurate one, and now the other two are accurate too, for the reason they give.

## Why this file exists separately

It needs both `Arithmetic.PALLAS_PRIME` (a definition, in `Arithmetic.lean`) and the `Fact` instance
(in `Axioms.lean`), and `Arithmetic.lean` must not import `Axioms.lean` — `Axioms` imports it. So
the theorem cannot live in either; it lives here, under `namespace Arithmetic` so it keeps the
fully-qualified name the axiom had.
-/

namespace Arithmetic

/-- The two spellings of the modulus — `Arithmetic.PALLAS_PRIME` as an `Int`, `PALLAS_MODULUS` as a
    `Nat` — are the same number. This is what lets `ZMod PALLAS_MODULUS` be used to prove a
    statement written with `PALLAS_PRIME`. -/
@[axiom_budget 0]
theorem pallasPrime_eq_modulus : PALLAS_PRIME = (PALLAS_MODULUS : Int) := by
  rw [PALLAS_PRIME, PALLAS_MODULUS]; norm_num

/-- **`b^(p−2)` is the multiplicative inverse of `b`.** For `b` not divisible by the modulus,
    `(a · b^(p−2)) · b ≡ a (mod p)`.

    Was an axiom; is Fermat's little theorem through `ZMod`. Budget 2 — it rests on `pallasPrime`,
    which is what makes `ZMod PALLAS_MODULUS` a field and hence what gives `b^(p−1) = 1`. -/
@[axiom_budget 2]
theorem base_div_mul_cancel (a b : Int) (hb : b % PALLAS_PRIME ≠ 0) :
    ((a * (b ^ (PALLAS_PRIME.toNat - 2))) % PALLAS_PRIME * b) % PALLAS_PRIME = a % PALLAS_PRIME := by
  have hcast : PALLAS_PRIME = (PALLAS_MODULUS : Int) := pallasPrime_eq_modulus
  have htoNat : PALLAS_PRIME.toNat = PALLAS_MODULUS := by rw [hcast]; simp
  rw [hcast] at hb
  rw [htoNat, hcast]
  -- Two `% p` forms are equal iff their casts to `ZMod p` are, so move the whole goal into the field.
  rw [← ZMod.intCast_eq_intCast_iff']
  push_cast
  -- The instance is named rather than inferred: it used to be a global instance in `Axioms.lean`,
  -- and `inferInstance` found it there. `Axioms.lean` declares none now, so the fact is supplied
  -- locally — which is also what keeps the *statement* above clear of it, since only the proof
  -- needs the field.
  haveI : Fact (Nat.Prime PALLAS_MODULUS) := ⟨pallasPrime⟩
  -- `b % p ≠ 0` means `b` is not divisible by `p`, so its image in the field is nonzero —
  -- which is what Fermat's little theorem needs.
  have hb0 : (b : ZMod PALLAS_MODULUS) ≠ 0 := by
    intro h
    exact hb (Int.emod_eq_zero_of_dvd ((ZMod.intCast_zmod_eq_zero_iff_dvd b PALLAS_MODULUS).mp h))
  have hfermat : (b : ZMod PALLAS_MODULUS) ^ (PALLAS_MODULUS - 1) = 1 :=
    ZMod.pow_card_sub_one_eq_one hb0
  have h1 : (b : ZMod PALLAS_MODULUS) ^ (PALLAS_MODULUS - 2) * (b : ZMod PALLAS_MODULUS) = 1 := by
    have hPge : 2 ≤ PALLAS_MODULUS := by rw [PALLAS_MODULUS]; norm_num
    have hlt : PALLAS_MODULUS - 1 = (PALLAS_MODULUS - 2) + 1 := by omega
    rw [hlt, pow_succ] at hfermat
    exact hfermat
  calc (a : ZMod PALLAS_MODULUS) * (b : ZMod PALLAS_MODULUS) ^ (PALLAS_MODULUS - 2)
        * (b : ZMod PALLAS_MODULUS)
      = (a : ZMod PALLAS_MODULUS)
        * ((b : ZMod PALLAS_MODULUS) ^ (PALLAS_MODULUS - 2) * (b : ZMod PALLAS_MODULUS)) := by
        ring
    _ = (a : ZMod PALLAS_MODULUS) * 1 := by rw [h1]
    _ = (a : ZMod PALLAS_MODULUS) := by ring

end Arithmetic
