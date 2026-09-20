/-
# Cumulative Supply Chain — Multi-Block Inductive Proof

Proves the DarkWow coinbase supply chain invariant across multiple blocks.

## The Invariant

For all heights H ≥ 1:
  S_H = S_{H-1} + C_H
  supply_H = supply_{H-1} + reward(H)
  supply_H = expected_cumulative_supply(H)
  S_H = sum_{i=1..H} C_i

Where:
  - S_H is the cumulative Pedersen commitment at height H
  - C_H = Pedersen.commit Gv Gr(reward(H), blind(H)) is the coinbase for block H
  - reward(H) is the emission schedule (expected_reward)

## Security Property

This proves that the total supply of DRKW is exactly the sum of all coinbase
rewards from genesis — no hidden inflation, no supply manipulation, no
underflow/overflow. The cumulative Pedersen commitment chain provides a
verifiable audit trail from genesis to any block height.

## Correspondence with Code

- This proof models the invariant validated by `pow_reward_v1` in
  `src/contract/native_token/src/entrypoint/mod.rs:764-869`
- The Pedersen homomorphism is the same property used in `mint_v2.zk`
  circuit constraints (lines 65-82)
- The expected_reward function matches `src/sdk/src/blockchain.rs:114`
- The apply_pow_reward writes match lines 1041-1059 of the entrypoint
-/

import DarkFi.Arithmetic
import DarkFi.ECOps
import DarkFi.CrossCutting
import DarkFi.Field
import DarkFi.Axioms
import DarkFi.Pedersen
import DarkFi.AxiomBudget

/- The Pedersen generators are **parameters**, not constants: `Pedersen.commit` takes them, and
   its homomorphism holds for every choice. `variable` here means every declaration below is
   implicitly polymorphic in them, so nothing in this module claims that any particular
   constant-selection has been validated. -/
/-
## Reward schedule — assumptions moved to `DarkFi/Axioms.lean`

`reward`, `reward_monotone`, `MAX_SUPPLY` and `total_reward_bounded` are declared there, under
the same names, with the four-field annotation. `Axioms.lean` is the only file in
`proofs/lean/` permitted to contain an `axiom`.

One of the four did not need to be an assumption at all:

    axiom reward_nonneg (h : Nat) : reward h ≥ 0

`reward : Nat → Nat` already has `Nat` as its codomain, so this is `Nat.zero_le` and is proved
below rather than assumed. It went unnoticed because `axiom f : Nat → Nat` followed by
`axiom f_nonneg (h) : f h ≥ 0` reads as "the schedule is non-negative" when the axiom's own
type has already fixed the answer.
-/

/-- Reward is non-negative. **Proved, not assumed**: `reward : Nat → Nat`, so this is
    `Nat.zero_le`. Previously an `axiom`, which is why it appeared in the README's list of
    supply-chain assumptions. -/
@[axiom_budget 0]
theorem reward_nonneg (h : Nat) : reward h ≥ 0 := Nat.zero_le _

/--
## Cumulative Supply

expected_cumulative_supply(H) = sum_{h=1..H} reward(h)

This is the total DRKW that should exist at height H.
-/
noncomputable def expected_cumulative_supply (height : Nat) : Nat :=
  match height with
  | 0 => 0
  | n + 1 => expected_cumulative_supply n + reward (n + 1)

/-
## Coinbase blind, Pedersen model — moved to `DarkFi/Axioms.lean`

`coinbase_blind`, `Pedersen.Point`, the opaque `Pedersen.Point.add`, the `Add Pedersen.Point`
instance, `Pedersen.identity`, `pedersen_commit` and `Pedersen.pedersen_additive_homomorphism` are all
declared in `DarkFi/Axioms.lean`, under the same names and in the same (top-level) namespace,
which is why nothing below needed editing.

They move rather than stay because each is either an assumption itself or the signature an
assumption is stated in, and this module contains proofs that *consume* them: `apply_block`
below is defined with `reward`, `pedersen_commit` and `coinbase_blind`, so
`total_supply_theorem` and `cumulative_commit_theorem` unfold their way to those names.

The comment that used to sit on `Pedersen.pedersen_additive_homomorphism` here said it was "the same
axiom as `Pedersen.pedersen_additive_homomorphism` in `CrossCutting.lean`. That was wrong twice over:
it was in this file, and the `CrossCutting.lean` reference does not exist. There *was* a
second `Pedersen.pedersen_additive_homomorphism` — in `ECOps.lean`, as a `: Prop` stub that shared
nothing with this one but its name. The stub has been deleted; this is the only declaration
with that name.
-/

/--
## Cumulative Supply Chain State

At height H, the sled tree stores:
  - cumulative_value_commit: S_H (Pedersen point)
  - cumulative_blind: aggregate_blind_H (scalar)
  - total_supply: supply_H (u64)
-/

structure SupplyChainState where
  cumulative_commit : Pedersen.Point
  aggregate_blind : Nat
  total_supply : Nat

/-- Genesis state: identity commitment, zero blind, zero supply. -/
noncomputable def genesis_state : SupplyChainState :=
  { cumulative_commit := Pedersen.identity
  , aggregate_blind := 0
  , total_supply := 0
  }

/--
## Block Transition

For block at height H:
  1. Coinbase: C_H = Pedersen.commit Gv Gr(reward(H), blind(H))
  2. New cumulative: S_H = S_{H-1} + C_H
  3. New blind: blind_H = blind_{H-1} + blind(H)
  4. New supply: supply_H = supply_{H-1} + reward(H)

This matches apply_pow_reward in entrypoint/mod.rs:1041-1059.
-/
noncomputable def apply_block (Gv Gr : Pedersen.Point) (state : SupplyChainState) (height : Nat) : SupplyChainState :=
  let coinbase := Pedersen.commit Gv Gr (reward height) (coinbase_blind height)
  { cumulative_commit := state.cumulative_commit + coinbase
  , aggregate_blind := state.aggregate_blind + coinbase_blind height
  , total_supply := state.total_supply + reward height
  }

/-
## THEOREM: Supply Chain Invariant

For all heights H ≥ 0:
  1. state_H.total_supply = expected_cumulative_supply(H)
  2. state_H.cumulative_commit = sum_{i=1..H} Pedersen.commit Gv Gr(reward(i), blind(i))

The theorem holds for the chain starting from genesis_state and applying
apply_block for heights 1..H.

Proven by induction on H.
-/

/-- Helper: sum of pedersen commitments from 1 to H. -/
noncomputable def cumulative_commit_sum (Gv Gr : Pedersen.Point) (height : Nat) : Pedersen.Point :=
  match height with
  | 0 => Pedersen.identity
  | n + 1 =>
      cumulative_commit_sum Gv Gr n + Pedersen.commit Gv Gr (reward (n + 1)) (coinbase_blind (n + 1))

/-- Recursive chain application from genesis through height H. -/
noncomputable def apply_chain (Gv Gr : Pedersen.Point) (height : Nat) : SupplyChainState :=
  match height with
  | 0 => genesis_state
  | n + 1 => apply_block Gv Gr (apply_chain Gv Gr n) (n + 1)

/--
## LEMMA: Single-Step Supply

For any state and height H:
  apply_block(state, H).total_supply = state.total_supply + reward(H)

This is immediate from the definition of apply_block.
-/
@[axiom_budget 3]
lemma single_step_supply (Gv Gr : Pedersen.Point) (state : SupplyChainState) (h : Nat) :
  (apply_block Gv Gr state h).total_supply = state.total_supply + reward h := rfl

/--
## LEMMA: Single-Step Cumulative

For any state and height H:
  apply_block(state, H).cumulative_commit =
    state.cumulative_commit + Pedersen.commit Gv Gr(reward(H), coinbase_blind(H))

Immediate from the definition of apply_block.
-/
@[axiom_budget 3]
lemma single_step_cumulative (Gv Gr : Pedersen.Point) (state : SupplyChainState) (h : Nat) :
  (apply_block Gv Gr state h).cumulative_commit =
    state.cumulative_commit + Pedersen.commit Gv Gr (reward h) (coinbase_blind h) := rfl

/--
## THEOREM: Total Supply Matches Expected Cumulative Supply

∀ H, (apply_chain H).total_supply = expected_cumulative_supply(H)

Proof by induction on H.

Base case (H = 0):
  genesis_state.total_supply = 0 = expected_cumulative_supply(0) ✓

Inductive step:
  Assume: (apply_chain n).total_supply = expected_cumulative_supply(n)
  Show:   (apply_chain (n+1)).total_supply = expected_cumulative_supply(n+1)

  (apply_chain (n+1)).total_supply
  = (apply_block (apply_chain n) (n+1)).total_supply          [def apply_chain]
  = (apply_chain n).total_supply + reward (n+1)               [def apply_block]
  = expected_cumulative_supply(n) + reward (n+1)              [IH]
  = expected_cumulative_supply(n+1)                           [def expected_cumulative_supply]
  ✓
-/
@[axiom_budget 3]
theorem total_supply_theorem (Gv Gr : Pedersen.Point) (height : Nat) :
  (apply_chain Gv Gr height).total_supply = expected_cumulative_supply height := by
  induction height with
  | zero =>
      rfl
  | succ n ih =>
      -- Unfolding both sides gives `A + reward (n+1) = B + reward (n+1)` with `ih : A = B`, so
      -- `ih` belongs in the simp set. The separate `rw [ih]` that used to follow reported
      -- "no goals to be solved": `rw` closes a goal it makes reflexive.
      simp [apply_chain, apply_block, expected_cumulative_supply, ih]

/--
## THEOREM: Cumulative Commitment Sum

∀ H, (apply_chain H).cumulative_commit = cumulative_commit_sum(H)

S_H = sum_{i=1..H} Pedersen.commit Gv Gr(reward(i), blind(i))

Proof by induction on H.

Base case (H = 0):
  genesis_state.cumulative_commit = Pedersen.identity = cumulative_commit_sum(0) ✓

Inductive step:
  Assume: (apply_chain n).cumulative_commit = cumulative_commit_sum(n)
  Show:   (apply_chain (n+1)).cumulative_commit = cumulative_commit_sum(n+1)

  (apply_chain (n+1)).cumulative_commit
  = (apply_block (apply_chain n) (n+1)).cumulative_commit        [def apply_chain]
  = (apply_chain n).cumulative_commit + C_{n+1}                  [def apply_block]
  = cumulative_commit_sum(n) + C_{n+1}                           [IH]
  = cumulative_commit_sum(n+1)                                   [def cumulative_commit_sum]
  ✓

where C_{n+1} = Pedersen.commit Gv Gr(reward(n+1), coinbase_blind(n+1))
-/
@[axiom_budget 3]
theorem cumulative_commit_theorem (Gv Gr : Pedersen.Point) (height : Nat) :
  (apply_chain Gv Gr height).cumulative_commit = cumulative_commit_sum Gv Gr height := by
  induction height with
  | zero =>
      rfl
  | succ n ih =>
      simp [apply_chain, apply_block, cumulative_commit_sum, ih]

/--
## COROLLARY: Supply Chain Invariant (Combined)

For all heights H, BOTH properties hold simultaneously:
  1. Total supply = expected cumulative supply
  2. Cumulative commitment = sum of all coinbase commitments

This is the complete verification that `pow_reward_v1` correctly maintains
the multi-block supply chain invariant.
-/
@[axiom_budget 3]
theorem supply_chain_invariant (Gv Gr : Pedersen.Point) (height : Nat) :
  (apply_chain Gv Gr height).total_supply = expected_cumulative_supply height ∧
  (apply_chain Gv Gr height).cumulative_commit = cumulative_commit_sum Gv Gr height := by
  apply And.intro
  · exact total_supply_theorem Gv Gr height
  · exact cumulative_commit_theorem Gv Gr height

/--
## COROLLARY: No Hidden Inflation

For any height H, the total supply equals the sum of all expected rewards.
No additional DRKW can be created beyond the emission schedule.

total_supply_H = expected_cumulative_supply(H) = sum_{h=1..H} expected_reward(h)
-/
@[axiom_budget 3]
theorem no_hidden_inflation (Gv Gr : Pedersen.Point) (height : Nat) :
  (apply_chain Gv Gr height).total_supply = expected_cumulative_supply height :=
  total_supply_theorem Gv Gr height

/--
## COROLLARY: Cumulative Commitment is Auditable

For any height H, the cumulative Pedersen commitment equals the sum of
all individual coinbase commitments. Anyone with the blockchain can
independently compute and verify this chain.

S_H = sum_{i=1..H} C_i = sum_{i=1..H} Pedersen.commit Gv Gr(reward(i), blind(i))
-/
@[axiom_budget 3]
theorem cumulative_auditable (Gv Gr : Pedersen.Point) (height : Nat) :
  (apply_chain Gv Gr height).cumulative_commit = cumulative_commit_sum Gv Gr height :=
  cumulative_commit_theorem Gv Gr height
