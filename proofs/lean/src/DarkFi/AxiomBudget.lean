/-
# The `@[axiom_budget N]` attribute

Registers the annotation that every theorem in `proofs/lean/` carries:

    @[axiom_budget 1]
    theorem purse_chained_nullifiers_distinct … -- depends on one assumption

`N` is the number of assumptions the theorem's proof reaches, as measured by
`src/CheckAxioms.lean` (which walks the compiled environment with `Lean.collectAxioms` — the same
function `#print axioms` calls). `script/check_lean_axioms.py` fails the build when an annotation
is missing or disagrees with the measurement, and prints the whole table.

## Why this is not in `Axioms.lean`

It was, and that made the annotation unusable almost everywhere. `Axioms.lean` imports
`Arithmetic`, `ECOps`, `Capability.Composition` and `Combinatorial.StateSpace` — so any of those
files importing `Axioms` to get the attribute would be importing itself, transitively. The result
was 124 `unknown attribute [axiom_budget]` errors the moment the annotation pass touched a file
outside that set.

The attribute is a *tool*, not an assumption, so it lives in a leaf module that imports only
`Lean` and can be imported from anywhere.

## What the budget counts

Assumptions declared in `Axioms.lean`, plus `Classical.choice`, `Lean.ofReduceBool`,
`Lean.trustCompiler` and `sorryAx` — so using any of those raises the budget visibly, which is what
"declaring" them means. It does **not** count `propext` or `Quot.sound`, which are Lean's own
logic and are printed in the table as `foundation` rather than charged. `0` therefore means "no
assumption beyond Lean's logic".

Two things the number does *not* capture, both recorded in the HAZOP register:

* **Value-less `opaque` declarations are invisible to `collectAxioms`.** `poseidon_hash_output`
  and `PedersenPoint.add` are assumptions spelled `opaque`, and a theorem resting on one can still
  read `@[axiom_budget 0]`. That hole is closed by a different check — `axiom` and value-less
  `opaque` are permitted only in `Axioms.lean`, where each carries its four fields.
* **`Classical.choice` is reached by ordinary automation.** `decide`, `omega` and `linarith` on
  `String`/`Finset` equality pick up classical decidability, so most theorems here read `1` rather
  than `0`. That is the measurement being honest, not a defect in it.
-/

import Lean

namespace Lean

initialize registerBuiltinAttribute {
  name := `axiom_budget
  descr := "Declares the number of assumptions this declaration's proof depends on."
  add := fun _ _ _ => pure () }

end Lean
