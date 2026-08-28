/-
DarkWow.Capability.PublicInputs — public-input congruence (T4)

Formalizes the invariant that the generic prover's public inputs equal the
contract's `metadata()` output, in order and value. The prover reads the
`constrain_instance` opcodes from the compiled zkas binary in opcode order and
extracts `bound[heap_idx]` for each (Rust `extract_instances`); the contract's
`metadata()` extracts the same values from the wire params in a fixed order. The
proof verifies iff these two sequences coincide.

The structural fact is that public inputs are a pure function of the target
(heap-index) list: if the constraint-instance order and the metadata order are
the SAME list, the outputs agree; if they differ and the bound values are
distinct, the outputs differ and the L2 proof fails.

NOTE (T4 scope): the theorems here prove the ORDER half of T4 (target-list
equality ⟺ output equality). They do NOT prove the VALUE half — that `bound[slot]`
equals the circuit's in-circuit computation of that slot. The value half is the
derived-rule congruence (T3, `compute_derived` == the circuit expression), which
is cryptographic and rests on the `poseidon_*`/`merkle_*` axioms in
`HashOps`/`CrossCutting`. The observed L2 `invalid proof` is a VALUE failure
(wrong merkle root), not an order failure — so this module names a necessary
condition of T4, not the whole of it.
-/

import Mathlib

namespace DarkFi.Capability

/- Read the public inputs from an ordered list of witness heap indices, in
   order — the model of `extract_instances`. -/
def evaluatePublicInputs (targets : List Nat) (bound : Nat → Nat) : List Nat :=
  targets.map bound

/- The output is order-preserving over target concatenation: the opcode stream
   is processed left-to-right, so a block of opcodes contributes its own
   contiguous slice. -/
theorem evaluatePublicInputs_append (a b : List Nat) (bound : Nat → Nat) :
    evaluatePublicInputs (a ++ b) bound =
      evaluatePublicInputs a bound ++ evaluatePublicInputs b bound := by
  simp [evaluatePublicInputs, List.map_append]

/- The number of public inputs equals the number of constrain_instance targets:
   every target contributes exactly one public input. -/
theorem evaluatePublicInputs_length (targets : List Nat) (bound : Nat → Nat) :
    (evaluatePublicInputs targets bound).length = targets.length := by
  simp [evaluatePublicInputs]

/- `List.map` is injective when its function is: two lists that map to the same
   result are equal (distinct heap indices cannot collapse under an injective
   bound). -/
theorem map_injective_of_injective (bound : Nat → Nat) (h : Function.Injective bound) :
    Function.Injective (List.map bound) := by
  intro xs ys hEq
  induction xs generalizing ys with
  | nil =>
      cases ys with
      | nil => rfl
      | cons y ys' => simp at hEq
  | cons x xs' ih =>
      cases ys with
      | nil => simp at hEq
      | cons y ys' =>
          simp at hEq
          have hx : x = y := h hEq.1
          have hxs : xs' = ys' := ih hEq.2
          rw [hx, hxs]

/- If the bound values are distinct per heap index (injective), then reading the
   public inputs is injective in the target list: two DIFFERENT target orders
   produce two DIFFERENT public-input sequences. This is the structural reason a
   mismatched `constrain_instance` order fails L2 verification. -/
theorem evaluatePublicInputs_injective (bound : Nat → Nat) (h : Function.Injective bound) :
    Function.Injective (fun ts => evaluatePublicInputs ts bound) := by
  intro ts1 ts2 hEq
  exact map_injective_of_injective bound h hEq

/- T4: the prover's public inputs equal the metadata's public inputs. -/
def publicInputsCongruent (proverTargets metadataTargets : List Nat) (bound : Nat → Nat) : Prop :=
  evaluatePublicInputs proverTargets bound = evaluatePublicInputs metadataTargets bound

/- T4 (structural): congruence holds iff the two target lists are equal, given
   an injective bound. So the prover's `constrain_instance` order MUST equal the
   metadata's extraction order — a reversed or reordered pair breaks the proof. -/
theorem publicInputsCongruent_iff_targets_eq (proverTargets metadataTargets : List Nat)
    (bound : Nat → Nat) (h : Function.Injective bound) :
    publicInputsCongruent proverTargets metadataTargets bound ↔ proverTargets = metadataTargets := by
  constructor
  · intro hCong
    exact map_injective_of_injective bound h hCong
  · intro hEq
    rw [hEq]
    rfl

end DarkFi.Capability
