/-
# Structural congruence — the equations the transition system is stated up to

`type-system.md` §1.2 states the bisimulation laws in prose (`P | Q ≅ Q | P`,
`(P | Q) | R ≅ P | (Q | R)`) and the Lean that was supposed to back them said
`Finset.union_comm` — because there were no processes for the laws to be about. This module is the
first half of the repair: the congruence is now a relation on `Proc`, so the laws are statements
about terms, and `Semantics/LTS.lean` can close transitions up to it.

## Why this is a relation and not an equation

`≡` could have been declared as a function `normalise : Proc → Proc` with `SCong P Q := normalise P =
normalise Q`. That would have been cheaper and worse. A normaliser has to be *proved* to be
idempotent and to compute the right thing, and the interesting content here is not the normal form —
it is *which* equations hold, which is exactly what the constructors below list. The corpus made the
same call for the genesis stages (`Genesis/Ceremony.lean`: "relations, so that determinism is a
theorem") and for the same reason: an equality-of-normal-forms definition would make every law below
true by `rfl` and therefore unreadable.

## The freshness proviso, and why it needs a second relation

`nu_par` is scope extrusion — `νx.(P | Q)  ≡  P | νx.Q` — and it is only sound when `x` is not free in
`P`. The proviso is a field of the constructor rather than a side condition bolted onto uses, so that
**a reader of the relation cannot miss it**: every extrusion in the tree carries its freshness
witness, and a proof that needs one has to produce it.

*Which* freshness notion is not a detail, and the syntactic one does not work. `Proc.lean`'s `Fresh` is
`¬ Occurs x P` over the term as written, and this relation changes what a term mentions: `cong_bang`
with `nu_nil` identifies `⌈νx.0⌉` and `⌈0⌉`, so the *channel* of `out ⌈νx.0⌉ b` can be read as `⌈0⌉` —
a name that does not occur in that term at all. An extrusion asking only `Fresh x P` therefore lets
`x` out through the back door, and `Semantics/LTS.lean` carries the witness.

The notion the rule needs is the invariant one — "no term congruent to `P` mentions `x`" — and that is
what `FreshUpToScong` below is. It cannot be defined in terms of `SCong`, because `SCong` is the
relation whose constructor wants it: the definition would be circular, and no well-formedness trick
removes that. A congruence-invariant *structural* occurrence predicate does not replace it either —
its clauses for name position would have to compare terms up to the congruence, which is the same
reference — and a mutual inductive fails because the freshness rules would need a negative occurrence
of `SCong`. So this file carries two relations, and the first is not a modelling choice: it is what
makes the second's proviso sayable.

* `SCong0` — the same equations with extrusion **unconditional**. A *reachability* closure and nothing
  else: never used to reason about processes, and deliberately unsound as a congruence.
* `FreshUpToScong x P` — `P`, and everything reachable from it, never mentions `x`.
* `SCong` — the relation the rest of the tree uses, with `FreshUpToScong` as extrusion's proviso.

`scong0_of_scong` embeds the second into the first, which is what every argument reaching a term by
`SCong` needs; `fresh_of_freshUpToScong` records that the new notion is strictly stronger than
`Proc.lean`'s syntactic one.
-/

import DarkFi.Semantics.Proc

namespace DarkFi.Semantics

open Proc

/-! ==========================================================================
   Part 1 — The unconditional rearrangement relation
   ========================================================================== -/

/-- `SCong0 P Q`: `Q` is a term `P` can be rearranged into, allowing §1.2's equations *and* scope
    extrusion with no proviso.

    A reachability closure, not a congruence the calculus reasons with: its `nu_par` has no side
    condition, so it relates terms that are not the same process. Its one use is to define
    `FreshUpToScong`, where being over-inclusive is the sound direction — a freshness condition
    quantifying over *more* rearrangements is stronger, not weaker. -/
inductive SCong0 : Proc → Proc → Prop where
  /-- `P` is reachable from itself. -/
  | refl (P : Proc) : SCong0 P P
  /-- Reachability is symmetric. -/
  | symm {P Q : Proc} : SCong0 P Q → SCong0 Q P
  /-- Reachability is transitive. -/
  | trans {P Q R : Proc} : SCong0 P Q → SCong0 Q R → SCong0 P R
  /-- `P | Q ≡ Q | P`. -/
  | par_comm (P Q : Proc) : SCong0 (par P Q) (par Q P)
  /-- `(P | Q) | R ≡ P | (Q | R)`. -/
  | par_assoc (P Q R : Proc) : SCong0 (par (par P Q) R) (par P (par Q R))
  /-- `P | 0 ≡ P`. -/
  | par_nil (P : Proc) : SCong0 (par P nil) P
  /-- `νx.0 ≡ 0`. -/
  | nu_nil (x : Proc) : SCong0 (nu x nil) nil
  /-- `νx.νy.P ≡ νy.νx.P`. -/
  | nu_nu (x y P : Proc) : SCong0 (nu x (nu y P)) (nu y (nu x P))
  /-- Scope extrusion, **with no freshness proviso** — see the module note: this relation
      over-approximates on purpose, and that is what makes it usable as a hypothesis. -/
  | nu_par {x P Q : Proc} : SCong0 (nu x (par P Q)) (par P (nu x Q))
  /-- `!P ≡ P | !P`. -/
  | rep_unfold (P : Proc) : SCong0 (rep P) (par P (rep P))
  /-- `P ≡ Q` gives `⌈P⌉ ≡ ⌈Q⌉`. -/
  | cong_bang {P Q : Proc} : SCong0 P Q → SCong0 (bang P) (bang Q)
  /-- Congruence in an output's channel and payload. -/
  | cong_out {a b c d : Proc} : SCong0 a c → SCong0 b d → SCong0 (out a b) (out c d)
  /-- Congruence in an input's channel, binder and body. -/
  | cong_inp {a b P c d Q : Proc} :
      SCong0 a c → SCong0 b d → SCong0 P Q → SCong0 (inp a b P) (inp c d Q)
  /-- Congruence under restriction, in the bound name and in the body. -/
  | cong_nu {x P y Q : Proc} : SCong0 x y → SCong0 P Q → SCong0 (nu x P) (nu y Q)
  /-- Congruence under replication. -/
  | cong_rep {P Q : Proc} : SCong0 P Q → SCong0 (rep P) (rep Q)
  /-- Congruence under parallel composition. -/
  | cong_par {P Q R S : Proc} : SCong0 P R → SCong0 Q S → SCong0 (par P Q) (par R S)

/-! ==========================================================================
   Part 2 — Freshness up to rearrangement
   ========================================================================== -/

/-- `FreshUpToScong x P`: nothing reachable from `P` by rearrangement mentions `x`.

    The notion the restriction rule and scope extrusion need, and **strictly stronger** than
    `Proc.lean`'s syntactic `Fresh` — `fresh_of_freshUpToScong` is that direction, and the difference
    is not exotic: `⌈νx.0⌉` is `⌈0⌉` under the congruence, so a term whose channel is written `⌈0⌉`
    mentions `⌈0⌉` however its own syntax reads.

    Stated against `SCong0` rather than `SCong` because it has to be — see the module note. Being
    stated against the over-approximating relation makes it the *stronger* hypothesis, which is the
    sound direction for a side condition.

    Two things this does **not** claim, recorded rather than discovered later. It quantifies over
    `Occurs`, which counts bound occurrences, so it is stronger than "`x` is not free in `P`" and
    therefore rejects extrusions the standard rule would permit: it is the strongest condition
    expressible without a binding convention, and a free-occurrence reading is what would tighten it
    to exactly non-freeness. And it is not cheap to discharge — no instance is proved in this tree,
    the derivations that used to pass through extrusion being exactly the ones this proviso exists to
    reject. A side condition carrying a semantic requirement costs what a semantic requirement costs;
    what it buys is that the rule no longer fires where its own witness says it should not. -/
def FreshUpToScong (x P : Proc) : Prop := ∀ Q : Proc, SCong0 P Q → ¬ Occurs x Q

/-! ==========================================================================
   Part 3 — The relation
   ========================================================================== -/

/-- Structural congruence `P ≡ Q`: the equations under which two processes are the same term.

    The first three constructors make it a congruence relation; the rest are the calculus's
    equations, one per line of `type-system.md` §1.2's prose plus the two scoping rules the LTS's
    restriction rule needs. -/
inductive SCong : Proc → Proc → Prop where
  /-- `P ≡ P`. -/
  | refl (P : Proc) : SCong P P
  /-- `P ≡ Q` implies `Q ≡ P`. -/
  | symm {P Q : Proc} : SCong P Q → SCong Q P
  /-- `P ≡ Q ≡ R` implies `P ≡ R`. -/
  | trans {P Q R : Proc} : SCong P Q → SCong Q R → SCong P R
  /-- `P | Q ≡ Q | P` — §1.2's commutativity of parallel composition. -/
  | par_comm (P Q : Proc) : SCong (par P Q) (par Q P)
  /-- `(P | Q) | R ≡ P | (Q | R)` — §1.2's associativity. -/
  | par_assoc (P Q R : Proc) : SCong (par (par P Q) R) (par P (par Q R))
  /-- `P | 0 ≡ P`. -/
  | par_nil (P : Proc) : SCong (par P nil) P
  /-- `νx.0 ≡ 0`. -/
  | nu_nil (x : Proc) : SCong (nu x nil) nil
  /-- `νx.νy.P ≡ νy.νx.P` — two restrictions commute. -/
  | nu_nu (x y P : Proc) : SCong (nu x (nu y P)) (nu y (nu x P))
  /-- **Scope extrusion**, `νx.(P | Q) ≡ P | νx.Q`, **with its freshness proviso**.

      `FreshUpToScong`, not `Proc.lean`'s syntactic `Fresh`: the syntactic condition lets `x` out
      through a term the congruence rewrites, and the module note above records where. -/
  | nu_par {x P Q : Proc} (h : FreshUpToScong x P) : SCong (nu x (par P Q)) (par P (nu x Q))
  /-- `!P ≡ P | !P` — replication is its own unfolding. This is the equation that makes the
      nullifier marker set a *replication* in §0's reading: consuming a name does not exhaust it,
      the supply of fresh names is infinite. -/
  | rep_unfold (P : Proc) : SCong (rep P) (par P (rep P))
  /-- `P ≡ Q` implies `⌈P⌉ ≡ ⌈Q⌉` — the congruence rule for reflection, which is what makes the
      calculus *reflective* rather than merely higher-order: names are compared by the equations of
      the processes they quote. -/
  | cong_bang {P Q : Proc} : SCong P Q → SCong (bang P) (bang Q)
  /-- Congruence in an output's channel and payload. -/
  | cong_out {a b c d : Proc} : SCong a c → SCong b d → SCong (out a b) (out c d)
  /-- Congruence in an input's channel, binder and body. -/
  | cong_inp {a b P c d Q : Proc} :
      SCong a c → SCong b d → SCong P Q → SCong (inp a b P) (inp c d Q)
  /-- Congruence under restriction, in the bound name and in the body. -/
  | cong_nu {x P y Q : Proc} : SCong x y → SCong P Q → SCong (nu x P) (nu y Q)
  /-- Congruence under replication. -/
  | cong_rep {P Q : Proc} : SCong P Q → SCong (rep P) (rep Q)
  /-- Congruence under parallel composition. -/
  | cong_par {P Q R S : Proc} : SCong P R → SCong Q S → SCong (par P Q) (par R S)

/-! ==========================================================================
   Part 4 — Derived laws
   ==========================================================================

   Each of these is a *derivation*, not a constructor: something a reader can check follows from the
   equations above, which is the difference between this module and a relation declared to be true.
   The LTS's rules are stated up to `SCong`, so these are what its proofs consume.

   The first two are the bridge between this file's two relations rather than laws of §1.2; the rest
   are §1.2's equations in the directions a proof actually needs. -/

/-- **`SCong` embeds in `SCong0`**: everything the sound relation relates, the unconditional one
    relates too.

    This is what makes the stronger proviso usable. An argument that reaches a term `Q` by `SCong` —
    `barb_nu_subject_occurs` in `Semantics/LTS.lean` is the one that matters — needs `Q` inside the
    reachability closure before `FreshUpToScong` can say anything about it, and this is that step.
    A 16-case induction; the `nu_par` case is the easy one, because `SCong0`'s extrusion has no
    proviso to discharge. -/
@[axiom_budget 0]
theorem scong0_of_scong {P Q : Proc} (h : SCong P Q) : SCong0 P Q := by
  induction h with
  | refl P => exact SCong0.refl P
  | symm _ ih => exact SCong0.symm ih
  | trans _ _ ih1 ih2 => exact SCong0.trans ih1 ih2
  | par_comm P Q => exact SCong0.par_comm P Q
  | par_assoc P Q R => exact SCong0.par_assoc P Q R
  | par_nil P => exact SCong0.par_nil P
  | nu_nil x => exact SCong0.nu_nil x
  | nu_nu x y P => exact SCong0.nu_nu x y P
  | nu_par _ => exact SCong0.nu_par
  | rep_unfold P => exact SCong0.rep_unfold P
  | cong_bang _ ih => exact SCong0.cong_bang ih
  | cong_out _ _ ih1 ih2 => exact SCong0.cong_out ih1 ih2
  | cong_inp _ _ _ ih1 ih2 ih3 => exact SCong0.cong_inp ih1 ih2 ih3
  | cong_nu _ _ ih1 ih2 => exact SCong0.cong_nu ih1 ih2
  | cong_rep _ ih => exact SCong0.cong_rep ih
  | cong_par _ _ ih1 ih2 => exact SCong0.cong_par ih1 ih2

/-- **`FreshUpToScong` is strictly stronger than `Fresh`.** The direction a reader needs to connect
    the rules' proviso to `Proc.lean`'s definition: whatever satisfies the invariant notion satisfies
    the syntactic one, by instantiating its quantifier at `P` itself. The converse is false, and the
    witness is the one `Semantics/LTS.lean`'s record carries. -/
@[axiom_budget 0]
theorem fresh_of_freshUpToScong {x P : Proc} (h : FreshUpToScong x P) : Fresh x P :=
  h P (SCong0.refl P)

/-- `0 | P ≡ P` — the mirror of `par_nil`, and the reason `par_nil` alone would be a half-statement:
    `P | 0 ≡ P` and `0 | P ≡ P` are different facts about parallel composition, and a proof that
    needs one cannot use the other. -/
@[axiom_budget 0]
theorem scong_nil_par (P : Proc) : SCong (par nil P) P :=
  SCong.trans (SCong.par_comm nil P) (SCong.par_nil P)

/-- The other associativity direction: `P | (Q | R) ≡ (P | Q) | R`. Needed because a proof that
    wants to reassociate must be able to go the way its term is shaped. -/
@[axiom_budget 0]
theorem scong_par_assoc' (P Q R : Proc) : SCong (par P (par Q R)) (par (par P Q) R) :=
  SCong.symm (SCong.par_assoc P Q R)

/-- Extrusion in the other direction: `P | νx.Q ≡ νx.(P | Q)` when nothing reachable from `P`
    mentions `x`. Recorded separately rather than left as an application of `symm` because it is the
    direction the LTS's restriction rule needs, and a reader should be able to find the rule's
    shape. -/
@[axiom_budget 0]
theorem scong_par_nu {x P Q : Proc} (h : FreshUpToScong x P) :
    SCong (par P (nu x Q)) (nu x (par P Q)) :=
  SCong.symm (SCong.nu_par h)

/-- Replication unfolding in the other direction: `P | !P ≡ !P`. -/
@[axiom_budget 0]
theorem scong_par_rep (P : Proc) : SCong (par P (rep P)) (rep P) :=
  SCong.symm (SCong.rep_unfold P)

/- `!!P ≡ !P` is **not** derivable here, and this note is the record of finding that out rather
    than assuming it.

    A plain comment and not a doc comment: it attaches to no declaration, and Lean rejects a doc
    comment that is followed by another doc comment rather than by a declaration — which is what the
    first version of this note did, and the error was that the note itself was the unexpected token.
    (The fix cannot be written literally here: Lean's block comments nest, so a note that quotes the
    doc-comment opener inside a block comment opens a nested comment and the file ends unterminated.
    That cost a second compile, and is recorded because the next person to write about comment
    syntax in a comment will meet it.)

    The standard presentation of the π-calculus' structural congruence has three replication
    equations — `!P ≡ P | !P`, `!0 ≡ 0` and `!(P | Q) ≡ !P | !Q` — and this module has only the
    first. Unfolding `!!P` with it gives `!P | !!P`, i.e. it makes the term *larger*; the collapse to
    `!P` needs `!(P | Q) ≡ !P | !Q` to turn `!!P` into `!P | !P` and then use the first equation to
    absorb one copy. So a proof of `!!P ≡ !P` from `rep_unfold` alone does not exist, and the reflex
    to write one anyway is what produced this note.

    The equations are omitted deliberately, not by oversight: they are not needed for the barb
    predicate (`Semantics/LTS.lean`) or for either bisimulation, and adding an equation costs a
    constructor whose soundness a reader must judge. **They are added when something consumes them,
    and the thing that would is a nullifier argument that treats the marker set as a set rather than
    as a stack** — which is `type-system.md` §0's reading of replication, and is exactly the claim
    `Combinatorial/NullifierStorage.lean` already makes about `markSpent_idempotent`. Until then the
    omission is visible here rather than latent. -/

/-- Congruence in a parallel composition's *right* component alone, with the left held fixed. The
    constructor `cong_par` needs both sides; most rules need one, and writing `SCong.refl` at every
    use is noise that hides which side moved. -/
@[axiom_budget 0]
theorem scong_par_right {P Q R : Proc} (h : SCong Q R) : SCong (par P Q) (par P R) :=
  SCong.cong_par (SCong.refl P) h

/-- Congruence in a parallel composition's *left* component alone. -/
@[axiom_budget 0]
theorem scong_par_left {P Q R : Proc} (h : SCong P Q) : SCong (par P R) (par Q R) :=
  SCong.cong_par h (SCong.refl R)

/-- `≡` is an equivalence relation, as a single statement. The three constructors already give it;
    this records it in the form the rest of the development quotes, so that a use site does not have
    to spell out which of the three it means. -/
@[axiom_budget 0]
theorem scong_equivalence : Equivalence SCong :=
  ⟨SCong.refl, fun h => SCong.symm h, fun h1 h2 => SCong.trans h1 h2⟩

end DarkFi.Semantics
