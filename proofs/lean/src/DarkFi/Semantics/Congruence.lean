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

## Scope extrusion, and why it is not in this relation

`nu_par` was scope extrusion — `νx.(P | Q) ≡ P | νx.Q` — and it is only sound when `x` is not free in
`P`. It is **deleted**, and the reason is worth more than the rule was.

*Which* freshness notion the rule needs is not a detail, and the syntactic one does not work.
`Proc.lean`'s `Fresh` is `¬ Occurs x P` over the term as written, and this relation changes what a term
mentions: `cong_bang` with `nu_nil` identifies `⌈νx.0⌉` and `⌈0⌉`, so the *channel* of `out ⌈νx.0⌉ b`
can be read as `⌈0⌉` — a name that does not occur in that term at all. An extrusion asking only
`Fresh x P` therefore lets `x` out through the back door. The two mechanisms are
`occurs_not_invariant_nu_nil` and `occurs_not_invariant_cong_bang` below, refuting
`occurs_not_scong_invariant`.

The repair was to ask for the invariant notion instead — "no term reachable from `P` mentions `x`" —
and it cannot be defined from `SCong`, because `SCong` is the relation whose constructor wants it, and
no well-formedness trick removes that circularity. So the file carried two relations:

* `SCong0` — the same equations with extrusion **unconditional**. A *reachability* closure and nothing
  else: never used to reason about processes, and deliberately unsound as a congruence.
* `FreshUpToScong x P` — `P`, and everything reachable from it, never mentions `x`.
* `SCong` — the relation the rest of the tree uses.

**That repair made the rule dead, and `not_freshUpToScong` is the proof.** Quantifying over the
*unconditional* relation is what breaks it: `SCong0` can put a binder of any name anywhere, so
`SCong0 P (P | νx.0)` holds for every `P` and every `x`, and `Occurs` counts a binder as an occurrence.
The condition was never expensive to discharge, it was empty — so extrusion could not be applied at
all, and for four commits it read as an available rule while being unusable.

**Extrusion is therefore not in `SCong`, and the equation lives on in `SCong0.nu_par`** —
unconditionally, as the reachability rule it always was. What would bring it back is the proviso the
*body* side genuinely needs: a freshness notion that does not count binders, which is what a binding
convention supplies and what `Proc.lean` deliberately does not invent. `Semantics/LTS.lean`'s
restriction rule does not wait on it, because its condition is on the **label** rather than the body —
and the difference between the two sides is the finding this section records.

`scong0_of_scong` embeds `SCong` into the reachability relation. It is what a reader needs in order to
see that `FreshUpToScong` quantifies over strictly more than `SCong` reaches, which is the whole reason
the condition came out empty rather than strong.
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

    **No `x` and no `P` satisfy this**, and `not_freshUpToScong` below is the proof. It is not a
    condition that is expensive to discharge; it is one that is false — and it is why `SCong`'s
    extrusion rule and `Semantics/LTS.lean`'s `Step.nu` could not fire as they were written.

    What went wrong is worth keeping beside it, because the shape recurs. The proviso quantifies over
    `SCong0`, the *unconditional* reachability relation, and that relation can put a binder of any name
    anywhere: `cong_par` composed with `nu_nil` and `par_nil` gives `SCong0 P (P | νx.0)` for every `P`
    and every `x`, and `Occurs` counts a binder as an occurrence. So the quantifier always reaches a
    term that mentions `x`.

    The note that stood here called this "the strongest condition expressible without a binding
    convention" and said it was "not cheap to discharge". Both were wrong in the same direction: a
    condition no term satisfies is not the strongest of anything, it is empty, and the missing
    instances were not a cost. What survives from the diagnosis is that the *body* side of the
    provisos needs a notion which does not count binders — a free-occurrence reading — and that is
    where the binding convention stays owed. What does not survive is that `Step.nu` needed it at all:
    its condition is about the *label*, `¬ SCong x (Label.subject μ)`, which is satisfiable and is what
    the two refuted witnesses were describing.

    The definition stays, with its refutation beside it, while the extrusion rule that used it does
    not — and the difference is not sentiment. A definition with a theorem saying it is false records a
    mistake. A *rule* whose hypothesis can never hold records one while still reading as an available
    rule, which is what it did for four commits. -/
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
  -- **Scope extrusion stood here, and is deleted.** `νx.(P | Q) ≡ P | νx.Q` carried
  -- `FreshUpToScong x P` as its proviso; `not_freshUpToScong` below proves that is false for every
  -- argument, so the constructor could never be applied — and a rule that cannot fire reads as an
  -- available rule and is not one. It is removed rather than repaired because the repair is the
  -- *body-side* freshness notion, which is what a binding convention would supply and what this
  -- layer does not have. `SCong0.nu_par` is where the equation still lives, unconditionally, as a
  -- reachability rule the calculus deliberately does not reason with; and the LTS's restriction rule
  -- no longer depended on this constructor, because its condition was moved onto the label.
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

    Retained after the extrusion rule's deletion even though nothing consumes it now: it is the
    statement that this file's two relations are *ordered*, and a reader who meets `SCong0` and
    `FreshUpToScong` below needs to know that the second quantifies over strictly more terms than
    `SCong` reaches. That is precisely why the condition it carries turned out to be empty rather than
    strong — see `not_freshUpToScong`.

    A 15-case induction, which was sixteen until the extrusion rule left. -/
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
  | rep_unfold P => exact SCong0.rep_unfold P
  | cong_bang _ ih => exact SCong0.cong_bang ih
  | cong_out _ _ ih1 ih2 => exact SCong0.cong_out ih1 ih2
  | cong_inp _ _ _ ih1 ih2 ih3 => exact SCong0.cong_inp ih1 ih2 ih3
  | cong_nu _ _ ih1 ih2 => exact SCong0.cong_nu ih1 ih2
  | cong_rep _ ih => exact SCong0.cong_rep ih
  | cong_par _ _ ih1 ih2 => exact SCong0.cong_par ih1 ih2

-- **`fresh_of_freshUpToScong` stood here and is deleted.** It said the invariant notion implies
-- `Proc.lean`'s syntactic `Fresh`, by instantiating the quantifier at `P`. It is a true implication
-- between two definitions and it is **vacuous**, because the hypothesis is unsatisfiable
-- (`not_freshUpToScong`, below) — so it is a statement true of nothing, and this corpus deletes those
-- rather than keeping them for their names. What it recorded — that the two notions are ordered, and
-- the invariant one is the stronger — is in `FreshUpToScong`'s docstring.

/-- **`FreshUpToScong` is unsatisfiable.** For every `x` and every `P` there is a term reachable from
    `P` that mentions `x`: `P | νx.0`, which `cong_par` with `nu_nil` and `par_nil` puts in `SCong0`'s
    reach of `P`, and in which `Occurs` counts the binder.

    Stated at arbitrary `x` and `P` rather than as the refutation of a `∀`, because that is the form a
    caller needs: to show that a rule premised on this can never fire, one instantiates it. It is why
    `SCong.nu_par` and the restriction rule `Step.nu` could not fire as written, and why
    `no_barb_nu_of_fresh` — the theorem that stated this layer's obligation 2 — was true of nothing.

    This is the notion's whole remaining content, and it is why the notion is kept while the rule that
    carried it is not: a definition with a refutation beside it records a mistake, whereas a *rule*
    with an unsatisfiable hypothesis records one while still reading as available. -/
@[axiom_budget 0]
theorem not_freshUpToScong (x P : Proc) : ¬ FreshUpToScong x P := by
  intro h
  have hreach : SCong0 P (Proc.par P (Proc.nu x Proc.nil)) :=
    SCong0.symm (SCong0.trans (SCong0.cong_par (SCong0.refl P) (SCong0.nu_nil x))
      (SCong0.par_nil P))
  exact h (Proc.par P (Proc.nu x Proc.nil)) hreach (Or.inr (Or.inl rfl))

/-- **`Occurs` is not invariant under `SCong` — mechanism 1: `nu_nil`.** `νx.0 ≡ 0`, and `x` occurs in
    `νx.0` as the binder while it does not occur in `0` at all.

    A theorem rather than the prose it was, because it is the fact the whole two-relation construction
    exists for, and prose cannot be checked. `Proc.lean` and this file's module note both assert it;
    they now have something to cite. -/
@[axiom_budget 0]
theorem occurs_not_invariant_nu_nil (x : Proc) :
    SCong (Proc.nu x Proc.nil) Proc.nil ∧ Occurs x (Proc.nu x Proc.nil) ∧
      ¬ Occurs x Proc.nil := by
  refine ⟨SCong.nu_nil x, Or.inl rfl, ?_⟩
  intro h
  exact h

/-- **`Occurs` is not invariant under `SCong` — mechanism 2: `cong_bang` with `nu_nil`.** `⌈νx.0⌉` and
    `⌈0⌉` are congruent; `⌈0⌉` occurs in `⌈0⌉`, and it does not occur in `⌈ν0.0⌉`.

    This is the mechanism `LTS.lean`'s scope note blames for the second refuted witness, and it is
    worth separating from the first: `nu_nil` moves a *binder*, while this one rewrites a term in name
    position into a name it does not literally mention. Only the second survives every obvious repair
    to the syntactic notion. -/
@[axiom_budget 0]
theorem occurs_not_invariant_cong_bang :
    SCong (Proc.bang (Proc.nu Proc.nil Proc.nil)) (Proc.bang Proc.nil) ∧
      Occurs (Proc.bang Proc.nil) (Proc.bang Proc.nil) ∧
      ¬ Occurs (Proc.bang Proc.nil) (Proc.bang (Proc.nu Proc.nil Proc.nil)) := by
  refine ⟨SCong.cong_bang (SCong.nu_nil Proc.nil), Or.inl rfl, ?_⟩
  intro h
  rcases h with h | h
  · cases h
  · rcases h with h | h
    · cases h
    · exact h

/-- **`Occurs` is not invariant under the structural congruence**, stated as the refutation of the
    universally quantified claim — so what fails is the tempting statement rather than a straw man. -/
@[axiom_budget 0]
theorem occurs_not_scong_invariant :
    ¬ (∀ (x P Q : Proc), SCong P Q → (Occurs x P ↔ Occurs x Q)) := by
  intro h
  obtain ⟨hs, hp, hnq⟩ := occurs_not_invariant_nu_nil Proc.nil
  exact hnq ((h Proc.nil _ _ hs).1 hp)

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

-- **`scong_par_nu` stood here and is deleted with the rule it mirrored.** It was extrusion in the
-- other direction, `P | νx.Q ≡ νx.(P | Q)`, and it was an application of `symm` to `SCong.nu_par` —
-- which no longer exists. The LTS's restriction rule does not need it: its condition is on the label.

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
