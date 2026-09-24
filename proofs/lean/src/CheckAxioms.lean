/-
# Axiom collector — the fact base for `@[axiom_budget]`

Run with:

    cd proofs/lean && lake env lean --run src/CheckAxioms.lean

It walks every theorem in the imported `DarkFi` environment and prints one TSV line per
declaration:

    <declName>\t<count>\t<axioms,>\t<stmtConsts,>\t<trivial>\t<projection>

The last three columns are the tautology signals, read off the theorem's *type*:
`stmtConsts` is every constant its statement mentions; `trivial` is `True`, or `a = b` with
syntactically equal sides, or a `∧` of such; `projection` is "the proof returns one of its own
binders", i.e. the conclusion restates a hypothesis. The Python side decides what
those mean — in particular only it knows which constants belong to this project, since that is a
property of the source tree rather than of the elaborated term.

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

/-! ## The tautology signals

Three of the four columns this file now emits exist to make "no tautologies" a check rather than
a claim. Each is deliberately a *signal* rather than a verdict: the Python side decides, because
what counts as "a constant of this project" is a property of the source tree, not of the
elaborated term.

The artefacts this is aimed at were all found by reading, not by a tool — `coin b = coin b`,
`a < b → a < b`, `g.output = g.output`, `x = x ∧ y = y ∧ True`, `ocap_scaling : True`. Their
common shape is that the *statement* is true of nothing: either it mentions nothing this project
declares, or it is an identity, or one of its hypotheses restated as its conclusion. -/

/-- Every constant mentioned anywhere in `e`, including under binders.

    Traverses the *statement*, not the proof, so this stays small — a type mentions tens of
    constants where a proof term of a `simp`-heavy theorem can mention thousands. -/
partial def collectConsts (e : Expr) (acc : NameSet) : NameSet :=
  match e with
  | .const n _ => acc.insert n
  | .app f a => collectConsts a (collectConsts f acc)
  | .lam _ t b _ => collectConsts b (collectConsts t acc)
  | .forallE _ t b _ => collectConsts b (collectConsts t acc)
  | .letE _ t v b _ => collectConsts b (collectConsts v (collectConsts t acc))
  | .mdata _ b => collectConsts b acc
  | .proj _ _ b => collectConsts b acc
  | _ => acc

/-- `True`, `a = b` or `a ≤ b` with syntactically equal sides, an `↔` of the same proposition, or
    a conjunction of such — the statements that are true of nothing.

    Not a definitional-equality test and not a proof search: `Expr` equality is syntactic, so this
    flags exactly the statements that say `x = x`, never the ones that need a lemma to see are
    equal. `theorem pallas_coefficients : pallasCurve.a₁ = 0 ∧ ...` has two *different* sides and
    is not flagged, which is the intended behaviour. `≤`/`<` are included because
    `scanRate * blockInterval ≤ scanRate * blockInterval := Nat.le_refl _` is the same artefact as
    `x = x` wearing an inequality. -/
def isTrivialProp : Expr → Bool
  | .const ``True _ => true
  -- `Eq` and `Iff` take two explicit arguments; `LE.le`/`LT.lt` take an implicit carrier and an
  -- implicit instance before them, i.e. four `.app` nodes rather than three. Getting that arity
  -- wrong is silent: the pattern simply never matches, which is how
  -- `scanRate * blockInterval ≤ scanRate * blockInterval` survived the first version of this.
  | .app (.app (.app (.const ``Eq _) _) a) b => a == b
  | .app (.app (.const ``Iff _) a) b => a == b
  | .app (.app (.app (.app (.const ``LE.le _) _) _) a) b => a == b
  | .app (.app (.app (.app (.const ``LT.lt _) _) _) a) b => a == b
  | .app (.app (.const ``And _) a) b => isTrivialProp a && isTrivialProp b
  | .forallE _ _ b _ => isTrivialProp b
  | _ => false

/-- Is the proof term a bare projection — does the theorem, once its binders are stripped, just
    return one of them? `theorem t (h : P) : P := h` and `... := by exact h` both elaborate to
    `fun h => h`.

    This is the test for "the conclusion restates a hypothesis", and it is deliberately done on the
    *proof term* rather than by comparing the conclusion against the binder types. Those two are
    not syntactically equal even when they are the same proposition: the binder's type and the
    conclusion mention the same variable at different de Bruijn depths, so `Expr` equality misses
    `(cv : Int) (h : cv = 0) : cv = 0` — which is the exact shape this column exists to catch.

    It cannot false-positive: if the value is `fun xs => bvar i`, the type is a `∀` whose codomain
    is the `i`-th binder's type by construction. -/
def proofIsProjection (v : Expr) : Bool :=
  let rec go : Expr → Bool
    | .lam _ _ b _ => go b
    | .forallE _ _ b _ => go b
    | .mdata _ b => go b
    | .bvar _ => true
    | _ => false
  go v

/-- **Weak-head-normalise a statement's body**, so a statement that *reduces* to a trivial one is seen
    as one.

    This is the decisive half of the tautology arm, and it was found by measurement rather than by
    reading: `theorem txCommitment_source_bindable (nf pf : List String) : bindable txCommitment nf pf`
    is literally `∀ nf pf, True`, because `bindable`'s `_` catch-all sends that constructor to `True` —
    but the syntactic test saw `bindable txCommitment nf pf`, which is not `True`, so the theorem
    survived the arm written to catch exactly its class. Three such theorems were live in
    `Capability/Prover.lean`. One reduction step turns the statement into `True` and the syntactic test
    then does its job.

    **`forallTelescope` and not a manual strip**, and the first version of this was wrong in a way worth
    keeping: stripping the binders with a plain recursion leaves the body's `bvar`s *unbound*, and
    `Meta.whnf` **panics** on a loose bvar (`PANIC at Lean.Meta.whnfEasyCases …: loose bvar in
    expression`). A smoke test over five hand-picked theorems passed, because the panic needs a body
    whose reduction actually walks the free variable. `forallTelescope` introduces a *free* variable per
    binder in the local context, so the body reduces with nothing loose — the same shape of mistake as
    the gate's arity bug recorded above, where a wrong-looking pattern silently matched nothing.

    Weak-head only, deliberately: it reduces the *head* of the statement (a `def`/`match` application)
    and stops. Full normalisation would unfold every definition in the statement, which is both
    expensive and the wrong test — a statement is vacuous when it *is* `True`, not when some over-eager
    normal form is. -/
def whnfType (env : Environment) (e : Expr) : IO Expr := do
  let ctx : Core.Context := { fileName := "", fileMap := default }
  let st : Core.State := { env := env }
  let (e', _) ← (Meta.MetaM.run' (Meta.forallTelescope e fun _ body => Meta.whnf body)).toIO ctx st
  return e'

/-- **How many of a theorem's explicit binders its proof term never mentions**, paired with how many
    there are.

    A theorem `∀ (a : A) (h : P), Q` elaborates to `fun a h => body`, so inside `body` the outermost
    binder carries the *largest* de Bruijn index: the binder stripped at position `i` of `n` is
    `bvar (n - 1 - i)` from inside. This strips the leading binders, then walks the body counting the
    binders it crosses, so every `bvar` can be mapped back to the outer binder it names.

    `proofIsProjection` is the extreme case of this — a body that is *exactly* one binder, i.e. every
    other binder unreferenced. This is the general one, and it is the class the two existing arms leave
    between them: a statement that is neither an identity nor a restatement of a hypothesis, but whose
    proof ignores an argument the statement asked for.

    Only **explicit** binders are counted. A typeclass or instance binder is routinely unreferenced in a
    proof while being load-bearing through another binder's *type* (`(h : a ≤ a)` carries `LE.le`'s
    instance), so counting those would flag most of the tree and the signal would be worth nothing.

    And the signal is not a verdict, for a reason a reader of it should know: a proof can legitimate-
    ly ignore an explicit binder because *another binder's type already carries it* (`(a : α) (h : a ≤ a) :
    a ≤ a := h` ignores `a` and is not a defect). What the count is genuinely sharp about is the case
    where **every** explicit binder is ignored — then the proof mentions no argument at all, and the
    statement is either constant in all of them or vacuous in all of them. -/
def unreferencedExplicitBinders (v : Expr) : Nat × Nat :=
  let rec strip (e : Expr) (n : Nat) (pos : List Nat) : Nat × List Nat × Expr :=
    match e with
    | .lam _ _ b bi => strip b (n + 1) (if bi == BinderInfo.default then n :: pos else pos)
    | .forallE _ _ b bi => strip b (n + 1) (if bi == BinderInfo.default then n :: pos else pos)
    | .mdata _ b => strip b n pos
    | _ => (n, pos, e)
  let (n, pos, body) := strip v 0 []
  let rec used (e : Expr) (d : Nat) (acc : List Nat) : List Nat :=
    match e with
    | .bvar j => if j ≥ d then (j - d) :: acc else acc
    | .app f a => used a d (used f d acc)
    | .lam _ _ b _ => used b (d + 1) acc
    | .forallE _ _ b _ => used b (d + 1) acc
    | .letE _ _ val b _ => used b (d + 1) (used val d acc)
    | .mdata _ b => used b d acc
    | .proj _ _ b => used b d acc
    | _ => acc
  let u := used body 0 []
  let outer := pos.map (fun i => n - 1 - i)
  ((outer.filter (fun i => !u.contains i)).length, outer.length)

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
    | some (.thmInfo ti) =>
        let axs := (axiomsOf env nm).qsort (fun a b => a.toString < b.toString)
        let axNames := ",".intercalate (axs.toList.map (·.toString))
        let sc := (collectConsts ti.type {}).toList.map (·.toString)
        let stmtConsts := ",".intercalate ((sc.toArray.qsort (· < ·)).toList)
        let (unref, total) := unreferencedExplicitBinders ti.value
        -- The triviality test reads the *reduced* statement, so a statement that reduces to a trivial
        -- one is caught; see `whnfType`.
        let ty ← whnfType env ti.type
        IO.println s!"{nm}\t{axs.size}\t{axNames}\t{stmtConsts}\t{isTrivialProp ty}\t{proofIsProjection ti.value}\t{unref}/{total}"
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
