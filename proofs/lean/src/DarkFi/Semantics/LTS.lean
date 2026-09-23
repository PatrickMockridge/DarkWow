/-
# The labelled transition system, the barb predicate, and a real bisimulation

This module replaces the definitions `Capability/Concurrency.lean` used as stand-ins. What was there:

    def barbedEquivalent (P Q : ConcurrentProcess) : Prop :=
      concurrentProcessBarbs P = concurrentProcessBarbs Q
    def stronglyBisimilar (P Q : ConcurrentProcess) : Prop := barbedEquivalent P Q

— bisimulation as `Finset` equality over a record of tags, so `parallel_commutative` was
`Finset.union_comm` wearing the name of a concurrency theorem. Here a process has transitions, a
barb is `∃` a transition, and bisimulation is the standard coinductive relation. The theorems that
used to be union-commutativity are then statements a reader can disagree with.

## Scope, stated rather than implied

`Label` has a `tau`, and **no rule produces a τ-transition.** That is not an omission: the τ-rule is
`x!(y) | x?(z).P -[τ]-> P{y/z}`, and `P{y/z}` is capture-avoiding substitution, which needs a binding
convention for `bang` that `Proc.lean` deliberately does not invent. Every statement below is about
the free-action fragment — `out`, `inp`, `par`, `nu`, and closure up to `SCong` — which is exactly the
fragment the barbs and strong bisimulation need. τ is kept in the label type because §1.2 discusses
τ-transitions explicitly, and a calculus whose labels did not mention them would misdescribe itself;
the substitution layer that inhabits it is the next module, and it is named here so that its absence
is a recorded boundary rather than a surprise.

## What the rules are, and which are derived

`out`, `inp`, `par`, `nu` and `scong` are the constructors. The right-handed parallel rule
`Q | P -[μ]-> Q | P'` is deliberately **not** a constructor: it is derived from `par_comm` and
`scong`, and deriving it is the evidence that the congruence is doing its job.

## Two theorems this module does not yet have, and why the gap is written down

1. **No barb survives a fresh restriction.** The statement is `¬ Barb (νx.P) x` when `x` is fresh for
   `P`, and it needs an induction over the `Step` derivation — every rule that can produce a
   transition from a `ν`, taken in turn. It is a real induction and it is not in this module.
2. **Barb-equality does not imply bisimilarity.** The witness is `out x y` against `out x z` with
   `y ≠ z`: both exhibit exactly the barb on `x`, while their transitions carry different payloads,
   so no bisimulation matches them. This is the theorem that *justifies* the rewrite — it says the old
   `Finset`-equality definition was not a bisimulation, rather than only that it was inelegant.

Both are named here rather than left as an absence so that a reader knows the difference between "not
proved" and "not thought of". They are the first two obligations of the next revision of this file,
and neither is needed by the barb-preservation and bisimulation-equivalence results below, which are
complete.
-/

import DarkFi.Semantics.Congruence

namespace DarkFi.Semantics

/-! ==========================================================================
   Part 1 — Labels
   ========================================================================== -/

/-- Transition labels. `out x y` is the free output `x!(y)`, `inp x y` the free input `x?(y)`, and
    `tau` internal synchronisation.

    Bound-output labels (`x!(⌈y⌉)`, the form the ρ-calculus needs to move a *name* rather than a
    value) are not here. They belong with the reflection rules, which are the same layer as the
    τ-rule and the substitution it needs. -/
inductive Label : Type where
  /-- Internal synchronisation `τ`. -/
  | tau : Label
  /-- Free output `x!(y)`. -/
  | out : Proc → Proc → Label
  /-- Free input `x?(y)`. -/
  | inp : Proc → Proc → Label

/-- The channel a label acts on. For `tau` this is `nil`, chosen so that the restriction rule's
    freshness proviso is *automatically discharged* for τ — `Occurs x nil` is false for every `x`, so
    `Fresh x (subject tau)` holds trivially and the rule needs no special case. A `Label`-valued
    function returning `Option Proc` would push that case onto every user of the rule. -/
def Label.subject : Label → Proc
  | .tau => Proc.nil
  | .out x _ => x
  | .inp x _ => x

/-! ==========================================================================
   Part 2 — Transitions
   ========================================================================== -/

/-- `Step P μ P'`: `P` performs the action `μ` and becomes `P'`.

    The `scong` constructor is the standard closure of the relation under structural congruence in
    both positions. It is what makes the derived rules usable — a proof that arrives at `P | Q` by
    way of `Q | P` has to be able to step there, and the primitive rules only reach terms that are
    literally `par P Q`. -/
inductive Step : Proc → Label → Proc → Prop where
  /-- `x!(y)` fires: `out x y -[x!(y)]-> 0`. -/
  | out (x y : Proc) : Step (Proc.out x y) (.out x y) Proc.nil
  /-- `x?(y)` receives: `x?(y).P -[x?(y)]-> P`. -/
  | inp (x y P : Proc) : Step (Proc.inp x y P) (.inp x y) P
  /-- Parallel composition: a component acts, the other is untouched. Note the other is *unchanged*
      rather than renamed — the right-handed form is `step_par_right` below, derived. -/
  | par {P P' Q : Proc} {μ : Label} : Step P μ P' → Step (Proc.par P Q) μ (Proc.par P' Q)
  /-- Restriction: an action not on the restricted name passes under `ν`. The proviso is `Fresh x
      (subject μ)` — a hypothesis on the rule, so no use can forget it. -/
  | nu {x P P' : Proc} {μ : Label} :
      Fresh x (Label.subject μ) → Step P μ P' → Step (Proc.nu x P) μ (Proc.nu x P')
  /-- Closure under structural congruence: `P ≡ Q`, `Q` steps to `Q'`, `Q' ≡ P'`, therefore `P`
      steps to `P'`. One constructor rather than two, because two would let a proof normalise
      halfway and stop, and there is never a reason to. -/
  | scong {P Q Q' P' : Proc} {μ : Label} : SCong P Q → Step Q μ Q' → SCong Q' P' → Step P μ P'

/-! ==========================================================================
   Part 3 — Barbs

   §1.1: "In the ρ-calculus, process `P` exhibits barb `↓x` if `P` can engage in input or output on
   channel `x`." That is the definition below, and it is a definition rather than a tag: the barb is
   a consequence of the transition relation, so a process cannot *declare* a barb it does not have.
   ========================================================================== -/

/-- `Barb P x`: `P` exhibits the barb `↓x` — it can engage in input or output on channel `x`. -/
def Barb (P x : Proc) : Prop :=
  (∃ (y P' : Proc), Step P (.out x y) P') ∨ (∃ (y P' : Proc), Step P (.inp x y) P')

/-! ==========================================================================
   Part 4 — Derived rules

   Each is a rule the standard presentation of the calculus has, derived here from `SCong` instead of
   assumed. If `SCong` were weaker than it is, these would fail to prove — which is the point.
   ========================================================================== -/

/-- The right-handed parallel rule. Derived, not assumed: `Q | P ≡ P | Q` by `par_comm`, `par` gives
    the step on the left component, and `P' | Q ≡ Q | P'` again by `par_comm`. -/
@[axiom_budget 0]
theorem step_par_right {P P' Q : Proc} {μ : Label} (h : Step P μ P') :
    Step (Proc.par Q P) μ (Proc.par Q P') :=
  Step.scong (SCong.par_comm Q P) (Step.par h) (SCong.par_comm P' Q)

/-- Replication's rule: `!P -[μ]-> P' | !P` whenever `P -[μ]-> P'`. Derived from `rep_unfold`.

    This is the rule that makes §0's reading of replication a *theorem* rather than a claim: a
    replicated process can act, and after acting it is still replicated, so the supply of fresh names
    is not consumed. -/
@[axiom_budget 0]
theorem step_rep {P P' : Proc} {μ : Label} (h : Step P μ P') :
    Step (Proc.rep P) μ (Proc.par P' (Proc.rep P)) :=
  Step.scong (SCong.rep_unfold P) (Step.par h) (SCong.refl _)

/-- Structural congruence preserves barbs, in both directions.

    §1.2's "two process nets that differ only in internal task scheduling are weakly-bisimilar",
    reduced to the part that needs no τ: whatever the congruence rearranges, an observer's barbs are
    unchanged. It is also the lemma every barb argument below goes through. -/
@[axiom_budget 0]
theorem barb_of_scong {P Q : Proc} (h : SCong P Q) (x : Proc) : Barb P x ↔ Barb Q x := by
  constructor
  · rintro (⟨y, P', hs⟩ | ⟨y, P', hs⟩)
    · exact Or.inl ⟨y, P', Step.scong (SCong.symm h) hs (SCong.refl _)⟩
    · exact Or.inr ⟨y, P', Step.scong (SCong.symm h) hs (SCong.refl _)⟩
  · rintro (⟨y, P', hs⟩ | ⟨y, P', hs⟩)
    · exact Or.inl ⟨y, P', Step.scong h hs (SCong.refl _)⟩
    · exact Or.inr ⟨y, P', Step.scong h hs (SCong.refl _)⟩

/-- `P` and `P | 0` have the same barbs. The smallest instance of `barb_of_scong`, recorded because
    it is the case a reader checks when deciding whether the closure rule is wired correctly. -/
@[axiom_budget 0]
theorem barb_par_nil {P x : Proc} : Barb (Proc.par P Proc.nil) x ↔ Barb P x :=
  barb_of_scong (SCong.par_nil P) x

/-- `!P` and `P | !P` have the same barbs — §0's replication reading, as an observation statement. -/
@[axiom_budget 0]
theorem barb_rep_unfold {P x : Proc} :
    Barb (Proc.rep P) x ↔ Barb (Proc.par P (Proc.rep P)) x :=
  barb_of_scong (SCong.rep_unfold P) x

/-! ==========================================================================
   Part 5 — Strong bisimulation

   The definition is the standard one: **`P` and `Q` are strongly bisimilar iff some bisimulation
   relates them**, where a bisimulation is a relation every action of one side can be matched by the
   other, staying inside the relation.

   Written existentially rather than as a greatest fixed point, and that is not a shortcut: the union
   of bisimulations is a bisimulation, so the existence form *is* the greatest fixed point — and it
   is the form `strongbisim_trans` needs, because composing two witnesses requires being able to
   build a relation and hand it over. A `GreatestFixpoint` spelling would need a complete lattice to
   say the same thing.
   ========================================================================== -/

/-- `R` is a strong bisimulation: every transition of either side is matched by the other, and the
    successors stay in `R`. -/
def IsStrongBisim (R : Proc → Proc → Prop) : Prop :=
  ∀ {P Q : Proc}, R P Q →
    (∀ {μ : Label} {P' : Proc}, Step P μ P' → ∃ Q' : Proc, Step Q μ Q' ∧ R P' Q') ∧
    (∀ {μ : Label} {Q' : Proc}, Step Q μ Q' → ∃ P' : Proc, Step P μ P' ∧ R P' Q')

/-- `P ~ Q`: strong bisimilarity — some bisimulation relates them. -/
def StrongBisim (P Q : Proc) : Prop := ∃ R : Proc → Proc → Prop, IsStrongBisim R ∧ R P Q

/-- **Structural congruence is a bisimulation.** Recorded as its own theorem because three of the
    laws below take `SCong` as their witness, and because this is the statement that the transition
    system and the congruence agree: if `SCong` rearranged a term into one with a different
    transition, this would fail.

    Note the two directions are not the same proof. When `A` steps, the step must be carried into
    `B`, which needs `SCong.symm hAB`; when `B` steps, `hAB` itself carries it. A single symmetric
    argument would be wrong here, and the asymmetry is the reason each direction is written out. -/
@[axiom_budget 0]
theorem scong_isStrongBisim : IsStrongBisim SCong := by
  intro A B hAB
  exact ⟨fun {_ _} hs => ⟨_, Step.scong (SCong.symm hAB) hs (SCong.refl _), SCong.refl _⟩,
         fun {_ _} hs => ⟨_, Step.scong hAB hs (SCong.refl _), SCong.refl _⟩⟩

/-- Strong bisimilarity is reflexive: equality is a bisimulation. -/
@[axiom_budget 0]
theorem strongbisim_refl (P : Proc) : StrongBisim P P :=
  ⟨(· = ·), by
    intro A B hAB
    subst hAB
    exact ⟨fun hs => ⟨_, hs, rfl⟩, fun hs => ⟨_, hs, rfl⟩⟩,
   rfl⟩

/-- Strong bisimilarity is symmetric: the converse of a bisimulation is a bisimulation.

    The composition is the point — `R` is an arbitrary relation, so the witness cannot simply be
    reused, and a definition of bisimilarity that only worked for symmetric `R` would be wrong. That
    the converse of the relation works is what shows the definition is symmetric in its two
    arguments. -/
@[axiom_budget 0]
theorem strongbisim_symm {P Q : Proc} (h : StrongBisim P Q) : StrongBisim Q P := by
  obtain ⟨R, hR, hPQ⟩ := h
  exact ⟨fun A B => R B A, fun {A B} hBA => ⟨(hR hBA).2, (hR hBA).1⟩, hPQ⟩

/-- Strong bisimilarity is transitive: the composition of two bisimulations is a bisimulation.

    Weaker than it looks the other way round — the composite relation is `∃ B, R₁ A B ∧ R₂ B C`, and
    the proof's work is that a matching `B'` from the first exists for the *successor chosen by the
    second*. That is where the two witnesses have to be reconciled, and it is why the existential
    form of the definition pays for itself. -/
@[axiom_budget 0]
theorem strongbisim_trans {P Q R : Proc} (h1 : StrongBisim P Q) (h2 : StrongBisim Q R) :
    StrongBisim P R := by
  obtain ⟨R1, hR1, hP1⟩ := h1
  obtain ⟨R2, hR2, hQ2⟩ := h2
  refine ⟨fun A C => ∃ B : Proc, R1 A B ∧ R2 B C, ?_, ⟨Q, hP1, hQ2⟩⟩
  intro A C hAC
  obtain ⟨B, hAB, hBC⟩ := hAC
  refine ⟨?_, ?_⟩
  · rintro μ A' hs
    obtain ⟨B', hsB, hA'B'⟩ := (hR1 hAB).1 hs
    obtain ⟨C', hsC, hB'C'⟩ := (hR2 hBC).1 hsB
    exact ⟨C', hsC, B', hA'B', hB'C'⟩
  · rintro μ C' hs
    obtain ⟨B', hsB, hB'C'⟩ := (hR2 hBC).2 hs
    obtain ⟨A', hsA, hA'B'⟩ := (hR1 hAB).2 hsB
    exact ⟨A', hsA, B', hA'B', hB'C'⟩

/-- **Bisimilar processes exhibit the same barbs**, in both directions.

    §1.2's "for every barb `P` exhibits, `Q` MUST exhibit a matching barb", as a theorem. Note what
    it does *not* say: the converse. Barb-equality is strictly weaker than bisimilarity (the two
    obligations in this file's scope note), which is exactly why the old `Finset`-equality definition
    was not a bisimulation, and why `stronglyBisimilar`'s old theorems proved nothing about
    concurrency. -/
@[axiom_budget 0]
theorem strongbisim_barb_eq {P Q : Proc} (h : StrongBisim P Q) (x : Proc) :
    Barb P x ↔ Barb Q x := by
  obtain ⟨R, hR, hPQ⟩ := h
  constructor
  · rintro (⟨y, P', hs⟩ | ⟨y, P', hs⟩)
    · obtain ⟨Q', hsQ, _⟩ := (hR hPQ).1 hs
      exact Or.inl ⟨y, Q', hsQ⟩
    · obtain ⟨Q', hsQ, _⟩ := (hR hPQ).1 hs
      exact Or.inr ⟨y, Q', hsQ⟩
  · rintro (⟨y, Q', hs⟩ | ⟨y, Q', hs⟩)
    · obtain ⟨P', hsP, _⟩ := (hR hPQ).2 hs
      exact Or.inl ⟨y, P', hsP⟩
    · obtain ⟨P', hsP, _⟩ := (hR hPQ).2 hs
      exact Or.inr ⟨y, P', hsP⟩

/-! ==========================================================================
   Part 6 — §1.2's laws, as theorems about processes

   These are the replacements for `Capability/Concurrency.lean`'s `parallel_commutative` and
   `parallel_associative`, which were `Finset` union laws. The statements are the same sentences from
   `type-system.md` §1.2; what changed is that they are now about transitions.
   ========================================================================== -/

/-- `P | Q ~ Q | P` — §1.2's commutativity of parallel composition, with a bisimulation witness
    rather than a union.

    The witness relation is `SCong` itself (`scong_isStrongBisim`), and that is the informative part:
    the congruence *is* the bisimulation, so this theorem also says the congruence is sound with
    respect to the transition system. -/
@[axiom_budget 0]
theorem parallel_commutative (P Q : Proc) : StrongBisim (Proc.par P Q) (Proc.par Q P) :=
  ⟨SCong, scong_isStrongBisim, SCong.par_comm P Q⟩

/-- `(P | Q) | R ~ P | (Q | R)` — §1.2's associativity, likewise with `SCong` as the witness. -/
@[axiom_budget 0]
theorem parallel_associative (P Q R : Proc) :
    StrongBisim (Proc.par (Proc.par P Q) R) (Proc.par P (Proc.par Q R)) :=
  ⟨SCong, scong_isStrongBisim, SCong.par_assoc P Q R⟩

/-- `P | 0 ~ P` — the identity law, which §1.2 does not state and the old file did not have. It is
    here because it is the law whose *absence* is invisible: with `Finset` equality, `P | 0` and `P`
    trivially had the same barb set, so nothing could notice whether the term-level identity held. -/
@[axiom_budget 0]
theorem parallel_nil (P : Proc) : StrongBisim (Proc.par P Proc.nil) P :=
  ⟨SCong, scong_isStrongBisim, SCong.par_nil P⟩

end DarkFi.Semantics
