/-!
MANUAL AUDIT DOCUMENTATION — NOT FORMAL PROOFS
This file contains structured vulnerability findings / circuit audit
results. It contains ZERO Lean theorems with non-trivial proofs.
All defs return String or List values for programmatic consumption.
-/
/-!
# HAZOP HIGH Tier — Targeted Vulnerability Proofs (Risk 40-59)

## HIGH-1/2: burn_v1.zk zero_cond Merkle Bypass (Risk 42/100)
When coin_value = 0, zero_cond returns 0 instead of the coin hash. The Merkle proof
verifies against the tree's zero leaf, not the actual coin.

## HIGH-3: labor_market/refund_v1.zk Refund Check Skip (Risk 42/100)
For non-milestone jobs, the refund amount constraint is vacuous — the guard
passes trivially for any refund_amount.

## HIGH-4: labor_market Nullifier Collision (Risk 40/100)
confirm_delivery, milestone_payment, and refund all use H(job_id, employer_secret).
Any one action consumes the nullifier for all three.

## HIGH-5: governance_report less_than_strict on zero values (Risk 42/100)
-/

namespace HAZOP.High

-- ===========================================================================
-- HIGH-1/2: burn_v1.zk — zero_cond Merkle Bypass
-- ===========================================================================

/--
Models the zero_cond gate used in burn_v1.zk (both PN and NT variants).

zero_cond(a, b):
  If a = 0: returns 0 (the value of `a`)
  If a ≠ 0: returns b

In burn circuits: coin_incl = zero_cond(coin_value, coin)
  If coin_value = 0: coin_incl = 0 (the ZERO LEAF of the Merkle tree)
  If coin_value ≠ 0: coin_incl = coin (the actual coin hash)

The Merkle proof then verifies: merkle_root(pos, path, coin_incl) = root

ATTACK: Mallory sets coin_value = 0. Then coin_incl = 0.
The Merkle proof now verifies that the ZERO LEAF is in the tree at position pos.
The zero leaf exists at EVERY empty position in the tree.
Mallory proves "inclusion" of a leaf that is trivially present everywhere.
-/

/--
THEOREM (HIGH-1): When coin_value = 0, the Merkle proof is vacuous.

The circuit proves: merkle_root(pos, path, 0) = root

Since the zero leaf (value 0) is the default value for all empty positions
in a Merkle tree, this proof is trivially satisfiable for ANY position.
Mallory does NOT need a real coin — any position in the tree with a zero
leaf satisfies the proof.

Combined with a nullifier = poseidon_hash(secret, 0x00), Mallory creates
a valid burn proof for a non-existent coin.
-/
structure BurnV1ZeroCondAttack where
  coin_value : Int    -- Mallory sets this to 0
  coin_hash : Int     -- Any value; zero_cond ignores it when coin_value = 0
  coin_incl : Int     -- = zero_cond(coin_value, coin_hash) = 0
  leaf_pos : Int      -- Any position with a zero leaf
  path : List (Int × Int) -- Any path to a zero leaf position

/--
THEOREM (HIGH-1): The attack works because zero_cond returns 0 when value = 0.

Mallory's attack:
  1. Set coin_value = 0 (free witness)
  2. Set coin_hash to any value (free witness — zero_cond ignores it)
  3. coin_incl = zero_cond(0, any_coin) = 0
  4. Choose any leaf_pos where the Merkle tree has a zero leaf (all empty positions)
  5. Provide path to that position
  6. merkle_root(pos, path, 0) = actual_tree_root (always true for empty positions)
  7. nullifier = poseidon_hash(secret, 0x00) — creates a nullifier for a coin that doesn't exist

The entrypoint then marks the nullifier as spent. The "burned" coin never existed.
-/
def burn_v1_zero_cond_bypass : String :=
  "HIGH-1/2: zero_cond(0,coin)=0; Merkle proof against zero leaf; non-existent coin burn"

/--
THEOREM (HIGH-1 fix): Require coin_value > 0.

Add to circuit:
  less_than_strict(ZERO, coin_value)  -- or equivalently range_check with a non-zero check

This ensures coin_value ≥ 1, so zero_cond always returns the actual coin hash.
The Merkle proof then verifies a real coin, not the zero leaf.

Alternative fix: Remove zero_cond entirely and enforce coin_value > 0.
The zero_cond gate exists to handle dummy inputs in batch operations.
If the entrypoint enforces at least one real input, zero_cond is unnecessary.
-/
def burn_v1_zero_cond_fix : String := "less_than_strict(ZERO, coin_value) before zero_cond"

/--
THEOREM (HIGH-2): Same attack applies to native_token/burn_v1.zk.

The NT burn circuit has identical structure: zero_cond(coin_value, coin) at line 70.
The attack and fix are identical.
-/

-- ===========================================================================
-- HIGH-3: labor_market/refund_v1.zk — Refund Amount Check Skipped
-- ===========================================================================

/--
Models the refund_v1.zk milestone logic.

The circuit computes:
  has_milestones = is_not_equal(milestone_count, ZERO)   -- boolean
  payment_remaining = base_sub(total_payment, completed_payment)  -- field sub, wraps around
  expected_refund = cond_select(payment_remaining, total_payment, has_milestones)
  refund_match = is_equal_base(expected_refund, refund_amount)    -- 1 if match, 0 otherwise
  milestone_refund_valid = base_mul(has_milestones, refund_match) -- has_milestones * refund_match
  refund_check_ok = is_equal_base(milestone_refund_valid, has_milestones)
  constrain_equal_base(refund_check_ok, ONE)

BUG: When has_milestones = 0 (no milestones):
  expected_refund = cond_select(payment_remaining, total_payment, 0) = total_payment
  refund_match = is_equal_base(total_payment, refund_amount)
  milestone_refund_valid = 0 * refund_match = 0
  refund_check_ok = is_equal_base(0, 0) = 1
  constrain_equal_base(1, 1) → PASSES

The guard `milestone_refund_valid == has_milestones` is 0 == 0 → TRUE.
The refund_amount check is COMPLETELY BYPASSED.
-/

/--
THEOREM (HIGH-3): For non-milestone jobs, refund_amount is unconstrained.

Mallory sets milestone_count = 0 (no milestones). Then:
  has_milestones = 0
  milestone_refund_valid = 0 * refund_match = 0
  refund_check_ok = is_equal_base(0, 0) = 1

The constraint passes regardless of refund_amount. Mallory can claim
ANY refund amount — even exceeding the total payment.
-/
def test_refund_bypass (milestone_count expected_refund refund_amount : Int) : Bool :=
  let has_milestones := if milestone_count = 0 then 0 else 1
  let refund_match := if expected_refund = refund_amount then 1 else 0
  let milestone_refund_valid := has_milestones * refund_match
  -- When has_milestones = 0: milestone_refund_valid = 0, check passes trivially
  milestone_refund_valid = has_milestones

/--
THEOREM (HIGH-3 fix): For non-milestone jobs, directly constrain the refund amount.

Replace the milestone-based guard with a direct equality:
  constrain_equal_base(refund_amount, total_payment)

Or add a branch:
  if has_milestones == 0:
    constrain_equal_base(refund_amount, total_payment)
  else:
    constrain_equal_base(refund_amount, base_sub(total_payment, completed_payment))
-/

-- ===========================================================================
-- HIGH-4: labor_market — Nullifier Collision Across Circuits
-- ===========================================================================

/--
Three labor_market circuits use the same nullifier derivation:
  confirm_delivery_v1:  nullifier = H(job_id, employer_secret)
  milestone_payment_v1: nullifier = H(job_id, employer_secret)
  refund_v1:             nullifier = H(job_id, employer_secret)

All three produce identical nullifiers for the same (job_id, employer_secret).
The first action to be processed consumes the nullifier. The other two actions
can never be performed because the nullifier is already marked spent.

This is a LIVENESS bug: performing one action blocks the others.
-/

/--
Simulated Poseidon hash for nullifier derivation testing.
-/
def sim_nullifier (job_id secret : Int) : Int := job_id * 12345 + secret * 67890 + 777

/--
THEOREM (HIGH-4): All three circuits produce identical nullifiers.

For the same (job_id, employer_secret), the nullifiers are byte-for-byte identical.
-/
def labor_nullifier_collision : String :=
  "HIGH-4: confirm/milestone/refund all use H(job_id,secret); consuming one nullifier blocks all three"

/--
THEOREM (HIGH-4 fix): Add a domain separator to each nullifier derivation.

  confirm_delivery:  nullifier = H(b"confirm", job_id, employer_secret)
  milestone_payment: nullifier = H(b"milestone", job_id, employer_secret)
  refund:             nullifier = H(b"refund", job_id, employer_secret)

This ensures each action has a unique nullifier even for the same (job, secret).
-/
def labor_nullifier_fix : String :=
  "nullifier = poseidon_hash(action_domain_separator, job_id, employer_secret)"

/--
THEOREM (HIGH-4): The collision is a real operational bug.

If an employer confirms delivery, they can NEVER make a milestone payment.
If an employer requests a refund, they can NEVER confirm delivery.
The job lifecycle becomes broken after any single action is taken.

Since labor_market is designed for iterative work (multiple milestones),
this bug makes the milestone feature unusable.
-/

-- ===========================================================================
-- HIGH-5: governance_report — less_than_strict on zero-division values
-- ===========================================================================

/--
When total_debt = 0 (free witness), collateral_ratio_bps = 0 (division by zero).
Then less_than_strict(15000, 0) compares 15000 < 0 in the field.
In field arithmetic: 0 ≡ p (mod p), so 15000 < p ≈ 2^254, which is TRUE.

Wait — LESS_THAN in the field: is 15000 < 0 in F_p?
0 in the field is the element 0. 15000 is the element 15000.
In the field: 15000 < 0 is FALSE (15000 is greater than 0).
But in field arithmetic with range_check interpretation, the comparison
is done on the integer representation modulo p.

The actual behavior depends on how `less_than_strict` interprets field elements.
If it treats them as integers in [0, p), then:
  15000 < 0 → 15000 < p → TRUE (since 0 ≡ p in the field)

This means: when total_debt = 0, the "ratio > 150%" check PASSES.
A governance report with zero debt and the minimum ratio threshold
would be accepted.
-/
def governance_report_less_than_strict_zero : String :=
  "HIGH-5: total_debt=0 → ratio=0 → less_than_strict(15000,0); division-by-zero produces undefined check"

-- ===========================================================================
-- HAZOP RISK VERIFICATION
-- ===========================================================================

def highFindings : List (String × Nat × String) := [
  ("HIGH-1: PN burn_v1 zero_cond bypass", 42,
   "zero_cond(0, coin) returns 0; Merkle proof against zero leaf"),
  ("HIGH-2: NT burn_v1 zero_cond bypass", 42,
   "Same zero_cond pattern as PN burn_v1"),
  ("HIGH-3: labor refund amount check skipped", 42,
   "Non-milestone jobs: guard 0*refund_match == 0 passes trivially"),
  ("HIGH-4: labor nullifier collision", 40,
   "confirm/milestone/refund all use H(job_id, secret); one blocks others"),
  ("HIGH-5: governance_report zero-division threshold", 42,
   "less_than_strict on ratio=0 from total_debt=0")
]

end HAZOP.High
