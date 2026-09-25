# Security Analysis: Unofficial DarkWow Smart Contracts

*This document analyzes security issues in the non-standard smart contracts added to the DarkWow dev branch. These contracts are NOT part of the official DarkWow master and should be considered experimental.*

---

## Severity Ratings

| Rating | Description |
|--------|-------------|
| **CRITICAL** | Fund loss imminent or occurring |
| **MAJOR** | Significant vulnerability exploitable under specific conditions |
| **MODERATE** | Design weakness or missing functionality |
| **MINOR** | Informational, edge case, or hygiene issue |

---

## Contract-by-Contract Analysis

### Subscription Contract

#### Issue 1: Value Commitment Uses Poseidon Hash Instead of Pedersen (MAJOR) — FIXED

**Location**: subscribe_v1.zk

**Problem (Original)**: The deposit commitment used `poseidon_hash(deposit, value_blind, asset_id)` instead of proper Pedersen commitment.

**Fix Applied**: The circuit now implements proper Pedersen commitment verification:
```zk
vcv = ec_mul_short(deposit, VALUE_COMMIT_VALUE);
vcr = ec_mul(value_blind, VALUE_COMMIT_RANDOM);
value_commit_computed = ec_add(vcv, vcr);
constrain_equal_base(computed_x, value_commit_x);
constrain_equal_base(computed_y, value_commit_y);
```

**Impact** (resolved): The deposit amount is now properly hidden using standard Pedersen commitment. The value_blind cannot be derived from the commitment without solving the discrete log problem.

---

#### Issue 2: Permission Bitmask Checking is Absent (MAJOR) — FIXED (Tiered Approach)

**Location**: [verify_access.zk](../../../src/contract/subscription/proof/verify_access.zk)

**Problem (Original)**: The circuit did not enforce permission bitmask checking - any subscriber could claim any permission level.

**Fix Applied**: Tiered access approach using `less_than_strict`

The circuit now implements tiered permission checking:
```zk
# Tier definitions:
#   TIER_BASIC = 1    -> READ only
#   TIER_PREMIUM = 2  -> READ + WRITE
#   TIER_ADMIN = 3    -> READ + WRITE + ADMIN

# Current implementation: Check claimed_tier >= required_tier
# Uses less_than_strict(required_tier - 1, claimed_tier)
# This constrains: (required_tier - 1) < claimed_tier
less_than_strict(required_tier - 1, permissions_claimed);
```

**Impact** (Current implementation - privacy limitation):
- Permission tier IS revealed on-chain (privacy regression vs true zero-knowledge)
- Tier 1 (BASIC), Tier 2 (PREMIUM), or Tier 3 (ADMIN) is exposed
- Cannot do arbitrary bitmask combinations (READ+ADMIN without WRITE is impossible)
- But: DOES prevent unauthorized access (tier must be >= required)

`base_div` is now implemented, enabling a proper bitmask approach. The current
tiered approach prevents unauthorized access but leaks tier level.

**Status**: FIXED (Tiered approach).

**See Also**:
- [Opcodes Reference: BaseDiv analysis](zk/opcodes.md)

---

#### Issue 3: No Cancellation Nullifier Verification (MODERATE) — PROVISIONAL FIX

**Location**: [verify_access.zk:150-151](../../../src/contract/subscription/proof/verify_access.zk#L150-L151) (now Phase 5)

**Problem (Original)**: The circuit did not verify the subscription hasn't been cancelled. A subscriber who cancels retains a valid capability.

**PROVISIONAL FIX Applied**: Phase 5 added to verify subscription is Active via Merkle proof.

```zk
# Compute the subscription's spent nullifier
computed_spent_nullifier = poseidon_hash(subscription_id, subscriber_secret);
constrain_equal_base(computed_spent_nullifier, subscription_spent_nullifier);

# Compute the subscription leaf hash
subscription_leaf = poseidon_hash(
    subscription_id,
    subscription_state,
    subscription_spent_nullifier,
);

# Verify the Merkle proof
verified_root = merkle_root(subscription_leaf_pos, subscription_path, subscription_leaf);
constrain_equal_base(verified_root, subscription_state_root);

# Verify the subscription is in Active state (0 = Active)
less_than_strict(subscription_state, 1);
```

**IMPORTANT LIMITATION: This makes capabilities SINGLE-USE**

After each successful verify, the contract MUST mark `subscription_spent_nullifier` as spent in the nullifiers tree. This prevents replay, but:

**Privacy Leak**:
- The `spent_nullifier` is revealed on each access
- All uses of the same subscription can be linked together
- Reveals usage patterns and frequency

**The PROPER Fix Would Require**:
1. A different architecture that doesn't mark nullifier as spent on each use
2. Or a state Merkle tree with non-membership proofs for cancellation
3. Or separating "cancellation nullifier" from "usage tracking"

**What This Fix Achieves**:
- ✅ Cancelled/expired subscriptions CANNOT access (nullifier check fails)
- ✅ Proper Merkle proof verification of subscription state
- ❌ Capabilities become effectively single-use
- ❌ Usage patterns are linkable via spent_nullifier

**Status**: PROVISIONALLY FIXED - enables cancellation enforcement but with privacy tradeoff. The proper fix requires a different architecture.

---

#### Issue 4: DAO-Escrow Bulla Used as Blind Factor (MODERATE) — FIXED

**Location**: [dao_escrow/pay_premium.zk](../../../src/contract/dao_escrow/proof/pay_premium.zk)

**Problem**: The `dao_escrow_bulla` was used directly as a blind factor — DAO alone chose the bulla (potentially predictable), low entropy, malicious DAO could deanonymize members.

**Fix**: MPC commit-reveal ceremony. 3 parties each generate a secret, publish commitments (`secret_i * G`), then reveal to the user who verifies and computes `bulla = H(member_pub_x, member_pub_y, secret_1, secret_2, secret_3)`. Circuit verifies MPC secrets match commitments and computed bulla matches expected. Security: as long as ONE MPC party is honest, the bulla is unpredictable (same model as Zcash's Powers of Tau).

**Impact** (resolved): Bulla unpredictable via MPC security. Malicious DAO cannot predict/track members. Privacy preserved against colluding MPC parties.

### Atomic Swap Contract — DEPRECATED

> **The Atomic Swap contract was a design exploration that was never created as a
> deployable contract crate.** `src/contract/atomic_swap/` does not exist.
> Cross-chain swap functionality is provided by the [Bridge](../contract/bridge.md)
> and [OTC Swap](../contract/otc_swap.md) contracts, which are implemented and
> tested. The audit issues below (formerly Issues 5-7) are retained for design
> reference only — they describe a design, not an implemented contract.

---

### Escrow Contract

#### Issue 8: No State Verification on Claim (MAJOR)

**Location**: [escrow/claim.zk](../../../src/contract/escrow/proof/claim.zk)

**Problem**: The circuit doesn't verify the escrow is in "Funded" state before allowing claim:

```zk
# This circuit proves:
# - Seller knows seller_secret
# - seller_pubkey derived from seller_secret matches escrow.seller_pubkey
# - The escrow is in Funded state (not already spent)  <-- NOT VERIFIED
```

**Impact**:
- An already-refunded escrow could be claimed again
- An already-claimed escrow could be claimed again (double-spend)
- The circuit assumes the contract state is correct, but doesn't verify it

**Recommendation**: Add a state verification step that checks the escrow's current state against an expected state enum.

---

#### Issue 9: Seller Public Key Stored in Plaintext (MODERATE) — FIXED

**Location**: [escrow/create_escrow.zk](../../../src/contract/escrow/proof/create_escrow.zk), [escrow/claim.zk](../../../src/contract/escrow/proof/claim.zk)

**Problem**: Seller's public key revealed as public input in claim, compromising seller privacy.

**Fix**: Store `H(seller_pub)` in the commitment instead of plaintext coordinates. At claim time, the circuit derives `seller_pub` from `seller_secret`, computes `H(seller_pub)`, and verifies it matches — without exposing the public key on-chain. The buyer knows `seller_pub` at creation (received out-of-band), so this is not circular.

**Impact** (resolved): Seller's public key never revealed on-chain. Claim verification is still binding.

---

### DAO-Escrow Contract

#### Issue 10: Endowment Fund Has No Drain Protection (MAJOR) — FIXED

**Problem**: `MODE_TREASURY_ENDOWMENT` accumulates endowment with no guardrails on drawdown — malicious DAO could drain the entire endowment.

**Fix**: [DrainProtection contract](../../../src/contract/drain_protection/README.md) with 8 configurable protections: graduated tiers, exit queue, circuit breaker, guardian pause, observation period, split proposals, no-loss reserve, dead man's switch. All features optional and configurable by deployer; DAO members control via governance. Outstanding: security audit, integration tests.

---

#### Issue 11: Membership Expiry is Witness, Not Verified (MODERATE) — FIXED

**Location**: [dao_escrow/pay_premium.zk](../../../src/contract/dao_escrow/proof/pay_premium.zk)

**Problem (Original)**: While `less_than_strict(current_block, expiry)` was verified, the `expiry` itself was provided as a witness with no maximum cap.

**Fix Applied**: Added maximum membership period check in the circuit:

```zk
max_membership_blocks = 52560;  # ~1 year at 5min blocks
max_expiry = add(current_block, max_membership_blocks);
less_than_strict(expiry, max_expiry);
```

**Impact** (resolved): Members cannot self-issue excessively long memberships. Maximum is ~1 year.

---

### DEX Contract

#### Issue 16: Public Keys Hardcoded to Zero (CRITICAL) — ARCHITECTURAL LIMITATION

**Location**: [dex/src/entrypoint/mod.rs:137-140](../../../src/contract/dex/src/entrypoint/mod.rs#L137-L140)

**Problem**: `dex_create_swap` stores zeroed public keys for proposer and acceptor. The signature field exists in params but cannot be verified without the full DarkWow transaction verification framework. Impact: no accountability for swap participants. Requires refactor to full DarkWow contract framework.

---

#### Issue 17: lock_proof Partially Verified (CRITICAL) — PARTIALLY FIXED

**Location**: [dex/src/entrypoint/mod.rs:114-116](../../../src/contract/dex/src/entrypoint/mod.rs#L114-L116)

**Problem**: The lock commitment Merkle proof was not being verified.

**Partial Fix Applied**: Basic validation added to ensure lock_proof is not empty:

```rust
// SECURITY NOTE: lock_proof should be verified against the promissory_note contract's
// Merkle tree. Currently this verification is stubbed.
if params.lock_proof.is_empty() {
    msg!("[dex_create_swap] ERROR: lock_proof is empty");
    return Err(DexError::InvalidMerkleProof.into())
}
```

**What remains unfixed**: The actual Merkle proof verification against the promissory_note contract's commitment tree is not implemented. This requires integration with the promissory_note contract's state.

**Impact**:
- A user could create a swap claiming locked funds they don't actually have
- The full Merkle proof verification is bypassed

**Recommendation**: Implement actual Merkle proof verification by accessing the promissory_note contract's commitment tree.

---

#### Issue 18: ZK Proof Verification Returns Empty (CRITICAL) — ARCHITECTURAL LIMITATION

**Location**: DEX's `get_metadata()` (inherited from contract framework)

**Problem**: The DEX uses a simplified bridge architecture that doesn't integrate with the full DarkWow zkVM for proof verification. The ZK circuits exist but the contract framework doesn't call the verifier.

**Impact**: All ZK circuit constraints (secret knowledge, lock proofs, etc.) are not enforced on-chain.

**Analysis**: The DEX ZK circuits are properly defined and would verify correctly if integrated with the zkVM. However, the current simplified bridge pattern doesn't support on-chain ZK verification.

**Recommendation**: Refactor DEX to use the full DarkWow contract framework for ZK proof verification.

---

### Bridge Contract

#### Issue 12: Weak Range Check on Amount (MODERATE)

**Location**: [bridge/withdraw.zk:48](../../../src/contract/bridge/proof/withdraw.zk#L48)

**Problem**: The range check only verifies `amount < 2^64`:

```zk
range_check(64, amount);
```

**Impact**:
- Zero-value withdrawals are allowed
- Dust amounts (very small values) could be used for griefing
- No economic floor on withdrawal size

**Recommendation**: Add a minimum amount check or configure the range based on the specific token's decimal精度.

---

## Cross-Cutting Issues

### Issue 19: Missing Public Key Constraint (MAJOR) — FIXED IN CIRCUITS BELOW

**Category**: ZK Circuit Soundness

**Vulnerability Pattern**: When a circuit derives a public key from a secret using `ec_mul_base` and exposes the derived coordinates as public inputs via `constrain_instance`, it must also bind the derived coordinates to the public inputs using `constrain_equal_base`. Without this binding, a malicious prover can claim any public key without knowing the corresponding secret.

**Incorrect Pattern (Vulnerable)**:
```zk
witness "Example" {
    Base secret,
    # ... pub_x and pub_y passed as public inputs but NOT constrained
}

circuit "Example" {
    # Derive public key from secret
    pub = ec_mul_base(secret, NULLIFIER_K);
    pub_x = ec_get_x(pub);
    pub_y = ec_get_y(pub);

    # WRONG: Only expose via constrain_instance, no binding constraint
    constrain_instance(pub_x);
    constrain_instance(pub_y);
}
```

**Why This is Vulnerable**: The circuit only proves knowledge of `secret`, but does NOT prove that the derived public key matches `pub_x/pub_y`. A prover could:
1. Choose any arbitrary `pub_x, pub_y` as public inputs
2. Provide any `secret` (doesn't need to correspond to the claimed pubkey)
3. The circuit accepts because the derived value is never checked against the public input

**Correct Pattern (Sound)**:
```zk
witness "Example" {
    Base secret,
    Base pub_x,  # Public key coordinate - MUST be constrained
    Base pub_y,  # Public key coordinate - MUST be constrained
}

circuit "Example" {
    # Derive public key from secret
    pub = ec_mul_base(secret, NULLIFIER_K);
    derived_pub_x = ec_get_x(pub);
    derived_pub_y = ec_get_y(pub);

    # CRITICAL: Bind derived public key to public inputs
    constrain_equal_base(derived_pub_x, pub_x);
    constrain_equal_base(derived_pub_y, pub_y);

    # Now expose as public inputs
    constrain_instance(pub_x);
    constrain_instance(pub_y);
}
```

**Impact (Theoretical)**: Without this constraint, the circuit has incomplete proof of knowledge:
- The circuit proves "I know a secret that derives to SOME public key"
- But does NOT prove "My derived public key matches the Input's public key"
- **Actual exploitability depends on transaction layer verification**

**Note on Severity**: The transaction layer may provide additional verification that mitigates this issue. However, clean circuit design dictates that circuits should be self-contained and provably correct in isolation. Defense in depth suggests fixing the circuit regardless of transaction layer protection.

**Why Fork**: We fork the promissory_note contract for clean, self-contained circuit design—not because we're under active attack. See [Promissory Note](../contract/promissory_note.md) for the contract design.

**Circuits Fixed** (this audit session):
| Contract | Circuit | Status |
|----------|---------|--------|

| native_token | fee_v1.zk | ✅ Fixed (signature_public) |
| promissory_note | burn_v1.zk | ✅ Fixed (signature_public) |
| oracle | register_oracle_v1.zk | ✅ Fixed |
| drain_protection | exit_v1.zk | ✅ Fixed |
| dao_escrow | exec.zk | ✅ Fixed |
| labor_market | create_job_v1.zk | ✅ Fixed |
| labor_market | accept_job_v1.zk | ✅ Fixed |
| labor_market | submit_deliverable_v1.zk | ✅ Fixed |
| labor_market | confirm_delivery_v1.zk | ✅ Fixed |
| labor_market | dispute_v1.zk | ✅ Fixed |
| labor_market | refund_v1.zk | ✅ Fixed |
| labor_market | submit_git_deliverable_v1.zk | ✅ Fixed |
| tender | create_tender_v1.zk | ✅ Fixed |
| tender | submit_bid_v1.zk | ✅ Fixed |
| tender | select_winner_v1.zk | ✅ Fixed |
| tender | reveal_bid_v1.zk | ✅ Fixed |

**Previously Fixed Circuits** (prior audit sessions):
- dex/execute_swap_v1.zk, dex/cancel_swap_v1.zk
- escrow/claim.zk, escrow/refund_v1.zk
- auction/claim_winnings_v1.zk, auction/close_auction_v1.zk, auction/refund_bid_v1.zk, auction/settle_auction_v1.zk
- attestation/consume_claim.zk, attestation/create_attestation_v1.zk

**Additional Fixes** (promissory_note and dex signature_public):
| Contract | Circuit | Status |
|----------|---------|--------|
| native_token | fee_v1.zk | ✅ Fixed |
| promissory_note | burn_v1.zk | ✅ Fixed |
| dex | create_swap_v1.zk | ✅ Fixed |
| dex | accept_swap_v1.zk | ✅ Fixed |

**DAO Circuits Fixed** (all 8 now fixed with constrain_equal_base):
| Contract | Circuit | Unconstrained Pubkeys |
|----------|---------|----------------------|
| dao_escrow | mint.zk | notes_public, proposer_public, proposals_public, votes_public, exec_public, early_exec_public | ✅ Fixed |
| dao_escrow | auth-money-transfer.zk | ephem_public | ✅ Fixed |
| dao_escrow | propose-main.zk | dao_proposer_public | ✅ Fixed |
| dao_escrow | vote-main.zk | ephem_public | ✅ Fixed |
| dao_escrow | early-exec.zk | dao_exec_public, dao_early_exec_public, signature_public | ✅ Fixed |
| dao_escrow | vote-input.zk | signature_public | ✅ Fixed |
| dao_escrow | propose-input.zk | signature_public | ✅ Fixed |
| dao_escrow | auth-money-transfer-enc-coin.zk | ephem_public | ✅ Fixed |

**Prevention: Git Pre-commit Hook** detects the vulnerable pattern and rejects commits. A `derive_pubkey` builtin for zkas is proposed to enforce soundness by construction.

**Correction (2026-09-24).** The hook exists and now does what this line says — but it did not for most of its life, and that is worth a reader knowing before treating the sentence as evidence. Until 2026-09-23 `hooks/pre-commit` enforced the **opposite** rule: it required that a value derived by `ec_get_x` be bound by `constrain_equal_base` before exposure, which is the unsound shape, so it would have rejected correct circuits and accepted the defect it names. Its own header now records the reversal, the sound and unsound shapes side by side, and the reason it never fired (the files it would have rejected were not edited while it existed). The rule it defers to is `scripts/check-pubkey-binding.sh`, wired in `scripts/run-all-tests.sh` as the `circuit pubkey binding` gate, which blocks on any finding absent from `script/circuit_pubkey_binding_exceptions.txt`; it passes today. **Two figures that stood in this sentence are given as names rather than numbers, because both had drifted**: the wiring was cited as `run-all-tests.sh:145` when the gate had moved to `:200`, and the run's tail was quoted as `… 49 adjudicated` where the checker now prints `51`. The count is the checker's to print (`PASS: no unexcepted vacuous bindings (… circuit(s) scanned; N adjudicated …)`), and `script/circuit_pubkey_binding_exceptions.txt`'s header records why its own classification is not a total. So "rejects commits" is true as of 2026-09-23, and was false before it.

---

### Issue 13: No Formal Verification

None of the unofficial contracts have formal verification. Complex cryptographic circuits like these benefit from formal methods to catch edge cases that testing misses.

**Correction (2026-09-24).** The first sentence is no longer true; the second is partly satisfied; and the correction cuts both ways, so both halves are stated rather than the flattering one. `proofs/lean/src/Transcribed.lean` is generated from the `.zk` sources by `scripts/gen_circuit_transcription.py`, is freshness-gated (`scripts/run-all-tests.sh`, gate 5), and carries one statement list per circuit with a verdict closed by the kernel rather than by hand. What that is **not** is a proof that these circuits are sound. The property it closes is the model's own (`NoFreeInstance` over a transcription), while the tree's enforcement is the checker's separate rule in `scripts/check-circuit-instance-derivation.sh`; the two disagree by design and the generator's preamble decomposes the disagreement. Nor is the obligation mechanized: `Axioms.NoFreeInstances` is still a name, because the step from a circuit `(r, s)` to its statement list is a Lean term that cannot read a `.zk` file. Measured today, the enforcement gate walks **178 circuits / 880 `constrain_instance` sites** and reports **4 unclassified**, exiting 1 — so the honest summary is that a mechanized *statement* of the model's property now exists and fails for most circuits, which is a different thing from a formal verification of them.

### Issue 14: Missing Integration Tests

Several circuits reference other contracts (Subscription → DAO-Escrow, Atomic Swap → Subscription) but integration tests across contract boundaries are minimal or absent.

### Issue 15: Opcode Soundness Tradeoffs — Formal Analysis of Injection Attacks

The circuits avoid `IsEqualBase` due to known soundness bugs. `LessThanOrEqual` is now **verified sound** ✅ via Lean 4 formal verification. The vulnerability class is **injection** — the prover can inject arbitrary values into underconstrained witness variables, altering comparison gate outputs.

**Correction (2026-09-24).** The verification is real, and the theorem is named rather than implied: `Gadgets.less_than_or_equal_sound`. What the sentence omits is what that theorem *takes as given*. Its range-check hypothesis is a `ℤ` inequality on the operands, while a circuit supplies it as a bit decomposition — so "verified sound" is a claim about the gate given bounded operands, and the bound's *production* is a separate theorem (`Comparison.range_check_64_is_bounded`, reached through `BaseDivGadget.less_than_or_equal_integer_reading`). The step still outside both is the passage from a circuit's `range_check(64, ·)` call to the hypothesis, which is register row `OBL-Z12`. Two opcodes also carried this same ✅ without any theorem behind it at all until 2026-09-24 (`OBL-Z20`, `OBL-Z21`), which is why the citation is spelled out here rather than left as a symbol.

For the full formal verification (gate constraints, delta-invert analysis, Lean4 machine-checkable proofs), see [Opcodes and Formal Verification](zk/opcodes.md). The `less_than_strict` opcode used throughout contracts avoids these issues because it is **constrain-only** — no usable output value, only a boolean constraint, which eliminates the underdetermined variable problem.

**Counterexample — `IsNotEqual` (0x62)**: Unlike `IsEqualBase`, the `IsNotEqual` opcode was designed from the start as a fully constrained pure Boolean operator. It has no delta-invert vulnerability because it uses a distinct gate design that treats the equality/inequality cases symmetrically — both branches are fully constrained, leaving no unconstrained witness variables for the prover to exploit. `IsNotEqual` is the third Lean4-verified opcode (alongside `LessThanOrEqual` and `BaseDiv`). See [Opcodes and Formal Verification](zk/opcodes.md) for the verification results.

**Correction (2026-09-24).** The theorem is `Gadgets.is_not_equal_fully_pure` — cited as already proving this at `proofs/lean/src/DarkFi/Comparison.lean:228` — but **"the third" was never a count**, and it is the kind of claim this page has had to retract elsewhere. The opcodes carrying a soundness theorem are now `less_than_or_equal` (0x55, `Gadgets.less_than_or_equal_sound`), `less_than_strict` (0x51, `Gadgets.less_than_strict_sound`), `base_div` (0x58, `BaseDivGadget.lean` under `pallasPrime`), `is_not_equal` (0x62), and — added 2026-09-24, and absent from every tree that listed them as verified — `base_lt_strict` (0x57, `Comparison.base_lt_strict_sound`) and `not_base` (0x56, `Comparison.not_base_correct`). The ordinal is dropped rather than incremented: an ordered count of verified opcodes goes stale on the next theorem, and in this case it was unsupported when written.

**Summary of key risks kept here for reference:**
- `IsEqualBase` delta-invert bug: `delta_invert` unconstrained when `a == b`
- `IsNotEqual` (0x62): ✅ No injection vulnerability — fully constrained symmetric gate design
- `LessThanOrEqual`: prover controls `out` and `a_offset` simultaneously — range checks are necessary but not sufficient
- No upgrade path: once deployed, buggy comparison results cannot be corrected without a hard fork
- For contract authors: add explicit `range_check(253, a)` before comparisons, use redundant `LessThanStrict` as sanity check for high-value operations

**Correction (2026-09-24).** This bullet is the only place in the tree that recommends `range_check(253, …)` before comparisons, and the tree does not do it. Measured across every `.zk` source the gate walks: **109 calls at width 64** and **one at 253** — `proofs/core/opcodes.zk:57`, a demonstration circuit. The rule actually enforced is not a width but a *bound*: `script/circuit_instance_derivation.py` requires every `less_than_*` operand to rest on witnesses a `range_check` bounds, because the comparison chip range-checks the **difference** of its operands rather than the operands themselves, so an unbounded operand is compared modulo the field. Any bound below `2^253` prevents that wrap, which is why 64 is what the repaired circuits use; `BaseDivGadget.qr_needs_bound` is the kernel-checked witness that the bound is not optional. The second clause is the opposite of a decision this tree made deliberately: redundant comparisons ("use `LessThanStrict` as a sanity check") were **deleted** rather than kept, on the register's own reasoning that a second, weaker claim about a property already enforced is a liability rather than a check — `OBL-Z12` records the escrow and `otc_swap` timelocks as that case by name.

**See also:** [Field Arithmetic](zk/field_arithmetic.md) for field-level constraints, [zkVM Primitive Layer](zk/zkvm_primitives.md) for contract integration patterns, and `proofs/lean/` for the machine-checkable proofs.

## Summary Table

| # | Contract | Issue | Severity | Status |
|---|----------|-------|----------|--------|
| 1 | Subscription | Poseidon hash instead of Pedersen for value commitment | MAJOR | ✅ FIXED |
| 2 | Subscription | Permission bitmask checking absent | MAJOR | ✅ FIXED (Tiered approach - tiered access with less_than_strict, privacy limitation) |
| 3 | Subscription | No cancellation nullifier verification | MODERATE | ⚠️ PROVISIONAL FIX (single-use, privacy leak) |
| 4 | Subscription | Bulla used as blind factor | MODERATE | ✅ FIXED (MPC commit-reveal for bulla) |
| 5 | Escrow | No state verification on claim | MAJOR | ⚠️ Entrypoint written, state check in contract |
| 6 | Escrow | Seller public key in plaintext | MODERATE | ✅ FIXED (H(seller_pub) in commitment, circuit verifies hash) |
| 7 | DAO-Escrow | No endowment drain protection | MAJOR | ✅ FIXED: drain_protection with 8 best practices |
| 8 | DAO-Escrow | Membership expiry as witness, no max cap | MODERATE | ✅ FIXED (max 1-year cap added) |
| 9 | Bridge | Weak range check (only < 2^64) | MODERATE | ✅ FIXED (min amount floor: 100_000_000) |
| 10 | DEX | Public keys hardcoded to zero | CRITICAL | ⚠️ ARCHITECTURAL LIMITATION (documented) |
| 11 | DEX | lock_proof never verified | CRITICAL | ⚠️ PARTIALLY FIXED (basic validation added) |
| 12 | DEX | ZK proof verification stubbed | CRITICAL | ⚠️ ARCHITECTURAL LIMITATION (documented) |
| 13 | Multiple | Missing public key constraint binding | MAJOR | ✅ FULLY FIXED (25+ circuits fixed, pre-commit hook added, zkas builtin proposed) |

**Correction (2026-09-24) to row 13 only.** "FULLY FIXED" is a status the register does not support, and the two documents should agree. The binding rule is now gated in both directions — `hooks/pre-commit` defers to `scripts/check-pubkey-binding.sh`, which blocks on any finding not in `script/circuit_pubkey_binding_exceptions.txt` and passes today over 178 circuits with 49 adjudicated. But the hook's own header states what that adjudication amounts to: of the 57 candidates read circuit-and-host together, **fifteen are genuine defects**, each named with a register row scheduling its repair (`OBL-C81`, `C82`, `C83`, `C84`, `OBL-C75`) — and the `zkas` builtin is still proposed rather than built. So the row's own vocabulary: *adjudicated and gated*, with fifteen named defects open, not fully fixed. The hook is also worth dating: until 2026-09-23 it enforced the reversed rule, so "pre-commit hook added" was, for its whole prior life, evidence of a check that would have accepted this very defect.

---

## Recommendations

Most issues are resolved. See the summary table above for per-issue status.

**Cannot fix without additional primitives**: Issue 2 (proper bitmask) requires `base_div` (now implemented).

**Outstanding for DrainProtection**: ZK circuit for vote authorization, vote weight calculation, integration tests.

**Deprecated — removed**: Atomic Swap (formerly Issues 5-7). The contract design was never implemented. Cross-chain swap functionality is provided by Bridge and OTC Swap.

**Architectural limitations**: DEX (Issues 10-12) requires deeper DarkWow framework integration for ZK proof verification and signature checks.

---

## 2026-06-05: Full Contract Security Audit — Double-Spend & Infinity-Mint Hardening

A 7-dimensional adversarial audit of all 30 smart contracts (144 ZK circuits) found 9 bugs:
4 CRITICAL, 4 HIGH, 1 MEDIUM. All 9 have been fixed or documented with mitigation plans.

**Correction (2026-09-24) to the counts, not to the finding.** The audit's 9 bugs and its 4/4/1 split are correct; the population they were found over has moved, so a reader comparing them to today's tree will not find these numbers. The tree holds **34 contract directories**, **31 with a `proof/` directory**, and **166 `.zk` circuits** under `src/contract/*/proof/` — **178** by the count the Orchard-class gate walks, which adds `proofs/core/` (10) and `bin/darkirc/proof/` (2). The nine findings themselves are not re-derived here; what is dated is the population.

### Critical Bugs (all fixed)

| ID | Bug | Fix |
|----|-----|-----|
| C1 | PromissoryNote `mint_public` unconstrained in Mint_V1 circuit | Added `backing_secret` witness + `mint_public = poseidon_hash(backing_secret)` constraint |
| C2 | NativeToken `Fee_V3` circuit — no `output_value = input_value - fee` constraint | Added `fee` witness + `base_add(output_value, fee) == input_value` constraint |
| C3 | NativeToken MintV1 — no authority check, no supply tracking | Disabled MintV1 from all dispatch tables (opcode 0x01 reserved) |
| C4 | NativeToken TransferV1 — no cross-proof value conservation | Added Pedersen homomorphic sum check per token_commit |

### High Bugs (all fixed)

| ID | Bug | Fix |
|----|-----|-----|
| H1 | Same-block double-spend via isolated execution overlays | Documented with TODO for merge-phase key-conflict detection |
| H2 | Independent `coin_secret`/`signature_secret` in burn circuits | Per-burn `signature_secret = poseidon_hash(coin_secret, nullifier)` in-circuit — binds signer to owner, unlinkable across burns |
| H3 | BearerBond IssueStakeV1 — no issuer authorization | Added `issuer_contract` comparison against stored series data |
| H4 | Bridge WithdrawV1 — `merkle_root_val` not `constrain_instance`d | Added `constrain_instance(merkle_root_val)` in circuit |

### Medium Bugs (fixed)

| ID | Bug | Fix |
|----|-----|-----|
| M1 | Stablecoin AccrueInterestV1 — `old_total_debt` not validated against on-chain | Added `constrain_instance(old_total_debt)` |

### False Positives (14 verified safe)

The audit also verified 14 findings as false positives — patterns that appeared suspicious
but were correctly implemented on closer inspection. See the full report at
`contrib/model/security_audit_2026-06-05.md`.

### Formal Verification (June 2026)

Since the manual security audit, all 120 contract ZK circuits across 26 contracts have been
**manually audited** against the Orchard-class vulnerability pattern (under-constrained
`constrain_instance` — the exact bug class that enabled unlimited minting in Zcash for ~4 years)
and the audit is documented per circuit.

**Correction (2026-09-24) to the opening count.** Measured now: **178 circuits** are what the gate
walks, over a tree with **34 contract directories** (31 of them carrying proofs) — not 120 over 26. The
paragraph's own next line already concedes the shape ("the audit they describe is manual"), and its
Correction below records what replaced it; only this opening count had not been brought along. The word
"manually" is also doing less work than it was: the reading side is a gate
(`scripts/check-circuit-instance-derivation.sh`) and the stating side is a generated, freshness-gated
Lean module (`proofs/lean/src/Transcribed.lean`) — which is what the Correction below describes.

**Correction.** This section previously said those circuits had been "formally verified in Lean
4". They were not. `proofs/lean/src/DarkFi/Circuits/` — the directory this section pointed at —
contains **no Lean declarations at all**: `All.lean`, `Bridge.lean` and `Exchange.lean` are
comment-only, and the eleven names once listed here as the "Circuit Audit Axioms" are comment
lines of the form `-- ASSUMPTION (not proven): …`. The single Lean declaration in that directory
that ever claimed otherwise, `burn_v1_no_free_instances`, had the statement
`x = x ∧ y = y ∧ True`.

What the Lean layer contains is the *obligation* the audit would have to discharge to be
mechanized — `Axioms.NoFreeInstances`, an uninterpreted predicate that no theorem consumes.

- **Layer 1**: two opcode properties are **assumptions** in `Axioms.lean`
  (`poseidon_collision_resistance`, `poseidon_hash_output`), not theorems. This list used to name
  four: `fixed_base_mul_uses_constant` and `variable_base_mul_is_prover_chosen` were **false** as
  stated and made the axiom set inconsistent — they are theorems in `ECOps.lean` now — and
  `base_div_mul_cancel` was `pallasPrime` restated over `Int`, discharged in `DarkFi/BaseDiv.lean`.
  Note also that `poseidon_collision_resistance` states *injectivity*, which is strictly stronger
  than collision-resistance and false of the real sponge; see `verification-hazop.md`.
- **Layer 2**: manual audit, per the correction above.
- **Layer 3**: only `value_conservation_no_wraparound` survives as a theorem. Four of the six
  named properties were `: Prop`-valued axioms — claims named but never stated — and are deleted
  (`proofs/lean/src/DarkFi/HAZOP/Elevated.lean`, ELEV-27 to ELEV-30).

The bugs identified in the manual audit (C1, C2, C4, H2, H3, M1) were fixed by hand; whether a
given fix is *proved* depends on the individual property, and the tables in
[Opcodes and Formal Verification](zk/opcodes.md) now say which is which. One additional bug discovered (IsEqualBase `delta_invert` unconstrained) is documented
as non-exploitable. See [Opcodes and Formal Verification](zk/opcodes.md) and
[Opcodes Status](zk/opcodes-status.md) for complete results.

### Documentation

- [safety.md](../dev/contracts/safety.md) — the twelve contract root causes, each with the findings that taught it and the rule it yields
- [opcodes.md](zk/opcodes.md) — the Lean 4 proof architecture per layer, and the status of the 31 zkVM opcodes it covers; Layer 1's status is **PARTIAL**, and which properties are assumptions is listed there and in `Axioms.lean`
- [opcodes-status.md](zk/opcodes-status.md) — the verification status for the circuits the Orchard-class gate walks; **178 as of 2026-09-24**, not the 120 this line carried
- [Full audit report](../../../contrib/model/security_audit_2026-06-05.md) — Detailed findings with code traces

**Correction (2026-09-24) to the two `opcodes` descriptions.** "All 32 opcode verification results" mixed two counts and overstated one of them. `src/zkas/opcode.rs` declares **32** opcode variants; `doc/src/arch/zk/opcodes.md` covers **31**, and the one it omits is `Noop` (0x00) — correctly, since a no-op has no verification subject. The number was therefore the *declaration* count where the verification surface is 31, and "all … verification results" is the part that was never true: `OBL-Z20` and `OBL-Z21` record two opcodes (0x57, 0x56) that four trees listed as verified with no theorem anywhere, and Layer 1's own row states that several of its properties are *assumptions* in `Axioms.lean` rather than theorems. Both links now describe what their targets say.

---

## Conclusion

**Correction (2026-09-24).** This paragraph previously read: *"All 9 bugs identified in the manual
audit are fixed and formally verified. The Orchard-class audit confirmed zero additional
under-constrained instances across all 120 contract circuits. The Lean 4 verification suite provides
ongoing regression protection against ZK circuit constraint omissions."* All three sentences are
refuted, and two of them by the section immediately above them in this same file:

- **"fixed and formally verified."** The section above says the opposite in its own words: the nine
  were fixed by hand, and *"whether a given fix is **proved** depends on the individual property, and
  the tables in [Opcodes and Formal Verification](zk/opcodes.md) now say which is which."* Nor were
  all nine even fixed — `C3` was **disabled** (removed from the dispatch tables) and `H1` was
  **documented with a TODO**, both recorded as such in their own rows. "Formally verified" also
  cannot attach to the circuits: the Correction above records that `proofs/lean/src/DarkFi/Circuits/`
  contains no declarations at all.
- **"zero additional under-constrained instances across all 120 contract circuits."** Measured today,
  `scripts/check-circuit-instance-derivation.sh` walks **178 circuits / 880 `constrain_instance`
  sites** and names **4 unclassified** — `insurance_market`'s two `required_capability_id`,
  `labor_market` `create_job`'s `attestation_id` and `oracle/attest_value`'s `threshold` — exiting 1.
  It has never reported zero.
- **"The Lean 4 verification suite provides ongoing regression protection against ZK circuit
  constraint omissions."** The mechanism that does this is a Python gate, not the Lean suite; and what
  the Lean transcription states is the *model's* property over a transcription of each circuit, which
  currently fails for most of them — an instrument that reports, not a protection that holds. The
  suite's real contribution is the per-opcode soundness theorems, whose scope each is stated in
  [Opcodes and Formal Verification](zk/opcodes.md).

The finding this page exists to record — the 2026-06-05 audit's nine bugs, the 14 false positives, and
the Orchard-class pattern that motivated them — is not retracted here. What is corrected is the
status sentence attached to it.

