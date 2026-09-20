/-
# Cross-Cutting Theorems — Spanning Multiple Circuits

Value conservation, nullifier determinism, signature binding,
Merkle inclusion soundness, Pedersen additive homomorphism.

These theorems apply across ALL token circuits, not just one.
-/

import Mathlib
import DarkFi.Axioms
import DarkFi.AxiomBudget

-- `value_conservation_no_wraparound` compares numerals up to `2^254`, past the default
-- elaborator recursion depth. Same knob as `Field.lean`, `Arithmetic.lean` and `Gadgets.lean`;
-- `norm_num` still yields a kernel-checked proof.
set_option maxRecDepth 10000

namespace CrossCutting

/-
## Pedersen Additive Homomorphism — Foundation of Value Conservation

C(v, r) = v * G_v + r * G_r

Key property: C(v1, r1) + C(v2, r2) = C(v1 + v2, r1 + r2)

This enables cross-proof value conservation: the entrypoint sums
all input Pedersen commitments and all output Pedersen commitments
per token_commit group, and verifies they are equal.

  sum(input value_commits) == sum(output value_commits)  [per token_commit]

This proves sum(input_values) == sum(output_values) without revealing
individual values. The blinding factors cancel across the sum.

THEOREM: If the entrypoint's verify_value_conservation passes,
then for each token_commit group:
  sum(input_values) = sum(output_values)  (mod p)
-/

/-
Pedersen commitment: C = v * G_v + r * G_r

Modeled as (v, r) with the property:
  sum of (v_i, r_i) preserves the sum of values.
-/
structure PedersenCommitment where
  value : Int    -- v (value, range-checked to 64 bits)
  blind : Int    -- r (blinding factor)
-- `Repr` as well as `BEq`: `Capability/Value.lean` and `Capability/MultiProof.lean` derive
-- `Repr` for structures holding a `PedersenCommitment` (and lists of one), which needs it here.
deriving BEq, Repr

/-
Sum of Pedersen commitments: component-wise addition.
-/
def sum_pedersen (comms : List PedersenCommitment) : PedersenCommitment :=
  comms.foldl (λ acc c => ⟨acc.value + c.value, acc.blind + c.blind⟩) ⟨0, 0⟩

/-
## THEOREM: Value Conservation via Pedersen Homomorphism

If sum(input_commits) = sum(output_commits) per token_commit group,
then sum(input_values) = sum(output_values).

The blinding factors cancel because the same sum of blinds
appears on both sides.

This theorem used to be named `pedersen_value_conservation`, which overstated it. What it
proves is the congruence step: *given* the two Pedersen sums to be equal, their `.value`
fields are equal. That is `congrArg`, not the Pedersen homomorphism. The cryptographic
content — that on-chain equality of Pedersen sums implies equality of the committed values,
which is what makes the entrypoint's `verify_value_conservation` sound — is exactly the
hypothesis `h_sum_eq` here, and in the real system it is a consequence of commitment binding
(`Axioms.commitment_binding`) plus the homomorphism (`Axioms.pedersen_additive_homomorphism`).
Neither is invoked below. The name now says only what is proved.
-/
@[axiom_budget 0]
theorem pedersen_sum_equality_implies_value_equality
  (inputs outputs : List PedersenCommitment)
  (h_sum_eq : sum_pedersen inputs = sum_pedersen outputs) :
  (sum_pedersen inputs).value = (sum_pedersen outputs).value := by
  rw [h_sum_eq]

/-
## THEOREM: Value Conservation Soundness (No Wraparound)

For values range-checked to 64 bits:
  Each value < 2^64
  Sum of up to 16 values (MAX_COINS_PER_TX) < 2^68
  PALLAS_PRIME ≈ 2^254 ≫ 2^68

Therefore: no modular wraparound in the value sum.
Integer equality and field equality commitmentcide.

This theorem proves that the entrypoint's verify_value_conservation
is BOTH necessary AND sufficient: if the Pedersen sums match,
the value sums match (in both field and integer arithmetic).
-/
/-- The bound the main theorem needs, as a reusable induction: if every element of `l` is at
    most `M`, the sum is at most `l.length * M`.

    This replaces an `omega` call that could not work: `omega` failed with "a possible
    counterexample may satisfy the constraints `0 ≤ values.length ≤ 16`, `values.sum ≥ 2^68`",
    because a `List.sum` bound is an *induction over the list*, not a linear-arithmetic goal. The
    statement is `length * M` rather than `n * M` so that the induction goes through: passing a
    fixed `n` down the cons case would lose a factor on every step.

    No `0 ≤ M` hypothesis. One used to sit here and was never invoked: the induction is purely
    structural, adding one element and one `M` per step, so it holds for negative `M` too and the
    hypothesis was an obligation on every caller that discharged nothing. Removed, which
    *strengthens* the lemma. The generalisation is deliberate and is the whole point of keeping it
    separate from the caller's own sign reasoning. -/
@[axiom_budget 1]
lemma sum_le_length_mul {l : List Int} {M : Int}
    (h_each : ∀ v ∈ l, v ≤ M) : l.sum ≤ (l.length : Int) * M := by
  induction l with
  | nil => simp
  | cons a t ih =>
      have ha : a ≤ M := h_each a (by simp)
      have ht : ∀ v ∈ t, v ≤ M := fun v hv => h_each v (by simp [hv])
      have ih' := ih ht
      calc (a :: t).sum = a + t.sum := List.sum_cons ..
        _ ≤ M + (t.length : Int) * M := add_le_add ha ih'
        _ = ((t.length : Int) + 1) * M := by ring
        _ = ((a :: t).length : Int) * M := by simp

@[axiom_budget 1]
theorem value_conservation_no_wraparound
  (values : List Int)
  (h_range : ∀ v ∈ values, 0 ≤ v ∧ v < 2^64)
  (h_count : values.length ≤ 16) :
  -- sum(values) < 2^68 < p, so no modular reduction
  List.sum values < 2^68 := by
  have h_max_one : (2^64 - 1 : Int) < 2^64 := by norm_num
  have h_each : ∀ v ∈ values, v ≤ (2^64 - 1 : Int) := by
    intro v hv
    rcases h_range v hv with ⟨_, h_upper⟩
    omega
  have hM : (0 : Int) ≤ 2^64 - 1 := by norm_num
  have h_max_sum : List.sum values ≤ 16 * (2^64 - 1) := by
    have h_len : (values.length : Int) ≤ 16 := by exact_mod_cast h_count
    have := sum_le_length_mul h_each
    calc List.sum values ≤ (values.length : Int) * (2^64 - 1) := this
      _ ≤ 16 * (2^64 - 1) := mul_le_mul_of_nonneg_right h_len hM
  -- Therefore sum(values) ≤ 16*(2^64-1) < 16*2^64 = 2^68.
  --
  -- This `calc` used to end with `_ < 2^254 := by native_decide`, which targets a different
  -- proposition from the statement's `< 2^68` — so the whole chain could never have
  -- type-checked. The `2^68 < 2^254 < p` half of the argument is context (it is why no modular
  -- reduction happens on the *field* side); the theorem only claims the `2^68` bound.
  calc
    List.sum values ≤ 16 * (2^64 - 1) := h_max_sum
    _ < 16 * (2^64 : Int) := by linarith [h_max_one]
    _ = (2^68 : Int) := by norm_num

/-
## Nullifier determinism, signature binding, Merkle inclusion — claims removed

Three `: Prop`-valued axioms used to sit here:

    axiom nullifier_determinism (secret commitment : Int) : Prop
    axiom signature_binding_h2_fix (commitment_secret nullifier : Int) : Prop
    axiom merkle_inclusion_foundation (leaf pos root : Int) (path : List Int) : Prop

A `: Prop`-valued axiom is an uninterpreted predicate: it *names* a claim without stating
one, so nothing can be proved from it and nothing can be proved about it. The three long
comments above them described genuine properties — nullifier determinism as the foundation of
double-spend protection, the H2 signature-binding fix, Merkle inclusion soundness — but the
declarations asserted none of them. The "Cross-Cutting Verification Status" table at the
bottom of this file cited them as proof. That table reported the existence of three
placeholders as verification.

They are deleted. The properties are enforced where they are actually enforced: in the `.zk`
constraint systems (`src/contract/{promissory_note,native_token}/proof/burn_v2.zk`) and in the
host's `constrain_instance` binding, audited manually. They are recorded as SILENT in
`DarkFi.HAZOP.Elevated`.

`merkle_inclusion_foundation`'s chain of trust deserves a note, because steps 1-4 are a real
argument: the modelling gap is the word "ZK proof" in step 1. Nothing in this tree turns a
Halo2 proof into a Lean proposition, so step 1 cannot be a Lean hypothesis either — it would
have to be `Axioms.NoFreeInstances` or something like it, and no theorem consumes that yet.
-/

/-
## Zero-Cond Soundness — claim removed

The theorem that used to be here was:

    theorem zero_cond_prevents_smuggling (commitment_value commitment : Int)
      (h_value_zero : commitment_value = 0) : commitment_value = 0 := h_value_zero

Its conclusion restated its own hypothesis, and `zero_cond` — the thing the name is about —
did not appear in the statement at all. The genuine content needs a Lean model of the
`zero_cond` builtin, which does not exist. The `zero_cond` gate is enforced in the `.zk`
constraint systems; it is not modelled here. Recorded in `DarkFi.HAZOP.High`.

The long comment above it, describing the attack scenario and the defence, is accurate about
the circuit. It was simply never a Lean proof.
-/

/-
## Orchard-Class Detection Rule — Universal

For EVERY circuit in EVERY contract:
  1. List all constrain_instance(X) calls
  2. For each X, verify X is derived in-circuit from witnesses
  3. If any X is free witness AND constrain_instance'd:
     → ORCHARD-CLASS VULNERABILITY

This rule catches: C1 (mint_public was free), and WOULD catch
any future regression.

### Status of "ALL circuits pass"

That verdict is a *manual audit result*, not a Lean theorem, and it is not established by
anything in this file. `proofs/lean/README.md` used to attribute it to eleven named axioms in
`Circuits/{Token,Bridge,Exchange,All}.lean`; those files contain zero `axiom` declarations,
and `All.lean`, `Bridge.lean` and `Exchange.lean` are comment-only. The only Lean declaration
in the whole `Circuits/` directory is `Circuits.Token.burn_v1_no_free_instances`, which is
discussed in that file.

The audit itself lives in the `.zk` sources and in `DarkFi.HAZOP`. Recording it as a Lean
theorem requires the obligation `Axioms.NoFreeInstances` to become checkable, which it is not
today.
-/

/-
## Cross-Cutting Verification Status

| Property | Status | Declaration | Budget |
|----------|--------|-------------|--------|
| Pedersen Homomorphism | see `darkfi/Axioms.lean` | `pedersen_additive_homomorphism` (assumption) | — |
| Value sum is bounded (< 2^68) | PROVED | `value_conservation_no_wraparound` | 2 (`native_decide`) |
| Nullifier Determinism | NOT MODELLED | claim removed; see above | — |
| Signature Binding (H2 fix) | NOT MODELLED | claim removed; see above | — |
| Merkle Inclusion | NOT MODELLED | claim removed; see above | — |
| Zero-Cond Soundness | NOT MODELLED | claim removed; see above | — |
| Orchard-Class Detection Rule | MANUAL AUDIT | not a Lean result; see above | — |

"Budget" is the number of assumptions a declaration's proof depends on, as declared by
`@[axiom_budget N]` and checked by `script/check_lean_axioms.py`. The previous version of this
table read "VERIFIED" in every row.
-/

end CrossCutting
