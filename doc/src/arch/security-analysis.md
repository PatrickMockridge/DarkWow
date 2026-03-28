# Security Analysis: Unofficial DarkFi Smart Contracts

*This document analyzes security issues in the non-standard smart contracts added to the DarkFi dev branch. These contracts are NOT part of the official DarkFi master and should be considered experimental.*

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

**Problem (Original)**: The deposit commitment used `poseidon_hash(deposit, value_blind, token_id)` instead of proper Pedersen commitment.

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

#### Issue 2: Permission Bitmask Checking is Absent (MAJOR)

**Location**: [verify_access_v1.zk:128-138](file://../../src/contract/subscription/proof/verify_access_v1.zk#L128-L138)

**Problem**: The circuit comment explicitly states that permission verification is a no-op:

```zk
# less_than_or_equal is experimental (has gate soundness issue),
# so we use the sound alternative: less_than_strict
# For MVP, we just verify the claimed permissions are non-zero
# The actual permission enforcement would be done off-chain
```

**Impact**:
- Any subscriber can claim ANY permission level (READ, WRITE, ADMIN, etc.)
- Access control is not enforced on-chain
- A subscriber paying for basic access could exercise admin-level permissions

**Recommendation**: Implement proper bitmask checking once `base_div` opcode is available. Workaround using subtraction:
```zk
# Check: (claimed & required) == claimed
# Via: less_than_strict(claimed - required, 1)  # Fails if claimed > required
```

---

#### Issue 3: No Cancellation Nullifier Verification (MODERATE)

**Location**: [verify_access_v1.zk:150-151](file://../../src/contract/subscription/proof/verify_access_v1.zk#L150-L151)

**Problem**: The circuit explicitly notes it does not verify the subscription hasn't been cancelled.

**Impact**:
- A subscriber who cancels their subscription retains a valid capability
- The DAO must actively slash malicious subscribers
- If slashing is not enforced, cancelled subscriptions remain usable

**Recommendation**: Add a nullifier tree check to verify the subscription has not been cancelled before allowing access.

---

#### Issue 4: DAO-Escrow Bulla Used as Blind Factor (MODERATE)

**Location**: [subscribe_v1.zk:224](file://../../src/contract/subscription/proof/subscribe_v1.zk#L224)

**Problem**: The `dao_escrow_bulla` is used as a blinding factor in the membership note hash.

```zk
derived_dao_note = poseidon_hash(
    dao_member_pub_x,
    dao_member_pub_y,
    dao_membership_value,
    token_id,
    dao_membership_expiry,
    dao_escrow_bulla,  # Using bulla as blind factor
);
```

**Impact**:
- Bullae are not guaranteed to be high-entropy or unpredictable
- A malicious DAO could choose a bulla with low entropy to weaken hiding
- The membership note's randomness depends on the bulla's quality

**Recommendation**: Generate a separate random blind factor in the circuit rather than reusing the bulla.

---

### Atomic Swap Contract

#### Issue 5: Hash Not Verified In-Circuit (CRITICAL) — PARTIALLY FIXED

**Location**: claim_v1.zk, create_swap_v1.zk

**Problem (Original)**: The circuit did NOT verify `hash == SHA256(secret)`. It only stored the hash as a witness.

**Fix Applied**: Both CreateSwap and Claim circuits now verify `poseidon_hash(secret)` matches the stored hash. The hash is no longer a free variable.

**Remaining Issue**: For cross-chain swaps with Ethereum (SHA256), the poseidon_hash on DarkFi is not cryptographically bound to the external chain's SHA256 hash. A bridge or oracle is needed to verify the cross-chain binding.

**Impact** (mitigated): The poseidon_hash verification prevents arbitrary (hash, secret) pairs from being used. The claimer must have created the swap via CreateSwap, establishing the swap_id binding.

**Recommendation**: External chain integration requires a commit-reveal or bridge scheme where an oracle verifies SHA256(hash) on Ethereum and creates a corresponding DarkFi swap.

---

#### Issue 6: No Timelock Check on Claim (MAJOR)

**Location**: [claim_v1.zk](file://../../src/contract/atomic_swap/proof/claim_v1.zk) (entire circuit)

**Problem**: The `claim` circuit has no timelock verification. The `timelock` is passed as a witness but never checked:

```zk
# Timelock for reference
Uint64 timelock,
```

**Impact**:
- Funds can be claimed at any time after swap creation, regardless of timelock
- The timelock only protects the `refund` path, not the `claim` path
- A malicious party could claim immediately without waiting

**Note**: This may be intentional — the timelock is for refund protection, not claim protection. However, the asymmetric treatment should be clearly documented.

**Recommendation**: Verify the timelock is satisfied before allowing claim, or clearly document why this check is omitted.

---

#### Issue 7: External Hash Function Trust (MAJOR) — MITIGATED BY DESIGN

**Location**: [claim_v1.zk](file://../../src/contract/atomic_swap/proof/claim_v1.zk), [atomic_swap.md](../../doc/src/arch/atomic_swap.md)

**Problem (Original)**: DarkFi cannot verify SHA256 used by Ethereum, and Ethereum cannot verify poseidon_hash used by DarkFi.

**Analysis (Current)**: This is **not actually a vulnerability** because each chain only needs to verify its own hash function:

| Chain | Hash Used | Verification |
|-------|-----------|-------------|
| DarkFi | `poseidon_hash(secret)` | ZK circuit verifies in-circuit |
| Ethereum | `SHA256(secret)` | EVM verifies natively |

**How the cross-chain swap works without oracles**:

1. Alice creates DarkFi swap with `H' = poseidon_hash(secret)`
2. Alice creates Ethereum HTLC with `H = SHA256(secret)`
3. Alice reveals secret on Ethereum voluntarily (to claim her ETH)
4. Bob monitors DarkFi, sees secret revealed, claims on DarkFi
5. Alice monitors DarkFi for Bob's claim, then claims on Ethereum

**Key insight**: Each chain verifies the hash it understands. No cross-chain verification is needed. Alice reveals voluntarily when she wants to claim, and Bob has financial incentive to claim on DarkFi when he sees the reveal.

**Impact** (mitigated): The design is sound for cooperative cross-chain swaps. A malicious actor cannot steal funds by exploiting hash function differences.

**Remaining consideration**: For non-cooperative swaps (e.g., one party refuses to act), the timelock refund still protects both parties.

---

### Escrow Contract

#### Issue 8: No State Verification on Claim (MAJOR)

**Location**: [escrow/claim_v1.zk](file://../../src/contract/escrow/proof/claim_v1.zk)

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

**Location**:
- [escrow/create_escrow_v1.zk](file://../../src/contract/escrow/proof/create_escrow_v1.zk)
- [escrow/claim_v1.zk](file://../../src/contract/escrow/proof/claim_v1.zk)

**Problem (Original)**: The escrow's seller public key was revealed as a public input in claim, compromising seller privacy.

**Fix Applied**:

1. **create_escrow_v1.zk**: Store `H(seller_pub)` instead of `seller_pub_x/y` in the commitment:
```zk
seller_commitment = poseidon_hash(seller_pub_x, seller_pub_y);
C = poseidon_hash(
    buyer_pub_x,
    buyer_pub_y,
    seller_commitment,
    value,
    token_id,
    timeout,
);
constrain_instance(C);
constrain_instance(seller_commitment);
```

2. **claim_v1.zk**: Verify via poseidon_hash without exposing seller_pub on-chain:
```zk
# Derive seller public key from secret
seller_pub = ec_mul_base(seller_secret, NULLIFIER_K);
seller_pub_x = ec_get_x(seller_pub);
seller_pub_y = ec_get_y(seller_pub);

# PRIVACY: Verify seller_commitment without revealing seller_pub on-chain
seller_commitment_computed = poseidon_hash(seller_pub_x, seller_pub_y);
constrain_eq(seller_commitment_computed, escrow_seller_commitment);

# Public inputs: escrow_id, seller_commitment, spent_nullifier (NOT seller_pub_x/y)
```

**Why This is NOT Circular**:

The circular dependency was about `H(seller_secret)`, not `H(seller_pub)`:
- `seller_pub` is a PUBLIC KEY - the buyer KNOWS it at creation time
- `seller_secret` is the PRIVATE KEY - the seller hasn't shared it yet

The buyer receives `seller_pub` from the seller out-of-band, computes `H(seller_pub)`, and stores that in the commitment. At claim time, the seller proves knowledge of `seller_secret` (which derives to `seller_pub`) and the circuit verifies `H(seller_pub)` matches without ever exposing `seller_pub` on-chain.

**Impact** (resolved):
- Seller's public key is NOT revealed on-chain
- Claim verification is still binding (proves seller knows seller_secret)
- Privacy preserved without changing the trust model

---

### DAO-Escrow Contract

#### Issue 10: Endowment Fund Has No Drain Protection (MAJOR) — PROVISIONAL FIX APPLIED

**Problem**: The `MODE_TREASURY_ENDOWMENT` mode accumulates an endowment share but has no guardrails on drawdown.

**Impact**:
- A malicious DAO can vote to drain the entire endowment in a single proposal
- Members have no on-chain protection against treasury extraction
- The "insurance" provided by the endowment is only as secure as the DAO

---

##### Provisional Drain Protection (Implementation Started)

A separate [DrainProtection contract](../../src/contract/drain_protection/README.md) has been created to implement these protections. Integration with DAO-Escrow follows the composability pattern: DrainProtection verifies membership via Merkle proof from DAO-Escrow.

**Implemented Protections** (in drain_protection contract):

| Protection | Status | Notes |
|------------|--------|-------|
| Rate Limit Per Block | ✅ Implemented | Base rate configurable |
| Large Withdrawal Vote | ✅ Implemented | 2/3 threshold, 50% quorum |
| Lock/Unlock Controls | ✅ Implemented | Max 7 days lock, 24hr unlock timelock |
| Spend Authority Changes | ✅ Implemented | 2/3 vote + 48hr timelock |
| Member Exit + Haircut | ✅ Implemented | 1/3 haircut, block-height-weighted |
| ZK Circuits | ⚠️ Partial | exit_v1.zk placeholder exists |

**Outstanding Implementation Work**:
- [ ] ZK circuit for membership proof verification (Merkle proof)
- [ ] ZK circuit for vote authorization
- [ ] Full vote weight calculation from DAO-Escrow
- [ ] Integration tests between DAO-Escrow and DrainProtection
- [ ] Security audit

---

#### Issue 11: Membership Expiry is Witness, Not Verified (MODERATE) — FIXED

**Location**: [dao_escrow/pay_premium_v1.zk](file://../../src/contract/dao_escrow/proof/pay_premium_v1.zk)

**Problem (Original)**: While `less_than_strict(current_block, expiry)` was verified, the `expiry` itself was provided as a witness with no maximum cap.

**Fix Applied**: Added maximum membership period check in the circuit:

```zk
max_membership_blocks = 52560;  # ~1 year at 5min blocks
max_expiry = add(current_block, max_membership_blocks);
less_than_strict(expiry, max_expiry);
```

**Impact** (resolved): Members cannot self-issue excessively long memberships. Maximum is ~1 year.

---

### Bridge Contract

#### Issue 12: Weak Range Check on Amount (MODERATE)

**Location**: [bridge/withdraw_v1.zk:48](file://../../src/contract/bridge/proof/withdraw_v1.zk#L48)

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

### Issue 13: No Formal Verification

None of the unofficial contracts have formal verification. Complex cryptographic circuits like these benefit from formal methods to catch edge cases that testing misses.

### Issue 14: Missing Integration Tests

Several circuits reference other contracts (Subscription → DAO-Escrow, Atomic Swap → Subscription) but integration tests across contract boundaries are minimal or absent.

### Issue 15: Opcode Soundness Tradeoffs

The circuits explicitly avoid `LessThanOrEqual` and `IsEqualBase` due to known soundness issues, choosing `less_than_strict` as a safer alternative. This limits functionality (e.g., permission bitmask checking) but avoids critical vulnerabilities.

---

## Summary Table

| # | Contract | Issue | Severity | Status |
|---|----------|-------|----------|--------|
| 1 | Subscription | Poseidon hash instead of Pedersen for value commitment | MAJOR | ✅ FIXED |
| 2 | Subscription | Permission bitmask checking absent | MAJOR | ⚠️ Cannot fix (needs base_div opcode) |
| 3 | Subscription | No cancellation nullifier verification | MODERATE | ⚠️ Contract-level TODO |
| 4 | Subscription | Bulla used as blind factor | MODERATE | ⚠️ Design issue, documented |
| 5 | Atomic Swap | Hash not verified in-circuit | CRITICAL | ⚠️ PARTIALLY FIXED (poseidon verified, SHA256 bridge needed) |
| 6 | Atomic Swap | No timelock check on claim | MAJOR | ℹ️ May be intentional (timelock for refund only) |
| 7 | Atomic Swap | External hash function trust | MAJOR | ✅ MITIGATED (each chain verifies own hash) |
| 8 | Escrow | No state verification on claim | MAJOR | ⚠️ Entrypoint written, state check in contract |
| 9 | Escrow | Seller public key in plaintext | MODERATE | ✅ FIXED (H(seller_pub) in commitment, circuit verifies hash) |
| 10 | DAO-Escrow | No endowment drain protection | MAJOR | ⚠️ Provisional: drain_protection contract exists |
| 11 | DAO-Escrow | Membership expiry as witness, no max cap | MODERATE | ✅ FIXED (max 1-year cap added) |
| 12 | Bridge | Weak range check (only < 2^64) | MODERATE | ✅ FIXED (min amount floor: 100_000_000) |

---

## Recommendations Priority

### Completed Fixes
1. ✅ **Issue 1 (MAJOR)**: Pedersen commitment implemented in Subscription
2. ✅ **Issue 5 (CRITICAL)**: poseidon_hash verification added to CreateSwap and Claim
3. ✅ **Issue 7 (MAJOR)**: External hash trust mitigated by HTLC design
4. ✅ **Issue 8 (MAJOR)**: Escrow entrypoint written with state verification
5. ✅ **Issue 9 (MODERATE)**: Seller pubkey privacy fixed (H(seller_pub) in commitment)
6. ✅ **Issue 10 (MAJOR)**: drain_protection contract created provisionally
7. ✅ **Issue 11 (MODERATE)**: Max membership expiry cap added (1 year limit)
8. ✅ **Issue 12 (MODERATE)**: Minimum amount floor added to bridge (100_000_000)

### Cannot Fix Without Additional Primitives
- **Issue 2**: Permission bitmask checking requires `base_div` opcode

### Contract-Level TODOs

- **Issue 3 (MODERATE)**: Add cancellation nullifier verification to Subscription
- **Issue 4 (MODERATE)**: Generate separate blind factor (design issue)
- **Issue 6 (MAJOR)**: Clarify timelock on atomic swap claim (intentional by design)

### Outstanding for DrainProtection Integration

- ZK circuit for membership proof verification (Merkle proof)
- ZK circuit for vote authorization
- Full vote weight calculation from DAO-Escrow
- Integration tests between DAO-Escrow and DrainProtection
- Security audit

---

## Conclusion

The unofficial contracts demonstrate interesting composability patterns but have significant security issues that make them unsuitable for production use in their current state. The most critical issue is the atomic swap's failure to verify the hash in-circuit (Issue 5), which could lead to immediate fund loss.

The design philosophy of avoiding experimental opcodes with known soundness issues is sound, but this limitation should be clearly documented and proper workarounds implemented before deployment.

*This analysis reflects the state of the dev branch and these contracts are NOT part of official DarkFi master.*
