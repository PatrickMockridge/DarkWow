/-
# Substitution, the binding convention it needs, and why `bang` seals

`LTS.lean`'s scope note names this module as the one that inhabits `Label.tau`: the synchronisation
rule is `x!(y) | x?(z).P -[τ]-> P{y/z}`, so it needs a substitution, and substitution needs a *binding
convention* — which occurrences an operation descends into. `Proc.lean` deliberately does not invent
one. This module does not invent one either: the project's own statement of the calculus fixes it.

## The convention, and where it comes from

* `doc/src/arch/type-system.md` §0's table — `| Reflection | quote(x) | Treat name x as data |` and
  `| Dereference | eval(x) | Treat data x as a name |`.
* `doc/src/arch/contract-wasm-type-system.md` §Quote/Eval — "`quote(val)` produces **canonical bytes**
  and `eval(bytes)` recovers the value".

A quote is *data*: the interior of `⌈P⌉` is reached only through `eval`, so it is not a position a
substitution descends into, and the names inside it are not free at the quote. `FreeOccurs x (bang P)`
is `False` definitionally, and `freshFree_bang` records that it is. This is not an imported convention
standing beside the project's: `Proc`'s own constructor table already says the same in its blockchain
column — "code as data — the deployed contract bytes", and deployed bytes are closed.

## What this module does not have, stated rather than implied

**No α-conversion, and so no laws that would need an occurs-check.** `SCong` has no renaming rule:
`νx.P` is not congruent to `νy.P{y/x}`. Two consequences, both deliberate:

* `subst` *arrests* rather than renames. At a binder for `z` it stops, because those occurrences are
  shadowed; at a quote it stops, because a quote is data. Under any other binder it descends blindly,
  and where that captures a free name of `y` the result is **not** the capture-avoiding substitution.
  That is why `CaptureFree` is a hypothesis the caller carries rather than a property of the
  definition: the definition is total *by arrest*, and the theory only constrains it where there is
  nothing to capture.
* `subst` carries no laws here beyond the quote's sealing, and the reason is worth recording because it
  is the same gap: every law one would state needs `P ≠ z` — `subst P z z = P` fails at `z = 0`, where
  the term `0` has no free occurrence of itself and is still the thing being replaced — and refuting
  `νz.P = z` for arbitrary `z` is an occurs-check, which a structural `cases` cannot close and which
  needs a size induction. So the laws arrive with the renaming rule that removes the need for the
  hypothesis, or they arrive as size inductions. Neither is written down here.
-/

import DarkFi.Semantics.Proc

namespace DarkFi.Semantics

open Proc

/-! ==========================================================================
   Part 1 — Free occurrence, bound occurrence, and capture-freedom
   ========================================================================== -/

/-- `FreeOccurs x P`: `x` occurs in `P` in a position that is not bound.

    `νa.P` and `inp a b P` bind, so an occurrence under one of those for the same term is not free;
    `bang` seals, so nothing inside a quote is free at the quote. See the module note for where that
    second convention comes from. -/
def FreeOccurs (x : Proc) : Proc → Prop
  | .nil => False
  | .bang _ => False
  | .out a b => x = a ∨ x = b
  | .inp a b P => x = a ∨ (x ≠ b ∧ FreeOccurs x P)
  | .nu a P => x ≠ a ∧ FreeOccurs x P
  | .rep P => FreeOccurs x P
  | .par P Q => FreeOccurs x P ∨ FreeOccurs x Q

/-- `BoundOccurs x P`: `x` is *used as a binder* somewhere inside `P`, by a restriction or an input.

    Not the negation of `FreeOccurs` — a term can be bound in one place and free in another — and not
    occurrence either: the question capture-avoidance asks is only about binders, and this is that
    predicate. -/
def BoundOccurs (x : Proc) : Proc → Prop
  | .nil => False
  | .bang _ => False
  | .out _ _ => False
  | .inp _ b P => x = b ∨ BoundOccurs x P
  | .nu a P => x = a ∨ BoundOccurs x P
  | .rep P => BoundOccurs x P
  | .par P Q => BoundOccurs x P ∨ BoundOccurs x Q

/-- `FreshFree x P`: `x` has no free occurrence in `P`. -/
def FreshFree (x P : Proc) : Prop := ¬ FreeOccurs x P

/-- `CaptureFree z y P`: no binder of `P` other than `z` itself binds a term free in `y`.

    `z` is exempt because a binder for `z` shadows the substitution — it stops there rather than
    renaming, and a binder never crossed cannot capture. This is the hypothesis `LTS.lean`'s
    synchronisation rule carries, and it is the standard side condition: it says the substitution is
    the capture-avoiding one without ever writing a renaming down. -/
def CaptureFree (z y P : Proc) : Prop := ∀ w : Proc, w ≠ z → FreeOccurs w y → ¬ BoundOccurs w P

/-! ==========================================================================
   Part 2 — Substitution
   ========================================================================== -/

/-- `subst P z y`: `y` replaces the free occurrences of `z` in `P`.

    Total by *arrest*, not by renaming: at `νz` or an input bound at `z` it stops, because the
    occurrences below are shadowed; at a `bang` it stops, because a quote is data; everywhere else it
    descends. Descending under a *different* binder is the documented gap — where that captures a free
    name of `y` the result is not the capture-avoiding substitution — so callers carry `CaptureFree`
    and the theory constrains the definition only where there is nothing to capture.

    The equality test at every subterm is why `Proc` derives `DecidableEq`: names are arbitrary terms
    here, so recognising the term being replaced is term equality. -/
def subst : Proc → Proc → Proc → Proc
  | x, z, y =>
    if x = z then y
    else
      match x with
      | .nil => .nil
      | .bang P => .bang P
      | .out a b => .out (subst a z y) (subst b z y)
      | .inp a b P => .inp (subst a z y) b (if b = z then P else subst P z y)
      | .nu a P => .nu a (if a = z then P else subst P z y)
      | .rep P => .rep (subst P z y)
      | .par P Q => .par (subst P z y) (subst Q z y)

/-! ==========================================================================
   Part 3 — The one law, and it is the convention itself
   ========================================================================== -/

/-- **A quote seals**: `x` has no free occurrence in `⌈P⌉`, however it occurs inside.

    This is §0's "treat name `x` as data" as a theorem, and it is recorded so that the module note
    and `subst`'s docstring have something to cite rather than asserting the convention. Stated in the
    unfolded form for the same reason `Proc.lean` states `occurs_bang` in its unfolded form: the
    definitional equality is the content, and `FreshFree x (bang P)` — `¬ False`, which is a
    projection and which the axiom gate rejects as one — says less than this does. -/
@[axiom_budget 0]
theorem freshFree_bang {x P : Proc} : FreeOccurs x (Proc.bang P) ↔ False := Iff.rfl

/-- **Substituting for a term that is not `0` leaves `0` alone.** The hypothesis is exactly the one the
    module note says every law here needs: `subst`'s equality test fires on the *term*, so a term with
    no free occurrence of itself is still replaced when it is the thing being replaced. `z ≠ 0` is how
    a caller says that is not what it meant, and it is the shape a general law would have to take. -/
@[axiom_budget 0]
theorem subst_nil_of_ne {z y : Proc} (h : z ≠ Proc.nil) : subst Proc.nil z y = Proc.nil := by
  show (if Proc.nil = z then y else Proc.nil) = Proc.nil
  exact if_neg (fun hc => h hc.symm)

end DarkFi.Semantics
