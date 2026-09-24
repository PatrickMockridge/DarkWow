/-
# Instance derivation — modelling `NoFreeInstances` instead of assuming it

`Axioms.NoFreeInstances (r : Resource) (s : Action) : Prop` is an **uninterpreted predicate**: it states
no claim, and its only consumer — `Capability/Inversion.lean`'s `CircuitDerivable` — carries it as a field
that the theorem using that structure never reads (register `OBL-T7`, `HAZOP/Elevated` ELEV-26, "SILENT").
The same file records what closing it *properly* would take: Halo2 constraint-system semantics, a
polynomial commitment scheme and Fiat–Shamir. That is a project.

This module takes the other route, which is the one this tree has already used three times (`CanStep` for
`Step`, `ActionFree` for step-freedom, `CanBarb` for barbs): **define the property over a model, and prove
what the property is for.** The model is a circuit's statement list — `Expr`/`Stmt` as a `.zk` file
declares them, with no PLONK, no polynomial commitment, no Fiat–Shamir.

## What the property is, and the rule that makes it non-trivial

`script/circuit_instance_derivation.py` (register `OBL-Z1`, the register's only `MECHANIZED` row) already
checks the property over circuit **sources**; this module is the thing that check is a check *of*. Reading
it gave the one rule the model has to get right, and it is subtler than the obvious statement:

> `is_determined` differs from `is_derived_expr` at the top level only: there a bare name counts if it is
> any of derived/constant/witness, because those are the leaf inputs a derivation is built from. Here a
> bare witness does **not** count — `constrain_equal_base(some_witness, X)` does not pin `X`, it merely
> re-exposes a variable the prover was already holding.

So there are **two** predicates and the difference is exactly one case: `derivedB` (the leaves a derivation
may be built from — witnesses included) and `determinedB` (what an *exposed instance* must be — a bare
witness is not enough, because the prover chooses it). A model with one predicate would prove the wrong
theorem, and `OBL-T7`'s prose does not distinguish them.

## The hypothesis that is the modelling content: satisfying valuations

The second thing the work turned up, and it is not in the checker at all, is `Satisfies`. The walk's
binding rule says a `constrainEq a b` binds `b`'s name when `a` is determined — sound *because the
circuit's own equality forces `b` to equal `a`*. That is a fact about the **valuation**, not about the name
set, and so is an assignment (`n`'s value *is* its expression's). A model that omits them proves nothing
about agreement: two valuations agreeing on the held inputs can still differ on a name the circuit merely
assigned or equated. The theorem was written without `Satisfies` first and does not close, which is how the
hypothesis was found.

The checker needs no such hypothesis because it analyses **one source** rather than comparing **two
evaluations**. That difference is worth stating: it is why a source analysis and a soundness proof are not
the same instrument, and `OBL-T7`'s "the model's job is to be the thing that check is a check *of*" is only
half the relation — the model is also asked for something the check is not.

## The theorem, and its converse

`soundness` is the property's *point*: if every exposed instance is determined by the statements before it,
then two **satisfying** valuations that agree on the circuit's held inputs produce the same public vector.
That is what "no free instance" means operationally: the public input is a function of what the prover
holds and the circuit's wiring, so a prover cannot choose it.

`free_instance_is_observable` is the converse, in the shape that makes the predicate *falsifiable in the
model* rather than true by construction: when an instance is a bare witness, two assignments agreeing on
every name the circuit **bound** — and differing only in that witness — produce *different* public vectors.
So "no free instance" is not vacuous, and the failure it names is the prover's degree of freedom.

A worked circuit closes the property by `decide` and its negative control fails it, so the worked instance
exercises both verdicts rather than only the positive one.

## What this does not model, stated rather than implied

* **The bridge to `(r, s)`.** This module defines the property **over a circuit's statement list**.
  `Axioms.NoFreeInstances (r : Resource) (s : Action)` is indexed by a resource and an action, and turning
  *that* into a definition needs the function `(r, s) ↦ the circuit source` — and a Lean term cannot read a
  `.zk` file. So the axiom is **not** replaced here, and the plan this unit was built from expected that it
  would be: the honest statement is that the bridge is the residual, and it is the same
  transcription-fidelity gap the checker's own docstring names (it reads `.zk` source, not `.zk.bin`). What
  changes is that the property now *has* a definition for the bridge to be *about*.
* **The reverse direction of `bindEq`.** The `.zk` files write `constrain_equal_base(derived, witness)`,
  which is the direction modelled; the reverse is the same rule with its arguments swapped, which the
  checker's `assign` map records symmetrically.
* **The opcode semantics.** `eval` takes the opcode table as a parameter, so the theorems hold for *any*
  opcode semantics. Deliberate — the property is about *derivation*, not about what the opcodes compute —
  and it is why nothing here rests on the zkas VM's dispatch.
* **`rangeCheck`.** Modelled as binding and determining nothing, which is what the checker does with it.
  Its own semantics is `Comparison.lean`'s subject.
* **Nothing here is a claim about the Rust or the `.zk` files.** The worked circuit is an instance *of the
  model*; that it is a faithful transcription of any particular file is exactly the residual above. -/

import Mathlib
import DarkFi.AxiomBudget

namespace Circuits.InstanceDerivation

/-- A name in the circuit's vocabulary: a constant, a witness, an assigned intermediate, or an opcode. -/
abbrev Name := String

/-- An expression, as a `.zk` `circuit` block writes one: a literal, a name, or an opcode applied to
    arguments.

    No `deriving DecidableEq, Repr`: the `op` constructor recurses through `List Expr`, so the default
    deriving fails ("default handlers have not been implemented yet"). Nothing here compares expressions
    for equality or prints them — the relations that matter are the predicates below. -/
inductive Expr where
  | lit : Nat → Expr
  | var : Name → Expr
  | op : Name → List Expr → Expr

/-- A statement, as a `.zk` `circuit` block declares them. `rangeCheck` is carried because a model that
    dropped it would have to say so, and the checker treats it as binding and determining nothing. -/
inductive Stmt where
  | assign : Name → Expr → Stmt
  | constrainEq : Expr → Expr → Stmt
  | constrainInstance : Expr → Stmt
  | rangeCheck : Expr → Stmt

/-- The bare name an expression is, if it is one. This is what a `constrainEq` can *bind*: an equality
    whose other side is determined turns its variable side into a determined name. -/
def varOf : Expr → Option Name
  | .var n => some n
  | _ => none

/-- `varOf` is injective on what it accepts: if it returns a name, the expression **is** that variable.
    Needed where a proof has the `Option` equation but needs the expression's shape. -/
@[axiom_budget 0]
theorem varOf_eq_some {e : Expr} {n : Name} (h : varOf e = some n) : e = .var n := by
  cases e <;> simp_all [varOf]

/-! ===== The two predicates, and the one case they differ in =====

`derivedB` is the *leaf* relation: an expression a derivation may be built from, where a bare witness
counts because a witness is an input the circuit holds. `determinedB` is the *instance* relation: what an
exposed public input must be, where a bare witness does **not** count. That single difference is the whole
content of the checker's `is_determined` docstring. -/

/- A `mutual` block, and that is not stylistic: written with `args.all (derivedB held bound)` the recursion
   is inside a higher-order argument and Lean's structural checker cannot see the descent ("insufficient
   number of parameters at recursive application"). The explicit list case makes it structural. -/
mutual
  /-- **Derivable from what the circuit holds and has bound** — a `Bool`, so the walk's `if` and a
      transcribed circuit's `decide` both see it structurally. -/
  def derivedB (held bound : List Name) : Expr → Bool
    | .lit _ => true
    | .var n => decide (n ∈ held ∨ n ∈ bound)
    | .op _ args => allDerivedB held bound args

  /-- The argument-list case: every argument derivable. Also the shape `determinedB` needs, which is why
      there is one helper and not two. -/
  def allDerivedB (held bound : List Name) : List Expr → Bool
    | [] => true
    | a :: rest => derivedB held bound a && allDerivedB held bound rest
end

/-- **Determined** — what an exposed instance must be. Identical to `derivedB` except at the top level,
    where a bare witness is **not** a determination: the prover chose it. Its arguments, by contrast, are
    *derivable* — a witness used to compute a value is ordinary wiring. -/
def determinedB (held bound : List Name) : Expr → Bool
  | .lit _ => true
  | .var n => decide (n ∈ bound)
  | .op _ args => allDerivedB held bound args

/-- The derivability relation, as a proposition, for the theorems to state. -/
def derivedExpr (held bound : List Name) (e : Expr) : Prop := derivedB held bound e = true

/-- The determination relation, as a proposition. -/
def determinedExpr (held bound : List Name) (e : Expr) : Prop := determinedB held bound e = true

/-- **The bound set a `constrainEq` produces**: if the first side is determined and the second is a bare
    name, that name becomes bound. This is the multi-hop binding the checker's `support` follows — the
    name is then determined for everything after it.

    A function rather than an inline `match`, so the walk's proofs reason about a *value*: a `match` left
    inside an `if` does not reduce in a goal, which the first version of `boundWalk` discovered. -/
def bindEq (held bound : List Name) (a b : Expr) : List Name :=
  if determinedB held bound a then
    match varOf b with
    | some n => n :: bound
    | none => bound
  else bound

/-! ===== The binding walk =====

The walk carries the set of names the circuit has **bound** so far, and records for each exposure the
expression exposed together with whether it was determined at that point. It is sequential — an assignment
or an equality must precede the exposure it matters for — which is what the checker's "order matters for
classifications 1 and 2" records. -/

/-- The walk over a statement list. `bound` is the set of names bound so far; the returned pair is the
    final bound set and the exposures in order, each with its verdict.

    The `assign` arm is where the multi-hop chain comes from: a name assigned from a derivable expression
    becomes bound, so a later expression over it is derivable, and so on. -/
def boundWalk (held bound : List Name) : List Stmt → List Name × List (Expr × Bool)
  | [] => (bound, [])
  | s :: rest =>
    match s with
    | .assign n e =>
      boundWalk held (if derivedB held bound e then n :: bound else bound) rest
    | .constrainEq a b => boundWalk held (bindEq held bound a b) rest
    | .constrainInstance e =>
      let (bound', exps) := boundWalk held bound rest
      (bound', (e, determinedB held bound e) :: exps)
    | .rangeCheck _ => boundWalk held bound rest

/-- The exposures a circuit makes, in order, each with whether it was determined by what preceded it. -/
def exposures (held : List Name) (cs : List Stmt) : List (Expr × Bool) := (boundWalk held [] cs).2

/-- **The property, as a computation** — which is what lets a transcribed circuit be checked by `decide`. -/
def noFreeInstance (held : List Name) (cs : List Stmt) : Bool :=
  (exposures held cs).all (fun p => p.2)

/-- **The property**: every instance the circuit exposes is determined by what the circuit has bound before
    that exposure. The computation above, as a proposition. -/
def NoFreeInstance (held : List Name) (cs : List Stmt) : Prop := noFreeInstance held cs = true

/-! ===== The semantics ===== -/

/- A `mutual` block for the same reason `derivedB` is — the list case has to recurse structurally. -/
mutual
  /-- The value of an expression under an assignment, given *any* opcode semantics `opVal`. Parameterised
      deliberately: the property is about derivation, not about what the opcodes compute. -/
  def eval (opVal : Name → List Nat → Nat) (v : Name → Nat) : Expr → Nat
    | .lit k => k
    | .var n => v n
    | .op f args => opVal f (evalList opVal v args)

  /-- The argument values, in order. -/
  def evalList (opVal : Name → List Nat → Nat) (v : Name → Nat) : List Expr → List Nat
    | [] => []
    | a :: rest => eval opVal v a :: evalList opVal v rest
end

/-- **A valuation satisfies a circuit**: it respects every assignment (`n`'s value *is* its expression's)
    and every equality.

    This is the hypothesis the binding rule needs, and finding that out is the unit's modelling content —
    see the module note. Neither conjunct is a fact about syntax, so without them the walk proves nothing
    about agreement between two valuations. -/
def Satisfies (opVal : Name → List Nat → Nat) (v : Name → Nat) (cs : List Stmt) : Prop :=
  (∀ n e, Stmt.assign n e ∈ cs → v n = eval opVal v e) ∧
  (∀ a b, Stmt.constrainEq a b ∈ cs → eval opVal v a = eval opVal v b)

/-! ===== Agreement =====

The two agreement theorems are **one `mutual` block**, and that is forced rather than chosen: `Expr` is a
*nested* inductive (`op` takes a `List Expr`), so `induction e with` is rejected outright ("does not
support nested inductive types … has multiple motives") and the expression case and the argument-list case
have to be proved together, each supplying the other's recursive call. -/
mutual
  /-- **A derivable expression has the same value in any two assignments agreeing on what is held and what
      is bound.** The base cases are the whole content: a held or bound name agrees by hypothesis, a
      literal always. -/
  @[axiom_budget 0]
  theorem derivedExpr_agrees (opVal : Name → List Nat → Nat) {held bound : List Name}
      {v₁ v₂ : Name → Nat} (hheld : ∀ n ∈ held, v₁ n = v₂ n)
      (hbound : ∀ n ∈ bound, v₁ n = v₂ n) :
      ∀ e : Expr, derivedB held bound e = true → eval opVal v₁ e = eval opVal v₂ e
    | .lit _, _ => rfl
    | .var n, h => by
      simp only [derivedB, decide_eq_true_eq] at h
      rcases h with h | h
      · exact hheld n h
      · exact hbound n h
    | .op _ args, h => by
      simp only [derivedB, allDerivedB, Bool.and_eq_true, List.all_eq_true] at h
      simp only [eval]
      rw [allDerivedB_agrees opVal hheld hbound args h]

  /-- And an all-derivable argument list has the same value list — the opcode's own semantics stays
      opaque, so only its arguments matter. -/
  @[axiom_budget 0]
  theorem allDerivedB_agrees (opVal : Name → List Nat → Nat) {held bound : List Name}
      {v₁ v₂ : Name → Nat} (hheld : ∀ n ∈ held, v₁ n = v₂ n)
      (hbound : ∀ n ∈ bound, v₁ n = v₂ n) :
      ∀ args : List Expr, allDerivedB held bound args = true →
        evalList opVal v₁ args = evalList opVal v₂ args
    | [], _ => rfl
    | a :: rest, h => by
      simp only [allDerivedB, Bool.and_eq_true, List.all_eq_true] at h
      simp only [evalList]
      rw [derivedExpr_agrees opVal hheld hbound a h.1,
          allDerivedB_agrees opVal hheld hbound rest h.2]
end

/-- **The same for a determined expression**, whose arguments are merely derivable — the one case that
    differs from `derivedB`, and the reason this can be a plain theorem: its recursive need is
    `allDerivedB_agrees`, already proved, so there is no cycle to break. -/
@[axiom_budget 0]
theorem determinedExpr_agrees (opVal : Name → List Nat → Nat) {held bound : List Name}
    {v₁ v₂ : Name → Nat} (hheld : ∀ n ∈ held, v₁ n = v₂ n)
    (hbound : ∀ n ∈ bound, v₁ n = v₂ n) :
    ∀ e : Expr, determinedB held bound e = true → eval opVal v₁ e = eval opVal v₂ e
  | .lit _, _ => rfl
  | .var n, h => by
    simp only [determinedB, decide_eq_true_eq] at h
    exact hbound n h
  | .op _ args, h => by
    simp only [determinedB] at h
    simp only [eval]
    rw [allDerivedB_agrees opVal hheld hbound args h]

/-! ===== The walk's specification ===== -/

/-- **The walk's specification, all of it at once** — every name it binds agrees, and every exposure it
    recorded as determined agrees. One induction, because these are the same argument: the bound set grows
    only by names whose value is pinned, and an exposure is recorded determined only against the bound set
    it had.

    The two `Satisfies` hypotheses are what make the binding arms go through — see the module note for why
    a source analysis does not need them and this proof does. -/
@[axiom_budget 0]
theorem boundWalk_spec (opVal : Name → List Nat → Nat) (held : List Name) {v₁ v₂ : Name → Nat}
    (hheld : ∀ n ∈ held, v₁ n = v₂ n) :
    ∀ cs : List Stmt, Satisfies opVal v₁ cs → Satisfies opVal v₂ cs →
      ∀ bound : List Name, (∀ n ∈ bound, v₁ n = v₂ n) →
      (∀ n ∈ (boundWalk held bound cs).1, v₁ n = v₂ n) ∧
      (∀ p ∈ (boundWalk held bound cs).2,
        p.2 = true → eval opVal v₁ p.1 = eval opVal v₂ p.1) := by
  intro cs
  induction cs with
  | nil => intro _ _ bound hb; exact ⟨hb, by simp [boundWalk]⟩
  | cons s rest ih =>
    intro hsat₁ hsat₂ bound hb
    have hs₁ : Satisfies opVal v₁ rest :=
      ⟨fun n e h => hsat₁.1 n e (List.mem_cons_of_mem _ h),
       fun a b h => hsat₁.2 a b (List.mem_cons_of_mem _ h)⟩
    have hs₂ : Satisfies opVal v₂ rest :=
      ⟨fun n e h => hsat₂.1 n e (List.mem_cons_of_mem _ h),
       fun a b h => hsat₂.2 a b (List.mem_cons_of_mem _ h)⟩
    cases s with
    | assign n e =>
      by_cases h : derivedB held bound e = true
      · have hin : Stmt.assign n e ∈ Stmt.assign n e :: rest := List.mem_cons_self _ _
        have hbn : ∀ m ∈ (n :: bound : List Name), v₁ m = v₂ m := by
          intro m hm
          rcases List.mem_cons.mp hm with hmn | hm
          · rw [hmn]
            rw [hsat₁.1 n e hin, hsat₂.1 n e hin]
            exact derivedExpr_agrees opVal hheld hb e h
          · exact hb m hm
        simpa [boundWalk, h] using ih hs₁ hs₂ (n :: bound) hbn
      · simpa [boundWalk, h] using ih hs₁ hs₂ bound hb
    | constrainEq a b =>
      have hin : Stmt.constrainEq a b ∈ Stmt.constrainEq a b :: rest := List.mem_cons_self _ _
      have hsa := hsat₁.2 a b hin
      have hsb := hsat₂.2 a b hin
      by_cases ha : determinedB held bound a = true
      · have hbn : ∀ m ∈ bindEq held bound a b, v₁ m = v₂ m := by
          intro m hm
          by_cases hv : varOf b = none
          · simp only [bindEq, ha, hv] at hm
            exact hb m hm
          · obtain ⟨n, hn⟩ := Option.ne_none_iff_exists'.mp hv
            have hbvar : b = .var n := varOf_eq_some hn
            simp only [bindEq, ha, hn] at hm
            rcases List.mem_cons.mp hm with hmn | hm
            · rw [hmn]
              have hda : eval opVal v₁ a = eval opVal v₂ a :=
                determinedExpr_agrees opVal hheld hb a ha
              rw [hbvar] at hsa hsb
              simp only [eval] at hsa hsb
              rw [← hsa, hda, hsb]
            · exact hb m hm
        simpa [boundWalk] using ih hs₁ hs₂ _ hbn
      · simpa [boundWalk, bindEq, ha] using ih hs₁ hs₂ bound hb
    | constrainInstance e =>
      dsimp only [boundWalk]
      obtain ⟨h1, h2⟩ := ih hs₁ hs₂ bound hb
      refine ⟨h1, ?_⟩
      intro p hp
      rcases List.mem_cons.mp hp with rfl | hp
      · intro hv
        exact determinedExpr_agrees opVal hheld hb e hv
      · exact h2 p hp
    | rangeCheck _ => simpa [boundWalk] using ih hs₁ hs₂ bound hb

/-- **Soundness — the property's point.** If every instance the circuit exposes is determined before that
    exposure, then two **satisfying** valuations agreeing on the circuit's held inputs produce the same
    public vector. So a public input cannot be moved without moving something the prover holds: that is
    what "no free instance" buys. -/
@[axiom_budget 0]
theorem soundness (opVal : Name → List Nat → Nat) {held : List Name} {cs : List Stmt}
    {v₁ v₂ : Name → Nat} (hheld : ∀ n ∈ held, v₁ n = v₂ n)
    (hsat₁ : Satisfies opVal v₁ cs) (hsat₂ : Satisfies opVal v₂ cs)
    (h : NoFreeInstance held cs) :
    ∀ p ∈ exposures held cs, eval opVal v₁ p.1 = eval opVal v₂ p.1 := by
  intro p hp
  have hb : p.2 = true := List.all_eq_true.mp h p hp
  exact (boundWalk_spec opVal held hheld cs hsat₁ hsat₂ [] (by simp)).2 p hp hb

/-! ===== The converse: a free instance is observable =====

`soundness` alone would be satisfied by a predicate that is never true, so the model has to exhibit the
failure it names. An exposure that is a bare witness — the prover's own choice, not the circuit's — is the
case: two assignments that agree on **every name the circuit bound** and differ on that witness produce
different public vectors. So the negation is inhabited, and what it names is a degree of freedom rather
than a defect in the wiring. -/

/-- **A bare-witness instance is free, and observably so.** Two assignments agreeing on everything the
    circuit holds and binds, differing only in the exposed witness, give different public values — so the
    public vector is *not* a function of the circuit's determined data, which is exactly the freedom
    `NoFreeInstance` forbids. -/
@[axiom_budget 0]
theorem free_instance_is_observable (opVal : Name → List Nat → Nat) :
    ∃ (held bound : List Name) (e : Expr) (v₁ v₂ : Name → Nat),
      e = .var "w" ∧ (∀ n ∈ bound, v₁ n = v₂ n) ∧
      ¬ determinedExpr held bound e ∧
      eval opVal v₁ e ≠ eval opVal v₂ e := by
  refine ⟨[], [], .var "w", (fun _ => 0), (fun n => if n = "w" then 1 else 0), rfl, by simp, ?_, ?_⟩
  · simp [determinedExpr, determinedB]
  · simp [eval]

/-! ===== A worked instance =====

A four-statement circuit in `issue.zk`'s shape: an intermediate assigned from two witnesses, an equality
that binds a third witness to it, and two exposures — of the intermediate, and of an expression over the
bound witness. `worked_circuit_has_no_free_instance` closes the property by `decide`, which is what the
decidability buys. `worked_circuit_without_the_binding_fails` is the negative control: the same circuit
**without** the equality exposes a bare witness, and the property fails — the two inputs the checker's
classifications 1 and 2 separate. -/

/-- The worked circuit: assign `mid` from the witnesses, bind `other` to `mid`, expose `mid`, expose an
    expression over `other`. -/
def workedCircuit : List Stmt :=
  [ .assign "mid" (.op "add" [.var "a", .var "b"])
  , .constrainEq (.var "mid") (.var "other")
  , .constrainInstance (.var "mid")
  , .constrainInstance (.op "add" [.var "other", .lit 1])
  ]

/-- And the same circuit without the equality — the binding that makes `other` determined. -/
def workedCircuitUnbound : List Stmt :=
  [ .assign "mid" (.op "add" [.var "a", .var "b"])
  , .constrainInstance (.var "mid")
  , .constrainInstance (.var "other")
  ]

/-- **The worked circuit has no free instance.** -/
@[axiom_budget 0]
theorem worked_circuit_has_no_free_instance : NoFreeInstance ["a", "b"] workedCircuit := by
  unfold NoFreeInstance
  decide

/-- **And without the binding it does** — `other` is a bare witness at that exposure, so the property
    fails. -/
@[axiom_budget 0]
theorem worked_circuit_without_the_binding_fails :
    ¬ NoFreeInstance ["a", "b"] workedCircuitUnbound := by
  unfold NoFreeInstance
  decide

end Circuits.InstanceDerivation
