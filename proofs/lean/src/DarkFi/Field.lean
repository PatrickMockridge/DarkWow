/-
# DarkFi Field Arithmetic

Formalization of Pallas field arithmetic for gadget verification.
Pallas operates on F_p where p = 2^254 + 45560315531419706090280762371685220353
(0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001)

## The Wraparound Problem

As integers:     0 < 1 < 2 < ... < p-2 < p-1
As field elts:     0 ≡ p < 1 < 2 < ... < p-2 < p-1 (mod p)

Values in [p - 2^32, p) exhibit field ordering that differs from integer ordering.

NOTE: This file uses Mathlib for the number-theoretic lemmas
(mul_comm, Int.div_lt_iff_lt_mul, pow_le_pow_right) that the wraparound and
range-check proofs rely on.
-/

import Mathlib
import DarkFi.AxiomBudget

-- `wraparound_safe` needs `2^222 < PALLAS_PRIME`, whose two sides are closed integer
-- expressions up to `2^254`. Deciding that normalises numerals well past the default
-- elaborator recursion depth ("maximum recursion depth has been reached"). Raised for this
-- file only; `norm_num` still produces a kernel-checked proof, unlike the `native_decide`
-- this replaces, which additionally hit the same limit while trusting the code generator.
set_option maxRecDepth 10000

-- Define the Pallas prime (type Int to match Arithmetic.lean convention)
def PALLAS_PRIME : Int := 2^254 + 45560315531419706090280762371685220353

/-
## Division Correctness (Fermat) — duplicate assumption removed

`pallas_div_mul_cancel` used to be declared here. Its signature was byte-identical to
`Arithmetic.base_div_mul_cancel`, under a different name, and `proofs/lean/README.md` itself
admitted "(duplicated)" for the pair. One assumption stated twice is a defect in the
assumption boundary, not a second assumption, so the copy here was deleted and the survivor
lives in `DarkFi/Axioms.lean` as `Arithmetic.base_div_mul_cancel` (with the four-field
annotation and the corrected justification).

Note that this file's own `PALLAS_PRIME` below is *not* the same constant as
`Arithmetic.PALLAS_PRIME` — the two are equal by definition but distinct declarations. That
duplication is why the two axioms looked identical without being the same statement.
-/

/--
## Cross-Multiplication Equivalence

In integer arithmetic: a < b * c → a / b < c  (for b > 0).
This is the foundation for the cross-multiplication workaround used
in comparison gadgets.

The previous proof cited `Int.div_lt_iff_lt_mul`, which **does not exist** — not in core Lean, and
not in mathlib under that name (`Mathlib/Algebra/Order/Group/Unbundled/Basic.lean:675` has
`div_lt_iff_lt_mul'`, but it requires `[Group α]` under multiplication, which `ℤ` is not). The
statement is not in doubt; the citation was. `Soundness.cross_mul_implies_ratio_bound` was
declared as an `axiom` on the strength of that broken proof, and is now a theorem discharged
from this one.

Proved instead from the division algorithm, which is in core:
`a / b * b + a % b = a` (`Int.ediv_add_emod`) and `0 ≤ a % b` for `b > 0` (`Int.emod_nonneg`)
give `a / b * b ≤ a`; chain with `a < b * c`, commute, and cancel the positive factor.
-/
@[axiom_budget 1]
theorem cross_mul_lt {a b c : Int} (hb : b > 0) :
  a < b * c → a / b < c := by
  intro h
  have hmod : 0 ≤ a % b := Int.emod_nonneg a (ne_of_gt hb)
  have hdecomp : b * (a / b) + a % b = a := Int.ediv_add_emod a b
  -- `b * (a / b) ≤ a`, because the remainder is non-negative.
  have hle : b * (a / b) ≤ a := by omega
  have hlt : b * (a / b) < b * c := lt_of_le_of_lt hle h
  exact (mul_lt_mul_left hb).mp hlt

/--
## Soundness Theorem: Bounded Inputs

If inputs a, b are guaranteed to be in range [0, 2^k) where k ≤ 222,
then for a, b ∈ [0, PALLAS_PRIME - 2^32), integer ordering and
field ordering coincide — no modular wraparound in comparisons.

This is the foundational theorem for all comparison gadget soundness proofs.
-/
@[axiom_budget 1]
theorem wraparound_safe {k : ℕ} (a b : Int) (ha : 0 ≤ a) (hb : 0 ≤ b)
    (ha_bound : a < 2^k) (hb_bound : b < 2^k) (hk : k ≤ 222) :
  a < b → a % PALLAS_PRIME < b % PALLAS_PRIME := by
  intro h_lt
  -- `2^k ≤ 2^222` must be stated over `ℤ`: `ha_bound` has `2^k : ℤ` (the numeral is coerced
  -- because `a : Int`), so a `ℕ`-valued statement does not compose with it.
  have hk_pow : (2 : Int) ^ k ≤ (2 : Int) ^ 222 :=
    pow_le_pow_right (by norm_num) hk
  -- `2^222 < PALLAS_PRIME` (≈ `2^254`). Both sides are closed integer
  -- expressions, so `norm_num` decides it in the kernel. This was `native_decide`, which hit
  -- `maximum recursion depth` here — and would have trusted the code generator even if it had not.
  have h_222_lt_p : (2 : Int) ^ 222 < PALLAS_PRIME := by
    rw [PALLAS_PRIME]; decide
  have h_a_lt_p : a < PALLAS_PRIME := lt_trans (lt_of_lt_of_le ha_bound hk_pow) h_222_lt_p
  have h_b_lt_p : b < PALLAS_PRIME := lt_trans (lt_of_lt_of_le hb_bound hk_pow) h_222_lt_p
  -- Both a and b are < p, so modulo reduction is the identity.
  have ha_mod : a % PALLAS_PRIME = a := Int.emod_eq_of_lt ha h_a_lt_p
  have hb_mod : b % PALLAS_PRIME = b := Int.emod_eq_of_lt hb h_b_lt_p
  rw [ha_mod, hb_mod]
  exact h_lt

/-
## The Gadget Framework

A gadget is a tuple (inputs, intermediate_vars, output, constraints).
Soundness means: for any assignment of inputs that satisfies constraints,
the output correctly implements the specified function.

This was a doc comment with no declaration after it — it documented a framework, not a
definition — and once a second comment block followed it, Lean reported
`unexpected token 'namespace'` at the `namespace` line below, because a doc comment must be
attached to a declaration. It is a plain comment now.

(Beware writing the doc-comment delimiter inside a block comment: block comments nest in Lean,
so the two characters open a new one and the closing delimiter only closes that. This comment
originally did exactly that and left itself unterminated.)
-/

/-
## The `Gadget` / `SoundGadget` typeclasses — removed

Two typeclasses used to be declared here:

    class Gadget (α : Type) where input : α; output : α; constraint : Prop
    class SoundGadget (α : Type) (f : α → α) extends Gadget α where
      sound : constraint → output = f input

Neither was used: no instance, no reference, anywhere in `proofs/lean/`. (The real gadget
soundness proofs are in `DarkFi/Gadgets.lean`, which is a different file and does not use
these.) They were not merely dead — `SoundGadget`'s `extends` clause made Lean generate an
instance whose `f` argument is not determined by the goal, which Lean rejects outright:

    cannot find synthesization order for instance @SoundGadget.toGadget with type
      {α : Type} → (f : α → α) → [self : SoundGadget α f] → Gadget α
    all remaining arguments have metavariables: SoundGadget α ?f

So `Field.lean` could not compile because of two unused classes. Removed. If a gadget-soundness
typeclass is wanted later, `f` has to be an `outParam` so instance search can determine it.
-/

namespace Gadget

/--
## Bounded Range Check Gadget

`range_check(n, x)` asserts `0 ≤ x < 2^n` — i.e. `0 ≤ x ∧ x < 2^n`. This is implemented in
Halo2 as a bit decomposition check.
-/

def range_check (n : ℕ) (x : ℤ) : Prop := 0 ≤ x ∧ x < 2^n

/-- The conjunction in `range_check`, with the two halves the other way round. Note that this is
    *not* `exact h`: `range_check n x` is `0 ≤ x ∧ x < 2^n`, and the theorem states
    `x < 2^n ∧ x ≥ 0`. `∧` commutes but the two are not definitionally equal, so the old
    `:= by exact h` could never have elaborated. `⟨h.2, h.1⟩` is the whole proof. -/
@[axiom_budget 0]
theorem range_check_safe (n : ℕ) (x : ℤ) (h : range_check n x) :
  x < 2^n ∧ x ≥ 0 := ⟨h.2, h.1⟩

end Gadget