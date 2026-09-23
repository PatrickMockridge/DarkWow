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
than hidden by a silent α-renaming.

Two boundaries remain, and they are separate:

* **Every statement about `CanStep` is about free actions.** `CanStep` is the static label set the barb
  obligations are proved through, and no clause of it produces `τ` — a synchronisation is a property of
  a *pair*, which a per-term label set cannot see. So its three lemmas carry `IsAction μ`, and `τ` never
  enters the term-level recursion. That is deliberate rather than a shortcut: a synchronisation clause
  inside the `par` case would have to relate the clauses of two different associations in
  `par_assoc`, which is where the invariance proof would stop being structural.
* **Strong bisimulation is still the only bisimulation.** §1.2's weak equation
  `P | a?(x).Q | a!(v).R ≈ P | Q{v/x} | R` needs `τ`-transitions *and* the weak relation, and neither
  the relation nor its laws are here. The barb results are unaffected by `τ`: a barb is an action, and
  `Barb` never saw `τ`.

## What the rules are, and which are derived

`out`, `inp`, `par`, `nu` and `scong` are the constructors. The right-handed parallel rule
`Q | P -[μ]-> Q | P'` is deliberately **not** a constructor: it is derived from `par_comm` and
`scong`, and deriving it is the evidence that the congruence is doing its job.

## The two obligations, both settled — and the second cost the calculus a proviso

An earlier revision of this module named two theorems it did not have. Both are now proved, and the
second needed the restriction rule to change rather than a better proof.

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

2. **No barb survives a fresh restriction** — proved, as `no_barb_nu_of_fresh`, and it is now a
   theorem about a rule this module had to fix first.

   Under `Proc.lean`'s *syntactic* freshness the sentence is **false**, in two independent ways, and
   the witnesses are recorded here rather than as theorems — which is the honest form, for a precise
   reason. Their proofs were derivations of the old rule, so with the rule restated they no longer
   derive; and their *statements* are not settled either way by the restatement, because the
   obligation's hypothesis is strictly stronger than the syntactic freshness they assumed. A statement
   that is neither proved nor refuted is exactly what this corpus does not put a name on, so what
   survives is the record, and the record is the reason the proviso is what it is:

   * at `x = 0` and `P = out (ν0.0) ⌈0⌉`, `Fresh 0 P` held and `Barb (ν0.P) 0` held with it, because
     `Occurs 0 0` is `False` — the proviso `Fresh x (subject μ)` was discharged *vacuously* when the
     action's channel was the restricted name, and `Label.subject .tau = 0` is what makes `0` a name
     as well as the τ-subject, so that instance bites there;
   * at `x = ⌈0⌉`, where `Fresh x x` fails and the first cause cannot apply, the barb escaped through
     `nu_par` and `cong_bang` instead: the channel `⌈ν0.0⌉` can be *read* as `⌈0⌉`, a name that does
     not occur in the term as written.

   The second is the general one, and note that it is **not** about binding: `⌈ν0.0⌉` is a channel, a
   free-name position already, and it is `cong_bang` that identifies it with `⌈0⌉`. So the freshness
   the rule needs must quantify over the congruence rather than over the syntax — that is
   `Congruence.lean`'s `FreshUpToScong` — and the restriction rule and `SCong.nu_par` now carry it,
   with `fresh_of_freshUpToScong` recording that it is strictly stronger than the syntactic notion it
   replaces. `barb_nu_subject_occurs` is then the lemma that discharges the obligation in one step: a
   barb on `x` out of `νx.P` produces a term congruent to `P` that mentions `x`, which is exactly what
   the proviso forbids.

   `Proc.lean`'s syntactic `Fresh` no longer appears in any rule. It remains the honest description of
   a term as written, and `FreshUpToScong` is defined from it.

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
      `FreshUpToScong x (subject μ)` — a hypothesis on the rule, so no use can forget it.

      The invariant notion rather than `Proc.lean`'s syntactic `Fresh`, because the label's channel is
      a term the congruence can rewrite: `out ⌈νx.0⌉ b` steps with the label `⌈0⌉!(b)`, so a syntactic
      proviso on `⌈0⌉` is discharged vacuously and the rule would let an action on the bound name
      through. The record in this file's scope note is the derivation that made that visible. -/
  | nu {x P P' : Proc} {μ : Label} :
      FreshUpToScong x (Label.subject μ) → Step P μ P' → Step (Proc.nu x P) μ (Proc.nu x P')
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
   (`nu_nil : νx.0 ≡ 0`, `nu_par : νx.(P | Q) ≡ P | νx.Q`) and a proviso tracking scope syntactically
   does not survive that. What the looseness costs is precision on terms under a `ν`; what it buys is
   `canStep_of_scong`, and with it every obligation in this file.
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

    Sixteen cases, one per constructor. Three are worth naming. `par_nil` and `nu_nil` hold because
    the atom clauses return `False`; `nu_par` needs nothing at all, because the `nu` clause ignores
    its binder — the freshness proviso that constructor carries is what *extrusion* needs, not what
    the label set needs, and this proof is where that distinction becomes visible. -/
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
  | nu_par _ => exact Iff.rfl
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

   The obligation is `¬ Barb (νx.P) x` under the restriction rule's freshness, and it is now a theorem
   about the rule rather than about the syntactic condition the rule used to carry. This part holds
   the two halves of that: the lemma that does the work, and the obligation itself.

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
    says exactly what "fresh" has to exclude, and the syntactic notion excluded too little. -/
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

/-- **Obligation 2: no barb survives a restriction on the name it would need.**

    `FreshUpToScong x P` is the restriction rule's own proviso (`Congruence.lean`), so this is the
    obligation in the form the rule can state. The proof is `barb_nu_subject_occurs` plus
    `scong0_of_scong`: the barb hands back a term congruent to `P` that mentions `x`, and the proviso
    says no term *reachable* from `P` does.

    The hypothesis is strictly stronger than `Proc.lean`'s syntactic `Fresh`
    (`fresh_of_freshUpToScong`), and that is why the rule changed rather than this theorem being stated
    with the syntactic condition. This module's scope note carries the two derivations that made the
    difference visible. -/
@[axiom_budget 0]
theorem no_barb_nu_of_fresh {x P : Proc} (h : FreshUpToScong x P) : ¬ Barb (Proc.nu x P) x := by
  intro hb
  obtain ⟨Q, hQ, hocc⟩ := barb_nu_subject_occurs hb
  exact h Q (scong0_of_scong hQ) hocc

end DarkFi.Semantics
