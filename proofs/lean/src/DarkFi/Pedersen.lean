import Mathlib
import DarkFi.Axioms
import DarkFi.AxiomBudget

/-!
# Pallas and Pedersen commitments, as a curve rather than a postulate

Seven assumptions used to stand in for this file:

    opaque PedersenPoint.add          axiom PedersenIdentity
    axiom  pedersen_add_identity      axiom pedersen_add_comm
    axiom  pedersen_add_assoc         axiom pedersen_commit
    axiom  pedersen_additive_homomorphism

The model was an abstract `structure PedersenPoint where point : Nat` with an opaque binary
operation, so every law of that operation had to be postulated. Here the curve is real:

* **The curve.** Pallas is `y² = x³ + 5` over `F_p` with
  `p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`
  (`= 2^254 + 45560315531419706090280762371685220353`), read from
  `pasta_curves-0.5.2/src/fields/fp.rs:32` and pinned to the Lean constant by
  `pallasModulus_eq_pasta_curves` below. The generator is `(-1, 2)`, checked against the same crate —
  `curves.rs` builds it with `impl_projective_curve_specific!($name, $base, special_a0_b5)` and
  `NEGATIVE_ONE, TWO`, i.e. `A = 0`, `B = 5`, and `4 = -1 + 5` makes `generator_on_curve` true.
  The *generator* was verified against that crate from the start; the **modulus was not**, and it
  was a different, composite number until 2026-09-20 — see the correction in `DarkFi/Axioms.lean`
  and `oldPallasModulus_was_composite` below. Checking one half of a constant pair and writing
  "verified against the vendored implementation" is how that survived.
* **The group.** `Mathlib/AlgebraicGeometry/EllipticCurve/Group.lean` provides
  `WeierstrassCurve.Affine.Point.instAddCommGroup`, and it is the **complete** law: it goes
  through the coordinate ring, so the point at infinity and the doubling case — the two cases the
  opaque `add` sidestepped — are handled rather than assumed away. That also closes
  `ECOps`' "`ec_add` doubling case not rejected" gap at the model level.
* **The four laws** are therefore `add_comm`, `add_assoc`, `add_zero` and `zero_add`, and the
  homomorphism is `add_nsmul` bookkeeping. Nothing is postulated.

## What is still assumed

Exactly one thing, and it is in `DarkFi/Axioms.lean`: `pallasPrime : Nat.Prime PALLAS_MODULUS`.
`ZMod p` is a `Field` only when `p` is prime, and `p` here is 254 bits, so that is a Pratt
certificate rather than a `norm_num` call. It is also the same fact `Arithmetic.base_div_mul_cancel`
needs, which is why it is stated once.

## What is *not* claimed

The generators are parameters, not constants. `commit Gv Gr v b := v • Gv + b • Gr` holds for any
`Gv Gr : Point`, so the homomorphism below is true of every choice — including choices that would
make the commitment *not* binding. Binding is a separate property, and it is not proved here; see
OBL-C6 in `doc/src/arch/verification-hazop.md`. Stating the law for all generators is honest about
what has been shown, where a `pedersen_commit` with hard-coded generators would have suggested the
constant-selection had been validated too.
-/

namespace Pedersen

open WeierstrassCurve

/-- Pallas: `y² = x³ + 5` over `ZMod PALLAS_MODULUS`.

    The type is spelled `WeierstrassCurve.Affine` (`Affine R` is an abbreviation of
    `WeierstrassCurve R`) because `Point` and `Equation` are declared in the `Affine` namespace:
    field notation resolves against the type *as written*, so `WeierstrassCurve.Point` would not
    be found. -/
def pallasCurve : WeierstrassCurve.Affine (ZMod PALLAS_MODULUS) := ⟨0, 0, 0, 0, 5⟩

/-- A Pallas point: a nonsingular `(x, y)` on `pallasCurve`, or the point at infinity. -/
abbrev Point := pallasCurve.Point

/-- The coefficients, which is what pins the curve: `A = 0`, `B = 5`, and no linear or
    quadratic junk. So `pallasCurve.Equation x y` — mathlib's `evalEval x y W.polynomial = 0` —
    says exactly `y² = x³ + 5`.

    Stated as the coefficients rather than as an `↔` with `y ^ 2 = x ^ 3 + 5` because
    `Equation` unfolds through `Polynomial.evalEval`, and rewriting that into the explicit
    polynomial is a `polynomial`/`evalEval` unfolding rather than arithmetic. -/
@[axiom_budget 0]
theorem pallas_coefficients :
    pallasCurve.a₁ = 0 ∧ pallasCurve.a₂ = 0 ∧ pallasCurve.a₃ = 0 ∧
      pallasCurve.a₄ = 0 ∧ pallasCurve.a₆ = 5 :=
  ⟨rfl, rfl, rfl, rfl, rfl⟩

/-- The generator `(-1, 2)` lies on the curve: `2² = (-1)³ + 5`. -/
@[axiom_budget 0]
theorem generator_on_curve :
    ((-1 : ZMod PALLAS_MODULUS) ^ 3 + 5) = (2 : ZMod PALLAS_MODULUS) ^ 2 := by
  norm_num

/-! ===== The field structure, local to the declarations that need it =====

`ZMod PALLAS_MODULUS` is a `Field` only because `pallasPrime` says the modulus is prime, and
`WeierstrassCurve.Affine.Point.instAddCommGroup` requires `[Field F]`. So everything below that adds,
negates or scalar-multiplies points needs that fact. Until 2026-09-24 it got it from a **global**
`instance : Fact (Nat.Prime PALLAS_MODULUS)` in `Axioms.lean`, which every importing file reached —
and the consequence was that measured budgets depended on scope rather than on content. The two
theorems *above* this section do not need a field; they need a commutative ring, which `ZMod` has
unconditionally, and `pallas_coefficients` is five `rfl`s and `generator_on_curve` is one `norm_num`.
Both were charged `pallasPrime` anyway, because resolution preferred the field path.

The instance is `local` to this section now, so it is in scope exactly where it is used, and the
measured outcome is the one the diagnosis predicts: `pallas_coefficients` and `generator_on_curve`
above measure **budget 0** — five `rfl`s and one `norm_num`, which never needed a field — while the
five group laws inside the section stay at 2, because `Point` really is an additive group only under
`[Field]`. Nothing about the mathematics moved; the annotations are what changed. -/

section

local instance : Fact (Nat.Prime PALLAS_MODULUS) := ⟨pallasPrime⟩

/-- The group operation, named as the model named it. -/
noncomputable def add (a b : Point) : Point := a + b

/-- The identity is the point at infinity. (`Point.zero` is the point at infinity, so this needs
    no nonsingularity proof — which is why the identity is the one point that was always easy.) -/
noncomputable def identity : Point := 0

/-- A Pedersen commitment `C(v, b) = v • G_v + b • G_r`, for any generators. -/
noncomputable def commit (Gv Gr : Point) (value blind : Nat) : Point := value • Gv + blind • Gr

/-! ===== The laws, as theorems =====

Budget 2 each: `pallasPrime`, because that is what makes `ZMod PALLAS_MODULUS` a field and hence
`Point` an additive group, plus `Classical.choice`. Nothing else. (This note said "Budget 1 each"
while the annotations said 2; the annotations are measured and the prose was not.) -/

@[axiom_budget 2]
theorem pedersen_add_comm (a b : Point) : add a b = add b a := add_comm a b

@[axiom_budget 2]
theorem pedersen_add_assoc (a b c : Point) : add (add a b) c = add a (add b c) :=
  add_assoc a b c

@[axiom_budget 2]
theorem pedersen_add_identity (a : Point) : add a identity = a := add_zero a

@[axiom_budget 2]
theorem pedersen_identity_add (a : Point) : add identity a = a := zero_add a

/-- **Pedersen additive homomorphism**, proved rather than assumed:
    `C(v₁+v₂, b₁+b₂) = C(v₁,b₁) + C(v₂,b₂)`.

    `(v₁+v₂) • Gv = v₁ • Gv + v₂ • Gv` is `add_nsmul`; the rest is commutativity and
    associativity of `+` in an abelian group, which is what the old model had to postulate. -/
@[axiom_budget 2]
theorem pedersen_additive_homomorphism (Gv Gr : Point) (v₁ v₂ b₁ b₂ : Nat) :
    commit Gv Gr (v₁ + v₂) (b₁ + b₂) = commit Gv Gr v₁ b₁ + commit Gv Gr v₂ b₂ := by
  simp only [commit, add_nsmul]
  abel

end

/-! ===== The modulus, tied to the constant the vendored crate documents =====

`PALLAS_MODULUS` was **wrong** until 2026-09-20: it read a composite number, so `pallasPrime`
asserted a falsehood and `ZMod PALLAS_MODULUS` was made a `Field` on the strength of it. Nothing
tied the Lean constant to the actual Pallas modulus, which is how the error survived a docstring
that said the curve had been "verified against the vendored implementation" — the *generator* had
been, the *modulus* had not.

These two theorems are the tie. The first is the check whose absence let the error through; the
second is the evidence for the claim that the old value was composite, so that a comment saying
"divisible by 3" is something the kernel agrees with rather than something a reader has to trust. -/

set_option maxRecDepth 10000 in
/-- **The modulus equals the constant `pasta_curves` documents.**

    `pasta_curves-0.5.2/src/fields/fp.rs:32` gives the Pallas base field modulus as
    `0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`, and this is a
    kernel-checked equality between that literal and `PALLAS_MODULUS`. `norm_num` decides it; no
    reflection, no trust.

    This theorem is the reason the previous value could not have gone unnoticed had it existed: a
    transcription of the modulus from any source that spells it in hex now fails the build. -/
@[axiom_budget 0]
theorem pallasModulus_eq_pasta_curves :
    PALLAS_MODULUS = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001 := by
  norm_num [PALLAS_MODULUS]

set_option maxRecDepth 10000 in
/-- **The value this file used to define was composite**, so `pallasPrime` was false.

    The old expression was `2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1`, and `3` divides it. That is
    enough to refute primality: a prime's only divisors are `1` and itself, and this number is
    neither `3` nor `1`.

    Kept in the tree rather than only described, because the correction is otherwise a prose claim
    about arithmetic, and this session has been about not leaving those unchecked. -/
@[axiom_budget 0]
theorem oldPallasModulus_was_composite :
    ¬ Nat.Prime (2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1) := by
  intro h
  have hdvd : 3 ∣ (2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1) := by norm_num
  have hne : (2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1) ≠ 3 := by norm_num
  rcases h.eq_one_or_self_of_dvd 3 hdvd with h1 | h3
  · norm_num at h1
  · exact hne h3.symm

end Pedersen
