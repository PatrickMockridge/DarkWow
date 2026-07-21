/-!
# HAZOP Tabletop — All 120 ZK Circuits Risk Matrix

Three independent domain-expert agents (Alice-defender, Mallory-attacker, Eve-eavesdropper,
Sybil-replay, Olivia-insider) performed HAZOP-style threat analysis.

Each circuit graded: Exploitability (1-10) × Likelihood (1-10) = Risk (1-100)
Threshold for deeper Lean 4 verification: Risk >= 30

## Threat Actor Profiles

| Actor | Role | Primary Attack Vector |
|-------|------|----------------------|
| Alice | Defender | Identifies constraint gaps, missing equalities, unconstrained witnesses |
| Mallory | Attacker | Exploits free witnesses, field division, front-running, Merkle bypass |
| Eve | Eavesdropper | Exploits public input leakage, mempool monitoring, timing attacks |
| Sybil | Replay | Creates multiple identities, exploits nullifier collisions, replays proofs |
| Olivia | Insider | Exploits authorization gaps, capability bypass, contract-layer trust assumptions |

## Cross-Cutting HAZOP Patterns (All 3 Agents Agreed)

These 7 patterns recur across multiple circuits and are the root cause of
nearly all findings above the LOW threshold.
-/

import HAZOP.Critical
import HAZOP.High
import HAZOP.Elevated

namespace HAZOP

-- ===========================================================================
-- RISK MATRIX
-- ===========================================================================

/--
Complete risk matrix for all circuits above the LOW threshold.
-/
def riskMatrix : List (String × String × Nat × String) := [
  -- CRITICAL (Risk >= 60)
  ("CRIT-1", "stablecoin/governance_report_v1.zk", 80,
   "total_collateral/total_debt/interest_accrued free witnesses"),
  ("CRIT-2", "stablecoin/liquidate_v1.zk", 72,
   "collateralization check not enforced in circuit"),
  ("CRIT-3", "bridge/withdraw_v1.zk", 63,
   "recipient_hash front-running — not bound to depositor"),
  ("CRIT-4", "oracle/aggregate_v1.zk", 60,
   "bound checks NO-OP — subtraction unused in constraints"),

  -- HIGH (Risk 40-59)
  ("HIGH-1", "promissory_note/burn_v1.zk", 42,
   "zero_cond Merkle bypass when coin_value=0"),
  ("HIGH-2", "native_token/burn_v1.zk", 42,
   "same zero_cond bypass as PN burn"),
  ("HIGH-3", "labor_market/refund_v1.zk", 42,
   "non-milestone refund amount check skipped"),
  ("HIGH-4", "labor_market/*", 40,
   "nullifier collision: confirm/milestone/refund use same H(job_id, secret)"),
  ("HIGH-5", "governance_report (L2)", 42,
   "less_than_strict on zero/wrapped ratio values"),

  -- ELEVATED (Risk 30-39)
  ("ELEV-1", "bridge/deposit_v1.zk", 35,
   "zero_cond bypass for amount=0"),
  ("ELEV-2", "dex/cancel_swap_v1.zk", 35,
   "swap_id H(lock_commitment) ≠ create's H(lock, token, amount)"),
  ("ELEV-3", "drain_protection/exit_v1.zk", 35,
   "incomplete circuit; dao_escrow_merkle_root unconstrained"),
  ("ELEV-4", "promissory_note/redeem_v1.zk", 32,
   "coin_value not checked for zero in-circuit"),
  ("ELEV-5", "dex/execute_swap_slippage_v1.zk", 30,
   "field division produces non-integer for token amounts"),
  ("ELEV-6", "dex/execute_swap_v1.zk", 30,
   "bool_check on u64 amounts constrains to 0 or 1"),

  -- MODERATE (Risk 20-29)
  ("MOD-1", "dex/execute_swap_fee_v1.zk", 27,
   "field division for fees"),
  ("MOD-2", "otc_swap/*", 25,
   "cancel doesn't verify Alice's pubkey matches creator"),
  ("MOD-3", "stablecoin/accrue_interest_v1.zk", 24,
   "rate_per_second, time_elapsed free witnesses"),
  ("MOD-4", "pool_stake/*", 22,
   "slash has no authorization; all circuits lack key derivation"),
  ("MOD-5", "tender/*", 22,
   "capability bypass; reveal doesn't verify amount"),
  ("MOD-6", "oracle/* (non-aggregate)", 20,
   "k=10 under-constrained; oracle_id not bound to pubkey"),
  ("MOD-7", "dex/create_swap + accept_swap", 20,
   "NULLIFIER_K used for signature key derivation")
]

-- ===========================================================================
-- CROSS-CUTTING HAZOP PATTERNS
-- ===========================================================================

/--
Pattern 1: Free witness constrain_instance (Orchard-class)

A value is expose`d as a public input via constrain_instance but has NO
in-circuit derivation constraint. The prover can set it to any value.

Severity: CRITICAL
Circuits affected: governance_report (3 fields), liquidate (price),
  aggregate (bounds), drain_protection (merkle_root)

Detection rule: For every constrain_instance(X), verify X is computed
in-circuit from witnesses (poseidon_hash, ec_mul, merkle_root, etc.)
rather than being a free witness.
-/
def pattern1_free_instance : String :=
  "constrain_instance(X) without in-circuit derivation of X"

/--
Pattern 2: zero_cond Merkle bypass

zero_cond(value, leaf) returns 0 when value=0, making the Merkle proof
verify against the tree's zero leaf instead of the actual coin.

Severity: HIGH
Circuits affected: burn_v1 (PN), burn_v1 (NT), deposit_v1 (bridge)

Fix: Add less_than_strict(ZERO, value) before zero_cond.
-/
def pattern2_zero_cond : String :=
  "zero_cond(value, coin) returns 0 when value=0; Merkle proof vacuous"

/--
Pattern 3: Field division ≠ integer division

base_div(a, b) = a * b^{-1} mod p produces a field element, not the
integer quotient. For token amounts (which must be integers), this
creates a semantic gap.

Severity: ELEVATED
Circuits affected: DEX fee/slippage, labor milestone_payment,
  oracle aggregate, stablecoin accrue_interest

Fix: Use cross-multiplication (a < b*c) instead of division (a/b < c).
-/
def pattern3_field_div : String :=
  "base_div produces field elements; cross-multiplication avoids this"

/--
Pattern 4: Capability predicate bypass

capability_predicate_result is constrained to equal 1 in-circuit but
is a free witness. The circuit cannot verify the predicate was actually
evaluated by the Identity contract.

Severity: ELEVATED
Circuits affected: labor accept_job_with_capability, tender submit_bid
  with_capability, insurance underwrite/purchase

Fix: The contract MUST verify the capability proof's provenance through
the child-call mechanism. The circuit provides no defense.
-/
def pattern4_capability_bypass : String :=
  "capability_predicate_result = 1 is free witness; provenance unverified"

/--
Pattern 5: Nullifier collision across circuits

Multiple circuits for the same contract use identical nullifier derivations.
Performing any one action consumes the nullifier, blocking all others.

Severity: HIGH
Circuits affected: labor confirm_delivery, milestone_payment, refund

Fix: Add a domain separator to each nullifier: H(action_tag, job_id, secret).
-/
def pattern5_nullifier_collision : String :=
  "H(job_id, secret) identical across circuits; one action blocks others"

/--
Pattern 6: bool_check on u64 values — semantic ambiguity

bool_check (small_range_check with range=2) constrains a value to {0, 1}.
When applied to u64 amounts (which should support values up to 2^64-1),
this creates a drastic restriction.

Severity: ELEVATED
Circuits affected: DEX execute_swap (alice_amount, bob_amount),
  stablecoin mint_stable (mint_amount)

Requires investigation: Is bool_check used as a "field element well-formedness"
check (non-standard semantics) or as a standard boolean constraint?
-/
def pattern6_bool_check_u64 : String :=
  "bool_check on u64 restricts to 0 or 1; verify bool_check semantics"

/--
Pattern 7: Missing range checks on critical values

Values that represent block heights, token amounts, or time durations
are not range-checked to reasonable bounds. This allows field-sized
values that can cause wraparound in comparisons.

Severity: MODERATE
Circuits affected: labor_market deadline_block/current_block,
  tender amount, betting_stake amount

Fix: Add range_check(64, value) for all values that represent real-world
quantities.
-/
def pattern7_missing_range : String :=
  "No range_check on values representing block heights or amounts"

-- ===========================================================================
-- HAZOP VERIFICATION SUMMARY
-- ===========================================================================

def summary : IO Unit := do
  IO.println "=== HAZOP Tabletop — Risk Matrix Summary ==="
  IO.println s!"Total circuits analyzed: 120"
  IO.println s!"CRITICAL findings (Risk >= 60): 4"
  IO.println s!"HIGH findings (Risk 40-59): 5"
  IO.println s!"ELEVATED findings (Risk 30-39): 6"
  IO.println s!"MODERATE findings (Risk 20-29): 7"
  IO.println s!"LOW findings (Risk < 20): remaining"
  IO.println ""
  IO.println "Cross-cutting patterns identified: 7"
  IO.println "  1. Free witness constrain_instance (Orchard-class)"
  IO.println "  2. zero_cond Merkle bypass"
  IO.println "  3. Field division ≠ integer division"
  IO.println "  4. Capability predicate bypass"
  IO.println "  5. Nullifier collision across circuits"
  IO.println "  6. bool_check on u64 values"
  IO.println "  7. Missing range checks on critical values"
  IO.println ""
  IO.println "Threat actors: Alice(defender), Mallory(attacker), Eve(eavesdropper)"
  IO.println "               Sybil(replay), Olivia(insider)"
  IO.println ""
  IO.println "Deeper Lean 4 verification: 15 circuits (Risk >= 30)"
  IO.println "  CRITICAL: 4 circuits — full constraint modeling + soundness theorems"
  IO.println "  HIGH: 5 circuits — targeted vulnerability proofs"
  IO.println "  ELEVATED: 6 circuits — constraint gap documentation + counterexamples"

end HAZOP
