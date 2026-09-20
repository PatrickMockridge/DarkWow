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

* **The curve.** Pallas is `y² = x³ + 5` over `F_p` with `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`.
  Verified against the vendored implementation — `pasta_curves-0.5.2/src/curves.rs` builds the
  generator with `impl_projective_curve_specific!($name, $base, special_a0_b5)` and
  `NEGATIVE_ONE, TWO`, i.e. `A = 0`, `B = 5`, generator `(-1, 2)` (`4 = -1 + 5`).
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
@[axiom_budget 2]
theorem pallas_coefficients :
    pallasCurve.a₁ = 0 ∧ pallasCurve.a₂ = 0 ∧ pallasCurve.a₃ = 0 ∧
      pallasCurve.a₄ = 0 ∧ pallasCurve.a₆ = 5 :=
  ⟨rfl, rfl, rfl, rfl, rfl⟩

/-- The generator `(-1, 2)` lies on the curve: `2² = (-1)³ + 5`. -/
@[axiom_budget 2]
theorem generator_on_curve :
    ((-1 : ZMod PALLAS_MODULUS) ^ 3 + 5) = (2 : ZMod PALLAS_MODULUS) ^ 2 := by
  norm_num

/-- The group operation, named as the model named it. -/
noncomputable def add (a b : Point) : Point := a + b

/-- The identity is the point at infinity. (`Point.zero` is the point at infinity, so this needs
    no nonsingularity proof — which is why the identity is the one point that was always easy.) -/
noncomputable def identity : Point := 0

/-- A Pedersen commitment `C(v, b) = v • G_v + b • G_r`, for any generators. -/
noncomputable def commit (Gv Gr : Point) (value blind : Nat) : Point := value • Gv + blind • Gr

/-! ===== The laws, as theorems =====

Budget 1 each: they rest on `pallasPrime`, because that is what makes `ZMod PALLAS_MODULUS` a
field and hence `Point` an additive group. Nothing else. -/

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

end Pedersen
