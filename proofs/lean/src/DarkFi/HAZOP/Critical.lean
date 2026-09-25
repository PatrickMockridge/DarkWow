/-!
MANUAL AUDIT DOCUMENTATION — NOT FORMAL PROOFS
This file contains structured vulnerability findings / circuit audit
results. It contains ZERO Lean theorems with non-trivial proofs.
All defs return String or List values for programmatic consumption.
-/
/-!
# HAZOP CRITICAL Tier — Formal Vulnerability Proofs (Risk >= 60)

Four circuits with confirmed exploit vectors requiring immediate deeper formal verification.

## CRIT-1: governance_report_v2.zk (Risk 80/100)
`total_collateral`, `total_debt`, `interest_accrued` are labeled "Public inputs" but NEVER
`constrain_instance`'d. Any values pass the circuit.

## CRIT-2: liquidate_v2.zk (Risk 72/100)
Price/ratio check computed but NEITHER constrained NOR compared. Liquidation condition
not enforced — any position is liquidatable regardless of collateralization.

## CRIT-3: withdraw_v2.zk (Risk 63/100)
`recipient_hash` is a free witness not bound to depositor identity. Front-running attack
steals in-flight withdrawals by changing the recipient.

## CRIT-4: aggregate_v2.zk (Risk 60/100)
Boundary checks `min_result <= result <= max_result` are NO-OP — subtraction computed
but never used in any constraint.
-/

namespace HAZOP.Critical

-- ===========================================================================
-- CRIT-1: governance_report_v2.zk — Free Public Inputs
-- ===========================================================================

/-
Models the governance_report_v2.zk circuit constraint system.

The circuit claims to verify that a governance report's values match on-chain state.
However, `total_collateral`, `total_debt`, and `interest_accrued` are labeled as
"Public inputs" in the source comment but are NEVER `constrain_instance`'d.

Only `collateral_ratio_bps` IS derived and constrained. The other three fields
are free witnesses — any values pass the circuit.
-/
structure GovernanceReportCircuit where
  total_collateral : Int    -- labeled "Public input" but NOT constrain_instance'd
  total_debt : Int          -- labeled "Public input" but NOT constrain_instance'd
  interest_accrued : Int    -- labeled "Public input" but NOT constrain_instance'd
  collateral_ratio_bps : Int -- ACTUALLY constrained (line 47)
  report_timestamp : Int    -- labeled "Public input" but NOT constrain_instance'd
  reporter_pub_x : Int      -- constrained via signature
  reporter_pub_y : Int      -- constrained via signature

/-
THEOREM (CRIT-1): governance_report_v2.zk accepts arbitrary total_collateral.

Since `total_collateral` is never `constrain_instance`'d, the prover can set it
to ANY value and the circuit will accept it.

This means Mallory can submit a governance report claiming:
  total_collateral = 10_000_000  (fabricated)
  total_debt = 100               (fabricated)
  collateral_ratio_bps = 100_000 (computed from fabricated values)
And the circuit accepts it — none of the raw values are bound to public inputs.
-/
def governance_report_free_total_collateral : String :=
  "CRIT-1: total_collateral/total_debt/interest_accrued NOT constrain_instance'd. Any values pass."

/-
THEOREM (CRIT-1): The only constrained public input is collateral_ratio_bps.

The circuit computes `collateral_ratio_bps = base_div(total_collateral, total_debt)`
and constrains it. But since both inputs are free, the ratio can be anything.
-/
/-- The parameter is gone: it was never read, and a `def f (c : Circuit) : …` whose body ignores
    `c` reads as though the answer were computed from the model. It is a constant list about the
    circuit named in the heading above, so it is declared as one.

    Note this question — which fields a circuit actually `constrain_instance`s — is now answered
    mechanically over the real `.zk` sources by `script/circuit_instance_derivation.py`; this list
    is the manual-audit record for this file and is kept as such. -/
def governance_report_constrained_fields : List String :=
  ["collateral_ratio_bps"]  -- Only this field is actually constrain_instance'd

/-
THEOREM (CRIT-1): Fields that SHOULD be constrain_instance'd but AREN'T:

- total_collateral (line 13: comment says "Public input")
- total_debt (line 14: comment says "Public input")
- interest_accrued (line 17: comment says "Public input")
- report_timestamp (line 15: comment says "Public input")
-/
def governance_report_unconstrained_fields : List String :=
  ["total_collateral", "total_debt", "interest_accrued", "report_timestamp"]

/-
THEOREM (CRIT-1 fix): All four fields must be `constrain_instance`'d.

The entrypoint (`process_governance_report_instruction`) reads these values
from the on-chain config DB and verifies them against the reported values.
For the ZK proof to be meaningful, the circuit MUST expose these as public
inputs so the host can verify they match the metadata.

Fix pattern:
  constrain_instance(total_collateral)
  constrain_instance(total_debt)
  constrain_instance(interest_accrued)
  constrain_instance(report_timestamp)
-/

-- ===========================================================================
-- CRIT-1 (Layer 2): governance_report_v2.zk — Division by zero risk
-- ===========================================================================

/-
THEOREM (CRIT-1 L2): `less_than_strict(15000, collateral_ratio_bps)` operates on
potentially zero or wrapped values.

If `total_debt = 0` (free witness), then `collateral_ratio_bps = total_collateral / 0`
is undefined. `base_div` returns 0 for division by zero, making
`collateral_ratio_bps = 0`. Then `less_than_strict(15000, 0)` in the field is
`15000 < 0` which wraps around to a large value — the comparison result depends
on field arithmetic, not integer arithmetic.

This means: when total_debt = 0, the ratio check can pass or fail unpredictably.
-/
def governance_report_division_by_zero : String :=
  "CRIT-1 L2: total_debt=0 → collateral_ratio_bps=0 → less_than_strict check wraps in field arithmetic"

-- ===========================================================================
-- CRIT-2: liquidate_v2.zk — Missing Collateralization Check
-- ===========================================================================

/-
Models the liquidate_v2.zk circuit. The circuit computes `debt_value` and
`collateral_value` but NEVER compares them. The comment at lines 58-64 says
the ratio check should use `less_than_strict` but this is NOT executed.

Only `bool_check(debt_amount)` is called — this constrains `debt_amount` to 0 or 1,
but does NOT verify the position is undercollateralized.
-/
structure LiquidateCircuit where
  debt_amount : Int
  collateral_amount : Int
  current_price : Int
  liquidator_reward : Int
  new_collateral : Int      -- = base_sub(collateral_amount, liquidator_reward)
  debt_value : Int          -- = base_mul(debt_amount, current_price) — NEVER USED
  collateral_value : Int    -- = base_mul(collateral_amount, 10000) — NEVER USED

/-
THEOREM (CRIT-2): The liquidation circuit does NOT enforce undercollateralization.

Mallory can liquidate ANY position, including healthy ones with 500% collateralization.
The circuit provides ZERO protection against this.

The entrypoint is the ONLY defense — it must independently verify the position's
collateralization ratio before accepting the liquidation proof.
-/
def liquidate_no_collateralization_check : String :=
  "CRIT-2: debt_value and collateral_value computed but never compared; no undercollateralization enforcement"

/-
THEOREM (CRIT-2): The missing constraint should be:

  collateral_value_lt = less_than_strict(collateral_value, threshold_times_debt)
  constrain_equal_base(collateral_value_lt, ONE)

Where:
  threshold_times_debt = base_mul(debt_value, LIQUIDATION_THRESHOLD)
  LIQUIDATION_THRESHOLD is the contract's liquidation threshold (e.g., 15000 = 150%)

Without this constraint, the ZK proof provides no guarantee of undercollateralization.
-/
/-- Same as `governance_report_constrained_fields` above: the `(c : LiquidateCircuit)` parameter was
    never read, so it is gone rather than left implying the string were derived from the model. -/
def liquidate_missing_constraint : String :=
  "less_than_strict(collateral_value, base_mul(debt_value, LIQUIDATION_THRESHOLD))"

/-
THEOREM (CRIT-2 L2): `current_price` is a free witness.

Mallory can set `current_price = 0` (or any value) and the circuit accepts it.
The entrypoint must verify `current_price` against an oracle. The circuit provides
no oracle binding.

Even if the collateralization check WERE enforced, Mallory could still manipulate
`current_price` to make any position appear undercollateralized.
-/
def liquidate_free_price : String :=
  "CRIT-2 L2: current_price is free witness; no oracle binding; Mallory can set price=0"

-- ===========================================================================
-- CRIT-3: withdraw_v2.zk — Recipient Hash Front-Running
-- ===========================================================================

/-
Models the withdraw_v2.zk circuit. The `recipient_hash` is a free witness.
`derived_recipient = poseidon_hash(recipient_hash)` is `constrain_instance`'d,
but `recipient_hash` is not bound to the depositor's identity.

Attack: Mallory monitors the mempool for pending withdrawals. She extracts the
nullifier and deposit_leaf (public inputs), then creates a new proof with
`recipient_hash = Mallory_address`. The bridge contract accepts her proof
because the circuit only proves knowledge of the secret, not that the recipient
matches the original depositor's intent.
-/
structure WithdrawCircuit where
  secret : Int
  amount : Int
  recipient_hash : Int       -- FREE WITNESS — not bound to anything
  nullifier : Int            -- = poseidon_hash(secret)
  deposit_leaf : Int         -- = poseidon_hash(secret, amount)
  derived_recipient : Int    -- = poseidon_hash(recipient_hash)

/-
THEOREM (CRIT-3): recipient_hash front-running IS possible.

The circuit has NO constraint binding `recipient_hash` to the depositor or
to any other circuit value. Any prover who knows `(secret, amount)` can
choose ANY `recipient_hash` and produce a valid proof.

This is a real-time attack: Mallory watches the mempool, extracts the
nullifier/deposit_leaf, creates a new proof with her own recipient_hash,
and submits with higher priority. The original withdrawal fails (nullifier
is now spent), and Mallory receives the funds.
-/
def withdraw_recipient_front_running_possible : String :=
  "CRIT-3: recipient_hash is free witness; front-running attack steals in-flight withdrawals"

/-
THEOREM (CRIT-3 fix): Bind recipient_hash to the nullifier derivation.

Fix: nullifier = poseidon_hash(secret, recipient_hash)

This makes the nullifier SPECIFIC to the recipient. If Mallory changes
`recipient_hash`, the nullifier changes, and her proof would produce
a different nullifier than the one registered in the bridge contract.

The original depositor creates: nullifier = H(secret, intended_recipient)
Mallory tries to create:     nullifier' = H(secret, Mallory_address)
Since intended_recipient ≠ Mallory_address, nullifier' ≠ nullifier,
and Mallory's proof would try to spend a nullifier that doesn't match
any registered withdrawal. The attack fails.
-/
def withdraw_recipient_binding_fix : String :=
  "nullifier = poseidon_hash(secret, recipient_hash)"

/-
THEOREM (CRIT-3): Without the fix, the attack success rate is 100%.

Mallory needs:
  1. Access to the mempool (trivial — she runs a node)
  2. The nullifier from the pending transaction (public input — visible in tx)
  3. Knowledge of the deposit_leaf (public input — visible in tx)
  4. Her own external chain address (trivial)

She does NOT need:
  - The depositor's secret (nullifier already published)
  - The depositor's private key
  - Any authorization from the depositor

The attack is permissionless once the transaction is in the mempool.
-/

-- ===========================================================================
-- CRIT-4: aggregate_v2.zk — Boundary Check NO-OP
-- ===========================================================================

/-
Models the oracle aggregate_v2.zk circuit. The circuit computes:
  diff_max = base_sub(max_result, result)
  diff_min = base_sub(result, min_result)

But NEITHER `diff_max` nor `diff_min` is used in ANY constraint afterward.
The subtraction is computed and discarded — it serves no purpose.

The circuit exposes `min_result` and `max_result` as `constrain_instance`'d
public inputs, but NEVER verifies that `result` is within these bounds.
-/
structure AggregateCircuit where
  result : Int           -- computed weighted average
  min_result : Int       -- constrain_instance'd but never compared to result
  max_result : Int       -- constrain_instance'd but never compared to result
  diff_max : Int         -- = base_sub(max_result, result) — COMPUTED BUT IGNORED
  diff_min : Int         -- = base_sub(result, min_result) — COMPUTED BUT IGNORED
  sum_weights : Int      -- correctly constrained

/-
THEOREM (CRIT-4): The boundary checks are NO-OP.

`diff_max` and `diff_min` are computed via `base_sub` but never appear in
any `constrain_equal_base`, `constrain_instance`, or `range_check`.
They are tokent code — computed and discarded.

The circuit provides ZERO guarantee that `min_result <= result <= max_result`.
-/
def aggregate_bound_checks_noop : String :=
  "CRIT-4: diff_max/diff_min computed via base_sub but never constrained; boundary checks are NO-OP"

/-
THEOREM (CRIT-4): What the circuit SHOULD enforce:

  -- Assert result <= max_result (i.e., max_result - result >= 0 in the field)
  diff_max = base_sub(max_result, result)
  range_check(64, diff_max)  -- ensures max_result >= result (non-negative difference)

  -- Assert result >= min_result (i.e., result - min_result >= 0)
  diff_min = base_sub(result, min_result)
  range_check(64, diff_min)  -- ensures result >= min_result

Or equivalently:
  within_bounds = less_than_or_equal(min_result, result)
  constrain_equal_base(within_bounds, ONE)
  within_bounds2 = less_than_or_equal(result, max_result)
  constrain_equal_base(within_bounds2, ONE)
-/
def aggregate_bound_check_fix : String :=
  "range_check(64, diff_max) ∧ range_check(64, diff_min)"

/-
THEOREM (CRIT-4): Mallory can submit an aggregate with result = 0 or result = p-1
regardless of the actual oracle values. The bound check does nothing to stop her.
-/

-- ===========================================================================
-- HAZOP RISK VERIFICATION: Run these checks
-- ===========================================================================

/-
Returns the HAZOP critical findings as structured data for the test suite.
-/
def criticalFindings : List (String × Nat × String) := [
  ("CRIT-1: governance_report free instances", 80,
   "total_collateral/total_debt/interest_accrued/report_timestamp not constrain_instance'd"),
  ("CRIT-2: liquidate no collateralization check", 72,
   "debt_value and collateral_value computed but never compared"),
  ("CRIT-3: withdraw recipient front-running", 63,
   "recipient_hash is free witness; front-running steals withdrawals"),
  ("CRIT-4: aggregate bound checks NO-OP", 60,
   "diff_max/diff_min computed but never constrained")
]

-- ===========================================================================
-- THE ONE LOUD ASSUMPTION (CRIT-5)
-- ===========================================================================
--
-- Same guideword as the other tiers: if this is false, does anything fail loudly or silently?
-- This is the only assumption in the tree where the answer is unambiguously "loudly" — because
-- it is the only one whose consumer cites it by name in a proof term.

/-- CRIT-5: `purseNullifier_nonce_injective`. Its consumer is
    `Capability.purse_chained_nullifiers_distinct`, which cites it directly:

      have h' := purseNullifier_nonce_injective c.ownerSecret c.purseId
        c.depositNonce (c.depositNonce + 1) h

    If the assumption is false, that proof term does not type-check. So this is the one entry
    in this pass that is genuinely LOUD, and the one assumption whose `IF FALSE:` field names a
    theorem rather than recording a silence.

    It is also the clearest case for discharging: `purseNullifier` is defined as an opaque
    `poseidon(1, owner_secret, purse_id, nonce)`, and `HashOps.poseidon_collision_resistance` is
    stated as injectivity over arbitrary lists. Defining the former in terms of the latter
    makes this a `theorem` with `Axioms` still the only file containing assumptions — the
    assumption count drops by one and the loud consumer keeps its proof. -/
def purseNonceInjectivityStatus : String :=
  "CRIT-5: LOUD. consumer: Capability.purse_chained_nullifiers_distinct (cites it by name)"

/-- CRIT-6: `Axioms.pallasPrime` was **FALSE** until 2026-09-20, because `PALLAS_MODULUS` was a
    composite number.

    This is not the "unproved assumption" class the HAZOP tiers were built around. An unproved
    assumption leaves its consumers *conditional*; a **false** one leaves them *vacuous*. The
    budget table cannot tell the two apart — both show a non-zero budget — and that is exactly the
    blind spot this whole pass exists to close, so it is worth the top risk band.

    What went wrong: `Axioms.PALLAS_MODULUS` was `2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1`, which
    is divisible by 3. The real Pallas modulus is
    `0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`. Four files
    (`Axioms.lean`, `Arithmetic.lean`, `Field.lean`, `Main.lean`) spelled the same wrong expression
    independently, so they agreed with each other and disagreed with Pallas.

    Blast radius: `instance : Fact (Nat.Prime PALLAS_MODULUS)` derives `ZMod PALLAS_MODULUS`'s
    `Field` structure from the axiom, so every theorem in `Pedersen.lean` rested on a falsehood.
    `Arithmetic.base_div_mul_cancel` needs the same fact.

    How it survived: `Pedersen.lean` said the curve had been "verified against the vendored
    implementation", which was true of the *generator* and false of the *modulus*, and nothing tied
    the Lean constant to the published one. That tie now exists —
    `Pedersen.pallasModulus_eq_pasta_curves`, budget 0 — and so does the evidence for the
    correction, `Pedersen.oldPallasModulus_was_composite`, which exhibits 3 as a divisor. -/
def pallasModulusWasCompositeStatus : String :=
  "CRIT-6: FIXED. `PALLAS_MODULUS` was composite, so `pallasPrime` was false and the Pedersen curve was vacuous; modulus corrected, tie to pasta_curves added, compositeness of the old value proved"

/-- CRIT-7: the actor's public key is not bound to the proof, in two genesis contracts.

    Found by following the OBL-Z1 residue — the 33 unclassified `constrain_instance` sites — into
    the entrypoints. The residue said "a human must look here"; `oracle` and `multisig` are what the
    human found, and both are in `GENESIS_CONTRACT_NAMES`.

    The shape is one shape. An authorization circuit does

        oracle_pub   = ec_mul_base(oracle_secret, NULLIFIER_K);
        derived_pub_x = ec_get_x(oracle_pub);
        constrain_equal_base(derived_pub_x, oracle_pub_x);   -- witness == witness
        constrain_instance(oracle_id);
        -- oracle_pub_x is declared `witness` and is NEVER exposed

    so the equality holds for *any* secret: the proof says "I know some curve secret", not "I am the
    registered oracle". The missing link is host-side too — `PushValueParamsV1` is
    `{proof, oracle_id, value, tx_binding, tx_nonce}` and carries no pubkey to compare.

    `oracle`: `push_value_v1` looks the oracle up, checks `is_active`, then writes
    `oracle.value = params.value`. Anyone can push any value to any registered oracle. And
    `set_oracle_active_v1` has *no circuit at all* ("Non-ZK function, no public inputs") with an
    only check that compares a prover-supplied copy of a **public** key against state.

    `multisig`: subtler, because the check that exists is correct.
    `mod.rs:303` rejects a signer not in `group.pubkeys` — but `params.signer_pub` is instruction
    data the proof does not bind, and the nullifier
    `poseidon_hash([group_id, msg_hash, pk_x, pk_y])` is built from that same claimed key. A
    non-member copies a member's public key, proves knowledge of its own secret, passes, and spends
    the member's nullifier — denial, not theft: the real member can never sign that message.

    These are not Lean findings and no Lean theorem is affected. They are recorded here because
    this file is where the critical findings live, and because the mechanism that surfaced them —
    `script/circuit_instance_derivation.py` reporting "unclassified" rather than "safe" — is the one
    the Lean work is written against.

    **Both halves were repaired after this file recorded them, and the register's rows are the
    evidence.** `oracle` was closed 2026-09-21: every authorizing circuit derives the operator
    commitment from the secret and exposes it (`push_value.zk:69`, domain `witness_base(8)`, and all six
    circuits carry the same derivation), the
    host compares it against the stored record, and `set_oracle_active` gained the circuit it lacked —
    `OBL-Z9`/`OBL-Z10`, both `CLOSED`. `multisig` was closed 2026-09-22 and was more severe than the
    paragraph above says: the attacker repeating the claim over the group's other members *forged
    threshold approval*, not merely blocked one signer — `sign.zk` now derives and exposes
    `member_commitment = poseidon_hash(witness_base(4), signer_secret)` and the host checks it against
    the group's set (`multisig/src/entrypoint/mod.rs:322`) — `OBL-Z11`, `CLOSED`. The analysis above is
    kept as it was found, which is what this file is for; `CRIT-7`'s own entry below carries the
    status. -/
def actorKeyNotBoundToProofStatus : String :=
  "CRIT-7: oracle (4 circuits) and multisig (2 circuits) authorize an actor by comparing a derived pubkey to an unexposed WITNESS; the proof binds no key, and neither ParamsV1 carries one. oracle: any value pushable to any registered oracle, and set_oracle_active is non-ZK and copy-keyable. multisig: a non-member can claim a member's key and spend their nullifier, blocking them"

def criticalAxiomFindings : List (String × Nat × String) := [
  ("CRIT-5: purseNullifier_nonce_injective", 70,
   "LOUD. The only assumption with a consumer that names it; falsity breaks purse_chained_nullifiers_distinct"),
  ("CRIT-6: pallasPrime was FALSE", 90,
   "FALSE, not merely unproved: PALLAS_MODULUS was divisible by 3, and Fact (Nat.Prime …) made ZMod PALLAS_MODULUS a Field from it, so all of Pedersen.lean was vacuous. Fixed; the tie to pasta_curves and a proof of the old value's compositeness are now in the tree"),
  ("CRIT-7: the actor's key is not bound to the proof", 85,
   "oracle and multisig — both genesis contracts — authorized by comparing a derived pubkey against an unexposed witness, which held for any secret, and no host-side check supplied the missing link. REPAIRED: oracle 2026-09-21, multisig 2026-09-22, each by deriving the actor's commitment from the secret and exposing it for the host to compare against stored state — OBL-Z9, OBL-Z10 and OBL-Z11 are CLOSED, and the multisig half was the more severe of the two (a non-member could forge threshold approval, not only block one signer). Recorded as the shape that was found; nothing in it is open")
]

end HAZOP.Critical
