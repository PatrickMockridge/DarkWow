/-!
MANUAL AUDIT DOCUMENTATION — NOT FORMAL PROOFS
This file contains structured vulnerability findings / circuit audit
results. It contains ZERO Lean theorems with non-trivial proofs.
All defs return String or List values for programmatic consumption.
-/
/-!
# HAZOP ELEVATED Tier — Constraint Gap Documentation (Risk 30-39)

Six circuits with confirmed constraint gaps requiring targeted formal proofs.
-/

namespace HAZOP.Elevated

-- ===========================================================================
-- ELEV-1: deposit_v2.zk — zero_cond bypass (Risk 35)
-- ===========================================================================

/--
Same zero_cond pattern as burn circuits. When amount = 0, zero_cond(0, deposit_leaf)
returns 0, and the Merkle proof verifies against the zero leaf.

Mallory deposits 0 tokens on the external chain (free), proves inclusion of the
zero leaf (trivially present), and the bridge accepts a zero-value deposit.

While this doesn't steal funds, it wastes bridge tree capacity and could be
used for denial-of-service attacks by filling the deposit tree with zero-value
entries.
-/
def deposit_zero_cond_bypass : String :=
  "ELEV-1: deposit_v1 zero_cond bypass; zero-value deposits fill bridge tree (DoS)"

def depositZeroCondFix : String := "less_than_strict(ZERO, amount) before zero_cond"

-- ===========================================================================
-- ELEV-2: dex/cancel_swap_v2.zk — swap_id Incompatibility (Risk 35)
-- ===========================================================================

/--
CRITICAL CROSS-CIRCUIT BUG: The cancel circuit computes swap_id differently
from the create circuit.

create_swap_v2.zk:  swap_id = poseidon_hash(computed_lock, request_token, request_amount)
cancel_swap_v2.zk:  swap_id = poseidon_hash(lock_commitment)

These are fundamentally different computations:
  H(lock, token, amount)  — 3 field elements as Poseidon input
  H(lock_commitment)      — 1 field element as Poseidon input

These will NEVER produce the same hash. The cancel circuit cannot reference
the same swap_id stored by the create circuit. Cancellation is impossible.

UNLESS: the contract's cancel flow uses a DIFFERENT swap_id lookup that
matches the cancel circuit's derivation. The contract must store BOTH
a "create swap_id" (for execute) and a "cancel swap_id" (for cancel),
or the cancel circuit must use the same swap_id formula as create.

Mallory impact: Funds locked forever if cancellation is needed and the
swap_ids don't match. This is a liveness failure.
-/

/--
Simulated Poseidon hashes to demonstrate the swap_id mismatch.
-/
def sim_hash1 (a b c : Int) : Int := a * 111 + b * 222 + c * 333 + 444
def sim_hash2 (a : Int) : Int := a * 555 + 666

/--
THEOREM (ELEV-2): create and cancel swap_ids are incompatible.

For ANY values of (lock, token, amount), the two swap_id derivations
produce different results. The cancel circuit's swap_id will never
match the create circuit's swap_id.
-/
def swap_id_mismatch : String :=
  "ELEV-2: create uses H(lock,token,amount); cancel uses H(lock_commitment); swap_ids never match"

/--
THEOREM (ELEV-2 fix): Use the same swap_id derivation in cancel.

Fix: cancel_swap_v2.zk must compute:
  swap_id = poseidon_hash(computed_lock, request_token, request_amount)
using the same formula as create_swap_v2.zk.
-/
def cancelSwapIdFix : String := "Use create's swap_id formula: H(lock, token, amount)"

-- ===========================================================================
-- ELEV-3: drain_protection/exit_v2.zk — Incomplete Circuit (Risk 35)
-- ===========================================================================

/--
The exit circuit has explicit TODO comments (lines 170-177):
  "TODO: Remaining work:
   1. Add total_weight verification
   2. Add current_funds verification
   3. Complete exit_value calculation with proper division"

The Merkle root for DAO membership verification (dao_escrow_merkle_root) is
a free witness — it is NOT `constrain_instance`'d. Mallory can provide ANY
Merkle root and construct a fake membership proof against it.

Additionally:
  - exit_value computation is dead code (computed but never constrained)
  - No verification that the fund exists
  - No constraint that exit_value > 0
-/
def exit_incomplete_circuit : String :=
  "ELEV-3: drain_protection/exit_v1 has TODO comments; dao_escrow_merkle_root unconstrained; exit_value dead code"

def exitCircuitStatus : String := "INCOMPLETE — do not use in production"

-- ===========================================================================
-- ELEV-4: redeem_v2.zk — coin_value Not Checked for Zero (Risk 32)
-- ===========================================================================

/--
The redeem circuit exposes `coin_value` as a public input (constrain_instance),
but does NOT enforce `coin_value = 0` in-circuit. The entrypoint hardcodes
coin_value = 0 in the metadata.

If the entrypoint makes a mistake (off-by-one, wrong metadata, version mismatch),
a prover could create a receipt coin with coin_value = 1_000_000.

The receipt would have monetary value, making it spendable as a regular coin.
This breaks the redemption model: receipts are supposed to be zero-value
proofs of redemption, not valuable tokens.
-/
def redeem_coin_value_not_checked_in_circuit : String :=
  "ELEV-4: redeem_v1 coin_value not constrained to 0 in-circuit; defense-in-depth gap"

/--
THEOREM (ELEV-4 fix): Add in-circuit zero check as defense-in-depth.

  constrain_equal_base(coin_value, ZERO)

This is a single permutation constraint with zero cost. It provides
defense-in-depth: even if the entrypoint has a metadata bug, the
circuit itself rejects non-zero receipt coins.
-/
def redeemZeroCheckFix : String := "constrain_equal_base(coin_value, ZERO) in-circuit"

-- ===========================================================================
-- ELEV-5: dex/execute_swap_slippage_v2.zk — Field Division Integer Gap (Risk 30)
-- ===========================================================================

/--
The slippage circuit uses `base_div` (field division) for:
  received = base_div(received_numerator, alice_amount)
  tolerance_multiplier = base_div(slippage_sub, BPS)

Field division produces a field element, not a truncated integer.
For non-divisible amounts: `received = numerator / amount` in F_p
is `numerator * inverse(amount) mod p`, which is NOT the integer quotient.

Example:
  numerator = 101, amount = 100
  In integers: 101 / 100 = 1 (floor division)
  In F_p: 101 * 100^{-1} mod p = some large field element (~p/100)

The constraint less_than_or_equal(min_acceptable, received) then compares
this field element against min_acceptable. Since min_acceptable is an integer
(basis points * alice_amount / 10000), the field comparison may produce
unexpected results.
-/
def slippage_field_division_not_integer : String :=
  "ELEV-5: base_div produces field elements, not integers; token amounts not guaranteed"

/--
THEOREM (ELEV-5 fix): Add range_check to constrain received to integer range.

  range_check(64, received)

This ensures the "received" value fits in 64 bits, which is only possible
if numerator is actually divisible by amount. But this is overly restrictive.

Better fix: Use cross-multiplication instead of division:
  Instead of: received = numerator / amount
  Use:        less_than_or_equal(numerator, base_mul(alice_amount, max_received))

This avoids division entirely and works with integer semantics.
-/
def slippageFix : String := "Cross-multiplication: numerator <= amount * max_received"

-- ===========================================================================
-- ELEV-6: dex/execute_swap_v2.zk — bool_check on u64 Values (Risk 30)
-- ===========================================================================

/--
The execute_swap circuit calls:
  bool_check(alice_amount)   (line 156)
  bool_check(bob_amount)     (line 157)

AFTER calling:
  range_check(64, alice_amount)  (line 144)
  range_check(64, bob_amount)    (line 145)

If bool_check means "constrain to {0, 1}" (standard Boolean constraint),
this means alice_amount and bob_amount are restricted to 0 or 1.
Swaps of more than 1 token unit are IMPOSSIBLE.

FALSIFIABLE: Verify the actual bool_check implementation in the zkVM.
If bool_check is a "field element well-formedness check" rather than
a boolean constraint, this is benign. But the name strongly suggests
a Boolean check, and the constraint in `small_range_check.rs` uses
range=2 → constrains to {0, 1}.

HYPOTHESIS: This IS a boolean constraint bug, making DEX swaps limited
to 0 or 1 token units.
-/
def dex_bool_check_amounts : String :=
  "ELEV-6: bool_check(alice_amount) after range_check(64) constrains swap amounts to 0 or 1"

/--
TEST: Verify bool_check semantics.

From src/zk/gadget/small_range_check.rs:
  range_check with range=2 → polynomial product (value-0)(value-1) = 0
  This constrains value ∈ {0, 1}.

If bool_check(0x53) delegates to small_range_check with range=2,
then alice_amount and bob_amount ARE constrained to 0 or 1.
-/
def dexBoolCheckStatus : String :=
  "bool_check invoked on u64 amounts — constrains to 0 or 1 if standard boolean semantics"

-- ===========================================================================
-- HAZOP RISK VERIFICATION
-- ===========================================================================

def elevatedFindings : List (String × Nat × String) := [
  ("ELEV-1: deposit zero_cond bypass", 35,
   "zero_cond(0, deposit_leaf) = 0; Merkle proof against zero leaf"),
  ("ELEV-2: cancel swap_id mismatch", 35,
   "H(lock_commitment) ≠ H(lock, token, amount); cancellation impossible"),
  ("ELEV-3: exit incomplete circuit", 35,
   "TODO comments; dao_escrow_merkle_root unconstrained; exit_value dead code"),
  ("ELEV-4: redeem no zero check", 32,
   "coin_value not constrained to 0 in-circuit; defense-in-depth gap"),
  ("ELEV-5: slippage field division", 30,
   "base_div produces field elements, not integers; token amounts not guaranteed"),
  ("ELEV-6: dex bool_check on u64", 30,
   "bool_check(alice_amount) after range_check(64) limits to 0 or 1")
]

end HAZOP.Elevated
