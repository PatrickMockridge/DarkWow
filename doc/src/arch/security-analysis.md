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

#### Issue 1: Value Commitment Uses Poseidon Hash Instead of Pedersen (MAJOR)

**Location**: [subscribe_v1.zk:257-264](file://../../src/contract/subscription/proof/subscribe_v1.zk#L257-L264)

**Problem**: The deposit commitment uses `poseidon_hash(deposit, value_blind, token_id)` instead of proper Pedersen commitment (`deposit * G1 + blind * G2`).

```zk
# Current (INSECURE):
commit_check = poseidon_hash(deposit, value_blind, token_id);
constrain_equal_base(commit_check, value_commit_x);

# Correct Pedersen:
# C = deposit * VALUE_COMMIT_VALUE + value_blind * VALUE_COMMIT_RANDOM
# verify ec_add(ec_mul_short(deposit, VALUE_COMMIT_VALUE),
#               ec_mul(value_blind, VALUE_COMMIT_RANDOM)) == value_commit
```

**Impact**:
- An attacker who knows `deposit` and `token_id` can derive `value_blind` by brute-forcing the Poseidon hash
- The "hidden" deposit amount is not actually hidden
- Anonymity guarantees are voided

**Recommendation**: Implement proper Pedersen commitment verification using `ec_mul_short` and `ec_add` opcodes, as done in the `money` contract's burn/mint circuits.

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

#### Issue 5: Hash Not Verified In-Circuit (CRITICAL)

**Location**: [claim_v1.zk:59-63](file://../../src/contract/atomic_swap/proof/claim_v1.zk#L59-L63)

**Problem**: The circuit does NOT verify `hash == SHA256(secret)`. It only stores the hash as a witness:

```zk
# NOTE: We trust the external chain's hash function.
# For the MVP, we just store the hash as a witness and constrain it
# A full implementation would compute poseidon_hash(secret) and verify
# it equals the stored hash
```

**Impact**:
- Anyone who obtains a valid `(hash, secret)` pair can claim the swap
- If the secret is leaked through any channel (side-channel, reused secret, etc.), an attacker can steal funds
- The HTLC's security relies on hash preimage secrecy, but the circuit doesn't enforce it

**Recommendation**: Implement proper hash verification in-circuit:
```zk
# Compute hash of secret and verify it matches stored hash
computed_hash = poseidon_hash(secret);  # or SHA256 if supported
constrain_equal_base(computed_hash, hash);
```

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

#### Issue 7: External Hash Function Trust (MAJOR)

**Location**: [claim_v1.zk:52-58](file://../../src/contract/atomic_swap/proof/claim_v1.zk#L52-L58)

**Problem**: The circuit trusts external chain hash functions (SHA256 for Ethereum) without verification:

```zk
# NOTE: We trust the external chain's hash function.
# For Ethereum: SHA256
# For Bitcoin: SHA256 or RIPEMD160(SHA256)
# FUTURE: Implement proper hash verification in-circuit
```

**Impact**:
- If the external chain uses a different hash than expected, funds can be stolen
- A malicious party could create a swap on a chain with a weaker hash function
- The circuit cannot detect hash function mismatches

**Recommendation**: Implement SHA256 or other required hash functions in-circuit, or use a commit-reveal scheme where the hash is verified by the DarkFi contract.

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

#### Issue 9: Seller Public Key Stored in Plaintext (MODERATE)

**Location**: [escrow/claim_v1.zk:36-37](file://../../src/contract/escrow/proof/claim_v1.zk#L36-L37)

**Problem**: The escrow's seller public key is revealed as a witness:

```zk
# Stored seller public key from escrow (revealed for verification)
Base escrow_seller_pub_x,
Base escrow_seller_pub_y,
```

**Impact**:
- The seller's identity is revealed when claiming
- Privacy is compromised compared to a zero-knowledge verification
- The buyer learns the seller's public key

**Recommendation**: Derive the seller public key from the escrow_id and seller_secret using the same key derivation scheme, rather than storing it in plaintext.

---

### DAO-Escrow Contract

#### Issue 10: Endowment Fund Has No Drain Protection (MAJOR)

**Problem**: The `MODE_TREASURY_ENDOWMENT` mode accumulates an endowment share but has no guardrails on drawdown.

**Impact**:
- A malicious DAO can vote to drain the entire endowment in a single proposal
- Members have no on-chain protection against treasury extraction
- The "insurance" provided by the endowment is only as secure as the DAO

**Recommendation**: Implement configurable drawdown limits:
- Maximum drawdown per proposal (e.g., 10% of endowment)
- Minimum time between large drawdowns
- Emergency circuit-breaker requiring supermajority for large drawdowns

---

#### Issue 11: Membership Expiry is Witness, Not Verified (MODERATE)

**Location**: [dao_escrow/pay_premium_v1.zk:69](file://../../src/contract/dao_escrow/proof/pay_premium_v1.zk#L69)

**Problem**: While `less_than_strict(current_block, expiry)` is verified, the `expiry` itself is provided as a witness:

```zk
Uint64 expiry,  # Membership expiry block (must be > current_block)
```

**Impact**:
- The member can choose any expiry they want (within reason)
- Nothing forces the expiry to be "reasonable" (e.g., 1 year max)
- A member could self-issue a 100-year membership

**Recommendation**: Add a maximum allowed expiry check in the circuit or enforce it in the contract layer.

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

| # | Contract | Issue | Severity |
|---|----------|-------|----------|
| 1 | Subscription | Poseidon hash instead of Pedersen for value commitment | MAJOR |
| 2 | Subscription | Permission bitmask checking absent | MAJOR |
| 3 | Subscription | No cancellation nullifier verification | MODERATE |
| 4 | Subscription | Bulla used as blind factor | MODERATE |
| 5 | Atomic Swap | Hash not verified in-circuit | CRITICAL |
| 6 | Atomic Swap | No timelock check on claim | MAJOR |
| 7 | Atomic Swap | External hash function trust | MAJOR |
| 8 | Escrow | No state verification on claim | MAJOR |
| 9 | Escrow | Seller public key in plaintext | MODERATE |
| 10 | DAO-Escrow | No endowment drain protection | MAJOR |
| 11 | DAO-Escrow | Membership expiry as witness, no max cap | MODERATE |
| 12 | Bridge | Weak range check (only < 2^64) | MODERATE |

---

## Recommendations Priority

### Immediate (Before Any Production Use)

1. **Issue 5 (CRITICAL)**: Implement hash verification in atomic_swap claim circuit
2. **Issue 1 (MAJOR)**: Replace Poseidon placeholder with proper Pedersen commitment in Subscription
3. **Issue 8 (MAJOR)**: Add state verification to Escrow claim
4. **Issue 10 (MAJOR)**: Add endowment drain protection to DAO-Escrow

### Short Term (Before Production)

5. **Issue 2 (MAJOR)**: Implement permission bitmask checking
6. **Issue 6 (MAJOR)**: Clarify/add timelock on atomic swap claim
7. **Issue 7 (MAJOR)**: Implement SHA256 or use commit-reveal scheme

### Medium Term

8. **Issue 3 (MODERATE)**: Add cancellation nullifier verification
9. **Issue 4 (MODERATE)**: Generate separate blind factor
10. **Issue 11 (MODERATE)**: Add max expiry cap
11. **Issue 12 (MODERATE)**: Add minimum amount floor to bridge

### Long Term

12. Formal verification of all circuits
13. Comprehensive integration test suite
14. Security audit by external cryptographers

---

## Conclusion

The unofficial contracts demonstrate interesting composability patterns but have significant security issues that make them unsuitable for production use in their current state. The most critical issue is the atomic swap's failure to verify the hash in-circuit (Issue 5), which could lead to immediate fund loss.

The design philosophy of avoiding experimental opcodes with known soundness issues is sound, but this limitation should be clearly documented and proper workarounds implemented before deployment.

*This analysis reflects the state of the dev branch and these contracts are NOT part of official DarkFi master.*
