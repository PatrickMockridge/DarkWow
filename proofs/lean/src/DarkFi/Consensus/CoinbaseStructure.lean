/-
# The coinbase's structure — what the four checks buy, and the bypass they close

`validation.rs::validate_block_structure` opens with four cheap structural checks before any PoW, ZK or
WASM work: the block is non-empty; the first transaction's first call is the pow reward; exactly one
transaction in the block is coinbase-classified; and that transaction has exactly one contract call. This
module models them and asks what they *buy* — because the code states their purpose in one of the most
concrete comments in the tree:

> Coinbase tx MUST have exactly 1 contract call (PoWRewardV1 only). Extra calls in tx[0] would bypass
> Pedersen mass balance (`proof_of_token_balance` skips entire tx[0] when first call is PoWRewardV1), ZK
> witness verification (`execution.rs` skips entire tx[0]), and pre-witness checks (`block_acceptor.rs`
> skips entire tx[0]). Structural fix makes call-level skip fixes defense-in-depth.

That claim is checkable, and this module checks it. **This obligation had no register row, so the unit
minted one** (`OBL-C118`) — the third consecutive unit to mint rather than append, which is itself a
finding about the register's coverage of `validation.rs`.

## Two guards, each making a later index safe

The checks are ordered so that every index is preceded by the check that makes it in range, and that is
worth stating because it is *load-bearing* rather than incidental:

* the empty-block check precedes `transactions[0]` (`empty_check_guards_the_first_index`), and
* the single-call check precedes `contract_calls[0]` (`single_call_guards_the_inner_index`).

Both are the shape `Consensus/UncleRules.lean` found in `check_uncles`'s slice alignment — a guard whose
purpose is an index — and both here are *correct*, with the refutations beside them showing what each
guard excludes. So this is the fourth site of that shape found and the first where nothing is missing.

## The four checks collapse to one line, and that line is what makes the skips sound

`wellFormed_gives_a_single_pow_call` proves the conjunction of the four checks equivalent to a shape:
**the block is the pow-reward transaction followed by anything at all.** That is not a tidying — it is the
reason the four exist, because the three skip sites are *whole-transaction* skips, and a whole-transaction
skip is only sound if the transaction it skips contains nothing else.

`full_coverage` is the payoff: for a structurally valid block, **the calls the mass-balance rule does not
see are exactly the pow reward's own call** — `b.join = powReward :: balanceView b` — so the rule's blind
spot has size one, and it is the call the block is *for*. And `compound_coinbase_escapes` is the
refutation, stated so that no weakening rescues it: drop the single-call rule and *some* call in `tx 0` is
invisible to the balance rule, with the two-call `tx 0` the comment describes as the witness. So the
compound-coinbase rule is not defence in depth — it is the rule that gives the skips their soundness, and
the comment's closing phrase reads the dependency backwards. The Rust test
`rejects_compound_coinbase_two_calls` constructs exactly that input, and
`accepts_coinbase_with_single_call` is the positive control.

## The uniqueness rule has a second use the comment does not mention

`block_acceptor`'s pre-witness loop and `chain_state.rs::check_coinbase_maturity` exempt a transaction by
**classifier alone, with no index gate** — a divergence from `proof_of_token_balance`, which gates on
`tx_idx == 0`. `tail_is_not_coinbase` is why that divergence is harmless: the uniqueness rule says no
transaction after the first is coinbase-classified, so a gate-free skip and a gated one exempt the same
transaction on any structurally valid block. This is the claim `Consensus/CoinbaseSplit.lean` makes in
prose about its own loop ("no index gate is needed alongside it: `validate_block_structure` requires
exactly one transaction in the block to satisfy this same classifier"), now a theorem — and it is worth a
theorem precisely because the two sites *look* inconsistent.

## What this does not model

* **The contract-id and payload checks.** The code checks the pow call targets the native-token contract
  and its data is at least two bytes; the model folds "is the pow reward" into one classifier, so those are
  inside its definition rather than separate conjuncts.
* **The arithmetic inside the coinbase's parameters.** The note-level value checks
  (`commitment_attrs.value == effective_value`, `effective + total_pin == input.value`) are
  `Consensus/CoinbaseSplit.lean`'s subject.
* **Header continuity** — `height == current + 1` and `previous == prev`. Those are two equalities with
  nothing to state, and a unit for them would have only completeness as its justification. Recorded here
  as a deliberate omission rather than left to look like an oversight.
* **Nothing here is a claim about the Rust.** The checks are transcribed from `validate_block_structure`. -/

import Mathlib
import DarkFi.AxiomBudget

namespace Consensus.CoinbaseStructure

/-- A contract call, as these checks see it: the pow reward, or anything else — the "anything else" case
    carrying a tag so a witness can say *which* call escaped. -/
inductive Call where
  | powReward : Call
  | other : Nat → Call
  deriving DecidableEq

/-- A block's calls: transactions, each a list of contract calls. -/
abbrev Block := List (List Call)

/-- **The coinbase classifier**: a transaction whose **first** call is the pow reward. The Rust's also
    requires the native-token contract id, which the structural checks establish separately; the model
    folds it into what "the pow reward" means. -/
def isCoinbase (tx : List Call) : Bool := tx.head? == some Call.powReward

/-- The classifier as a proposition, so the proofs below can use it as one. -/
@[axiom_budget 0]
theorem isCoinbase_iff (tx : List Call) :
    isCoinbase tx = true ↔ tx.head? = some Call.powReward := by
  simp [isCoinbase, beq_iff_eq]

/-- How many transactions in the block are coinbase-classified. -/
def powCount (b : Block) : Nat := (b.filter isCoinbase).length

/-- **The four structural checks**, as one predicate: non-empty; the first transaction's first call is the
    pow reward; exactly one coinbase-classified transaction; and that transaction has exactly one call. -/
def wellFormed (b : Block) : Prop :=
  ∃ t0 rest, b = t0 :: rest ∧ isCoinbase t0 = true ∧ powCount b = 1 ∧ t0.length = 1

/-- **The calls the mass-balance rule accumulates over**, as `proof_of_token_balance` does it: a
    transaction is skipped **whole** when it is the first one *and* its first call is the pow reward —
    the `tx_idx == 0 && tx.first_call_is_pow_reward()` gate, whose index half is why the `if` below
    applies only at the head and the tail is joined plainly. -/
def balanceView : Block → List Call
  | [] => []
  | tx0 :: rest => (if isCoinbase tx0 then [] else tx0) ++ rest.join

/-! ===== Part 1 — the two guards, each making a later index safe ===== -/

/-- The empty-block check is what makes `transactions[0]` safe. Stated for an arbitrary block so it is a
    property of the guard rather than of one input. -/
@[axiom_budget 0]
theorem empty_check_guards_the_first_index (b : Block) (h : b ≠ []) : (b.head?).isSome := by
  cases b <;> simp_all

/-- **And without it the index is unguarded** — the empty block is exactly what the check excludes. -/
@[axiom_budget 0]
theorem without_the_empty_check_the_index_is_unsafe :
    ¬ (∀ b : Block, (b.head?).isSome) := by
  intro h
  have hw := h []
  simp at hw

/-- The single-call check is what makes `contract_calls[0]` safe, for the same reason one level in. -/
@[axiom_budget 0]
theorem single_call_guards_the_inner_index (t0 : List Call) (h : t0.length = 1) :
    (t0.head?).isSome := by
  cases t0 <;> simp_all

/-- And without it the inner index is unguarded. -/
@[axiom_budget 0]
theorem without_the_single_call_check_the_inner_index_is_unsafe :
    ¬ (∀ t0 : List Call, (t0.head?).isSome) := by
  intro h
  have hw := h []
  simp at hw

/-! ===== Part 2 — the four checks collapse to one shape ===== -/

/-- **The conjunction is one line, with the count kept.** Non-empty, first call is the pow reward,
    exactly one coinbase, and exactly one call in it — together they say the block is
    `[powReward] :: rest`, *and* that the count rule still holds of that shape. That is what makes the
    whole-transaction skips sound, and it is why the four checks are not four independent guards. -/
@[axiom_budget 0]
theorem wellFormed_shape (b : Block) (h : wellFormed b) :
    ∃ rest, b = [Call.powReward] :: rest ∧ powCount ([Call.powReward] :: rest) = 1 := by
  obtain ⟨t0, rest, hb, hcb, hcount, hlen⟩ := h
  subst hb
  have ht0 : t0 = [Call.powReward] := by
    cases t0 with
    | nil => simp at hlen
    | cons a l =>
      cases l with
      | nil =>
        have ha : a = Call.powReward := by
          rw [isCoinbase_iff] at hcb
          simpa using hcb
        rw [ha]
      | cons b' l' => simp at hlen
  subst ht0
  exact ⟨rest, rfl, hcount⟩

/-- The shape alone, which is all Part 3 needs. -/
@[axiom_budget 0]
theorem wellFormed_gives_a_single_pow_call (b : Block) (h : wellFormed b) :
    ∃ rest, b = [Call.powReward] :: rest := by
  obtain ⟨rest, hb, _⟩ := wellFormed_shape b h
  exact ⟨rest, hb⟩

/-! ===== Part 3 — the payoff: the balance rule's blind spot is the pow call ===== -/

/-- **Only the pow reward's own call is invisible to the mass-balance rule.** For a structurally valid
    block the joined calls are the pow reward followed by exactly what the rule accumulates — so the
    rule's blind spot has size one, and it is the call the block is for. -/
@[axiom_budget 0]
theorem full_coverage (b : Block) (h : wellFormed b) :
    b.join = Call.powReward :: balanceView b := by
  obtain ⟨rest, rfl⟩ := wellFormed_gives_a_single_pow_call b h
  have hcb : isCoinbase [Call.powReward] = true := by
    rw [isCoinbase_iff]
    rfl
  simp [balanceView, hcb]

/-- **And without the single-call rule a call escapes** — the compound coinbase the code's comment
    describes, as a refutation of the universal so that no weakening of the hypothesis set rescues it.
    The witness is a `tx 0` carrying the pow reward *and* a second call, which the balance rule skips
    whole; the same input is what `rejects_compound_coinbase_two_calls` constructs. -/
@[axiom_budget 0]
theorem compound_coinbase_escapes :
    ¬ (∀ b : Block, (∃ t0 rest, b = t0 :: rest ∧ isCoinbase t0 = true) →
        ∀ c ∈ b.join, c ∈ balanceView b) := by
  intro h
  have hcb : isCoinbase [Call.powReward, Call.other 7] = true := by
    rw [isCoinbase_iff]
    rfl
  have hw := h [[Call.powReward, Call.other 7]]
    ⟨[Call.powReward, Call.other 7], [], rfl, hcb⟩ (Call.other 7) (by simp)
  simp [balanceView, hcb] at hw

/-- The positive control, so the coverage theorem is not a statement about blocks that cannot exist: the
    single-call coinbase of `accepts_coinbase_with_single_call` has an empty blind spot beyond its own
    call. -/
@[axiom_budget 0]
theorem single_call_block_witness :
    ([([Call.powReward] : List Call)] : Block).join
      = Call.powReward :: balanceView [([Call.powReward] : List Call)] := by
  have hcb : isCoinbase [Call.powReward] = true := by
    rw [isCoinbase_iff]; rfl
  simp [balanceView, hcb]

/-! ===== Part 4 — the uniqueness rule, and the gate-free skip sites ===== -/

/-- The uniqueness rule, read as a statement about the tail: nothing after the first transaction is
    coinbase-classified. The count hypothesis is what makes the filter empty. -/
@[axiom_budget 0]
theorem tail_filter_eq_nil (rest : List (List Call))
    (h : powCount ([Call.powReward] :: rest) = 1) : rest.filter isCoinbase = [] := by
  have hcb : isCoinbase [Call.powReward] = true := by
    rw [isCoinbase_iff]; rfl
  have hlen : 1 + (rest.filter isCoinbase).length = 1 := by
    simpa [powCount, hcb] using h
  have hzero : (rest.filter isCoinbase).length = 0 := by omega
  simpa using hzero

/-- **No transaction after the first is coinbase-classified**, given the uniqueness rule. This is what
    makes `block_acceptor`'s pre-witness loop and `check_coinbase_maturity` sound *without* the index gate
    that `proof_of_token_balance` carries — the two look inconsistent and are not, and this is why. -/
@[axiom_budget 0]
theorem tail_is_not_coinbase (rest : List (List Call))
    (h : powCount ([Call.powReward] :: rest) = 1) : ∀ tx ∈ rest, isCoinbase tx = false := by
  intro tx hmem
  have hnil := tail_filter_eq_nil rest h
  by_contra hc
  have hmem' : tx ∈ rest.filter isCoinbase := by
    simp only [List.mem_filter, decide_eq_true_eq]
    exact ⟨hmem, by simpa using hc⟩
  rw [hnil] at hmem'
  simp at hmem'

/-- And the divergence is harmless in the direction that matters: a gate-free skip over a structurally
    valid block's tail exempts **nothing**, so it and a gated skip agree. -/
@[axiom_budget 0]
theorem gate_free_skip_exempts_nothing (b : Block) (h : wellFormed b) (tx : List Call)
    (hmem : tx ∈ b.tail) : isCoinbase tx = false := by
  obtain ⟨rest, hb, hcount⟩ := wellFormed_shape b h
  subst hb
  exact tail_is_not_coinbase rest hcount tx (by simpa using hmem)

end Consensus.CoinbaseStructure
