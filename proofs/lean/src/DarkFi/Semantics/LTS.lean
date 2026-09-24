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

`Label`'s `tau` now has a producer: `Step.tau` is the synchronisation rule
`x!(y) | x?(z).P -[τ]-> P{y/z}`, and `Semantics/Substitution.lean` supplies `P{y/z}` together with the
binding convention it needs — the one `Proc.lean` declined to invent, and which the spec fixes rather
than this module: §0's "treat name `x` as data" and §Quote/Eval's "`quote(val)` produces canonical
bytes" make a quote seal, so substitution stops at it. The rule carries `CaptureFree` as a proviso
because `SCong` has no renaming rule; that incompleteness is recorded in `Substitution.lean` rather
than hidden by a silent α-renaming — and so is the *size* of the unit that would close it. `α` renames
the channels that labels are made of, so closing it means redesigning this file's label predicates as
well as adding a constructor to the congruence; `subst_moves_the_label` in Part 3 is that fact stated,
and `Substitution.lean`'s note says what it implies for `CanStep` and everything built on it.

Two boundaries remain, and they are separate:

* **Every statement about `CanStep` is about free actions.** `CanStep` is the static label set the barb
  obligations are proved through, and no clause of it produces `τ` — a synchronisation is a property of
  a *pair*, which a per-term label set cannot see. So its three lemmas carry `IsAction μ`, and `τ` never
  enters the term-level recursion. That is deliberate rather than a shortcut: a synchronisation clause
  inside the `par` case would have to relate the clauses of two different associations in
  `par_assoc`, which is where the invariance proof would stop being structural.
* **§1.2's weak equation is a false law, and both its readings are refuted.** Part 9 records it: the
  equation `P | a?(x).Q | a!(v).R ≈ P | Q{v/x} | R` fails at `Q = R = 0`, because its left side
  retains both prefixes and so exhibits barbs the right side need not.
  `section_1_2_equation_not_strong` settles the strong reading; `section_1_2_equation_not_weak` settles
  the weak one, through Part 3c's `ActionFree`. What the equation is reaching for is the reduction,
  which `Step.tau` now gives.

  The weak relation `WeakBisim` and its laws are here (Part 9b): `scong_isWeakBisim` — which is §1.2's
  "differ only in internal task scheduling" sentence in the reading a calculus of processes can support,
  rearranging a term rather than reordering a scheduler — with `strong_is_weak`, reflexivity, symmetry,
  transitivity and the three par laws. Transitivity is the one whose proof is not `τ`-blind: the two
  witnesses' `τ`-runs have to be woven, which is what `stepTauStar_of_weak` and its mirror are for.
  Nothing models a scheduler, so "scheduling" in the net sense stays prose rather than becoming an
  absence — `WeakBisim` is the relation that would make it provable, and the sentence reads as that
  relation's motivation rather than as one of its consequences.

  §1.2's *third* relation is here too: `BarbedEq` is its `≅` read literally as "the barbs match", with
  both stated laws. It is strictly weaker than bisimilarity — `barb_eq_not_strongbisim` is the witness —
  and the standard barbed equivalence closes the relation under every context, which matching barbs
  alone does not. The barb results are unaffected by all of this: a barb is an action, and `Barb` never
  saw `τ`.

## What the rules are, and which are derived

`out`, `inp`, `par`, `nu` and `scong` are the constructors. The right-handed parallel rule
`Q | P -[μ]-> Q | P'` is deliberately **not** a constructor: it is derived from `par_comm` and
`scong`, and deriving it is the evidence that the congruence is doing its job.

## The two obligations — one settled, one open again

An earlier revision of this module named two theorems it did not have, and both have since been
answered twice over. The first answer holds. The second does not, and the reason is below.

1. **Barb-equality does not imply bisimilarity** — proved, as `barb_eq_not_strongbisim`. The witness
   is `out x 0` against `out x (out 0 0)`: by `barb_out_iff` both exhibit exactly the barbs `SCong x
   w`, and no bisimulation relates them, because the label `x!(0)` that `out x 0` takes as its axiom
   can be matched from `out x (out 0 0)` only by a transition whose payload is congruent to `0` — and
   `0` is not congruent to `out 0 0`. The property making the
   pair non-bisimilar is therefore `¬ SCong y z`, **not** `y ≠ z` — this module's earlier statement of
   the obligation said `y ≠ z`, and that was wrong: `Step.scong` composes with `Step.out`, so
   `out x (par y 0)` takes the label `x!(y)` too. This is the theorem that *justifies* the rewrite. It
   says the old `stronglyBisimilar` — `Finset` equality over a record of tags — was not a bisimulation,
   rather than only that it was inelegant.

2. **No barb survives a fresh restriction** — this module has had four answers to this one. The third
   said it was open and named the wrong residual; the fourth is the measurement that corrects it. The
   record is the point, so all four are kept.

   **First answer — refuted.** Under `Proc.lean`'s *syntactic* freshness the sentence is **false**, in
   two independent ways, and the witnesses are recorded rather than stated because neither proof nor
   refutation was available for them:

   * at `x = 0` and `P = out (ν0.0) ⌈0⌉`, `Fresh 0 P` held and `Barb (ν0.P) 0` held with it, because
     `Occurs 0 0` is `False` — the proviso `Fresh x (subject μ)` was discharged *vacuously* when the
     action's channel was the restricted name, and `Label.subject .tau = 0` is what makes `0` a name as
     well as the τ-subject, so that instance bites there;
   * at `x = ⌈0⌉`, where `Fresh x x` fails and the first cause cannot apply, the barb escaped through
     `nu_par` and `cong_bang` instead: the channel `⌈ν0.0⌉` can be *read* as `⌈0⌉`, a name that does
     not occur in the term as written.

   The second is the general one, and it is **not** about binding: `⌈ν0.0⌉` is a channel, a free-name
   position already, and it is `cong_bang` that identifies it with `⌈0⌉`.

   **Second answer — vacuous.** The repair was to move the proviso onto a notion invariant under the
   congruence: `Congruence.lean`'s `FreshUpToScong`, defined over `SCong0`, the *unconditional*
   reachability relation. `not_freshUpToScong` proves that notion is **false for every argument**, so
   `no_barb_nu_of_fresh` — the theorem that stated this obligation — was true of nothing, and the
   restriction rule could not fire at all. Its hypothesis was not expensive to discharge; it was
   unsatisfiable, and the reason is `cong_par` composed with `nu_nil` and `par_nil`:
   `SCong0 P (P | νx.0)` for every `P` and every `x`, and `Occurs` counts a binder as an occurrence.

   **Third answer — the rule fixed, and the obligation measured back to open.** The restriction rule
   now carries `¬ SCong x (Label.subject μ)`: the condition is on the *label*, which is where the
   standard rule puts it, rather than on the body. It is satisfiable, and both witnesses above fall to
   it in one step, each being an action whose channel is `SCong`-equal to the restricted name.

   What that does **not** do is close the obligation, and the measurement says exactly where it stops.
   Case-analysing a derivation of `Step (νx.P) μ P'` closes the `nu` constructor immediately
   (`hprov (SCong.refl x)`), and `cases` eliminates every other constructor as a head-shape
   contradiction on its own — a `ν`-headed term is not an `out`, an `inp`, a `tau`-source or a `par`.
   Exactly one case survives: `scong`, carrying `SCong (νx.P) Q` and `Step Q μ Q'`, a restriction
   reached through a *congruent* term.

   `SCong.nu_par` was removed for that reason — it was the one rule that let the congruence move a `ν`
   — and removing it is necessary but **not sufficient**.

   **Fourth answer — the residual is not a shape lemma, and both readings of it are settled.** The third
   answer recorded the missing step as "a congruence-class shape lemma: that a `ν`-headed term's
   congruence class is `ν`-headed, with a binder `SCong`-equal to the original". Neither reading of that
   is a step towards the obligation:

   * with `SCong` as the conclusion it is **free** — symmetry and transitivity give it from the
     hypothesis alone, at `y := x` and `Q' := R`, so it is a restatement and constrains nothing;
   * as an *equality* on `Q`'s head — which is what the `scong` case needs, since the step rules have to
     be applied to a term whose head is syntactically visible — it is **false**, and
     `scong_nu_shape_false` is the witness: `nu_nu` permutes two binders, so the head binder of a
     congruent `ν`-headed term need not be congruent to the original, and `nu_nil` removes one outright.

   What is left is a fact about the *chain* of restrictions rather than about its head — the set of
   binders along it, modulo permutation (`nu_nu`), congruence (`cong_nu`) and collapse (`nu_nil`) — and
   no predicate in this tree expresses that. The obligation's state is therefore sharp in both
   directions: `barb_nu_of_not_scong` is its **sufficiency** half, that a restriction blocks nothing it
   should not, and its **necessity** half is `¬ Barb (νx.P) x`, open.

   `no_barb_nu_of_fresh` is deleted rather than left standing, because a statement whose hypothesis is
   unsatisfiable is true of nothing and this corpus does not keep those for their names.

   `barb_nu_subject_occurs` survives and is unaffected: a barb on `x` out of `νx.P` still produces a
   term congruent to `P` that mentions `x`, and that is the lemma naming what a barb through a
   restriction would have to be.

   `Proc.lean`'s syntactic `Fresh` no longer appears in any rule. It remains the honest description of
   a term as written, and `FreshUpToScong` is defined from it — and is false, which
   `not_freshUpToScong` says beside the definition.

Neither obligation is needed by the barb-preservation and bisimulation-equivalence results below,
which are complete.
-/

import DarkFi.Semantics.Congruence
import DarkFi.Semantics.Substitution

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

/-- `IsAction μ`: `μ` is a free action — an output or an input, not `τ`.

    Named for the vocabulary the rules already use ("`P` performs the action `μ`"), and it is the
    shape the barbed fragment is stated over: a barb is an action on a channel, `τ` is not, and
    `CanStep` — the static label set the barb obligations are proved through — is an approximation of
    the *actions* only. Stating that as a hypothesis rather than folding it into `CanStep` is what
    keeps `τ` out of the term-level recursion, which is where the invariance proof would otherwise
    have to relate the synchronisation clauses of two different associations. -/
def IsAction (μ : Label) : Prop := ∃ x y : Proc, μ = Label.out x y ∨ μ = Label.inp x y

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
  /-- **Synchronisation**: `x!(y) | x?(z).P -[τ]-> P{y/z}`, the only rule that produces `τ`.

      This is the rule this module's scope note used to record as absent, and the reason
      `Semantics/Substitution.lean` exists. It is stated on the parallel composition of the two
      actions rather than left to the reader to assemble from `par` and `out`/`inp`: a version stated
      per-component would be a different calculus.

      The proviso is `CaptureFree`, and it is a *proviso* rather than a property of `subst` because
      `SCong` has no renaming rule — an α-equivalent pair of interactions is not derivable here, and
      that incompleteness is recorded in `Substitution.lean` rather than hidden by a silent renaming.
      The other order of the two components is reachable by `par_comm` and `Step.scong`, so it needs
      no rule of its own; nothing consumes it yet, so it is not stated. -/
  | tau {x y z P : Proc} (h : CaptureFree z y P) :
      Step (Proc.par (Proc.out x y) (Proc.inp x z P)) .tau (subst P z y)
  /-- Parallel composition: a component acts, the other is untouched. Note the other is *unchanged*
      rather than renamed — the right-handed form is `step_par_right` below, derived. -/
  | par {P P' Q : Proc} {μ : Label} : Step P μ P' → Step (Proc.par P Q) μ (Proc.par P' Q)
  /-- Restriction: an action not on the restricted name passes under `ν`. The proviso is
      `¬ SCong x (subject μ)` — a hypothesis on the rule, so no use can forget it.

      *About the label rather than the body*, which is the shape the standard rule has: `x ∉ n(μ)`
      for the action labels, and nothing to check for `τ`, whose `subject` is `0` and which no barb
      can be on. The condition has to be congruence-invariant because the channel is a term the
      congruence rewrites: `out ⌈νx.0⌉ b` steps with the label `⌈0⌉!(b)`, and `SCong ⌈0⌉ ⌈νx.0⌉`
      holds, so a *syntactic* proviso on `⌈0⌉` is discharged vacuously and the rule lets an action on
      a name it should not through. Both of the refuted witnesses this file's scope note records are
      that, in two forms, and both are blocked here.

      This replaces `FreshUpToScong x (subject μ)`, which `Congruence.lean`'s `not_freshUpToScong`
      proves is false for every argument. The rule described by the docstring above could therefore
      never fire, and no restricted process could step at all. That the repair is one line is the
      point: the condition belongs on the *label*, and it is the body-side reading — what extrusion
      needs — that is still waiting on the binding convention. -/
  | nu {x P P' : Proc} {μ : Label} :
      ¬ SCong x (Label.subject μ) → Step P μ P' → Step (Proc.nu x P) μ (Proc.nu x P')
  /-- Closure under structural congruence: `P ≡ Q`, `Q` steps to `Q'`, `Q' ≡ P'`, therefore `P`
      steps to `P'`. One constructor rather than two, because two would let a proof normalise
      halfway and stop, and there is never a reason to. -/
  | scong {P Q Q' P' : Proc} {μ : Label} : SCong P Q → Step Q μ Q' → SCong Q' P' → Step P μ P'

/-! ==========================================================================
   Part 3 — The static label set: what a term can step on, up to `SCong`

   `Step` is not structural. Its `scong` constructor steps a process by first being *rearranged*, so
   nothing about it can be established by recursion on the term — and both obligations this file owed
   needed exactly that. `CanStep` is the structural over-approximation that fixes it: defined by
   recursion on `Proc`, proved invariant under `SCong` (`canStep_of_scong`), and proved to contain
   every step (`canStep_of_step`). Together those two turn a question about transitions into a
   question about terms.

   It is deliberately *loose*, and the looseness is load-bearing. The `nu` and `rep` clauses drop
   their structure entirely, because `SCong` moves terms in and out of a restriction
   (`nu_nil : νx.0 ≡ 0`) and a proviso tracking scope syntactically does not survive that. What the
   looseness costs is precision on terms under a `ν`; what it buys is `canStep_of_scong`, and with it
   every obligation in this file. Extrusion used to be the second such equation and is no longer in
   `SCong` at all — `Congruence.lean`'s module note has that account, and it makes this looseness the
   more necessary rather than the less.
   ========================================================================== -/

/-- `CanStep P μ`: `μ` is a label `P` can engage in, read off the term rather than off the transition
    relation.

    `out a b` and `inp a b _` contribute their action closed up to `SCong` on the channel and the
    payload — `cong_out` rearranges `out a b` into `out c d` for any `c ≡ a`, `d ≡ b`, and the label
    records the rearranged form. `par` is the union; `nu` and `rep` are the body's, ignoring the
    binder; the atoms with no rule (`nil`, `bang`) contribute nothing. -/
def CanStep : Proc → Label → Prop
  | .nil, _ => False
  | .bang _, _ => False
  | .out a b, μ => ∃ c d : Proc, μ = Label.out c d ∧ SCong a c ∧ SCong b d
  | .inp a b _, μ => ∃ c d : Proc, μ = Label.inp c d ∧ SCong a c ∧ SCong b d
  | .nu _ P, μ => CanStep P μ
  | .rep P, μ => CanStep P μ
  | .par P Q, μ => CanStep P μ ∨ CanStep Q μ

/-- **`SCong` cannot change what a term can step on.** This is the lemma the file's obligations turn
    on, and it is provable *because* `CanStep` was defined structurally and then checked against the
    congruence — the opposite order to the one `Step` allows.

    Fifteen cases, one per constructor. Three are worth naming. `par_nil` and `nu_nil` hold because the
    atom clauses return `False`; `nu_nu` needs nothing at all, because the `nu` clause ignores its
    binder — the restriction rule's side condition is what the *rule* needs, not what the label set
    needs, and this proof is where that distinction becomes visible. It was sixteen cases until the
    extrusion constructor was deleted; see `Congruence.lean`'s module note for why. -/
@[axiom_budget 0]
theorem canStep_of_scong {P Q : Proc} (h : SCong P Q) (μ : Label) :
    CanStep P μ ↔ CanStep Q μ := by
  induction h with
  | refl _ => exact Iff.rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih1 ih2 => exact ih1.trans ih2
  | par_comm _ _ => simp only [CanStep]; exact or_comm
  | par_assoc _ _ _ => simp only [CanStep]; exact or_assoc
  | par_nil _ =>
    simp only [CanStep]
    exact ⟨fun h => h.elim id False.elim, Or.inl⟩
  | nu_nil _ => exact Iff.rfl
  | nu_nu _ _ _ => exact Iff.rfl
  | rep_unfold _ =>
    simp only [CanStep]
    exact ⟨Or.inl, fun h => h.elim id id⟩
  | cong_bang _ _ => exact Iff.rfl
  | cong_out h1 h2 _ _ =>
    simp only [CanStep]
    constructor
    · rintro ⟨c, d, he, hac, hbd⟩
      exact ⟨c, d, he, SCong.trans (SCong.symm h1) hac, SCong.trans (SCong.symm h2) hbd⟩
    · rintro ⟨c, d, he, hcc, hdd⟩
      exact ⟨c, d, he, SCong.trans h1 hcc, SCong.trans h2 hdd⟩
  | cong_inp h1 h2 _ _ _ _ =>
    simp only [CanStep]
    constructor
    · rintro ⟨c, d, he, hac, hbd⟩
      exact ⟨c, d, he, SCong.trans (SCong.symm h1) hac, SCong.trans (SCong.symm h2) hbd⟩
    · rintro ⟨c, d, he, hcc, hdd⟩
      exact ⟨c, d, he, SCong.trans h1 hcc, SCong.trans h2 hdd⟩
  | cong_nu _ _ _ ih2 => simp only [CanStep]; exact ih2
  | cong_rep _ ih => simp only [CanStep]; exact ih
  | cong_par _ _ ih1 ih2 => simp only [CanStep]; exact or_congr ih1 ih2

/-- Every *action* a term can take is in the static set. This is the direction that makes `CanStep`
    usable: it is an over-approximation, and an over-approximation of `Step` is what a proof gets to
    reason on.

    The `scong` case is the whole point of the design and takes one line — the induction hypothesis is
    about the *congruent* term `Q`, and `canStep_of_scong` carries it back to `P`. That is the case an
    induction over `Step` cannot close on its own, because `Q` is a variable there.

    The `IsAction` hypothesis is what `τ` costs. It cannot be dropped: a synchronisation step's label
    is not in any term's `CanStep`, because no clause of that definition produces `τ`, and the case
    below is the discharge — so with a `τ` rule in the tree, `CanStep` no longer contains *every*
    step's label and the statement has to say which labels it is about. `τ` cases stay out of the
    term-level recursion instead, which is what keeps `canStep_of_scong` from having to relate the
    synchronisation clauses of two different associations. -/
@[axiom_budget 0]
theorem canStep_of_step {P : Proc} {μ : Label} {P' : Proc} (h : Step P μ P') (hμ : IsAction μ) :
    CanStep P μ := by
  revert hμ
  induction h with
  | out x y => intro _; simp only [CanStep]; exact ⟨x, y, rfl, SCong.refl x, SCong.refl y⟩
  | inp x y P => intro _; simp only [CanStep]; exact ⟨x, y, rfl, SCong.refl x, SCong.refl y⟩
  | tau _ => intro hμ; exact absurd hμ (by rintro ⟨x, y, h | h⟩ <;> cases h)
  | par _ ih => intro hμ; simp only [CanStep]; exact Or.inl (ih hμ)
  | nu _ _ ih => intro hμ; simp only [CanStep]; exact ih hμ
  | scong h1 _ _ ih => intro hμ; exact (canStep_of_scong h1 _).2 (ih hμ)

/-- **The labels out of a bare `out`.** `Step (out a b) μ P'` pins `μ` to `c!(d)` with `SCong a c` and
    `SCong b d` — the channel and the payload are determined up to the congruence, and nothing else is
    reachable. This is the corollary the obligations below consume; it is `canStep_of_step` composed
    with the definition, and the work is all in the two lemmas above.

    `IsAction μ` is carried rather than derived, and that is not avoidable here: a bare `out` cannot
    take a `τ` step — the synchronisation rule's source is a parallel composition — but *proving* that
    means inverting `Step`, which is the same wall `CanStep` was built to route around. Its callers
    name a concrete label, so they discharge it by `Or.inl rfl`. -/
@[axiom_budget 0]
theorem step_out_label {a b : Proc} {μ : Label} {P' : Proc} (h : Step (Proc.out a b) μ P')
    (hμ : IsAction μ) : ∃ c d : Proc, μ = Label.out c d ∧ SCong a c ∧ SCong b d :=
  canStep_of_step h hμ

/-! ==========================================================================
   Part 3c — Terms with no action atoms, and why they cannot step

   `CanStep` approximates *which* labels a term can take. Refuting a claimed weak-bisimulation law
   needed the opposite kind of fact — that a term can take *no* step at all, `τ` included — and that is
   not a question a label set can answer.

   This is the predicate that can, and its shape is what makes it cheap. It is *conjunctive*: a term is
   action-free when it has no `out` and no `inp` anywhere in it. `CanSync`-style predicates ("can this
   term synchronise?") need an existential over a pair of components, and then the `par_assoc` case of
   the invariance proof has to distribute existentials over disjunctions. Conjunction distributes over
   nothing, so every case below is a rearrangement or an immediate contradiction.

   The definition is also forced in two places, and both were found by trying the other thing first.
   A quote's interior is `ActionFree P`, because `cong_bang` relates `⌈P⌉` to `⌈Q⌉` for `P ≡ Q` and an
   action could hide inside either. A binder is *ignored*, because `nu_nil` relates `νa.0` to `0` for
   **every** `a` — so any predicate that inspected the binder would have to prove `ActionFree a` for an
   arbitrary term, which is false. That is the congruence telling the definition what it may look at.
   ========================================================================== -/

/-- `ActionFree P`: `P` contains no output and no input anywhere — nothing for a transition to be on.

    Not the negation of anything: it is the property that makes a term inert, and it is *not* preserved
    by everything one might expect (a `ν` and a `rep` are fine, their binders are ignored and their
    bodies are checked; see the Part 3c note for why the binder has to be ignored). -/
def ActionFree : Proc → Prop
  | .nil => True
  | .bang P => ActionFree P
  | .out _ _ => False
  | .inp _ _ _ => False
  | .nu _ P => ActionFree P
  | .rep P => ActionFree P
  | .par P Q => ActionFree P ∧ ActionFree Q

/-- **`SCong` cannot introduce or remove an action atom** — the invariance that makes `ActionFree` a
    fact about a term's *behaviour* rather than about its syntax. Sixteen cases, and every one is a
    rearrangement of conjunctions, an immediate contradiction (`out` and `inp` are `False` outright,
    so the cases that would need them are vacuous), or an induction hypothesis. -/
@[axiom_budget 0]
theorem actionFree_of_scong {P Q : Proc} (h : SCong P Q) : ActionFree P ↔ ActionFree Q := by
  induction h with
  | refl _ => exact Iff.rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih1 ih2 => exact ih1.trans ih2
  | par_comm _ _ => simp only [ActionFree]; exact and_comm
  | par_assoc _ _ _ => simp only [ActionFree]; exact and_assoc
  | par_nil _ => simp only [ActionFree]; exact ⟨fun h => h.1, fun h => ⟨h, trivial⟩⟩
  | nu_nil _ => simp only [ActionFree]
  | nu_nu _ _ _ => simp only [ActionFree]
  | rep_unfold _ => simp only [ActionFree]; exact ⟨fun h => ⟨h, h⟩, fun h => h.1⟩
  | cong_bang _ ih => simp only [ActionFree]; exact ih
  | cong_out _ _ _ _ => simp only [ActionFree]
  | cong_inp _ _ _ _ _ _ => simp only [ActionFree]
  | cong_nu _ _ _ ih2 => simp only [ActionFree]; exact ih2
  | cong_rep _ ih => simp only [ActionFree]; exact ih
  | cong_par _ _ ih1 ih2 => simp only [ActionFree]; exact and_congr ih1 ih2

/-- **A term with no action atoms cannot step at all** — not by an output or an input, and not by `τ`
    either, because the synchronisation rule's source is a parallel composition of an output and an
    input and so is not action-free. Every label, one statement.

    This is the negative fact the LTS was missing: it is what lets a claimed weak-bisimulation law be
    refuted by showing that one side can do nothing at all, `τ`-reachable or not. -/
@[axiom_budget 0]
theorem no_step_of_actionFree {P : Proc} {μ : Label} {P' : Proc} (h : ActionFree P) :
    ¬ Step P μ P' := by
  intro hs
  revert h
  induction hs with
  | out x y => intro h; exact h
  | inp x y P => intro h; exact h
  | tau _ => intro h; exact h.1
  | par _ ih => intro h; exact ih h.1
  | nu _ _ ih => intro h; exact ih h
  | scong h1 _ _ ih => intro h; exact ih ((actionFree_of_scong h1).1 h)

/-- **A term can only step on a channel its congruence class mentions.** If `P` can engage in `μ`,
    then some `Q ≡ P` mentions `μ`'s subject.

    Note the `∃ Q` and why it cannot be dropped: `canStep_occurs_up_to_scong` is *not* the statement
    `CanStep P μ → Occurs (subject μ) P`. That statement is false, and `out 0 b` is the witness — it
    takes the label `(νx.0)!b`, because `SCong (νx.0) 0` lets `cong_out` rewrite the channel into a
    term that mentions `x`. The congruence can introduce occurrences, which is the second cause
    recorded in this module's scope note and the reason obligation 2 below is refuted. -/
@[axiom_budget 0]
theorem canStep_occurs_up_to_scong {P : Proc} {μ : Label} (h : CanStep P μ) :
    ∃ Q : Proc, SCong P Q ∧ Occurs (Label.subject μ) Q := by
  revert h
  induction P with
  | nil => intro h; exact False.elim h
  | bang _ _ => intro h; exact False.elim h
  | out a b =>
    intro h
    obtain ⟨c, d, rfl, hac, hbd⟩ := h
    exact ⟨Proc.out c d, SCong.cong_out hac hbd, Or.inl rfl⟩
  | inp a b P =>
    intro h
    obtain ⟨c, d, rfl, hac, hbd⟩ := h
    exact ⟨Proc.inp c d P, SCong.cong_inp hac hbd (SCong.refl P), Or.inl rfl⟩
  | nu x A _ ih =>
    intro h
    obtain ⟨Q, hQ, hocc⟩ := ih h
    exact ⟨Proc.nu x Q, SCong.cong_nu (SCong.refl x) hQ, Or.inr hocc⟩
  | rep A ih =>
    intro h
    obtain ⟨Q, hQ, hocc⟩ := ih h
    exact ⟨Proc.rep Q, SCong.cong_rep hQ, hocc⟩
  | par A B ihA ihB =>
    intro h
    rcases h with h | h
    · obtain ⟨A', hA', hocc⟩ := ihA h
      exact ⟨Proc.par A' B, SCong.cong_par hA' (SCong.refl B), Or.inl hocc⟩
    · obtain ⟨B', hB', hocc⟩ := ihB h
      exact ⟨Proc.par A B', SCong.cong_par (SCong.refl A) hB', Or.inr hocc⟩

/-- **Substitution relabels.** `subst` moves an action from the channel it replaces to the one it
    replaces it with, so the *label* changes — and that is the obstacle to the α-rule on `SCong`, the
    one `Substitution.lean`'s note records as the unit that would remove `Step.tau`'s `CaptureFree`
    proviso.

    It is recorded as a theorem rather than discovered halfway through that unit, because it says what
    the unit costs. An α-rule would relate `νx.(out x b)` to `νy.(out y b)`, and `CanStep` is a predicate
    over *labels*: the first has the label `x!(b)` and the second `y!(b)`, and neither is among the
    other's, since the membership test compares channels with `SCong`. So adding α to `SCong` would make
    `CanStep` — and with it `canStep_occurs_up_to_scong`, `barb_nu_subject_occurs`, and the restriction
    obligation that rests on them — *not invariant*, unless "the same channel" is relaxed to an
    α-aware notion everywhere it appears. Measured and then written down, in the order this corpus asks
    for.

    The hypothesis is the one `Substitution.lean`'s note predicts every statement about `subst` needs:
    nothing here decides whether `out x b` *is* `x`. -/
@[axiom_budget 0]
theorem subst_moves_the_label {x y b : Proc} (hne : Proc.out x b ≠ x) (hsc : ¬ SCong x y) :
    CanStep (subst (Proc.out x b) x y) (.out y (subst b x y)) ∧
      ¬ CanStep (Proc.out x b) (.out y (subst b x y)) := by
  have hsub : subst (Proc.out x b) x y = Proc.out y (subst b x y) := by
    show (if Proc.out x b = x then y else Proc.out (subst x x y) (subst b x y)) = _
    rw [if_neg hne, subst_self]
  constructor
  · rw [hsub]
    exact ⟨y, subst b x y, rfl, SCong.refl y, SCong.refl (subst b x y)⟩
  · rintro ⟨c, d, he, hxc, _⟩
    injection he with h1 _
    exact hsc (h1 ▸ hxc)

/-! ==========================================================================
   Part 4 — Barbs

   §1.1: "In the ρ-calculus, process `P` exhibits barb `↓x` if `P` can engage in input or output on
   channel `x`." That is the definition below, and it is a definition rather than a tag: the barb is
   a consequence of the transition relation, so a process cannot *declare* a barb it does not have.
   ========================================================================== -/

/-- `Barb P x`: `P` exhibits the barb `↓x` — it can engage in input or output on channel `x`. -/
def Barb (P x : Proc) : Prop :=
  (∃ (y P' : Proc), Step P (.out x y) P') ∨ (∃ (y P' : Proc), Step P (.inp x y) P')

/-- **The barbs of a bare `out` are exactly the processes congruent to its channel.** `out x y`
    exhibits `↓w` iff `w ≡ x` — the payload does not appear in the barb, and the input disjunct is
    empty because `step_out_label` forbids an input label from an `out`.

    This is the lemma that makes obligation 1 provable and it is also why the old `Finset`-equality
    definition could not see anything: a barb set is a function of the channel *up to congruence*, so
    two processes with different payloads are indistinguishable to it. -/
@[axiom_budget 0]
theorem barb_out_iff {x y w : Proc} : Barb (Proc.out x y) w ↔ SCong x w := by
  constructor
  · rintro (⟨y', P', hs⟩ | ⟨y', P', hs⟩)
    · obtain ⟨c, d, he, hxc, _⟩ := step_out_label hs ⟨w, y', Or.inl rfl⟩
      injection he with hwc _
      rw [← hwc] at hxc
      exact hxc
    · obtain ⟨c, d, he, _, _⟩ := step_out_label hs ⟨w, y', Or.inr rfl⟩
      cases he
  · intro h
    exact Or.inl ⟨y, Proc.nil,
      Step.scong (SCong.cong_out h (SCong.refl y)) (Step.out w y) (SCong.refl _)⟩

/-! ==========================================================================
   Part 5 — Derived rules

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
   Part 6 — Strong bisimulation

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
    it does *not* say: the converse. Barb-equality is strictly weaker than bisimilarity, which is what
    `barb_eq_not_strongbisim` below establishes — and that is exactly why the old `Finset`-equality
    definition was not a bisimulation, and why `stronglyBisimilar`'s old theorems proved nothing about
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

/-- Two processes that disagree on a label cannot be congruent — the contrapositive of
    `canStep_of_scong`, and the only tool in this file for proving a *non*-congruence.

    That is what a witness against a bisimulation needs, for a reason worth stating: `SCong` is itself
    a bisimulation (`scong_isStrongBisim`), so congruence alone can never separate two processes. A
    separation has to come from a label one side has and the other does not. -/
@[axiom_budget 0]
theorem not_scong_of_canStep {P Q : Proc} {μ : Label}
    (hQ : CanStep Q μ) (hP : ¬ CanStep P μ) : ¬ SCong P Q :=
  fun h => hP ((canStep_of_scong h μ).2 hQ)

/-- `0` is not congruent to any output: `0` takes no transition at all, so it cannot exhibit the
    label `a!(b)` that `out a b` exhibits as its axiom.

    This is the non-congruence the witness for obligation 1 needs, and it deserves its own name
    because the deleted `Finset` layer could never have stated it: with barbs as equality of tag sets,
    `0` and `out a b` are compared by a set that has no transition in it either. -/
@[axiom_budget 0]
theorem not_scong_nil_out (a b : Proc) : ¬ SCong Proc.nil (Proc.out a b) :=
  not_scong_of_canStep (μ := Label.out a b) (Q := Proc.out a b) (P := Proc.nil)
    ⟨a, b, rfl, SCong.refl a, SCong.refl b⟩ (fun h => h)

/-- **Barb-equality does not imply bisimilarity** — obligation 1's core: `out x y` and `out x z` are
    related by no bisimulation as soon as `y` and `z` are *not congruent*.

    The proof is one step and one inversion, and the inversion is the point. `out x y` steps with the
    label `x!(y)`; a bisimulation must match that label from `out x z`, and `step_out_label` says the
    only labels `out x z` has are `c!(d)` with `c ≡ x`, `d ≡ z`. Matching `x!(y)` therefore forces
    `z ≡ y` — so if `z` is not congruent to `y` there is no match, and no bisimulation.

    Note what the hypothesis is *not*: `y ≠ z` would be the wrong hypothesis, and this module's
    earlier statement of the obligation said it. `Step.scong` composes with `Step.out`, so
    `out x (par y 0)` takes the label `x!(y)` as well — an unequal payload can still be a matched
    one. Only non-congruence separates. -/
@[axiom_budget 0]
theorem not_strongbisim_of_not_scong {x y z : Proc} (h : ¬ SCong y z) :
    ¬ StrongBisim (Proc.out x y) (Proc.out x z) := by
  rintro ⟨R, hR, hxy⟩
  obtain ⟨Q', hs, _⟩ := (hR hxy).1 (Step.out x y)
  obtain ⟨c, d, he, _, hzd⟩ := step_out_label hs ⟨x, y, Or.inl rfl⟩
  injection he with _ hyd
  rw [← hyd] at hzd
  exact h (SCong.symm hzd)

/-- **Obligation 1, discharged: barb-equality is strictly weaker than bisimilarity.**

    The witness is `out ⌈0⌉ 0` against `out ⌈0⌉ (out 0 0)`. By `barb_out_iff` their barbs are both
    `SCong ⌈0⌉ w`, so **every** barb matches; and by `not_strongbisim_of_not_scong` with
    `not_scong_nil_out`, no bisimulation relates them. The deleted `stronglyBisimilar` would have
    called this pair equivalent, and a relation that equates two processes no bisimulation relates is
    not a bisimulation — which is the claim the rewrite was making and this is the proof of it.

    Stated with the pair existentially rather than as a function of `y z`: the content is that such a
    pair exists, and the pair above is the smallest one this file can exhibit. -/
@[axiom_budget 0]
theorem barb_eq_not_strongbisim :
    ∃ P Q : Proc, (∀ w : Proc, Barb P w ↔ Barb Q w) ∧ ¬ StrongBisim P Q :=
  ⟨Proc.out (Proc.bang Proc.nil) Proc.nil,
   Proc.out (Proc.bang Proc.nil) (Proc.out Proc.nil Proc.nil),
   fun w =>
     (barb_out_iff (x := Proc.bang Proc.nil) (y := Proc.nil) (w := w)).trans
       (barb_out_iff (x := Proc.bang Proc.nil) (y := Proc.out Proc.nil Proc.nil) (w := w)).symm,
   not_strongbisim_of_not_scong (x := Proc.bang Proc.nil) (not_scong_nil_out Proc.nil Proc.nil)⟩

/-! ==========================================================================
   Part 7 — §1.2's laws, as theorems about processes

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

/-! ==========================================================================
   Part 8 — Obligation 2: what a restriction can and cannot hide

   The obligation is `¬ Barb (νx.P) x`: a restriction never lets its own name out. It is **open**, and
   what it is waiting on is not what this module's earlier record said it was. This part holds the three
   things that are settled, in the order they were measured.

   **The condition is sufficient, and that half is a theorem.** `barb_nu_of_not_scong`: a restriction is
   *transparent* to a channel it does not bind. That is the positive content of the rule's proviso
   `¬ SCong x (subject μ)` — everything the rule should let through, it lets through.

   **Its necessity is the obligation**, and it is the second half of the same sentence: nothing gets
   through on a channel the restriction does bind. Stated without a freshness hypothesis, because the
   hypothesis that used to carry it was unsatisfiable (`not_freshUpToScong`) — so the obligation is the
   unconditional `¬ Barb (νx.P) x`, and the rule's proviso is exactly as strong as the obligation says.

   **What would close it is not a shape lemma.** The record this part replaces named the missing step as
   "a congruence-class shape lemma: that a `ν`-headed term's congruence class is `ν`-headed, with a
   binder `SCong`-equal to the original". Both readings of that are settled, and neither is a step:

   * As a `SCong` conclusion it is **free**. Given `SCong (νx.P) Q` and `SCong P (νx.R)`, symmetry and
     transitivity already give `Q ≡ νx.R`, so the statement holds with `y := x`, `Q' := R`. It is a
     restatement of the hypothesis and says nothing about the class.
   * As an *equality* on `Q`'s head — which is what the `scong` case needs, because the step rules have
     to be applied to a term whose head is syntactically visible — it is **false**, and
     `scong_nu_shape_false` below is the witness: `nu_nu` permutes two binders, so the head binder of a
     congruent `ν`-headed term need not be congruent to the original, and `nu_nil` removes a binder
     outright.

   What the `scong` case needs is therefore a fact about the *chain* of restrictions rather than about
   its head: the set of binders along it, modulo permutation (`nu_nu`), congruence (`cong_nu`) and the
   collapse (`nu_nil`). No predicate in this tree expresses that, and it is not an oversight that
   `CanStep` cannot: its `nu` clause drops the binder *by construction*, because Part 3c's note records
   that `nu_nil` makes any binder-inspecting predicate non-invariant. This is the fourth wall of that
   shape in this file, and the one predicate that would get past it has not been written.

   `barb_nu_subject_occurs` is kept, and its hypothesis is the open question — which is worth knowing
   when reading it: what it says about the obligation is nothing, because the mention of `x` it produces
   may be a *bound* one.

   Read with §0's notation: `0` is `Proc.nil`, `⌈P⌉` is `Proc.bang P`, `νx.P` is `Proc.nu x P`,
   `x!(y)` is `Proc.out x y`.
   ========================================================================== -/

/-- **Obligation 2's engine.** A barb on `x` exhibited by `νx.P` means some process congruent to `P`
    mentions `x`.

    The barb's label is `x!(y)` or `x?(y)`, so `x` is a channel `νx.P` engages in; `canStep_of_step`
    puts that label in `CanStep (νx.P)`, whose `nu` clause is the body's, and
    `canStep_occurs_up_to_scong` then produces a congruent process mentioning `x`. Nothing about the
    restriction itself is used, and that is the finding: the *congruence*, not the restriction, is what
    can put a mention of `x` behind `νx`. It is also why the rule's proviso had to change — this lemma
    says exactly what "fresh" has to exclude, and the syntactic notion excluded too little.

    Read against Part 8's account of the obligation: its hypothesis *is* the open question, so it is not
    a step towards the answer, and its conclusion is the wrong kind of mention — `Occurs` counts the
    binder, and the binder is precisely what `νx.P` may legitimately contain. -/
@[axiom_budget 0]
theorem barb_nu_subject_occurs {x P : Proc} (h : Barb (Proc.nu x P) x) :
    ∃ Q : Proc, SCong P Q ∧ Occurs x Q := by
  rcases h with ⟨y, P', hs⟩ | ⟨y, P', hs⟩
  · have hc : CanStep P (Label.out x y) := by
      simpa only [CanStep] using canStep_of_step hs ⟨x, y, Or.inl rfl⟩
    exact canStep_occurs_up_to_scong hc
  · have hc : CanStep P (Label.inp x y) := by
      simpa only [CanStep] using canStep_of_step hs ⟨x, y, Or.inr rfl⟩
    exact canStep_occurs_up_to_scong hc

/- **Obligation 2 was stated here and is now deleted.** `no_barb_nu_of_fresh` carried
   `FreshUpToScong x P` and concluded `¬ Barb (νx.P) x`. `Congruence.lean`'s `not_freshUpToScong`
   proves that hypothesis is false for every `x` and every `P`, so the theorem was true of nothing —
   and it is removed rather than restated because **no `Occurs`-based freshness can replace it**:
   `nu_nil` with `cong_par` and `par_nil` makes `SCong P (P | νx.0)` hold for every `P`, so any
   invariant *occurrence* condition is unsatisfiable, over `SCong0` or over `SCong` alike. A
   replacement needs a notion which does not count binders, and `barb_nu_subject_occurs` — which
   survives, and is above — yields `Occurs`, not a free occurrence.

   What the obligation is waiting on has since been measured twice more, and the third answer was wrong.
   The third said `Step.scong` needs "a congruence-class shape lemma — that a `ν`-headed term relates
   only to `ν`-headed terms, with a `SCong`-equal binder"; Part 8's note and `scong_nu_shape_false`
   below record why that is not it. What is left is the chain of restrictions rather than its head. -/

/-- **A restriction is transparent to a channel it does not bind** — the proved half of obligation 2,
    and the content of the rule's proviso from the direction that can be established.

    `barb_nu_of_not_scong` is `Step.nu` read as a statement about *observations*: `νx.P` barbs on `a`
    whenever `P` does and `a` is not congruent to `x`. Read beside the obligation it is the
    *sufficiency* half — the rule blocks nothing it should not — and the obligation is its necessity.

    The hypothesis is not decoration: at `a ≡ x` the conclusion fails, and *that* is the obligation.
    Stated as an implication rather than as the `↔` it wants to be, because only one direction is
    available. -/
@[axiom_budget 0]
theorem barb_nu_of_not_scong {x P a : Proc} (h : ¬ SCong x a) (hb : Barb P a) :
    Barb (Proc.nu x P) a := by
  rcases hb with ⟨y, P', hs⟩ | ⟨y, P', hs⟩
  · exact Or.inl ⟨y, Proc.nu x P', Step.nu (by simpa only [Label.subject] using h) hs⟩
  · exact Or.inr ⟨y, Proc.nu x P', Step.nu (by simpa only [Label.subject] using h) hs⟩

/-- **The shape lemma this module's record named as the obligation's missing step is false in the form
    the obligation needs.**

    The `Step.scong` case has a step out of a term `Q` that is only *congruent* to `νx.P`, and the step
    rules need `Q`'s head syntactically — so what it asks for is an **equality**: `Q = νy.Q'` with a
    binder congruent to the original. That is what is refuted here.

    The witness is `nu_nu`: `ν0.ν(0!(0)).0 ≡ ν(0!(0)).ν0.0`, and the head binder moves from `0` to
    `0!(0)`, which is not congruent to `0` (`not_scong_nil_out`). `nu_nil` is the second mechanism and
    is not needed for this witness: it removes a binder outright, `νx.0 ≡ 0`.

    Recorded as a refutation of the universally quantified statement rather than as prose about the
    witness, because the universally quantified statement is the one that was written down as the plan
    of record — and the reason it was written down is that the congruence form *is* available
    trivially, by symmetry, and so reads like a fact about the class without being one. -/
@[axiom_budget 0]
theorem scong_nu_shape_false :
    ¬ (∀ (x P Q : Proc), SCong (Proc.nu x P) Q →
        ∃ y Q', SCong x y ∧ SCong P Q' ∧ Q = Proc.nu y Q') := by
  intro h
  obtain ⟨y, _, hay, _, heq⟩ :=
    h Proc.nil (Proc.nu (Proc.out Proc.nil Proc.nil) Proc.nil)
      (Proc.nu (Proc.out Proc.nil Proc.nil) (Proc.nu Proc.nil Proc.nil))
      (SCong.nu_nu Proc.nil (Proc.out Proc.nil Proc.nil) Proc.nil)
  injection heq with hy _
  rw [← hy] at hay
  exact not_scong_nil_out Proc.nil Proc.nil hay

/-! ==========================================================================
   Part 9 — §1.2's weak equation: what it says, what is true, and what is not

   §1.2's text, verbatim: "**Weak bisimulation** (`P ≈ Q`): internal synchronization actions
   (τ-transitions) are unobservable. Two process nets that differ only in internal task scheduling are
   weak-bisimilar. `P | (a?(x).Q) | a!(v).R ≈ P | Q{v/x} | R` — internal communication on channel `a`
   is transparent to observers."

   The equation is not a law of this calculus, and the reason is worth being exact about rather than
   filing as an omission. Its left side **retains** both prefixes: `a?(x).Q` can still take a free
   input step and `a!(v).R` a free output one, so the left exhibits the barb `↓a` — and at `Q = R = 0`
   the right exhibits none. A bisimulation requires each side to match the other's steps, so no
   relation of either strength relates them.

   What *is* true, and what §1.2's prose is reaching for, is the reduction. `Step.tau` gives
   `a?(x).Q | a!(v).R -[τ]-> Q{v/x}`: one `τ`-step from the composed prefixes to the substituted body.
   That is "internal communication on channel `a`" as an *event*, and it is a different claim from
   equating the two processes — the prefixes are still there to be observed afterwards. The standard
   π-calculus does not equate them either; the equation reads as a law because the `τ`-step is silent,
   and silence is not the same as absence.

   Both readings are mechanized below, and the weak one is why Part 3c exists. Its witness is the same;
   its reason is different — the right side cannot match a step it cannot *take*, `τ`-reachable or not —
   and that needed a negative fact about a term's transitions, which `CanStep` cannot supply because it
   is a set of *actions* by design.

   The route that works is a predicate whose invariance is cheap. `ActionFree` is conjunctive — "no
   `out` and no `inp` anywhere" — so its sixteen cases are rearrangements of conjunctions rather than
   existentials distributed over disjunctions. The first attempt was `CanSync`, "can this term
   synchronise?", which is the natural formulation and the wrong one: its `par_assoc` case has to relate
   the synchronisation clauses of `par (par P Q) R` and `par P (par Q R)`, and that is where the cost
   sits. What the two share is the wall this file has now hit three times: `SCong` relates terms of
   different syntactic *shape*, so a fact about a term's transitions has to be proved *invariant* rather
   than computed from the term. `FreshUpToScong` answered it for the restriction rule by quantifying
   over `SCong0`; `CanStep` answered it for actions by recursion plus a sixteen-case invariance;
   `ActionFree` answers it for inertness by staying conjunctive.
   ========================================================================== -/

/-- **§1.2's weak-bisimulation equation is false in its strong reading.** At `P = Q = R = 0`, `a = 0`,
    `v = 0` and `x = ⌈0⌉`, the left side exhibits the barb `↓0` from its retained output prefix, and the
    right side — which `subst`'s `nil` clause makes `0 | 0 | 0` — exhibits none. By `strongbisim_barb_eq`
    no strong bisimulation relates them.

    Strongly non-bisimilar is the weaker of the two conclusions, and it is the one this tree can settle
    today: the weak reading's refutation needs the `τ`-step-freedom Part 9's note describes. -/
@[axiom_budget 0]
theorem section_1_2_equation_not_strong :
    ¬ StrongBisim
      (Proc.par (Proc.par Proc.nil (Proc.inp Proc.nil (Proc.bang Proc.nil) Proc.nil))
        (Proc.out Proc.nil Proc.nil))
      (Proc.par (Proc.par Proc.nil (subst Proc.nil (Proc.bang Proc.nil) Proc.nil)) Proc.nil) := by
  intro h
  have hsub : subst Proc.nil (Proc.bang Proc.nil) Proc.nil = Proc.nil :=
    subst_nil_of_ne (fun hc => by cases hc)
  have hright : ¬ Barb (Proc.par (Proc.par Proc.nil (subst Proc.nil (Proc.bang Proc.nil) Proc.nil))
      Proc.nil) Proc.nil := by
    rw [hsub]
    rintro (⟨y, P', hs⟩ | ⟨y, P', hs⟩)
    · exact absurd (canStep_of_step hs ⟨Proc.nil, y, Or.inl rfl⟩)
        (by simp only [CanStep, false_or, or_false]; exact id)
    · exact absurd (canStep_of_step hs ⟨Proc.nil, y, Or.inr rfl⟩)
        (by simp only [CanStep, false_or, or_false]; exact id)
  exact hright ((strongbisim_barb_eq h Proc.nil).1
    (Or.inl ⟨Proc.nil, _, step_par_right (Step.out Proc.nil Proc.nil)⟩))

/-! ==========================================================================
   Part 9b — Weak bisimulation, and the equation's weak reading
   ========================================================================== -/

/-- `StepTauStar P Q`: `Q` is reached from `P` by any number of `τ`-steps, zero included. -/
inductive StepTauStar : Proc → Proc → Prop where
  /-- No steps at all. -/
  | refl (P : Proc) : StepTauStar P P
  /-- One `τ`-step, then some more. -/
  | step {P Q R : Proc} : Step P Label.tau Q → StepTauStar Q R → StepTauStar P R

/-- `StepWeak P μ Q`: a run of `τ`s, then one `μ`-step, then another run. The standard weak
    transition, and the reason `τ` is called unobservable: it can happen anywhere around an
    observable step without being part of it. -/
inductive StepWeak : Proc → Label → Proc → Prop where
  /-- `P -[τ*]-> P' -[μ]-> Q -[τ*]-> Q'`. -/
  | intro {P P' Q Q' : Proc} {μ : Label} :
      StepTauStar P P' → Step P' μ Q → StepTauStar Q Q' → StepWeak P μ Q'

/-- `IsWeakBisim R`: every step of either side is matched by the other, up to `τ*` on both sides. -/
def IsWeakBisim (R : Proc → Proc → Prop) : Prop :=
  ∀ {P Q : Proc}, R P Q →
    (∀ {μ : Label} {P' : Proc}, Step P μ P' → ∃ Q' : Proc, StepWeak Q μ Q' ∧ R P' Q') ∧
    (∀ {μ : Label} {Q' : Proc}, Step Q μ Q' → ∃ P' : Proc, StepWeak P μ P' ∧ R P' Q')

/-- `P ≈ Q`: weak bisimilarity — some weak bisimulation relates them. Existential, like `StrongBisim`
    and for the same reason: the union of weak bisimulations is one, so this *is* the greatest fixed
    point, and composing two witnesses needs a relation to hand over. -/
def WeakBisim (P Q : Proc) : Prop := ∃ R : Proc → Proc → Prop, IsWeakBisim R ∧ R P Q

/-! ### The weak relation's laws, and the sentence they are the mechanization of

   §1.2 says "two process nets that differ only in internal task scheduling are weak-bisimilar". What
   can be said about *processes* here is the congruence reading: rearranging a term — `par_comm`,
   `par_assoc` — does not change what an observer sees, so congruent terms are weakly bisimilar. That is
   `scong_isWeakBisim`, and the three par laws below are §1.2's sentence in the form the calculus can
   state.

   What is *not* mechanized is scheduling in the net sense — the interleaving of `τ`s *between*
   independent subprocesses — because nothing here models a scheduler. `WeakBisim` is the relation that
   would make it provable, which is why the sentence is a motivation for the definition rather than a
   consequence of it. Recorded rather than left as an impression. -/

/-- **Every strong bisimulation is a weak one**: matching each step with a step of the same label is a
    special case of matching it up to `τ*` on both sides. The three `τ*`-runs are all reflexive. -/
@[axiom_budget 0]
theorem strong_is_weak {P Q : Proc} (h : StrongBisim P Q) : WeakBisim P Q := by
  obtain ⟨R, hR, hPQ⟩ := h
  refine ⟨R, ?_, hPQ⟩
  intro A B hAB
  constructor
  · rintro μ A' hs
    obtain ⟨B', hsB, hrel⟩ := (hR hAB).1 hs
    exact ⟨B', StepWeak.intro (StepTauStar.refl B) hsB (StepTauStar.refl B'), hrel⟩
  · rintro μ B' hs
    obtain ⟨A', hsA, hrel⟩ := (hR hAB).2 hs
    exact ⟨A', StepWeak.intro (StepTauStar.refl A) hsA (StepTauStar.refl A'), hrel⟩

/-- The weak version of `scong_isStrongBisim`, and the reason the par laws below can be stated for
    `≈` at all: the congruence is a weak bisimulation because it is a strong one. -/
@[axiom_budget 0]
theorem scong_isWeakBisim : IsWeakBisim SCong := by
  intro A B hAB
  constructor
  · rintro μ A' hs
    exact ⟨_, StepWeak.intro (StepTauStar.refl B)
      (Step.scong (SCong.symm hAB) hs (SCong.refl _)) (StepTauStar.refl _), SCong.refl _⟩
  · rintro μ B' hs
    exact ⟨_, StepWeak.intro (StepTauStar.refl A)
      (Step.scong hAB hs (SCong.refl _)) (StepTauStar.refl _), SCong.refl _⟩

/-- Weak bisimilarity is reflexive: equality is a weak bisimulation. -/
@[axiom_budget 0]
theorem weakbisim_refl (P : Proc) : WeakBisim P P :=
  ⟨(· = ·), by
    intro A B hAB
    subst hAB
    exact ⟨fun hs => ⟨_, StepWeak.intro (StepTauStar.refl _) hs (StepTauStar.refl _), rfl⟩,
           fun hs => ⟨_, StepWeak.intro (StepTauStar.refl _) hs (StepTauStar.refl _), rfl⟩⟩,
   rfl⟩

/-- Weak bisimilarity is symmetric: the converse of a weak bisimulation is one. -/
@[axiom_budget 0]
theorem weakbisim_symm {P Q : Proc} (h : WeakBisim P Q) : WeakBisim Q P := by
  obtain ⟨R, hR, hPQ⟩ := h
  exact ⟨fun A B => R B A, fun {A B} hBA => ⟨(hR hBA).2, (hR hBA).1⟩, hPQ⟩

/-- `P | Q ≈ Q | P` — §1.2's commutativity, for the weak relation. The same witness as the strong law,
    because `SCong` is both. -/
@[axiom_budget 0]
theorem weak_parallel_commutative (P Q : Proc) : WeakBisim (Proc.par P Q) (Proc.par Q P) :=
  ⟨SCong, scong_isWeakBisim, SCong.par_comm P Q⟩

/-- `(P | Q) | R ≈ P | (Q | R)` — §1.2's associativity, for the weak relation. -/
@[axiom_budget 0]
theorem weak_parallel_associative (P Q R : Proc) :
    WeakBisim (Proc.par (Proc.par P Q) R) (Proc.par P (Proc.par Q R)) :=
  ⟨SCong, scong_isWeakBisim, SCong.par_assoc P Q R⟩

/-- `P | 0 ≈ P` — the identity law, weakly. -/
@[axiom_budget 0]
theorem weak_parallel_nil (P : Proc) : WeakBisim (Proc.par P Proc.nil) P :=
  ⟨SCong, scong_isWeakBisim, SCong.par_nil P⟩

/-! #### Closure of the weak transitions, and transitivity

   Transitivity is where the `τ*`-runs of two witnesses have to be woven, and the weaving is what these
   three lemmas exist for. They are the standard ones: `τ*` is transitive, a *weak* `τ`-transition is
   just a `τ*`-run (`StepWeak`'s three parts with `τ` in the middle are all `τ`), and a bisimulation's
   relation survives an entire `τ*`-run rather than only a single step. -/

/-- `StepWeak`'s witnesses, named. The constructor's intermediate processes are implicit parameters, so
    a consumer that needs to *mention* them — as transitivity does, to relate them by the second
    relation — gets them here rather than by unfolding the definition at the use site. -/
@[axiom_budget 0]
theorem stepWeak_witnesses {P Q : Proc} {μ : Label} (hw : StepWeak P μ Q) :
    ∃ P' Q' : Proc, StepTauStar P P' ∧ Step P' μ Q' ∧ StepTauStar Q' Q := by
  cases hw with
  | intro h1 h2 h3 => exact ⟨_, _, h1, h2, h3⟩

/-- `τ*` is transitive. -/
@[axiom_budget 0]
theorem stepTauStar_trans {P Q R : Proc} (h1 : StepTauStar P Q) :
    StepTauStar Q R → StepTauStar P R := by
  induction h1 with
  | refl _ => intro h2; exact h2
  | step hτ _ ih => intro h2; exact StepTauStar.step hτ (ih h2)

/-- A weak `τ`-transition is a `τ*`-run: with `τ` in the middle, all three parts are `τ*`. -/
@[axiom_budget 0]
theorem stepTauStar_of_stepWeak_tau {P Q : Proc} (hw : StepWeak P Label.tau Q) :
    StepTauStar P Q := by
  obtain ⟨_, _, h1, h2, h3⟩ := stepWeak_witnesses hw
  exact stepTauStar_trans h1 (stepTauStar_trans (StepTauStar.step h2 (StepTauStar.refl _)) h3)

/-- **A weak bisimulation's relation survives a whole `τ*`-run**, not only a single step. The induction
    is on the run, and the step that is *not* `τ` is a weak `τ`-transition, which is a `τ*`-run. -/
@[axiom_budget 0]
theorem stepTauStar_of_weak {S : Proc → Proc → Prop} (hS : IsWeakBisim S) {P P' : Proc}
    (hs : StepTauStar P P') : ∀ {Q : Proc}, S P Q → ∃ Q' : Proc, StepTauStar Q Q' ∧ S P' Q' := by
  induction hs with
  | refl _ => intro Q hPQ; exact ⟨Q, StepTauStar.refl Q, hPQ⟩
  | step hτ _ ih =>
      intro Q hPQ
      obtain ⟨Q₁, hw, hrel⟩ := (hS hPQ).1 hτ
      obtain ⟨Q₂, hQ₂, hrel₂⟩ := ih hrel
      exact ⟨Q₂, stepTauStar_trans (stepTauStar_of_stepWeak_tau hw) hQ₂, hrel₂⟩

/-- The mirror of `stepTauStar_of_weak`: a run on the relation's *second* component lifts too, with the
    orientation kept. Transitivity needs both, because the two witnesses' runs are on opposite sides.

    Derived from the primary by flipping the relation rather than by repeating the induction, which is
    why it is four lines: `S`'s second component is the flip's first, and the flip's conclusion is
    `S`-oriented again once it is read back. -/
@[axiom_budget 0]
theorem stepTauStar_of_weak_right {S : Proc → Proc → Prop} (hS : IsWeakBisim S) {Q Q' : Proc}
    (hs : StepTauStar Q Q') : ∀ {P : Proc}, S P Q → ∃ P' : Proc, StepTauStar P P' ∧ S P' Q' := by
  have hSc : IsWeakBisim (fun A B => S B A) :=
    fun {A B} hBA => ⟨(hS hBA).2, (hS hBA).1⟩
  intro P hPQ
  exact stepTauStar_of_weak hSc hs hPQ

/-- **Weak bisimilarity is transitive**: the composite relation `∃ B, R₁ A B ∧ R₂ B C` is a weak
    bisimulation.

    The weaving is the content. `R₁` matches a step of `A` with a step of `B` surrounded by two `τ*`-runs
    on `B`'s side; each of those runs has to be carried into `C` by `R₂` before the middle step can be
    matched there, which is what `stepTauStar_of_weak` is for, and the resulting `τ*`-runs have to be
    composed, which is what `stepTauStar_trans` is for. The strong version needs none of this — its two
    matches compose directly — which is the whole difference between the two proofs: nothing here is
    `τ`-blind, so every run has to be carried across before the next match can be made. -/
@[axiom_budget 0]
theorem weakbisim_trans {P Q R : Proc} (h1 : WeakBisim P Q) (h2 : WeakBisim Q R) : WeakBisim P R := by
  obtain ⟨R₁, hR₁, hPQ⟩ := h1
  obtain ⟨R₂, hR₂, hQR⟩ := h2
  refine ⟨fun A C => ∃ B : Proc, R₁ A B ∧ R₂ B C, ?_, ⟨Q, hPQ, hQR⟩⟩
  intro A C hAC
  obtain ⟨B, hAB, hBC⟩ := hAC
  constructor
  · rintro μ A' hs
    obtain ⟨B₁, hw, hA'B₁⟩ := (hR₁ hAB).1 hs
    obtain ⟨_, _, hτ₁, hsB, hτ₂⟩ := stepWeak_witnesses hw
    obtain ⟨C₁, hC₁, hBC₁⟩ := stepTauStar_of_weak hR₂ hτ₁ hBC
    obtain ⟨C₂, hw₂, hB₁C₂⟩ := (hR₂ hBC₁).1 hsB
    obtain ⟨_, _, hτ₃, hsC, hτ₄⟩ := stepWeak_witnesses hw₂
    obtain ⟨C₄, hC₄, hB₁'C₄⟩ := stepTauStar_of_weak hR₂ hτ₂ hB₁C₂
    exact ⟨C₄, StepWeak.intro (stepTauStar_trans hC₁ hτ₃) hsC (stepTauStar_trans hτ₄ hC₄),
      ⟨B₁, hA'B₁, hB₁'C₄⟩⟩
  · rintro μ C' hs
    obtain ⟨B₁, hw, hBC'⟩ := (hR₂ hBC).2 hs
    obtain ⟨_, _, hτ₁, hsB, hτ₂⟩ := stepWeak_witnesses hw
    obtain ⟨A₁, hA₁, hA₁B₂⟩ := stepTauStar_of_weak_right hR₁ hτ₁ hAB
    obtain ⟨A₂, hw₂, hA₂B₃⟩ := (hR₁ hA₁B₂).2 hsB
    obtain ⟨_, _, hτ₃, hsA, hτ₄⟩ := stepWeak_witnesses hw₂
    obtain ⟨A₄, hA₄, hA₄B₁⟩ := stepTauStar_of_weak_right hR₁ hτ₂ hA₂B₃
    exact ⟨A₄, StepWeak.intro (stepTauStar_trans hA₁ hτ₃) hsA (stepTauStar_trans hτ₄ hA₄),
      ⟨B₁, hA₄B₁, hBC'⟩⟩

/-! ### §1.2's third relation, `≅`

   §1.2 defines barbed bisimulation as "two concurrent processes are equivalent if their observable
   concurrent barbs match, even if their internal scheduling order differs", and states two laws for it:
   commutativity and associativity of parallel composition.

   That reading is mechanizable exactly as written — `BarbedEq` is barbs matching — and the two laws
   follow from `barb_of_scong`. Two things are worth recording about it. It is **strictly weaker than
   bisimilarity**: `barb_eq_not_strongbisim` exhibits a pair with equal barbs that no bisimulation
   relates, so `≅` read this way cannot be the same relation as `≈` (Part 5 and Part 9b). And the
   *standard* barbed equivalence — the one that is a congruence for all contexts — closes the relation
   under every context, which "the barbs match" alone does not; nothing here builds a context closure,
   so this is the reading the sentence supports and not the standard notion's full strength. -/

/-- `BarbedEq P Q`: §1.2's `≅`, read as "their observable barbs match" — the definition the sentence
    gives, with no context closure added. -/
def BarbedEq (P Q : Proc) : Prop := ∀ x : Proc, Barb P x ↔ Barb Q x

/-- `P | Q ≅ Q | P` — §1.2's commutativity law for the barbed relation. -/
@[axiom_budget 0]
theorem barbed_parallel_commutative (P Q : Proc) : BarbedEq (Proc.par P Q) (Proc.par Q P) :=
  fun x => barb_of_scong (SCong.par_comm P Q) x

/-- `(P | Q) | R ≅ P | (Q | R)` — §1.2's associativity law for the barbed relation. -/
@[axiom_budget 0]
theorem barbed_parallel_associative (P Q R : Proc) :
    BarbedEq (Proc.par (Proc.par P Q) R) (Proc.par P (Proc.par Q R)) :=
  fun x => barb_of_scong (SCong.par_assoc P Q R) x

/-- An action-free term's `τ*`-closure is itself, because it has no `τ`-steps to take. -/
@[axiom_budget 0]
theorem actionFree_of_tauStar {P Q : Proc} (hs : StepTauStar P Q) : ActionFree P → ActionFree Q := by
  induction hs with
  | refl _ => intro h; exact h
  | step hτ _ ih => intro h; exact ih (False.elim (no_step_of_actionFree h hτ))

/-- **§1.2's weak-bisimulation equation is false in its weak reading too** — the same witness as the
    strong one, at `P = Q = R = 0`, `a = v = 0`, `x = ⌈0⌉`.

    The left side steps with the label `0!(0)` from its retained output prefix. A weak bisimulation
    would have to match that label from *some* `τ`-reachable state of the right side, and the right side
    is `0 | 0 | 0`: `ActionFree` holds of it, so `actionFree_of_tauStar` says every state it can reach
    is action-free too, and `no_step_of_actionFree` says none of them steps at all. The prefix is not
    merely unmatched after the `τ` — there is no `τ` to unmatch it with.

    This is the fact the LTS could not state until Part 3c: `CanStep` is a set of actions, so "this term
    can do nothing" needed a predicate of its own, and one whose invariance is provable. -/
@[axiom_budget 0]
theorem section_1_2_equation_not_weak :
    ¬ WeakBisim
      (Proc.par (Proc.par Proc.nil (Proc.inp Proc.nil (Proc.bang Proc.nil) Proc.nil))
        (Proc.out Proc.nil Proc.nil))
      (Proc.par (Proc.par Proc.nil (subst Proc.nil (Proc.bang Proc.nil) Proc.nil)) Proc.nil) := by
  have hsub : subst Proc.nil (Proc.bang Proc.nil) Proc.nil = Proc.nil :=
    subst_nil_of_ne (fun hc => by cases hc)
  have hM : ActionFree (Proc.par (Proc.par Proc.nil (subst Proc.nil (Proc.bang Proc.nil) Proc.nil))
      Proc.nil) := by
    rw [hsub]
    exact ⟨⟨trivial, trivial⟩, trivial⟩
  rintro ⟨R, hR, hLM⟩
  obtain ⟨_Q', hw, _⟩ := (hR hLM).1 (step_par_right (Step.out Proc.nil Proc.nil))
  obtain ⟨hstar, hs, _⟩ := hw
  exact no_step_of_actionFree (actionFree_of_tauStar hstar hM) hs

end DarkFi.Semantics
