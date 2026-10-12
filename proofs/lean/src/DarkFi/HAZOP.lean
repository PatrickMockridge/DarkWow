/-
MANUAL AUDIT DOCUMENTATION — NOT FORMAL PROOFS
This file contains structured vulnerability findings / circuit audit
results. It contains ZERO Lean theorems with non-trivial proofs.
All defs return String or List values for programmatic consumption.

The circuit paths and counts in `riskMatrix` and `summary` are a *snapshot* of the
2026 audit; circuit inventories move and this file does not. The normative sources are:

  * root causes          — doc/src/dev/contracts/safety.md, RC1-RC12
  * open obligations     — doc/src/arch/verification-hazop.md (the register)

Each patternN_* below names the root cause it is an instance of. The names are stable
because the register's alias table cites them; only the strings change.
-/
/-
# HAZOP Tabletop — Circuit Risk Matrix

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
nearly all findings above the LOW threshold. Seven names, seven instances — but
four root causes: RC1 (patterns 4), RC2 (1, 2, 6, 7), RC3 (5), RC4 (3).
-/

import DarkFi.HAZOP.Critical
import DarkFi.HAZOP.High
import DarkFi.HAZOP.Elevated

namespace HAZOP

-- ===========================================================================
-- RISK MATRIX
-- ===========================================================================

/--
Complete risk matrix for all circuits above the LOW threshold.
-/
def riskMatrix : List (String × String × Nat × String) := [
  -- Paths are relative to `src/contract/`. They were written as
  -- `stablecommitment/…_v2.zk` in the 2026 audit snapshot, and **fourteen of the fourteen
  -- distinctive ones failed to resolve** — no `stablecommitment/` directory has ever existed, and
  -- no circuit file carries a `_v2` suffix (the version lives inside the file; see
  -- `doc/src/arch/circuit-versioning.md` §7). Corrected 2026-10-12 (`F-136`), each path checked
  -- against the tree. The risk numbers and the findings are the snapshot's and are not revised:
  -- `Main.test_hazop_summary` asserts the band counts, and `F-137`–`F-155` record, per finding,
  -- what the circuit carries *now*.
  -- CRITICAL (Risk >= 60)
  ("CRIT-1", "stablecoin/proof/governance_report.zk", 80,
   "total_collateral/total_debt/interest_accrued free witnesses — FIXED: `reporter_pub_x/y` are `constrain_instance`d (OBL-Z14), so the equalities compare against the verifier's values"),
  ("CRIT-2", "stablecoin/proof/liquidate.zk", 72,
   "collateralization check not enforced in circuit — FIXED: the file carries a `HAZOP CRIT-2 FIX` guard"),
  ("CRIT-3", "bridge/proof/withdraw.zk", 63,
   "recipient_hash front-running — FIXED: `computed_nullifier = poseidon_hash(DOMAIN_NULLIFIER, secret, recipient_hash)`"),
  ("CRIT-4", "oracle/proof/aggregate.zk", 60,
   "bound checks NO-OP — FIXED: `range_check`ed under a `HAZOP CRIT-4 FIX` guard"),

  -- HIGH (Risk 40-59)
  ("HIGH-1", "promissory_note/proof/revoke.zk", 42,
   "zero_cond Merkle bypass when commitment_value=0 — the finding names a PN `burn` circuit; PN has none. `revoke.zk` is the spend circuit, and no `zero_cond` bypass remains there"),
  ("HIGH-2", "native_token/proof/burn.zk", 42,
   "same zero_cond bypass — FIXED: `less_than_strict(ZERO, value)` guards the conditional"),
  ("HIGH-3", "labor_market/proof/refund.zk", 42,
   "non-milestone refund amount check skipped — the described `cond_select` branch no longer exists; the residual is `OBL-C96`"),
  ("HIGH-4", "labor_market/proof/{confirm_delivery,submit_deliverable,submit_git_deliverable,refund}.zk", 40,
   "nullifier collision across actions — FIXED: distinct action tags (`HAZOP HIGH-4` guard)"),
  ("HIGH-5", "stablecoin/proof/governance_report.zk", 42,
   "less_than_strict on zero/wrapped ratio values — premise gone: the free `total_debt` and the 15000 floor were both removed (`OBL-C204`)"),

  -- ELEVATED (Risk 30-39)
  ("ELEV-1", "bridge/proof/deposit.zk", 35,
   "zero_cond bypass for amount=0 — neither `zero_cond` nor the Merkle proof the finding describes is in the file"),
  ("ELEV-2", "dex/proof/cancel_swap.zk", 35,
   "incompatible `swap_id` derivations — FIXED: the two derivations are now identical"),
  ("ELEV-3", "drain_protection/proof/exit.zk", 35,
   "incomplete circuit; `dao_escrow_merkle_root` unconstrained — read against the file; the register's `OBL-C97` row names the residual"),
  ("ELEV-4", "promissory_note/proof/redeem.zk", 32,
   "commitment_value not checked for zero — FIXED: `constrain_equal_base(value, witness_base(0))` at `:58-59`"),
  ("ELEV-5", "dex/proof/execute_swap_slippage.zk", 30,
   "field division produces non-integer — no `base_div` remains; the tolerance is cross-multiplied"),
  ("ELEV-6", "dex/proof/execute_swap.zk", 30,
   "bool_check on u64 amounts — no `bool_check` remains; the file is a rewritten circuit"),

  -- MODERATE (Risk 20-29)
  ("MOD-1", "dex/proof/execute_swap_fee.zk", 27,
   "field division for fees — FIXED: an exact integer quotient-remainder (`OBL-Z13`)"),
  ("MOD-2", "otc_swap/proof/cancel_swap.zk", 25,
   "cancel doesn't verify Alice's pubkey matches creator"),
  ("MOD-3", "stablecoin/proof/accrue_interest.zk", 24,
   "rate_per_second, time_elapsed free witnesses — now range-checked (partly superseded)"),
  ("MOD-4", "pool_stake/proof/", 22,
   "slash has no authorization; all circuits lack key derivation"),
  ("MOD-5", "tender/proof/reveal_bid.zk", 22,
   "capability bypass; reveal doesn't verify amount — recorded twice, unlinked: this matrix's MOD-5 (risk 22) and a register ZK row at severity H"),
  ("MOD-6", "oracle/proof/", 20,
   "k=10 under-constrained; `oracle_id` not bound to pubkey"),
  ("MOD-7", "dex/proof/{create_swap,accept_swap}.zk", 20,
   "`NULLIFIER_K` used for signature key derivation — that constant is the tree-wide generator, not a signature-specific one")
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
  "constrain_instance(X) without in-circuit derivation of X  [RC2]"

/--
Pattern 2: zero_cond Merkle bypass

zero_cond(value, leaf) returns 0 when value=0, making the Merkle proof
verify against the tree's zero leaf instead of the actual commitment.

Severity: HIGH
Circuits affected: burn_v1 (PN), burn_v1 (NT), deposit_v1 (bridge)

Fix: Add less_than_strict(ZERO, value) before zero_cond.
-/
def pattern2_zero_cond : String :=
  "zero_cond(value, commitment) returns 0 when value=0; Merkle proof vacuous  [RC2]"

/--
Pattern 3: Field division ≠ integer division

base_div(a, b) = a * b^{-1} mod p produces a field element, not the
integer quotient. For token amounts (which must be integers), this
creates a semantic gap.

Severity: ELEVATED
Circuits affected: DEX fee/slippage, labor milestone_payment,
  oracle aggregate, stablecommitment accrue_interest

Fix: Use cross-multiplication (a < b*c) instead of division (a/b < c).
-/
def pattern3_field_div : String :=
  "base_div produces field elements; cross-multiplication avoids this  [RC4]"

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
  "capability_predicate_result = 1 is free witness; provenance unverified  [RC1]"

/--
Pattern 5: Nullifier collision across circuits

Multiple circuits for the same contract use identical nullifier derivations.
Performing any one action consumes the nullifier, blocking all others.

Severity: HIGH
Circuits affected: labor confirm_delivery, milestone_payment, refund

Fix: Add a domain separator to each nullifier: H(action_tag, job_id, secret).
-/
def pattern5_nullifier_collision : String :=
  "H(job_id, secret) identical across circuits; one action blocks others  [RC3]"

/--
Pattern 6: bool_check on u64 values — semantic ambiguity

bool_check (small_range_check with range=2) constrains a value to {0, 1}.
When applied to u64 amounts (which should support values up to 2^64-1),
this creates a drastic restriction.

Severity: ELEVATED
Circuits affected: DEX execute_swap (alice_amount, bob_amount),
  stablecommitment mint_stable (mint_amount)

Requires investigation: Is bool_check used as a "field element well-formedness"
check (non-standard semantics) or as a standard boolean constraint?
-/
def pattern6_bool_check_u64 : String :=
  "bool_check on u64 restricts to 0 or 1; verify bool_check semantics  [RC2]"

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
  "No range_check on values representing block heights or amounts  [RC2]"

-- ===========================================================================
-- HAZOP VERIFICATION SUMMARY
-- ===========================================================================

def summary : IO Unit := do
  IO.println "=== HAZOP Tabletop — Risk Matrix Summary ==="
  IO.println "This is a 2026 snapshot. The circuit inventory and per-circuit status are"
  IO.println "in doc/src/arch/verification-hazop.md, which is the authority; do not read"
  IO.println "a circuit count from here."
  IO.println ""
  IO.println s!"Findings recorded in this matrix (Risk >= 20): {riskMatrix.length}"
  IO.println ""
  IO.println "Cross-cutting patterns identified: 7, over 4 root causes"
  IO.println "  1. Free witness constrain_instance (Orchard-class)   [RC2]"
  IO.println "  2. zero_cond Merkle bypass                            [RC2]"
  IO.println "  3. Field division ≠ integer division                  [RC4]"
  IO.println "  4. Capability predicate bypass                        [RC1]"
  IO.println "  5. Nullifier collision across circuits                [RC3]"
  IO.println "  6. bool_check on u64 values                           [RC2]"
  IO.println "  7. Missing range checks on critical values             [RC2]"
  IO.println ""
  IO.println "Threat actors: Alice(defender), Mallory(attacker), Eve(eavesdropper)"
  IO.println "               Sybil(replay), Olivia(insider)"
  IO.println ""
  IO.println "Root causes: doc/src/dev/contracts/safety.md RC1-RC12"
  IO.println "Open obligations: doc/src/arch/verification-hazop.md"

end HAZOP
