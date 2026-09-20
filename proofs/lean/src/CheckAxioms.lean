/-
# Axiom collector — the fact base for `@[axiom_budget]`

Run with:

    cd proofs/lean && lake env lean --run src/CheckAxioms.lean

It walks every theorem in the imported `DarkFi` environment and prints one TSV line per
declaration:

    <declName>\t<count>\t<axiom1>,<axiom2>,...

`count` is the size of the axiom set `Lean.collectAxioms` reports — the same function
`#print axioms` calls (`Lean/Elab/Print.lean:124`), so this is a real environment walk rather
than a parse of `#print axioms` output. The Python side (`script/check_lean_axioms.py`) splits
that set into:

  * **project assumptions** — names declared in `DarkFi/Axioms.lean`. These are the ones the
    boundary is about.
  * **trust** — `Lean.ofReduceBool`, `Lean.trustCompiler`, `sorryAx`. A
    `native_decide` proof carries the first two, which is why such a proof must not read as
    budget 0.
  * **foundation** — `propext`, `Quot.sound`. Lean's own logic; reported but not charged.
  * **Classical.choice** — reported and charged: the annotation is what "declaring" it means.

The budget of a declaration is `|project| + |trust| + |Classical.choice|`.

## Why an environment walk and not `#print axioms`

`#print axioms` is a command, so reading it means parsing its prose output for every theorem
and hoping the format does not change. `collectAxioms` returns the set directly. It also
recurses through definitions and opaques used by the proof, which is what makes the
`SupplyChain` theorems report `reward` and `pedersen_commit`: those appear in the *body of
`apply_block`*, which those proofs unfold.
-/

import Lean
import DarkFi
import DarkFi.AxiomBudget

open Lean

/-- Collect the axiom set of one declaration by walking the environment. Mirrors
    `Lean.collectAxioms` but usable from `IO` with an explicit environment. -/
def axiomsOf (env : Environment) (name : Name) : Array Name :=
  (((CollectAxioms.collect name).run env).run {}).2.axioms

/--
Reads declaration names from **stdin**, one per line, and prints an axiom row for each.

Why not walk `env.constants`: the environment contains all of Mathlib, and running
`CollectAxioms.collect` (which recurses through every definition a proof mentions) over every
constant in it does not finish. The caller already parses the sources and knows which names are
theorems, so it supplies the list.
-/
def main : IO UInt32 := do
  -- Signature in Lean 4.12: `importModules (imports : Array Import) (opts : Options)
  -- (trustLevel : UInt32 := 0) (leakEnv := false)`.
  let env ← importModules #[{ module := `DarkFi }] {}
  let stdin ← IO.getStdin
  let mut found := 0
  let mut unresolved := 0
  let mut line ← stdin.getLine
  while line.trim.length > 0 do
    let nm : Name := line.trim.toName
    match env.find? nm with
    | some (.thmInfo _) =>
        let axs := (axiomsOf env nm).qsort (fun a b => a.toString < b.toString)
        IO.println s!"{nm}\t{axs.size}\t{",".intercalate (axs.toList.map (·.toString))}"
        found := found + 1
    | some _ =>
        IO.eprintln s!"check_axioms: not a theorem: {nm}"
        unresolved := unresolved + 1
    | none =>
        IO.eprintln s!"check_axioms: unknown declaration: {nm}"
        unresolved := unresolved + 1
    line ← stdin.getLine
  IO.eprintln s!"check_axioms: {found} theorems reported, {unresolved} unresolved"
  return 0
