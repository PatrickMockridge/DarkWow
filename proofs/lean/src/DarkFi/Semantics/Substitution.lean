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

  **Part 4 supplies that condition, and with it the law the paragraph above was waiting for.** `NoSub`
  is defined in lockstep with `subst`'s recursion, so its clauses constrain exactly the nodes `subst`
  visits — which is what makes the identity law provable, and what makes "no subterm equals `z`" the
  wrong guess: `inp` visits its channel and not its binder, and `nu` does not visit its binder at all.
  `subst_eq_self_of_noSub` is the law, for every `y` and with no capture reasoning; `subst_nil_of_ne` is
  recognised below as its `nil` instance; and `noSub_self` is the other end of the same fact, since no
  term satisfies `NoSub z z` — which is `subst_self`, the equality test firing, read as a property of the
  condition. The relation to the refuted candidate is a strictness theorem in both directions
  (`noSub_implies_freshFree`, `noSub_not_of_freshFree`, the second on the *same* witness as the
  refutation above), so the two conditions are not near-misses of each other. The obligation is
  registered as `OBL-T13` in `doc/src/arch/verification-hazop.md`.

**The α-rule is not a mechanical extension, and that is measured rather than suspected.** The renaming
rule that would remove `CaptureFree` from `LTS.lean`'s `Step.tau` relates `νx.P` to `νy.P{x/y}` — and
`subst` *relabels*: an action that was on the channel `x` is on `y` afterwards. `CanStep` is a predicate
over labels, and its membership test compares channels with `SCong`, so α-related terms have *different*
label sets. `LTS.lean`'s `subst_moves_the_label` is that fact, carrying the hypothesis `¬ SCong x y` that
makes it non-vacuous. Adding α to the congruence would therefore make `CanStep` — and with it
`canStep_occurs_up_to_scong` and the barb predicates that rest on the same channel test, `CanBarb` and
`scong_channel` among them — fail to be invariant, unless "the same channel" is relaxed to an α-aware
notion everywhere it appears.

**And the design question that unit was waiting on is now answered, against the relaxation above.**
`LTS.lean`'s Part 10 asks whether the gap is a missing rule or a difference in what a label can express,
and settles it by measurement: α-variants are *separated* — no strong bisimulation relates `νx.(out b x)`
to `νy.(out b y)`, and the coarsest relation in the file distinguishes them too, because the extruded name
occurs free in the label. "Relax the channel test to an α-aware notion" is therefore not a repair that can
be applied at this label type. What α needs is a label whose payload is *bound* — the bound-output form
`LTS.lean`'s `Label` docstring records as absent — and that is a change to the label type and to every rule
that reads one, since `Step.scong` closes steps under the congruence and a larger congruence is a larger
`Step`. Said here, next to the gap it would close, and stated in `LTS.lean` as two theorems rather than as
an estimate: the unit is not a constructor to add, and that is now a measured claim rather than a repeated
guess.

**The α-unit has four parts, and one of them is now done.** (1) the subterm-freeness predicate and the
laws `subst` needs — Part 4 below; (2) what a label's channel *means* when names are defined only up to
renaming; (3) the constructor on `SCong`/`SCong0`; (4) the invariance re-proofs, since `Step.scong` makes
a larger congruence a larger `Step`. Parts 2–4 are the redesign the two paragraphs above measure, and (1)
was the only one that was mechanical — which is the distinction worth keeping: this module's remaining gap
is not a missing lemma.
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

/-! ==========================================================================
   Part 4 — `NoSub`: the side condition the identity law needs

   The note above says the missing laws wait on *subterm-freeness with `subst`'s own shadowing
   structure*, and that `Proc.lean`'s `Occurs` cannot supply it — its `nil` clause is exactly right for
   freshness and exactly wrong here. This part supplies it, and the condition is not the one a reader
   writes down first.

   `NoSub z P` is defined in **lockstep with `subst`'s recursion**: it constrains exactly the nodes
   `subst` visits and nothing else, which is what makes the identity law provable. Two clauses carry
   that, and neither is guessable from freeness:

   * `inp a b P` — the *channel* `a` is visited, the binder `b` is not (it is kept as written), and the
     body `P` is visited only when the binder does not shadow the substitution (`b ≠ z`); so the body's
     clause is discharged by `b = z ∨ ·`;
   * `nu a P` — the binder is *not* visited at all, unlike `inp`'s channel, so the clause carries no
     condition on `a`, and a body under a shadowing binder is exempt.

   From these two the shape of the condition follows, and so does the difference from freeness — in both
   directions, as theorems rather than as a remark. `noSub_implies_freshFree` is the direction that
   holds: every occurrence freeness excludes, subterm-freeness excludes too.
   `noSub_not_of_freshFree` refutes the converse **on the same witness that refutes the `FreshFree`-based
   law above**, so the two conditions are not near-misses of each other — freeness neither follows from
   subterm-freeness nor implies it.

   And what it buys is the law: `subst_eq_self_of_noSub`, for every `y`, with no capture reasoning. The
   condition is about `subst`'s positions and the law is about those positions alone, which is why the
   proof is a structural recursion with no side conditions left over. `subst_nil_of_ne` is that law's
   `nil` instance — `NoSub z nil` *is* `z ≠ nil` — and `noSub_self` is the other end of the same fact:
   no term satisfies `NoSub z z`, which is `subst_self` — the equality test firing — read as a property
   of the condition rather than of the definition. `subst_identity_is_false` records that the hypothesis
   is necessary rather than decorative, and `noSub_witness` measures the law where it is not vacuous.
   ========================================================================== -/

/-- `NoSub z P`: `subst P z y` replaces nothing in `P`, for every `y`.

    Defined in lockstep with `subst`'s recursion, so each clause carries the node's own equality test
    plus the tests of exactly the children the recursion descends into. The two clauses worth reading
    twice are `inp`'s — the channel is visited, the binder is kept, and the body is exempt when the
    binder shadows `z` — and `nu`'s, which has no condition on the binder at all because `subst` does
    not visit it. "No subterm equals `z`" gets both wrong, which is why the predicate is written against
    the definition rather than against occurrence. -/
def NoSub (z : Proc) : Proc → Prop
  | .nil => z ≠ Proc.nil
  | .bang P => Proc.bang P ≠ z
  | .out a b => Proc.out a b ≠ z ∧ NoSub z a ∧ NoSub z b
  | .inp a b P => Proc.inp a b P ≠ z ∧ NoSub z a ∧ (b = z ∨ NoSub z P)
  | .nu a P => Proc.nu a P ≠ z ∧ (a = z ∨ NoSub z P)
  | .rep P => Proc.rep P ≠ z ∧ NoSub z P
  | .par P Q => Proc.par P Q ≠ z ∧ NoSub z P ∧ NoSub z Q

/-- **`NoSub` includes the equality test at every node it visits.** The clause that carries this for a
    compound term is its first conjunct; for `nil` and `bang` the clause *is* the test, in the direction
    the recursion evaluates it. Stated separately because it is the half of `NoSub` that is about the
    root — the other half is about the children — and because `subst_eq_self_of_noSub` consumes it at
    every case. -/
@[axiom_budget 0]
theorem noSub_ne {z : Proc} : ∀ P : Proc, NoSub z P → P ≠ z := by
  intro P
  induction P with
  | nil => intro h hc; exact h hc.symm
  | bang P _ => intro h; exact h
  | out a b _ _ => intro h; exact h.1
  | inp a b P _ _ _ => intro h; exact h.1
  | nu a P _ _ => intro h; exact h.1
  | rep P _ => intro h; exact h.1
  | par P Q _ _ => intro h; exact h.1

/-- **The law the module note says is missing, and the condition it needed.** `subst` leaves `P` alone
    under `NoSub z P` — for every `y`, with no `CaptureFree` and no capture reasoning, because the
    condition is stated at the positions `subst` visits and the proof is that recursion run once with
    every equality test discharged. `subst_nil_of_ne` is its `nil` instance. -/
@[axiom_budget 0]
theorem subst_eq_self_of_noSub {z y : Proc} : ∀ P : Proc, NoSub z P → subst P z y = P := by
  intro P
  induction P with
  | nil =>
      intro h
      show (if Proc.nil = z then y else Proc.nil) = Proc.nil
      exact if_neg (fun hc => h hc.symm)
  | bang P _ih =>
      intro h
      show (if Proc.bang P = z then y else Proc.bang P) = Proc.bang P
      exact if_neg h
  | out a b iha ihb =>
      intro h
      obtain ⟨hr, ha, hb⟩ := h
      show (if Proc.out a b = z then y else Proc.out (subst a z y) (subst b z y)) = Proc.out a b
      rw [if_neg hr, iha ha, ihb hb]
  | inp a b P iha _ihb ihip =>
      intro h
      obtain ⟨hr, ha, hb⟩ := h
      show (if Proc.inp a b P = z then y
        else Proc.inp (subst a z y) b (if b = z then P else subst P z y)) = Proc.inp a b P
      rw [if_neg hr, iha ha]
      by_cases hc : b = z
      · rw [if_pos hc]
      · rw [if_neg hc, ihip (hb.resolve_left hc)]
  | nu a P _iha ihip =>
      intro h
      obtain ⟨hr, hb⟩ := h
      show (if Proc.nu a P = z then y else Proc.nu a (if a = z then P else subst P z y))
        = Proc.nu a P
      rw [if_neg hr]
      by_cases hc : a = z
      · rw [if_pos hc]
      · rw [if_neg hc, ihip (hb.resolve_left hc)]
  | rep P ih =>
      intro h
      obtain ⟨hr, hP⟩ := h
      show (if Proc.rep P = z then y else Proc.rep (subst P z y)) = Proc.rep P
      rw [if_neg hr, ih hP]
  | par P Q ihp ihq =>
      intro h
      obtain ⟨hr, hP, hQ⟩ := h
      show (if Proc.par P Q = z then y else Proc.par (subst P z y) (subst Q z y)) = Proc.par P Q
      rw [if_neg hr, ihp hP, ihq hQ]

/-- **No term is its own `NoSub`.** The equality test fires at the root of `subst z z y`, which is
    `subst_self` — `subst z z y = y` — read as a property of the condition rather than of the
    definition. It is also the reason the condition cannot be spelled "no subterm equals `z`" and be
    equivalent to this one: that spelling says nothing about the root. -/
@[axiom_budget 0]
theorem noSub_self (z : Proc) : ¬ NoSub z z := by
  cases z <;> simp [NoSub]

/-- **`NoSub` is stronger than freeness.** Every occurrence `FreshFree` excludes, subterm-freeness
    excludes too — so the false law's hypothesis was not wrong about *which terms* it let through, only
    about which positions the substitution reaches. -/
@[axiom_budget 0]
theorem noSub_implies_freshFree {z : Proc} : ∀ P : Proc, NoSub z P → FreshFree z P := by
  intro P
  induction P with
  | nil => intro _ hf; exact hf
  | bang P _ => intro _ hf; exact hf
  | out a b _ _ =>
      intro h
      obtain ⟨_, ha, hb⟩ := h
      rintro (h1 | h2)
      · exact noSub_ne a ha h1.symm
      · exact noSub_ne b hb h2.symm
  | inp a b P _ _ ihip =>
      intro h
      obtain ⟨_, ha, hb⟩ := h
      rintro (h1 | ⟨h2, h3⟩)
      · exact noSub_ne a ha h1.symm
      · rcases hb with hbz | hP
        · exact h2 hbz.symm
        · exact ihip hP h3
  | nu a P _ ihip =>
      intro h
      obtain ⟨_, hb⟩ := h
      rintro ⟨h1, h2⟩
      rcases hb with haz | hP
      · exact h1 haz.symm
      · exact ihip hP h2
  | rep P ih =>
      intro h
      exact ih h.2
  | par P Q ihp ihq =>
      intro h
      rintro (h1 | h2)
      · exact ihp h.2.1 h1
      · exact ihq h.2.2 h2

/-- **And strictly so, on the witness that already refuted the freeness-based law.** The converse
    `FreshFree z P → NoSub z P` is false at the same `z = nil`, `P = nil | ⌈nil⌉` as
    `subst_of_freshFree_is_false`: `nil` is not free in `P` (`FreeOccurs z nil` is `False` **by
    definition**, for every `z`) and is a position `subst` visits, so `NoSub nil P` fails on its `nil`
    child. Together with `noSub_implies_freshFree` this makes the two conditions incomparable rather than
    one a near-miss of the other, and it is why the repair is a *different* predicate rather than a
    strengthened `FreshFree`. -/
@[axiom_budget 0]
theorem noSub_not_of_freshFree :
    ¬ (∀ (z P : Proc), FreshFree z P → NoSub z P) := by
  intro h
  have hfresh : FreshFree Proc.nil (Proc.par Proc.nil (Proc.bang Proc.nil)) := by
    rintro (h1 | h2)
    · exact h1
    · exact h2
  have hno : ¬ NoSub Proc.nil (Proc.par Proc.nil (Proc.bang Proc.nil)) := by
    simp [NoSub]
  exact hno (h Proc.nil (Proc.par Proc.nil (Proc.bang Proc.nil)) hfresh)

/-- **The identity law is not unconditional**, so `NoSub` is doing work rather than decorating a
    theorem: at `z = nil`, `y = ⌈nil⌉` the substitution is `y` and not `P`. This is `subst_self` —
    the equality test firing at the root — read as the negation of the hypothesis-free form, and it is
    what makes the pair with `noSub_self` a partition rather than a coincidence. -/
@[axiom_budget 0]
theorem subst_identity_is_false :
    ¬ (∀ (z y P : Proc), subst P z y = P) := by
  intro h
  have hc := h Proc.nil (Proc.bang Proc.nil) Proc.nil
  rw [subst_self] at hc
  exact absurd hc (fun he => by cases he)

/-- **Non-vacuity, measured at a concrete term rather than assumed.** At `z = ⌈nil⌉` and `P = nil!(nil)`
    the equality test is evaluated at three nodes — the root and both children — and fires at none, so
    the law is instantiated where the recursion really descends rather than where it is stopped by a
    clause. And the two children are `nil`, whose clause is the one the `FreshFree` witness above fails:
    the distinction between the two witnesses is the distinction between the two conditions, with
    `z = ⌈nil⌉` here and `z = nil` there. -/
@[axiom_budget 0]
theorem noSub_witness :
    NoSub (Proc.bang Proc.nil) (Proc.out Proc.nil Proc.nil) ∧
      subst (Proc.out Proc.nil Proc.nil) (Proc.bang Proc.nil)
          (Proc.par (Proc.bang Proc.nil) Proc.nil) = Proc.out Proc.nil Proc.nil :=
  ⟨by simp [NoSub],
   subst_eq_self_of_noSub (z := Proc.bang Proc.nil)
     (y := Proc.par (Proc.bang Proc.nil) Proc.nil) (Proc.out Proc.nil Proc.nil) (by simp [NoSub])⟩

end DarkFi.Semantics
