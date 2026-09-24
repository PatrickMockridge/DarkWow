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

**What the convention is worth is a theorem, and the agreement is one-directional.** `LTS.lean`'s Part
4c puts the convention against the observations, and the two do not line up the way one would write them
down first:

* **Every barb has a free name.** `canBarb_has_free_name`: a barb of `P` is on a channel congruent to a
  name that *does* occur free in `P`. No observable comes from nowhere, which is the direction any
  well-formedness argument needs.
* **A fresh name does not block a barb.** `not_barb_of_freshFree_is_false` refutes the converse with `0`
  and `out ⌈ν0.0⌉ ⌈0⌉`: `0` occurs nowhere in that term — `FreeOccurs 0` is the disjunction
  `0 = ⌈ν0.0⌉ ∨ 0 = ⌈0⌉`, both false — and the term barbs on `0` anyway, because a barb compares
  *channels up to `SCong`* and `ν0.0 ≡ 0`. It is `Congruence.lean`'s `occurs_not_invariant_nu_nil` read
  operationally.

So freeness is a syntactic notion here and the congruence is what it is not invariant under, which is
why no proviso in `LTS.lean` tests a name syntactically: `Step.nu`'s condition and `CanBarb`'s `nu`
clause both test the *channel* with `SCong`. The layer gets away with a non-invariant convention because
it only ever uses freeness in the direction that is sound.

## What this module does not have, stated rather than implied

**No α-conversion, and so no laws that would need an occurs-check.** `SCong` has no renaming rule:
`νx.P` is not congruent to `νy.P{y/x}`. Two consequences, both deliberate:

* `subst` *arrests* rather than renames. At a binder for `z` it stops, because those occurrences are
  shadowed; at a quote it stops, because a quote is data. Under any other binder it descends blindly,
  and where that captures a free name of `y` the result is **not** the capture-avoiding substitution.
  That is why `CaptureFree` is a hypothesis the caller carries rather than a property of the
  definition: the definition is total *by arrest*, and the theory only constrains it where there is
  nothing to capture.
* `subst` carries **three** laws and no more, and the reason the rest are missing is *not* the one this
  note gave first. The laws that assert `subst` leaves a term alone need a side condition, and the
  obvious candidate is wrong: `FreshFree z P → P ≠ z → subst P z y = P` is **false**, with the witness
  `Substitution.lean`'s `subst_of_freshFree_is_false` carries — at `z = 0`, `P = 0 | ⌈0⌉`, `y = ⌈0⌉`, the
  term `0` is not *free* in `P` (because `FreeOccurs z 0` is `False` **by definition**, for every `z`)
  and `subst` replaces it anyway. Substitution works on positions, and `nil` is a position.

  So the condition has to be subterm-freeness with `subst`'s own shadowing structure, which
  `Proc.lean`'s `Occurs` cannot supply: its `nil` clause is exactly right for freshness and exactly
  wrong here, and `FreshFree` conflates the two uses. That — not an occurs-check — is what the missing
  laws are waiting for. What is here is the sealing (`freshFree_bang`), the nil case with its hypothesis
  stated (`subst_nil_of_ne`), and the one that needs no hypothesis because the equality test *is* the
  case it is about (`subst_self`).

**The α-rule is not a mechanical extension, and that is measured rather than suspected.** The renaming
rule that would remove `CaptureFree` from `LTS.lean`'s `Step.tau` relates `νx.P` to `νy.P{x/y}` — and
`subst` *relabels*: an action that was on the channel `x` is on `y` afterwards. `CanStep` is a predicate
over labels, and its membership test compares channels with `SCong`, so α-related terms have *different*
label sets. `LTS.lean`'s `subst_moves_the_label` is that fact, carrying the hypothesis `¬ SCong x y` that
makes it non-vacuous. Adding α to the congruence would therefore make `CanStep` — and with it
`canStep_occurs_up_to_scong` and the barb predicates that rest on the same channel test, `CanBarb` and
`scong_channel` among them — fail to be invariant, unless "the same channel" is relaxed to an α-aware
notion everywhere it appears. So the unit is a redesign of the label-level predicates *and* a
constructor on the congruence,
and its first question is what a label's channel *means* when names are defined only up to renaming. Said
here, next to the gap it would close, rather than discovered halfway into it.
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

/-- **Substituting a term for itself is the identity**, and it needs no hypothesis: `z = z` is exactly
    the case where the definition's equality test fires, so the term is replaced by `y` — which is `z`
    here. It is the third law, and the one that shows the test doing its job rather than needing to be
    worked around. -/
@[axiom_budget 0]
theorem subst_self (z y : Proc) : subst z z y = y := by
  unfold subst
  exact if_pos rfl

/-- **The obvious law about `subst` is false, and the witness says why the note above was wrong about
    the reason.** "Nothing to substitute is no change" would read `FreshFree z P → P ≠ z → subst P z y =
    P`, and it fails at `z = 0`, `P = 0 | ⌈0⌉`, `y = ⌈0⌉`: `0` is not free in `P` — `FreeOccurs z 0` is
    `False` **by definition**, for every `z`, including `z = 0` — and `subst` replaces it anyway, because
    substitution works on positions and `nil` is a position.

    That is the real obstacle, and it is a different one from the occurs-check the note used to name: the
    side condition the law needs is *subterm-freeness with `subst`'s own shadowing structure*, not
    freeness. `Proc.lean`'s `Occurs` cannot supply it — its `nil` clause is `False`, which is exactly
    right for freshness and exactly wrong here, and the two uses are conflated in `FreshFree`. Stated as
    a refutation of the universal so that what fails is the tempting statement, the same shape as the
    `Occurs`-invariance refutations in `Congruence.lean`. -/
@[axiom_budget 0]
theorem subst_of_freshFree_is_false :
    ¬ (∀ (z y P : Proc), FreshFree z P → P ≠ z → subst P z y = P) := by
  intro h
  have hfresh : FreshFree Proc.nil (Proc.par Proc.nil (Proc.bang Proc.nil)) := by
    rintro (h1 | h2)
    · exact h1
    · exact h2
  have hne : Proc.par Proc.nil (Proc.bang Proc.nil) ≠ Proc.nil := fun hc => by cases hc
  have hbang : subst (Proc.bang Proc.nil) Proc.nil (Proc.bang Proc.nil) = Proc.bang Proc.nil := by
    show (if Proc.bang Proc.nil = Proc.nil then _ else Proc.bang Proc.nil) = Proc.bang Proc.nil
    exact if_neg (fun hc => by cases hc)
  have hsub : subst (Proc.par Proc.nil (Proc.bang Proc.nil)) Proc.nil (Proc.bang Proc.nil)
      = Proc.par (Proc.bang Proc.nil) (Proc.bang Proc.nil) := by
    show (if Proc.par Proc.nil (Proc.bang Proc.nil) = Proc.nil then _
      else Proc.par (subst Proc.nil Proc.nil (Proc.bang Proc.nil))
        (subst (Proc.bang Proc.nil) Proc.nil (Proc.bang Proc.nil))) = _
    rw [if_neg hne, subst_self, hbang]
  have hbad := h Proc.nil (Proc.bang Proc.nil) (Proc.par Proc.nil (Proc.bang Proc.nil)) hfresh hne
  rw [hsub] at hbad
  exact absurd hbad (fun hc => by cases hc)

end DarkFi.Semantics
