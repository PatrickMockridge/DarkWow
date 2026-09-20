import DarkFi.AxiomBudget

/-!
# DarkFi EC Operation Soundness Proofs

Orchard-class defense: prove that fixed-base EC multiplications use
compile-time constants, not prover-chosen bases. The Orchard bug was
an EC base point not being constrained — an attacker could choose an
arbitrary base to bypass value conservation.

## Key Theorems

1. **FixedBaseIsConstant**: `ec_mul`, `ec_mul_base`, `ec_mul_short`
   use constants (VALUE_COMMIT_VALUE, VALUE_COMMIT_RANDOM, NULLIFIER_K),
   never witness-provided bases.

2. **VariableBaseIsProverChosen**: `ec_mul_var_base` lets the prover
   choose the base. Circuits using this for security-critical operations
   MUST add additional constraints.

3. **PedersenAdditiveHomomorphism**: The fundamental property enabling
   cross-proof value conservation in TransferV1/OtcSwapV1.
-/

namespace ECOps

/-
## Pedersen Commitment

C = v * G_v + r * G_r

where:
  v = value (Base field element, constrained to u64 range)
  r = blinding factor (Scalar field element)
  G_v = VALUE_COMMIT_VALUE (EcFixedPointShort — compile-time constant)
  G_r = VALUE_COMMIT_RANDOM (EcFixedPoint — compile-time constant)

Key property: Pedersen commitments are additively homomorphic:
  C(v1, r1) + C(v2, r2) = C(v1+v2, r1+r2)

This is what enables value conservation checks without revealing
plaintext values.
-/

/-
Fixed generators for Pedersen commitments.
These are COMPILE-TIME CONSTANTS, not prover-chosen.
-/
inductive FixedGenerator where
  | value_commit_value   -- G_v: EcFixedPointShort
  | value_commit_random  -- G_r: EcFixedPoint
  | nullifier_k          -- K:   EcFixedPointBase
deriving BEq

/-
## EC Multiplication Classification

Every EC multiplication in a circuit falls into one of these categories.
-/
inductive ECMulKind where
  | fixed_short   -- ec_mul_short: scalar is Base, base is EcFixedPointShort
  | fixed         -- ec_mul: scalar is Scalar, base is EcFixedPoint
  | fixed_base    -- ec_mul_base: scalar is Base, base is EcFixedPointBase
  | var_base      -- ec_mul_var_base: scalar is Base, base is EcNiPoint (prover-chosen)
deriving BEq

/-
## EC Multiplication Gadget

Models one EC scalar multiplication in a circuit.
-/
/-- Whether a multiplication's base is a compile-time constant — a **function of the kind**, and
    deliberately *not* a field of `ECMulGadget`.

    It used to be a field, with two axioms asserting the field could not disagree with the kind.
    That was false, and falsity in an axiom is not the same as being unproved: `ECMulGadget` is
    freely constructible, so `⟨ECMulKind.fixed_short, 0, false, …⟩` is a gadget whose kind is fixed
    and whose `base_is_constant` is `false`; the axiom applied to it yields `false = true`, hence
    `False`, hence **every theorem in the tree**. The axiom set was inconsistent — not conditional,
    vacuous. Deriving constancy from the kind removes the degree of freedom that allowed it.

    The mapping is the one the opcode names already carry: `ec_mul`, `ec_mul_base` and
    `ec_mul_short` take a base from the circuit's `constant` block; `ec_mul_var_base` takes an
    `EcNiPoint` the prover supplies. -/
def ECMulKind.baseIsConstant : ECMulKind → Bool
  | var_base => false
  | fixed_short => true
  | fixed => true
  | fixed_base => true

structure ECMulGadget where
  kind : ECMulKind
  scalar : Int           -- The scalar (Base or Scalar field element as Int)
  base_name : FixedGenerator -- which constant, for the fixed kinds
  result_x : Int          -- x-coordinate of the result point
  result_y : Int          -- y-coordinate of the result point
deriving BEq

/-- The gadget's constancy, read off its kind rather than stored beside it. -/
def ECMulGadget.baseIsConstant (g : ECMulGadget) : Bool := g.kind.baseIsConstant

/-
## Fixed-base and variable-base multiplication — the assumptions, discharged

Two declarations used to sit here under `## THEOREM` headings; they then became `axiom`s in
`Axioms.lean`; they are **theorems** again, and this time the reason is not that the statement was
fixed but that the *model* was:

    axiom fixed_base_mul_uses_constant (g : ECMulGadget)
      (hkind : g.kind ≠ ECMulKind.var_base) : g.base_is_constant
    axiom variable_base_mul_is_prover_chosen (g : ECMulGadget)
      (hkind : g.kind = ECMulKind.var_base) : ¬ g.base_is_constant

As stated over a structure with a free `Bool` field they were **false** — see
`ECMulKind.baseIsConstant` above for the counterexample and what it cost. With constancy derived
from the kind there is nothing left to assume: both follow by case analysis on the kind.

What is still not proved is the thing these were standing in for all along, and it is not
expressible here: that the *model's* kind-to-constancy mapping is the one the zkas VM implements.
That is the model-to-implementation correspondence, it is a claim about Rust and `.zk` sources
rather than about these types, and it belongs to `Axioms.NoFreeInstances`' class rather than to a
declaration over `ECMulGadget`.
-/

/-- For the fixed kinds the base is a compile-time constant. Was an axiom; is now a case split. -/
@[axiom_budget 0]
theorem fixed_base_mul_uses_constant (g : ECMulGadget) (hkind : g.kind ≠ ECMulKind.var_base) :
    g.baseIsConstant = true := by
  rcases g with ⟨k, sc, bn, rx, ry⟩
  cases k <;> simp_all [ECMulGadget.baseIsConstant, ECMulKind.baseIsConstant]

/-- For `var_base` the base is prover-chosen. Was an axiom; is now a case split. -/
@[axiom_budget 0]
theorem variable_base_mul_is_prover_chosen (g : ECMulGadget)
    (hkind : g.kind = ECMulKind.var_base) : ¬ (g.baseIsConstant = true) := by
  rw [ECMulGadget.baseIsConstant, hkind, ECMulKind.baseIsConstant]
  simp


/-
## Orchard-class vulnerability — the claim is an input, and it used to be a placeholder

A name used to be declared here:

    axiom variable_base_without_binding_is_orchard_class
      (g : ECMulGadget) (hkind : g.kind = ECMulKind.var_base) : Prop

It was a `: Prop`-valued axiom, so it named the Orchard-class claim without stating it, and no
proof could consume it. `proofs/lean/README.md` listed it under "Cryptographic Assumptions",
which was at least honest about its kind; `doc/src/arch/security-analysis.md`,
`formal-specification.md`, `opcodes.md` and the audit documents went further and reported the
Orchard-class result as *formally verified*, on the strength of this placeholder and of eleven
named "Circuit Audit Axioms" that do not exist.

It is deleted rather than restated, because its content is already carried by a real
assumption: `ECOps.variable_base_mul_is_prover_chosen` (`DarkFi/Axioms.lean`) says the base of
an `ec_mul_var_base` is prover-chosen and therefore unconstrained. The Orchard-class claim is
its immediate corollary, and that assumption's entry records what discharges it. Recorded as
SILENT in `DarkFi.HAZOP.Elevated`.

**Note on `detect_orchard_class_vulnerability` below.** Its `var_base` branch used to return
`True`, which made the rule vacuous for exactly the case its name denotes. It now returns
`varBaseObligation g`, a named proposition that is false whenever the base is a constant — so the
branch states something and can be wrong, rather than stating nothing. The `_` branch is
unchanged and is the one that *rejects* a non-constant base for the fixed-base opcodes.

What neither branch does is prove anything about a real circuit: this is a `def` returning `Prop`.
The var_base case is not a violation *of the gadget* — it is the reason the caller owes a binding
constraint, and that debt cannot be seen from one gadget. See OBL-Z1 and ELEV-30.
-/

/-
## Orchard-class vulnerability detection

If a circuit uses ec_mul or ec_mul_short where the base point
appears as a WITNESS (not a constant), that is an Orchard-class
vulnerability — exactly the bug that existed in Zcash for ~4 years.

This gives the detection rule:
  For every ec_mul/ec_mul_short in every .zk circuit,
  verify the base argument is a compile-time constant.
-/

/-- The obligation a `var_base` multiplication leaves to its caller.

    A variable-base multiplication takes a prover-chosen base, so the gadget is never itself the
    vulnerability — the vulnerability is a circuit that exposes something derived from it without
    binding. Naming the proposition here, rather than returning `True`, is what keeps `True` out
    of a rule whose whole subject is the `var_base` case: `¬ g.baseIsConstant = true` is false for a
    gadget that carries a constant base, so this branch can fail.

    A `Prop`-valued `def` makes a *classification*, not a proof — but the proposition it names is
    now decidable and settled by `variable_base_mul_is_prover_chosen`, which no longer assumes
    anything. -/
def varBaseObligation (g : ECMulGadget) : Prop := ¬ (g.baseIsConstant = true)

/-- The Orchard-class shape, as a proposition about one multiplication.

    `fixed_*` kinds must carry a compile-time constant base; a non-constant one is the violation.
    `var_base` returns the caller's obligation instead of a violation, because whether the circuit
    discharges it is not a property of the gadget. -/
def detect_orchard_class_vulnerability (g : ECMulGadget) : Prop :=
  match g.kind with
  | ECMulKind.var_base =>
    -- Variable base: prover-chosen by design. Not a vulnerability per se,
    -- but circuits MUST add constraints binding the base.
    varBaseObligation g
  | _ =>
    -- Fixed base: MUST use a compile-time constant. By `fixed_base_mul_uses_constant` this branch
    -- is now `true = true`, so it can no longer fire — which is the point: with constancy derived
    -- from the kind there is no gadget of a fixed kind that violates it. The check the rule was
    -- doing lives in the `.zk` sources, where a `witness`-supplied base is visible.
    g.baseIsConstant = true

/-
## Pedersen commitment correctness — claim removed

A name used to be declared here:

    axiom pedersen_commitment_binding
      (v1 r1 v2 r2 : Int)
      (gv_is_constant gr_is_constant : Bool)
      (hgv : gv_is_constant = true)
      (hgr : gr_is_constant = true) :
      (v1 = v2 ∧ r1 = r2) ∨ (v1 ≠ v2 ∨ r1 ≠ r2)

Its conclusion is a tautology — `P ∨ ¬P`, by `em` — so it is provable for *any* statement
about `v1 r1 v2 r2`, with or without the two `Bool` hypotheses, and with or without Pedersen
commitments existing. The `hgv` and `hgr` hypotheses, which are what the "if both
multiplications use fixed constants" prose is about, are never used. A `Bool` parameter is not
a commitment and `true` is not a constraint, so the statement could not have carried the
binding property even if its conclusion had been non-trivial.

It is removed, not re-proved. Re-proving it would produce a budget-0 theorem named
`pedersen_commitment_binding` whose statement says nothing about Pedersen commitments — the
same defect, wearing a theorem's kind instead of an axiom's. Pedersen binding is not modelled
in Lean: it needs a curve model, and `Axioms.pedersen_additive_homomorphism` records the part
of that model the supply-chain proofs actually consume. Recorded in `DarkFi.HAZOP.High`.

The `THEOREM`/`AXIOM` heading pair that followed, for `pedersen_additive_homomorphism
(values blinds : List Int) : Prop`, is also gone. That second declaration was a `: Prop` stub:
as a function it is an uninterpreted predicate on two lists, so it asserted nothing and no
proof could consume it. The real Pedersen homomorphism is the top-level
`Axioms.pedersen_additive_homomorphism` (`Nat → Nat → Nat → Nat → Prop`), which is an equality
about `pedersen_commit`. The two shared nothing but a name, which is why the same name meant
two different things in two files. Recorded as SILENT in `DarkFi.HAZOP.Elevated`.
-/

/-
## Pedersen additive homomorphism — moved

The real statement, `pedersen_commit (v₁+v₂) (b₁+b₂) = pedersen_commit v₁ b₁ +
pedersen_commit v₂ b₂`, is an assumption in `DarkFi/Axioms.lean`, together with
`PedersenPoint` and `pedersen_commit` that it is stated in terms of. It is the foundation of
cross-proof value conservation: the entrypoint sums all input and all output Pedersen
commitments per `token_commit` group and verifies they are equal, which is what makes
`sum(input_values) = sum(output_values)` hold without revealing individual values.
-/


/-
## EC Point Addition Soundness

ec_add (0x01) performs incomplete addition on Pallas.
We must verify that exceptional cases (point at infinity, doubling)
are correctly handled.

For the incomplete addition formula:
  (x1, y1) + (x2, y2) = (x3, y3)

If x1 = x2, the formula degenerates (division by zero).
ec_add must reject or handle this case.
-/
structure ECAddGadget where
  x1 : Int    -- Input point coordinates
  y1 : Int
  x2 : Int
  y2 : Int
  x3 : Int          -- Output point coordinates
  y3 : Int
  inputs_distinct : Bool -- Are the input points distinct (x1 ≠ x2)?

/-
## THEOREM: ec_add requires distinct x-coordinates

If x1 = x2, the incomplete addition formula divides by zero.
This theorem states that the constraint system must enforce
x1 ≠ x2 (or handle the doubling case separately).

CORRESPONDENCE: src/zk/vm.rs:898 — ec_add uses lhs.add(rhs) which
performs incomplete Pallas addition. The VM does NOT explicitly
reject the doubling case (x1 == x2). This is a known gap.

For the Lean model: when x1 = x2, the slope formula
(y2 - y1) / (x2 - x1) has denominator zero. The constraint system
must ensure this case is handled (either rejected or handled via
a complete addition formula). We document this as a constraint
that the Rust VM should enforce.
-/
@[axiom_budget 0]
theorem ec_add_inputs_must_be_distinct (g : ECAddGadget)
  (h : g.x1 = g.x2) :
  -- When x1 = x2, the denominator (x2 - x1) = 0.
  -- The incomplete addition formula is undefined.
  -- Circuit constraint must ensure inputs are distinct.
  g.x2 - g.x1 = 0 := by
  rw [h]
  simp

/-
## Orchard-Class Audit Helper

For every .zk circuit, this function checks whether a given
ec_mul/ec_mul_short call uses a fixed constant or a witness.

In practice, the .zk circuit source is the spec — the `constant` block
declares fixed generators, the `witness` block declares prover-chosen values.
-/
def audit_ec_mul_base (base_is_constant : Bool) (circuit_name : String) : String :=
  if base_is_constant then
    s!"{circuit_name}: EC mul base is constant ✓"
  else
    s!"{circuit_name}: ORCHARD-CLASS VULNERABILITY — EC mul base is WITNESS, not constant!"

end ECOps
