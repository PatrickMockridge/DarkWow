/-
# The header rules — which of them are rules, and which are diagnostics

`validation.rs::check_block_header` runs six checks over a block's wire fields: the version, the
two-stage proof-of-work, the height continuity, the previous hash, the merkle root, and the target
equality. This module models the ones that are *rules about values* rather than plumbing, and asks the
question the surrounding comments invite: **of these checks, which one is doing the work, and is the
order they run in part of the rule?**

## Stage 1 alone constrains nothing, and that is a theorem, not a suspicion

The code's docstring states the two stages and their purpose:

> Stage 1: `hash_u32 <= block.header.target` — hash meets header's target.
> Stage 2: `block.header.target == expected_target` — target matches consensus rules
> (GetNextWorkRequired). This prevents self-declared-target attacks.

`stage_one_is_monotone_in_the_declared_target` is *why* stage 2 is needed, stated as the mechanism rather
than as the attack: the stage-1 predicate is an **up-set in the target the header declares**, so a
producer who wants a block to pass declares a larger one — and `stage_one_admits_every_hash_at_the_maximum_
target` is the extreme case, where every hash in the domain passes. Stage 1 is therefore not a difficulty
rule at all in isolation; it is a comparison against a number the block supplies. This obligation had no
register row before this module, so the unit that modelled it **minted** one: register row `OBL-C115`.

**And the two stages collapse into one condition** — `two_stages_collapse`: stage 1 *for some* declared
target plus stage 2 is exactly `reached h expected`. The quantifier has to be existential, and that is
not a stylistic choice: the same statement with `declared` free is **false**, because from `h ≤ expected`
there is no `h ≤ declared` to be had — stage 2 is precisely what removes the freedom. So the pair is the
consensus comparison with the declared field existentially eliminated, and the two-stage structure is a
redundancy that buys a **diagnostic** — which check failed — rather than a second condition. That is worth
a theorem because the code presents the two stages as the security structure, and the near-miss form above
is worth recording because it is the shape a reader would write.

**Two statements were written for this section and deleted, and the gate is why.** A theorem saying the
declared target is pinned (`h : declared = expected ⊢ declared = expected`) and one saying every hash
passes the maximum target (`hdom : h ≤ MAX_TARGET ⊢ stageOne h MAX_TARGET`) are both *bare projections of
their hypotheses* — the class `verification-hazop.md`'s HAZOP record already names, and the class this
layer's gate refuses. Both facts are true and both are already in the theorems above; restating them as
theorems would have bought nothing and the gate said so. The second was rewritten to carry a second
conjunct a smaller target refuses, which is non-definitional and is the finding.

**One modelling boundary, stated rather than glossed.** `hash_is_valid` is applied to a `u32` taken from
the hash's first four bytes (`check_pow_stage`), so the comparison's domain is 2³²−1 and not the hash's
256 bits: the work a target buys is capped by that domain. The model keeps the Rust's domain
(`MAX_TARGET`) and does not claim anything about the hash's remaining bytes.

## The Monero branch's anchor rule, and the exemption that is deliberate

A merge-mined block skips the native comparison entirely and must instead carry a valid coinbase merkle
proof **and** an anchor hash consistent with its own proof. The rule's shape is the interesting part: the
check is `claimed = 0 ∨ claimed = derived`, with the zero case meaning "not reported" — which is what
every production block carries, since the merge-mining RPC builds blocks with both anchor fields zeroed.
So the check makes the field *honest about the proof*; register row `OBL-C67` records what it does not
do — establish that the Monero block is real or on the Monero chain, since the three receipts prove only
coinbase-inclusion in some serialized block. The three facts below are the rule's two acceptances and its
one refusal, at concrete values.

## The order of the checks is a diagnostic, not a rule

`check_block_header`'s comments read as if the order were load-bearing, and one of them records a real
bug fixed by reordering:

> Previously this was checked AFTER Stage 2 target, causing fork blocks to fail with misleading "target
> mismatch" errors.

Reordering that fix changed *which error a fork block reports*, not whether it is accepted — because
every function in the module is pure and the checks are conjunctive, so acceptance is order-invariant.
`order_is_diagnostic` proves that for a list of predicate checks, and the module's own purity claim is
what makes the abstraction faithful. This is the third finding of the campaign's recurring shape — a
claim that looks like a rule and measures as a diagnostic — and it is the first where the code's comment
is *right* about the effect and only the emphasis is wrong. It is register row `OBL-C116`, minted with
`OBL-C115` because neither proposition had a row.

## What this does not model

* **The hash.** `hash_with_vm` is RandomX over the header; the model takes the comparison's operands as
  given, as every module here does.
* **`get_next_work_required`.** The expected target is an *input* to stage 2, not a function this module
  derives. Register row `OBL-C18` is about that function's traversal bound and is a separate subject.
* **The merkle proof and the previous-hash check** are modelled only as opaque conjuncts, because their
  content is a hash and a root respectively.
* **Nothing here is a claim about the Rust.** The checks are transcribed from `check_block_header` and
  `check_pow_stage`; a model of a rule is not a proof that the code implements it. -/

import Mathlib
import DarkFi.AxiomBudget

namespace Consensus.BlockHeader

/-! ===== Part 1 — the target comparison, and why stage 1 is not a rule ==== -/

/-- The comparison's domain: `check_pow_stage` builds a `u32` from the hash's first four bytes, so the
    largest target the Rust can express is `u32::MAX`. Stated rather than elided — a `Nat`-valued model
    would silently admit targets the code cannot hold. -/
def MAX_TARGET : Nat := 2 ^ 32 - 1

/-- `BlockTarget::reached` — a hash meets a target iff it is at most it. -/
def reached (h t : Nat) : Prop := h ≤ t

/-- Stage 1: the hash meets the target the header **declares**. -/
def stageOne (h declared : Nat) : Prop := reached h declared

/-- Stage 2: the declared target is the one consensus requires at this height. -/
def stageTwo (declared expected : Nat) : Prop := declared = expected

/-- **The mechanism the self-declared-target attack runs on**: stage 1 is an up-set in the target the
    header supplies, so a producer who wants to pass declares a larger one. Nothing in stage 1 bounds it
    — which is the whole content of "This prevents self-declared-target attacks" being stage 2's job. -/
@[axiom_budget 0]
theorem stage_one_is_monotone_in_the_declared_target (h t t' : Nat)
    (h1 : stageOne h t) (hle : t ≤ t') : stageOne h t' := by
  unfold stageOne reached at h1 ⊢
  omega

/-- **And at the maximum target every hash in the domain passes** — including one that a substantive
    target refuses, which is the second conjunct and the reason this is not simply the hypothesis
    restated. So stage 1 in isolation refuses nothing a producer can produce. -/
@[axiom_budget 0]
theorem stage_one_admits_hashes_a_target_refuses (h t : Nat) (hlt : t < h) (hdom : h ≤ MAX_TARGET) :
    stageOne h MAX_TARGET ∧ ¬ reached h t := by
  refine ⟨?_, ?_⟩
  · unfold stageOne reached; exact hdom
  · unfold reached; omega

/-- **The two stages collapse into one condition.** Stage 1 *for some* declared target plus stage 2 is
    exactly "the hash meets the consensus target" — so the declared target is existentially eliminated,
    which is the precise form of "its value is a red herring". The statement has to be existential rather
    than a fixed `declared`, and the reason is worth recording because it is the finding: from
    `h ≤ expected` alone there is no `h ≤ declared`, so a version quantifying `declared` freely would be
    **false**. Stage 2 is what removes the quantifier. -/
@[axiom_budget 0]
theorem two_stages_collapse (h expected : Nat) :
    (∃ declared, stageOne h declared ∧ stageTwo declared expected) ↔ reached h expected := by
  constructor
  · rintro ⟨declared, h1, h2⟩
    unfold stageOne reached at h1
    unfold stageTwo at h2
    rw [h2] at h1
    exact h1
  · intro h1
    exact ⟨expected, h1, rfl⟩

/- **And the declared target's value is not recoverable from an accepted block**, because both stages
   together pin it to consensus's: `two_stages_collapse` existentially eliminates it and the second
   stage's equality is what makes the elimination unique. Recorded as prose rather than as a theorem of
   its own — a statement whose whole content is its hypothesis (`h : declared = expected ⊢ declared =
   expected`) is the bare-projection shape this layer's gate refuses, and it was written, caught by the
   gate, and deleted here. -/

/-! ===== Part 2 — the Monero branch's anchor rule ===== -/

/-- The anchor rule: the claimed Monero anchor hash is either **unreported** (zero — what every
    production block carries, the RPC zeroing both fields) or the value **derived from this block's own
    proof**. -/
def anchorOk (claimed derived : Nat) : Prop := claimed = 0 ∨ claimed = derived

/-- An unreported anchor passes: the exemption is deliberate, and it is why a zero field is not a
    mismatch. -/
@[axiom_budget 0]
theorem unreported_anchor_passes (derived : Nat) : anchorOk 0 derived := Or.inl rfl

/-- The derived value passes, so the check is not simply narrower than the field. -/
@[axiom_budget 0]
theorem derived_anchor_passes (derived : Nat) : anchorOk derived derived := Or.inr rfl

/-- **A rewritten anchor is refused** — the point of the rule, since before `OBL-C67` the field was an
    independent claim about the same block the proof describes and nothing compared the two. -/
@[axiom_budget 0]
theorem rewritten_anchor_is_refused (claimed derived : Nat)
    (hne : claimed ≠ 0) (hne' : claimed ≠ derived) : ¬ anchorOk claimed derived := by
  rintro (h | h)
  · exact hne h
  · exact hne' h

/-- The branch's two acceptances and its refusal at concrete values, so the rule is neither
    always-true nor always-false: a zero claim and the derived claim both pass, a third is refused. -/
@[axiom_budget 0]
theorem anchor_witness :
    anchorOk 0 12345 ∧ anchorOk 12345 12345 ∧ ¬ anchorOk 999 12345 :=
  ⟨unreported_anchor_passes 12345, derived_anchor_passes 12345,
   rewritten_anchor_is_refused 999 12345 (by omega) (by omega)⟩

/-! ===== Part 3 — the order of the checks is diagnostic ===== -/

/-- A list of predicate checks, all of which an input must satisfy — the shape `check_block_header`
    has once its early returns are read as a conjunction. -/
def acceptsAll {σ : Type} (ps : List (σ → Prop)) (s : σ) : Prop := ∀ p ∈ ps, p s

/-- **Acceptance is invariant under reordering the checks.** This is why the comment that records
    reordering Stage 2 after the previous-hash check describes a *diagnostic* fix: the fix changed which
    error a fork block reports, and the module's purity is what makes the checks a conjunction rather
    than a sequence of effects. Stated for arbitrary permutations, so it is the general form and not the
    one swap. -/
@[axiom_budget 0]
theorem order_is_diagnostic {σ : Type} (ps qs : List (σ → Prop)) (h : ps.Perm qs) (s : σ) :
    acceptsAll ps s ↔ acceptsAll qs s := by
  unfold acceptsAll
  exact ⟨fun hp p hmem => hp p ((List.Perm.mem_iff h).mpr hmem),
         fun hp p hmem => hp p ((List.Perm.mem_iff h).mp hmem)⟩

/-- And reordering cannot *change* the verdict: a block accepted under one order is accepted under
    every permutation of the checks. The one-directional form, because this is the direction the
    comment's "misleading error" claim is about. -/
@[axiom_budget 0]
theorem reordering_preserves_acceptance {σ : Type} (ps qs : List (σ → Prop)) (h : ps.Perm qs) (s : σ)
    (hacc : acceptsAll ps s) : acceptsAll qs s :=
  (order_is_diagnostic ps qs h s).mp hacc

/-- A concrete witness that the abstraction is inhabited: two checks, one satisfied and one not, with
    the verdict unchanged by swapping them. -/
@[axiom_budget 0]
theorem order_witness :
    (acceptsAll [fun n : Nat => n ≤ 10, fun n : Nat => 1 ≤ n] 5 ↔
     acceptsAll [fun n : Nat => 1 ≤ n, fun n : Nat => n ≤ 10] 5) ∧
    ¬ acceptsAll [fun n : Nat => n ≤ 10, fun n : Nat => 1 ≤ n] 12 := by
  refine ⟨?_, ?_⟩
  · exact order_is_diagnostic _ _ (List.Perm.swap _ _ _) 5
  · intro h
    have := h (fun n : Nat => n ≤ 10) (by simp)
    omega

end Consensus.BlockHeader
