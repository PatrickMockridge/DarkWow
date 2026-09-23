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

## The freshness proviso, and why it is on the constructor

`nu_par` is scope extrusion — `νx.(P | Q)  ≡  P | νx.Q` — and it is only sound when `x` is not free
in `P`. The proviso is a field of the constructor rather than a side condition bolted onto uses, so
that **a reader of the relation cannot miss it**: every extrusion in the tree carries its freshness
witness, and a proof that needs one has to produce it. `Fresh` is `Proc.lean`'s syntactic
`¬ Occurs`, whose definition explains why occurrence is not split into free and bound.
-/

import DarkFi.Semantics.Proc

namespace DarkFi.Semantics

open Proc

/-! ==========================================================================
   Part 1 — The relation
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
  /-- **Scope extrusion**, `νx.(P | Q) ≡ P | νx.Q`, **with its freshness proviso**. -/
  | nu_par {x P Q : Proc} (h : Fresh x P) : SCong (nu x (par P Q)) (par P (nu x Q))
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
   Part 2 — Derived laws
   ==========================================================================

   Each of these is a *derivation*, not a constructor: something a reader can check follows from the
   equations above, which is the difference between this module and a relation declared to be true.
   The LTS's rules are stated up to `SCong`, so these are what its proofs consume. -/

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

/-- Extrusion in the other direction: `P | νx.Q ≡ νx.(P | Q)` when `x` is fresh for `P`. Recorded
    separately rather than left as an application of `symm` because it is the direction the LTS's
    restriction rule needs, and a reader should be able to find the rule's shape. -/
@[axiom_budget 0]
theorem scong_par_nu {x P Q : Proc} (h : Fresh x P) : SCong (par P (nu x Q)) (nu x (par P Q)) :=
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
