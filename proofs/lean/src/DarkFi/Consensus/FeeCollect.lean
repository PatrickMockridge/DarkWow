/-
# The fee-collect rule — a decision table that is a presentation of a rule

`validation.rs::validate_block_structure` ends with a `match` over three components and four arms,
deciding whether a block's fee-collection structure is admissible. This module models it and asks the
question the shape invites: **is the four-arm decision table a rule, or a presentation of one?**

## The table, and the rule it turns out to be

The code reads:

    match (fee_collect_tx_position, fee_call_count, fee_collect_call_count) {
        (Some(_), 0, _) => Err("FeeCollectV1 present but block has zero fee calls"),
        (None, f, _) if f > 0 => Err("block has f fee call(s) but no FeeCollectV1 call"),
        (Some(pos), _, _) => { if pos != len - 1 { Err(...) } }
        (None, _, _) => {}
    }

preceded by `if fee_collect_call_count > 1 { Err(...) }`. `codeVerdict` below is that, as a function, with
the arms in the code's order — deliberately, so the equivalence is about the code's own structure rather
than about a tidied version of it. `codeVerdict_is_the_rule` proves the table equivalent to two clauses
plus the guard: **at most one collect call; the collection is present iff there are fee calls; and it is
the last transaction when it is present.** So the table is a presentation — the nested arms are the rule,
not a second rule — and the arms' *order* matters only for which message a refusal carries, which is the
same relation `Consensus/BlockHeader.lean` found between `check_block_header`'s checks and their order.

**This obligation had no register row, so the unit minted one** (`OBL-C117`), as the previous unit did for
the two-stage target rule.

## The guard is load-bearing, and it is why the third component is `_` in every arm

The `match`'s third component — the collect **call** count — is `_` in all four arms, which reads as a
component the table ignores. `arm_outcome_ignores_the_call_count` says exactly how far that reading goes:
*given* the guard, the verdict does not depend on the count at all. And the guard is what makes that true
rather than incidental — `call_count_guard_is_load_bearing` refutes the alternative, with the witness the
guard exists for: two collect calls, in a block whose fees are present and whose collection is final, is
accepted by the arms and refused only by the guard. So the component is inert because it has already been
excluded, not because it does not matter.

## The rule's one proxy, stated rather than glossed

The clause "present iff there are fee calls" is implemented over a **call count**, and the code says why
in its own comment: "FeeV3 fees are hidden behind Pedersen commitments — exact amounts are not available
in call data. The structural validator checks fee presence, not sum." So the structural rule cannot see
an amount, and a block carrying a zero-value FeeV3 call must also carry a collection — the proxy is
strictly wider than "the fees sum to more than zero". That is a *prerequisite* of the rule rather than a
defect: the fee pot's actual arithmetic is checked in WASM against the collection, and the structural
layer's job is to make the block's *shape* well-formed before that. The model states the proxy as the
predicate — `feeCalls` is a count, never an amount — and this paragraph is the boundary.

## Which proofs cost an axiom, measured

Four of the five theorems here are at **budget 1** and one is at **budget 0**, and the split is about the
tactic rather than the proposition: the four that use `cases`/`simp` over `Bool` equalities reach
`Classical.choice`, while `arm_outcome_ignores_the_call_count` — which proves the same kind of statement
with `rw` and `if_neg` against explicit `omega` witnesses — does not. All five were first annotated 0 and
the collector corrected four, which is the same correction `Consensus/CoinbaseSplit.lean` records; the
difference worth keeping is that the *style* is what costs the axiom here, so a later unit has a lever
rather than only a budget.

## What this does not model

* **The detectors.** `as_mass_balance_fee_v3` requires the native-token contract *and* a decodable FeeV3
  payload, and the collect call is found by contract id plus selector `0x06`. The model takes the counts
  as given: modelling the detectors is modelling two decoders, and the Rust tests already pin their
  precision (`phase05_short_fee_call_not_counted`, `phase05_ignores_other_contracts_zero_selector`).
* **The two-clause rule's own necessity.** That a collection must exist at all, and must be final, is the
  fee pot's design (`consensus-coinbase.md` §3.15); the model proves the table matches the rule, not that
  the rule is the right one.
* **The position arithmetic's truncated subtraction.** The code compares against
  `block.transactions.len() - 1`; an empty block would make that `0`, and `validate_block_structure`
  rejects an empty block several checks earlier, so the model's `collectIsLast` is a plain Bool rather
  than a subtraction.
* **Nothing here is a claim about the Rust.** The arms are transcribed from `validate_block_structure`,
  and the nine `phase05_*` tests in that file are the negative controls — one per arm, one per guard, and
  two for detector precision. -/

import Mathlib
import DarkFi.AxiomBudget

namespace Consensus.FeeCollect

/-- The block-level facts the fee-collect rule reads, as the code reads them. -/
structure FeeShape where
  /-- A `FeeCollectV1` call exists in the block. -/
  collectPresent : Bool
  /-- And its transaction is the block's last. -/
  collectIsLast : Bool
  /-- The block's `FeeV3` **call count**. A count, never an amount — see the module note on the proxy. -/
  feeCalls : Nat
  /-- The block's `FeeCollectV1` **call** count — calls, not transactions, which the guard is about. -/
  collectCalls : Nat

/-- **The code's verdict, arm by arm and in its order.** The guard first, then the four arms of the
    `match`; `false` is "refused" and the arm *order* is preserved because the equivalence below is a
    claim about the code's structure, not about a tidied form of it. -/
def codeVerdict (s : FeeShape) : Bool :=
  if s.collectCalls > 1 then false
  else if s.collectPresent = true ∧ s.feeCalls = 0 then false
  else if s.collectPresent = false ∧ s.feeCalls > 0 then false
  else if s.collectPresent = true then s.collectIsLast
  else true

/-- **The rule the table implements**, as its two clauses plus the guard: at most one collect call; the
    collection is present **iff** the block has fee calls; and it is final when present. -/
def rule (s : FeeShape) : Prop :=
  s.collectCalls ≤ 1 ∧
  (s.collectPresent = true ↔ s.feeCalls > 0) ∧
  (s.collectPresent = true → s.collectIsLast = true)

/-- **The decision table is the rule.** The four arms and the guard are exactly the two clauses — so the
    table is a *presentation* of the rule and not a second rule, and a reader who has the clauses has the
    whole of it. Stated as an equivalence in both directions, so neither side is merely implied. -/
@[axiom_budget 1]
theorem codeVerdict_is_the_rule (s : FeeShape) : codeVerdict s = true ↔ rule s := by
  obtain ⟨cp, cl, fc, cc⟩ := s
  unfold codeVerdict rule
  cases cp <;> cases cl <;> (cases fc with | zero => simp | succ k => simp)

/-- **The guard is what makes the `match`'s third component inert**, not the component's own
    irrelevance: once the call count is within the guard, the verdict does not depend on it at all. -/
@[axiom_budget 0]
theorem arm_outcome_ignores_the_call_count (s : FeeShape) (h : s.collectCalls ≤ 1) :
    codeVerdict s = codeVerdict { s with collectCalls := 0 } := by
  unfold codeVerdict
  rw [if_neg (by omega : ¬ s.collectCalls > 1),
      if_neg (by simp : ¬ ({ s with collectCalls := 0 } : FeeShape).collectCalls > 1)]

/-- **And the guard is load-bearing.** Without it the arms accept a block with **two** collect calls whose
    fees are present and whose collection is final — the exact input the count guard exists for, and the
    input its Rust test `phase05_rejects_two_collect_calls_in_one_tx` constructs. The statement is a
    refutation of the universal so that no weakening of the loop rescues it: the guard is not a
    belt-and-braces duplicate of anything in the arms. -/
@[axiom_budget 1]
theorem call_count_guard_is_load_bearing :
    ¬ (∀ s : FeeShape,
        (s.collectCalls ≤ 1 → codeVerdict s = true) → s.collectCalls ≤ 1) := by
  intro h
  have hw := h { collectPresent := true, collectIsLast := true, feeCalls := 5, collectCalls := 2 }
  norm_num [codeVerdict] at hw

/-- A collection that is present and non-final is refused — the arms' third case, which is the `if pos !=
    len - 1` arm rather than one of the two clauses. -/
@[axiom_budget 1]
theorem non_final_collection_is_refused (f : Nat) :
    codeVerdict { collectPresent := true, collectIsLast := false, feeCalls := f, collectCalls := 1 }
      = false := by
  unfold codeVerdict
  cases f <;> simp

/-! ===== Non-vacuity: the arms at concrete values, one per Rust test ===== -/

/-- **The four ways the table can be exercised, each named for the Rust test that constructs it.** Both
    acceptances and the four refusals, stated together so the predicate is neither always-true nor
    always-false — which is what a table of negations alone would not establish. -/
@[axiom_budget 1]
theorem arm_witness :
    -- phase05_accepts_fees_with_final_fee_collect
    codeVerdict { collectPresent := true, collectIsLast := true, feeCalls := 3, collectCalls := 1 } = true ∧
    -- phase05_accepts_zero_fee_block_without_fee_collect
    codeVerdict { collectPresent := false, collectIsLast := false, feeCalls := 0, collectCalls := 0 } = true ∧
    -- phase05_rejects_duplicate_fee_collect, and its per-call variant
    codeVerdict { collectPresent := true, collectIsLast := true, feeCalls := 3, collectCalls := 2 } = false ∧
    -- phase05_rejects_fee_collect_with_zero_fees
    codeVerdict { collectPresent := true, collectIsLast := true, feeCalls := 0, collectCalls := 1 } = false ∧
    -- phase05_rejects_fees_without_fee_collect
    codeVerdict { collectPresent := false, collectIsLast := false, feeCalls := 3, collectCalls := 0 } = false ∧
    -- phase05_rejects_fee_collect_not_final
    codeVerdict { collectPresent := true, collectIsLast := false, feeCalls := 3, collectCalls := 1 } = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ <;> unfold codeVerdict <;> norm_num

end Consensus.FeeCollect
