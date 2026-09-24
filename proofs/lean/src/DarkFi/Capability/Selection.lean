/-
DarkFi.Capability.Selection — wallet.md §6.2: barb-cover input selection

**What this models.** §6.2 puts a requirement on the capabilities the wallet *selects* to exercise an
action, and the requirement is about the set rather than about any one member:

    covers( ⋃ barbs(SelectedCapabilities), requiredBarbs(Action) ) = true

together with the exclusion in its last sentence (a capability that is spoken for is never selected) and
the negative claim that opens it — *selection by asset value alone SHALL NOT* satisfy the requirement.
All three are requirements on the *choice*, so what is modelled here is the predicate such a choice has
to satisfy, not a search algorithm.

**What it does not model.** The chooser. The deployment's selection is in `bin/dww`; modelling its
heuristic would model the heuristic, while the invariant §6.2 states is what a heuristic has to meet.
That is a deliberate narrowing of the plan's wording ("`select` returns a sublist"): a `select` function
whose soundness is provable is either `filter` over the predicate below — in which case the theorem is
the predicate — or a search, in which case the theorem is about the search. What is here is the
invariant, plus the two structural facts that give it content: coverage is monotone in the selected set,
and a spoken-for capability cannot be in any selection.

**Why `compose`, and not a second union.** Coverage is stated with `Composition.compose`, the same
function `CapabilityType.coversBarbs` is stated with, and `selection_yields_a_capability_type` below
*constructs* that structure's witness field out of a selection. That is what makes §6.2's "the write path
enforces the invariant the read path constructs" (`wallet.md` §6.2, §7.1) a fact about one operator
rather than two definitions claimed to agree.

**`Wallet.lean`'s `walletConstruct`, and why there is no theorem about it here.** §6.2 says selection
reuses the composition rule the read path uses, and `walletConstruct (primitives) r s` returns
`some { primitives, coversBarbs := h }` exactly when `r.requiredBarbs ⊆ compose primitives`. That is
literally the term `selection_yields_a_capability_type` builds, from the same `compose` — so a theorem
saying "a selection is accepted by `walletConstruct`" would unfold that `if` and return its own
hypothesis, the definitional-restatement class the register already records against
`walletConstruct_sound` itself. The relation is therefore carried by the shared operator, and by that
construction failing to type-check if either side changes it, rather than by a second theorem that
restates it.

**The spend state, in the code's names.** `CapStatus` carries the deployment's own three variants
(`bin/dww/src/capability.rs:29` — `Pending | Processing | Spent`), and the *available* state is `none`
rather than a fourth constructor, because that is what the code stores: no status for a capability that
is not spoken for. `wallet.md` uses two incompatible vocabularies for that one lifecycle: §6.5's own
diagram (`:928`) is the code's — `NULL → Pending → Processing → Spent` — while §3 (`:559`) and §6.2
(`:678`) call the states `Unspent`/`Reserved`/`Spent`, where `Reserved` names the spoken-for state the
code calls `Pending`, and §6.5's prose (`:969`) mixes the two (`Reserved → Unspent`). §6.2 cites §6.5
for a variant §6.5's diagram does not contain. That drift is a documentation defect rather than
something to encode here — encoding either spec vocabulary would put a name in the proofs that the code
does not use — so the code's names are the names.

**And one modelling fact worth stating rather than discovering: `IsSelection` is not decidable.**
`PrimitiveType` carries `BEq` and not `DecidableEq`, so a `Held` cannot be compared and `∀ h ∈ sel, …`
has no `Decidable` instance. The *coverage* half is decidable — `Finset Barb` with `Barb`'s
`DecidableEq` — and the *exclusion* half is not, which is why the witnesses below are proved by hand
instead of closed by `decide`. Their content is unaffected: §6.2's rule is about what a choice must
satisfy, and both halves of it are stated.
-/

import Mathlib
import DarkFi.AxiomBudget
import DarkFi.Capability.Types
import DarkFi.Capability.Composition

namespace DarkFi.Capability.Selection

open DarkFi.Capability.Types
open DarkFi.Capability.Composition

/-- A held capability's place in the spend lifecycle, under the deployment's own three names.
    `none` — no status recorded — is the available state. -/
inductive CapStatus where
  | pending
  | processing
  | spent
  deriving DecidableEq, Repr

/-- A capability the wallet holds, as selection sees it: the primitives its type composes, and its spend
    state. Deliberately *not* the asset value: §6.2 excludes it from the eligibility rule, and a field
    the rule never reads would invite a reader to think it reads it.

    No `deriving`: `PrimitiveType` has `BEq` and not `Repr` or `DecidableEq`, so neither can be
    derived here — and that second absence is what makes `IsSelection` undecidable (see the module note).
-/
structure Held where
  primitives : List PrimitiveType
  status : Option CapStatus

/-- §6.2's eligibility: not spoken for. -/
def available (h : Held) : Prop := h.status = none

/-- The barbs a whole selection covers: the union of every selected capability's primitives. Written as
    a recursion rather than as `compose (sel.bind Held.primitives)` so that `covered_append` below is an
    induction on this definition and needs no lemma about list mapping — the two are the same function,
    and which one carries the proofs is not the content. `compose` is `Composition.compose`, so a
    selection's coverage and a capability type's `coversBarbs` are one operator. -/
def covered : List Held → Finset Barb
  | [] => ∅
  | h :: t => compose h.primitives ∪ covered t

/-- §6.2's requirement on a selection, both halves: the action's barbs are covered by the *set*, and
    every capability in the set is available — §6.2's last sentence, which
    `spent_capability_is_in_no_selection` below makes consequential. -/
def IsSelection (required : Finset Barb) (sel : List Held) : Prop :=
  required ⊆ covered sel ∧ ∀ h ∈ sel, available h

/- ==========================================================================
   Coverage composes the way the selected list does
   ========================================================================== -/

/-- `compose` over a concatenation is the union of the two. Proved by induction on the first list, so
    it rests on `compose`'s own two equations and nothing else — the statement is what has to be true,
    and which library lemma happens to express it is not the content. -/
@[axiom_budget 0]
theorem compose_append (p q : List PrimitiveType) :
    compose (p ++ q) = compose p ∪ compose q := by
  induction p with
  | nil => simp [compose]
  | cons r rs ih => simp [compose, ih, Finset.union_assoc]

/-- And the same for `covered`, which is what makes coverage a property of the selected *list* rather
    than of any individual member. -/
@[axiom_budget 0]
theorem covered_append (a b : List Held) :
    covered (a ++ b) = covered a ∪ covered b := by
  induction a with
  | nil => simp [covered]
  | cons h t ih => simp [covered, ih, Finset.union_assoc]

/-- **The two ways of writing what a selection covers are the same thing**: the recursion above, and
    `compose` applied to every selected primitive. Recorded as a lemma rather than left as a `simp`
    detail because the two are *not* definitionally equal — the recursion exists so that
    `covered_append` needs no lemma about list mapping — while `CapabilityType`'s obligation is stated
    with `compose` of a primitive list. This is the bridge between them, and it is what
    `selection_yields_a_capability_type` crosses. -/
@[axiom_budget 0]
theorem covered_eq_bind (sel : List Held) :
    covered sel = compose (sel.bind Held.primitives) := by
  induction sel with
  | nil => simp [covered, compose]
  | cons h t ih => simp [covered, compose, ih, compose_append]

/- ==========================================================================
   §6.2's exclusion, and what it costs
   ========================================================================== -/

/-- **A spoken-for capability is in no selection.** §6.2's last sentence and §6.5's table, stated so
    that it is a consequence rather than a restatement of the definition: it quantifies over *every*
    required barb set, so no choice of action can select a capability that is already spoken for. The
    price is real and is the point — a capability whose barbs the action needs, but which is spoken
    for, cannot serve it, and the action must wait or fail. -/
@[axiom_budget 0]
theorem spent_capability_is_in_no_selection {required : Finset Barb} {sel : List Held} {h : Held}
    (hs : ¬ available h) (hm : h ∈ sel) : ¬ IsSelection required sel := by
  intro hsel
  exact hs (hsel.2 h hm)

/- ==========================================================================
   Coverage is monotone in the selected set
   ========================================================================== -/

/-- **Adding an available capability to a selection leaves it a selection.** Monotonicity is stated
    *with* the availability hypothesis rather than without it, and that is not a convenience: dropping
    it makes the statement false, because `covered` grows with the list while the exclusion half would
    be violated by an appended spoken-for capability. The hypothesis is exactly what makes it true,
    which is why §6.2's two halves cannot be separated. -/
@[axiom_budget 0]
theorem isSelection_append_available {required : Finset Barb} {sel extra : List Held}
    (h : IsSelection required sel) (he : ∀ g ∈ extra, available g) :
    IsSelection required (sel ++ extra) := by
  constructor
  · rw [covered_append]
    intro b hb
    exact Finset.mem_union.mpr (Or.inl (h.1 hb))
  · intro g hg
    rw [List.mem_append] at hg
    exact hg.elim (h.2 g) (he g)

/- ==========================================================================
   §6.2's relation to the read path, as a construction
   ========================================================================== -/

/-- **A selection is a `CapabilityType`.** Given a selection for the barbs a resource requires, the
    selected capabilities' primitives *are* the primitives of a capability type whose `coversBarbs`
    obligation holds — by the same `compose`. This is §6.2's claim that the write path enforces what the
    read path constructs, and it is stated as a construction so the two sides have to share the
    operator: change `covered` or `compose` and this stops type-checking rather than becoming subtly
    wrong.

    What it does not give: the ZK premise. `CapabilityType`'s field is a barb-coverage statement, and the
    deployment's `wallet_construct` additionally requires a proof inhabiting `L_{r,s}`; that gap is named
    where this tree names it (`Axioms.lean`'s ZK boundary) and is not closed here. -/
@[axiom_budget 0]
theorem selection_yields_a_capability_type {r : Resource} {s : Action} {sel : List Held}
    (h : IsSelection r.requiredBarbs sel) :
    ∃ ct : CapabilityType r s, ct.primitives = sel.bind Held.primitives :=
  ⟨{ primitives := sel.bind Held.primitives
   , coversBarbs := by rw [← covered_eq_bind]; exact h.1 }, rfl⟩

/- ==========================================================================
   Non-vacuity, and the refutation §6.2 asks for
   ==========================================================================
   §6.2's exclusion is not the only thing worth falsifying here. The requirement is a *conjunction*, and
   a predicate that no list satisfies — or that every list satisfies — would make the theorems above
   vacuous. Both directions are therefore witnessed on concrete data, by hand rather than by `decide`,
   because the exclusion half is not decidable (see the module note).
   ========================================================================== -/

/-- The capability §6.2's exclusion is about: `assetId` alone, available. Named because the two
    witnesses below are the two things §6.2 says about it — it does not cover an action needing
    `↓prove`, and it does cover one needing only `↓denominate`. `assetId`'s barb set is
    `{Barb.denominate}` (`Types.lean`), and that is what both witnesses turn on. -/
def assetValueOnly : Held := { primitives := [assetId], status := none }

/-- The barb set `assetValueOnly` composes to, as a lemma rather than a `simp` set repeated in two
    proofs — `compose [assetId]` is `assetId.barbs`, which is `{Barb.denominate}`. -/
@[axiom_budget 0]
theorem covered_assetValueOnly :
    covered [assetValueOnly] = {Barb.denominate} := by
  simp [covered, assetValueOnly, compose, assetId]

/-- **§6.2's requested refutation, in its sharp form.** A capability holding `assetId` — so, eligible by
    asset value — does not cover an action requiring `↓prove`. This is the statement §6.2's "selection by
    asset value alone SHALL NOT satisfy this requirement" makes, and it is falsifiable rather than true
    by construction: the positive witness below is what stops it being true of everything. -/
@[axiom_budget 0]
theorem asset_value_alone_does_not_cover :
    ¬ IsSelection {Barb.prove} [assetValueOnly] := by
  intro h
  have hp : Barb.prove ∈ covered [assetValueOnly] := h.1 (by simp)
  rw [covered_assetValueOnly] at hp
  exact absurd hp (by simp)

/-- **And the predicate is satisfiable**, so the theorems above quantify over a non-empty domain: the
    same capability *is* a selection when the action requires only what its barbs cover. -/
@[axiom_budget 0]
theorem a_selection_exists : IsSelection {Barb.denominate} [assetValueOnly] := by
  refine ⟨?_, ?_⟩
  · intro b hb
    rw [covered_assetValueOnly]
    exact hb
  · intro g hg
    rw [List.mem_singleton] at hg
    subst hg
    simp [available, assetValueOnly]

/-- **The exclusion is not vacuous either**: the same capability with a status recorded is a selection
    for nothing at all, and this one comes from the general theorem rather than from a second witness
    proof. -/
@[axiom_budget 0]
theorem a_spoken_for_capability_selects_nothing :
    ¬ IsSelection {Barb.denominate} [{ assetValueOnly with status := some CapStatus.pending }] :=
  spent_capability_is_in_no_selection
    (h := { assetValueOnly with status := some CapStatus.pending })
    (by simp [available]) (by simp)

end DarkFi.Capability.Selection
