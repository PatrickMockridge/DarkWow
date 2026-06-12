# Native Token Smart Contract — Independent ZK Constraint & Fitness-for-Purpose Audit

**Date:** 2026-06-07
**Scope:** `src/contract/native_token/` — all ZK circuits, WASM entrypoint, client proof generation, model definitions
**Method:** Full manual audit — circuit-level constraint tracing, metadata↔circuit instance reconciliation, client↔entrypoint↔circuit alignment
**Prior audit:** [security_audit_2026-06-05.md](security_audit_2026-06-05.md) — 4 CRITICAL findings in native token

---

## Executive Summary

The native token contract is the consensus-critical token for DarkWow, handling block rewards, fee payments, private transfers, and coin destruction. The architecture is well-conceived: a burn-mint privacy model with Pedersen cumulative supply chain verification providing defense-in-depth against hidden inflation. However, the implementation has **three CRITICAL bugs** that would cause proof verification failures at runtime, plus **three HIGH-severity issues**. Of the 4 CRITICAL findings from the 2026-06-05 audit, 2 are fixed, 1 is mitigated, and 1 has a partial fix that introduced a new bug.

The three CRITICAL bugs are all in the **metadata ↔ ZK circuit instance alignment** — the host extracts a different number (or different values) of public inputs than the ZK proof was generated with. This means proof verification would fail for:
- All FeeV1 calls (11 metadata instances vs 12 circuit instances)
- All PoWRewardV1 calls (4 metadata instances vs 6 circuit instances for Mint_V1)
- All TransferV1 calls that include outputs (same 4 vs 6 mismatch for Mint_V1)
- All BurnV1 calls (signature_public value mismatch)

These are not theoretical — they are deterministic mismatches that would prevent these functions from working in the execution pipeline.

---

## Previous Audit Findings — Fix Status

### C2 (2026-06-05): FeeV1 No Value Constraint — **FIXED in circuit, BROKEN at metadata layer**

The FeeV1 ZK circuit now correctly constrains `output_value + fee == input_value` at [fee_v1.zk:128-133](src/contract/native_token/proof/fee_v1.zk#L128-L133). Range checks on all three values prevent overflow. **However**, the fix is incomplete because `fee` is not included in `fee_get_metadata`'s public inputs — see CRITICAL finding #2 below.

### C3 (2026-06-05): MintV1 No Authority — **MITIGATED via dispatch-level disable**

MintV1 is now disabled at the dispatch level ([entrypoint/mod.rs:436-439](src/contract/native_token/src/entrypoint/mod.rs#L436-L439)). Calls to opcode 0x01 return `InvalidFunction`. The implementation still exists (dead code) but cannot be reached. This is a mitigation, not a fix — the dead code should be removed to prevent accidental reactivation.

### C4 (2026-06-05): TransferV1 No Value Conservation — **FIXED**

Cross-proof value conservation using Pedersen additive homomorphism is now implemented at [entrypoint/mod.rs:565-600](src/contract/native_token/src/entrypoint/mod.rs#L565-L600). Input and output value commitments are summed per token_commit and verified equal. This correctly prevents value inflation in transfers.

### H2 (2026-06-05): Independent coin/sig secrets — **FIXED in circuit, BROKEN in client builder**

The Burn_V1 circuit now correctly derives `signature_secret = poseidon_hash(coin_secret, nullifier)` at [burn_v1.zk:87-88](src/contract/native_token/proof/burn_v1.zk#L87-L88) and binds it to the signature_public instance. However, the `BurnCallBuilder` constructs `Input.signature_public` using the ephemeral key rather than the derived key — see CRITICAL finding #3 below.

---

## CRITICAL Findings

### C1: Mint_V1 Metadata/Circuit Instance Count Mismatch (4 vs 6)

**Severity:** CRITICAL — Blocks all PoWRewardV1 and TransferV1 operations
**Affected files:**
- [mint_v1.zk:46,55-56,60,78-79](src/contract/native_token/proof/mint_v1.zk) — circuit has 6 `constrain_instance` calls
- [entrypoint/mod.rs:740-748](src/contract/native_token/src/entrypoint/mod.rs#L740-L748) — `pow_reward_get_metadata` provides 4 instances for `Mint_V1`
- [entrypoint/mod.rs:356-364](src/contract/native_token/src/entrypoint/mod.rs#L356-L364) — `transfer_get_metadata` provides 4 instances per `Mint_V1` output
- [proof.rs:56-66](src/contract/native_token/src/client/transfer_v1/proof.rs#L56-L66) — `TransferMintRevealed::to_vec()` returns 6 values

**Details:**

The Mint_V1 ZK circuit source defines 6 `constrain_instance` calls:
```
constrain_instance(C);                          // line 46: coin hash
constrain_instance(ec_get_x(coin_value_commit)); // line 55: value commit x
constrain_instance(ec_get_y(coin_value_commit)); // line 56: value commit y
constrain_instance(coin_token_id_commit);        // line 60: token commit
constrain_instance(new_cumulative_x);            // line 78: cumulative supply x
constrain_instance(new_cumulative_y);            // line 79: cumulative supply y
```

However, both `pow_reward_get_metadata` and `transfer_get_metadata` (for mint outputs) provide only 4 values:
```rust
vec![
    params.output.coin.inner(),    // 1
    *value_coords.x(),             // 2
    *value_coords.y(),             // 3
    params.output.token_commit,    // 4
    // MISSING: new_cumulative_x, new_cumulative_y
]
```

The client-side proof is generated with 6 instances via `TransferMintRevealed::to_vec()`:
```rust
vec![
    self.coin.inner(),
    *valcom_coords.x(), *valcom_coords.y(),
    self.token_commit,
    *cumcom_coords.x(), *cumcom_coords.y(),  // cumulative supply coordinates
]
```

**Impact:** The host verifies the ZK proof using metadata-extracted instances. Since the proof was created with 6 instances but the verifier provides only 4, verification will fail. This blocks:
- All PoW reward claims (no block rewards can be distributed)
- All TransferV1 operations that create output coins

**Caveat:** If the compiled `mint_v1.zk.bin` was built from an earlier version of `mint_v1.zk` that had only 4 `constrain_instance` calls (before the cumulative supply chain was added), the mismatch would NOT exist at runtime — the compiled circuit would expect 4 instances. However, this means the cumulative supply chain constraint IS NOT ACTIVE in the compiled binary, which would be a different critical bug (missing supply audit protection).

**Recommendation:** Either (a) update metadata to provide all 6 instances including cumulative supply coordinates, or (b) split the Mint circuit into two: `MintTransfer_V1` (4 instances, no cumulative chain) and `MintCoinbase_V1` (6 instances, with cumulative chain), so transfer outputs don't need to carry cumulative supply witnesses.

---

### C2: FeeV1 Metadata Missing Fee Instance (11 vs 12)

**Severity:** CRITICAL — Blocks all fee payments
**Affected files:**
- [fee_v1.zk:134](src/contract/native_token/proof/fee_v1.zk#L134) — `constrain_instance(fee)` is the 12th call
- [entrypoint/mod.rs:221-236](src/contract/native_token/src/entrypoint/mod.rs#L221-L236) — `fee_get_metadata` provides 11 instances

**Details:**

The Fee_V1 circuit has 12 `constrain_instance` calls:
```
1.  nullifier
2.  ec_get_x(input_value_commit)
3.  ec_get_y(input_value_commit)
4.  token_commit
5.  root
6.  user_data_enc
7.  signature_public_x
8.  signature_public_y
9.  output_coin
10. ec_get_x(output_value_commit)
11. ec_get_y(output_value_commit)
12. fee                              <-- NOT in metadata
```

`fee_get_metadata` provides only 11 values:
```rust
vec![
    params.input.nullifier.inner(),       // 1
    *input_value_coords.x(),              // 2
    *input_value_coords.y(),              // 3
    params.input.token_commit,            // 4
    params.input.merkle_root.inner(),     // 5
    params.input.user_data_enc,           // 6
    sig_x,                                // 7
    sig_y,                                // 8
    params.output.coin.inner(),           // 9
    *output_value_coords.x(),            // 10
    *output_value_coords.y(),            // 11
    // MISSING: fee
]
```

The client generates the proof with 12 instances via `FeeRevealed::to_vec()`:
```rust
vec![
    self.nullifier.inner(),           // 1
    *input_vc_coords.x(),            // 2
    *input_vc_coords.y(),            // 3
    self.token_commit,               // 4
    self.merkle_root.inner(),        // 5
    self.input_user_data_enc,        // 6
    *sigpub_coords.x(),              // 7
    *sigpub_coords.y(),              // 8
    self.output_coin.inner(),        // 9
    *output_vc_coords.x(),           // 10
    *output_vc_coords.y(),           // 11
    self.fee,                        // 12
]
```

**Impact:** All fee payments would fail proof verification. The network cannot collect fees. Without working fees, the network has no spam protection and miners receive no fee income (only block rewards).

**Additional concern — dual fee representation:** Even if the instance count is fixed, the fee exists in TWO places:
1. Raw transaction bytes: `let fee: u64 = deserialize(&self_.data[1..9])?;` at [entrypoint/mod.rs:454](src/contract/native_token/src/entrypoint/mod.rs#L454) — used for the fee accumulator
2. ZK proof fee instance — constrained in-circuit (`output_value + fee == input_value`)

There is no verification that these two fee values are equal. A prover could set `fee=0` in the ZK proof (no fee actually paid) but `fee=1_000_000` in the raw bytes (appearing to pay a large fee). The fee accumulator would record the raw bytes value.

**Recommendation:**
1. Add `fee` to `fee_get_metadata`'s public inputs (as the 12th element)
2. Add a check in `fee_v1` entrypoint: verify `fee_from_metadata == fee_from_raw_bytes`

---

### C3: BurnV1 Signature Public Key Mismatch (Derived vs Ephemeral)

**Severity:** CRITICAL — Blocks all BurnV1 operations
**Affected files:**
- [burn_v1.zk:87-100](src/contract/native_token/proof/burn_v1.zk#L87-L100) — circuit derives `signature_secret = poseidon_hash(coin_secret, nullifier)`
- [burn_v1.rs:106-107](src/contract/native_token/src/client/burn_v1.rs#L106-L107) — `create_burn_proof` derives and uses the hash-derived key
- [burn_v1.rs:285](src/contract/native_token/src/client/burn_v1.rs#L285) — `BurnCallBuilder` uses ephemeral key for `Input.signature_public`

**Details:**

The ZK proof constrains:
```
derived_signature_secret = poseidon_hash(coin_secret, nullifier)
constrain_equal_base(derived_signature_secret, signature_secret)
signature_public = ec_mul_base(signature_secret, NULLIFIER_K)
constrain_instance(signature_public_x)
constrain_instance(signature_public_y)
```

The `create_burn_proof` function correctly derives the signature and returns it in `BurnRevealed`:
```rust
let signature_secret = SecretKey::from(poseidon_hash([secret.inner(), nullifier.inner()]));
let signature_public = PublicKey::from_secret(signature_secret);
```

However, in `BurnCallBuilder::build()`, the returned `_revealed` is **discarded** (line 233), and the `Input` struct is constructed with the **ephemeral** key instead:
```rust
let (proof, _revealed) = create_burn_proof(...)?;  // _revealed IS DISCARDED

inputs.push(Input {
    ...
    signature_public: PublicKey::from_secret(signature_secret),  // EPHEMERAL key!
});
```

Where `signature_secret = input.ephemeral_signature_secret` (line 225), NOT the hash-derived key.

The metadata extracts `params.input.signature_public` (the ephemeral key) and provides it as ZK instances. But the proof was created with the hash-derived key. The instance values won't match → verification fails.

**Impact:** All BurnV1 operations would fail proof verification. Coins cannot be burned. TransferV1, which uses Burn_V1 proofs for inputs, would also be affected (each transfer input is a burn proof).

**Recommendation:** Use the derived `signature_public` from the proof's revealed values for the `Input` struct:
```rust
let (proof, revealed) = create_burn_proof(...)?;  // DON'T discard

inputs.push(Input {
    ...
    signature_public: revealed.signature_public,  // DERIVED key
});
```

The `ephemeral_signature_secret` field in `BurnCallInput` can then be removed, or retained for transaction-level signing (distinct from the ZK proof's key binding).

---

## HIGH Findings

### H1: SpendV1 No Value Conservation Check

**Severity:** HIGH — Potential value inflation through SpendV1
**File:** [entrypoint/mod.rs:611-654](src/contract/native_token/src/entrypoint/mod.rs#L611-L654)

**Details:**

SpendV1 is a 1-input, 1-output function. Unlike TransferV1, it does NOT verify `sum(input value_commits) == sum(output value_commits)`. The entrypoint checks:
- Token commitment is DARK (✓)
- Merkle root exists (✓)
- Nullifier not spent (✓)
- Output coin doesn't exist (✓)
- **No value conservation check** (✗)

The burn proof verifies the input coin existed, and the mint proof verifies the output coin is well-formed. But the Pedersen value commitments are not compared. A prover with one coin of any value can create an output of any value.

**Impact:** Value inflation through the SpendV1 path. An attacker with a coin worth 1 DARK could create an output worth 1,000,000 DARK.

**Note:** SpendV1 may be intended for same-value spends (like a privacy mixer), but the circuit does not enforce output_value = input_value. The entrypoint must verify this.

**Recommendation:** Add value conservation to SpendV1:
```rust
if params.input.value_commit != params.output.value_commit {
    return Err(NativeTokenError::ValueMismatch.into())
}
```

### H2: apply_spend Doesn't Update Merkle Tree

**Severity:** HIGH — Output coins from SpendV1 cannot be spent
**File:** [entrypoint/mod.rs:965-973](src/contract/native_token/src/entrypoint/mod.rs#L965-L973)

**Details:**

`apply_spend` writes the output coin to the coins DB and marks the nullifier, but does NOT call `merkle_add`:
```rust
fn apply_spend(cid: ContractId, update: SpendUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;

    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier.inner()), &[])?;
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;
    // MISSING: merkle_add for the new coin
    Ok(())
}
```

Compare with `apply_fee` which does:
```rust
wasm::merkle::merkle_add(
    info_db,
    coin_roots_db,
    NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
    NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
    &[MerkleNode::from(update.coin.inner())],
)?;
```

**Impact:** Coins created via SpendV1 are stored in the coins DB (preventing duplicate creation) but are never added to the Merkle tree. Any future attempt to spend these coins would fail the Merkle root existence check at the entrypoint level. The coins are permanently frozen.

**Recommendation:** Add `merkle_add` to `apply_spend`.

### H3: PoWRewardV1 Reward Checks Allow Above-Schedule Minting

**Severity:** HIGH — Mitigated by cumulative supply check but fragile
**File:** [entrypoint/mod.rs:792-798](src/contract/native_token/src/entrypoint/mod.rs#L792-L798)

**Details:**

The reward validation uses a one-sided check:
```rust
let expected = expected_reward(verifying_block_height);
if params.input.value < expected {
    // reject — below schedule
    return Err(NativeTokenError::ValueMismatch.into())
}
// values >= expected are accepted
```

A miner could claim `value = expected + 1_000_000` and this check would pass. However, the cumulative supply check at line 806 provides defense:
```rust
let new_supply = current_supply.saturating_add(params.input.value);
if new_supply != params.expected_cumulative_supply {
    return Err(ContractError::InvalidFunction)
}
```

This catches the over-claim because `expected_cumulative_supply` must match exactly. But the defense is fragile:
- If `expected_cumulative_supply` is computed incorrectly by the client, it can be exploited
- The two-stage check (first >=, then exact match) is inconsistent and confusing
- A future refactor could remove the cumulative supply check without realizing the first check doesn't enforce an upper bound

**Recommendation:** Change the reward check to use exact equality:
```rust
if params.input.value != expected + fees {
    return Err(NativeTokenError::ValueMismatch.into())
}
```
This makes the check self-documenting — block reward is `expected_reward(height) + fees`, no more, no less.

---

## MEDIUM Findings

### M1: Reuse of Mint_V1 Circuit for Transfer Outputs is Semantically Wrong

**Severity:** MEDIUM — Design issue, not currently exploitable
**Files:** [mint_v1.zk:62-79](src/contract/native_token/proof/mint_v1.zk#L62-L79), [transfer_get_metadata:356-364](src/contract/native_token/src/entrypoint/mod.rs#L356-L364)

**Details:**

The Mint_V1 circuit constrains the cumulative supply chain (`S_H = S_{H-1} + C_H`). This is meaningful for PoW rewards (which extend the chain) but semantically incorrect for transfer outputs (which create coins from existing value, not new supply).

Transfer outputs currently use the same `Mint_V1` circuit, which means the prover must provide cumulative supply chain witnesses for every transfer output. These witnesses are:
- `old_cumulative_value`: set to `expected_cumulative_supply.saturating_sub(value)` in the PoW reward builder
- But for transfer outputs, there's no meaningful cumulative supply to extend

The circuit would compute `new_cumulative = pedersen_commit(old_cumulative_value + output_value, ...)` — which for transfer outputs would imply total supply increased by `output_value`. This is wrong — transfers don't increase total supply.

**Recommendation:** Create a separate `MintTransfer_V1` circuit (without cumulative supply chain) for transfer outputs, and keep `MintCoinbase_V1` (with cumulative chain) for PoW rewards only.

### M2: FeeV1 spend_hook Constrained to Zero But Not Verified On-Chain

**Severity:** MEDIUM — Circuit constrains it but metadata doesn't expose it
**Files:** [fee_v1.zk:92-94](src/contract/native_token/proof/fee_v1.zk#L92-L94), [fee_get_metadata:221-236](src/contract/native_token/src/entrypoint/mod.rs#L221-L236)

**Details:**

The Fee_V1 circuit constrains `input_spend_hook == 0` (line 94), but `spend_hook` is not in the fee metadata's public inputs. The circuit constrains it internally but doesn't expose it as `constrain_instance`. This means the constraint IS enforced in-circuit (the prover can't use a non-zero spend_hook), but the verifier can't independently confirm this — they must trust the ZK proof.

This is the correct design for privacy (not revealing spend_hook), but it means the spend_hook constraint cannot be verified without trusting ZK soundness. Consider whether this should be a `constrain_instance` for defense-in-depth.

Actually, re-reading the circuit: `constrain_equal_base(input_spend_hook, ZERO)` is equivalent to a `constrain_instance(input_spend_hook)` where the instance must be zero. The spend_hook IS zero-constrained in-circuit, it's just not exposed. This is correct for privacy.

### M3: Dead Code — MintV1 Implementation Still Present

**Severity:** LOW — Dead code risk
**Files:** [entrypoint/mod.rs:660-676](src/contract/native_token/src/entrypoint/mod.rs#L660-L676), [entrypoint/mod.rs:975-993](src/contract/native_token/src/entrypoint/mod.rs#L975-L993)

**Details:**

MintV1 is disabled at dispatch but the handler (`mint_v1`) and state applier (`apply_mint`) functions still exist. Dead code risks:
- Accidental reactivation through a refactor
- Confusion about which code paths are active
- Unused code that may diverge from the rest of the system

**Recommendation:** Remove the dead code or gate it behind a compile-time feature flag.

---

## Circuit-by-Circuit Analysis

### Mint_V1 Circuit

| Property | Status | Notes |
|----------|--------|-------|
| Coin commitment (Poseidon hash) | ✓ Correct | All 7 attributes hashed |
| Value commitment (Pedersen) | ✓ Correct | Homomorphic for supply audit |
| Token commitment (Poseidon) | ✓ Correct | |
| Cumulative supply chain | ✓ Correct in source | `S_H = S_{H-1} + C_H` correctly constrained |
| Range checks (64-bit) | ✓ Correct | On value and old_cumulative_value |
| Circuit instances exposed | ⚠ Mismatch | 6 instances in circuit, 4 in metadata |
| k=11 security parameter | ✓ Adequate | Standard for this application |

### Burn_V1 Circuit

| Property | Status | Notes |
|----------|--------|-------|
| Public key derivation | ✓ Correct | `pub = coin_secret * K` |
| Coin reconstruction | ✓ Correct | Uses derived pubkey coordinates |
| Nullifier computation | ✓ Correct | `poseidon_hash(coin_secret, coin)` |
| Value commitment (Pedersen) | ✓ Correct | |
| Token commitment (Poseidon) | ✓ Correct | |
| Merkle proof with zero_cond | ✓ Correct | Handles dummy inputs |
| Per-burn signature derivation | ✓ Correct | `poseidon_hash(coin_secret, nullifier)` fixes H2 |
| Signature binding to instances | ✓ Correct | Both coordinates are constrain_instance'd |
| Range check (64-bit) | ✓ Correct | |
| Circuit instances exposed | ⚠ Value mismatch | Derived vs ephemeral signature_public in client |

### Fee_V1 Circuit

| Property | Status | Notes |
|----------|--------|-------|
| Input coin derivation | ✓ Correct | Standard pattern |
| Nullifier computation | ✓ Correct | |
| Value commitment (Pedersen) | ✓ Correct | For both input and output |
| Token commitment (Poseidon) | ✓ Correct | |
| Merkle proof | ✓ Correct | |
| Spend hook = 0 | ✓ Correct | Enforced in-circuit |
| Fee conservation | ✓ Correct | `output_value + fee == input_value` |
| Range checks (64-bit) | ✓ Correct | On input, output, and fee |
| Circuit instances exposed | ✗ Missing fee | 12 instances in circuit, 11 in metadata |
| Signature binding | ✓ Correct | Both coordinates constrain_instance'd |

---

## Fitness for Purpose Assessment

### Consensus Reward Distribution (PoWRewardV1)

| Requirement | Status | Notes |
|-------------|--------|-------|
| Accurate emission schedule enforcement | ⚠ Partial | H3: allows above-schedule values (mitigated) |
| Cumulative supply audit trail | ⚠ Broken | C1: metadata/circuit mismatch prevents verification |
| Token ID restriction (DARK only) | ✓ | Enforced at entrypoint |
| Double-claim prevention | ✓ | Nullifier/coin uniqueness checks |
| Inflation detection | ⚠ Broken | C1 breaks the verification pipeline |

### Network Fee Payment (FeeV1)

| Requirement | Status | Notes |
|-------------|--------|-------|
| Deterministic fee collection | ✗ Broken | C2: metadata/circuit mismatch |
| Value conservation (no inflation) | ✓ in circuit | Fixed from 2026-06-05 C2 |
| Spend hook = 0 enforcement | ✓ in circuit | |
| Spam protection | ✗ Broken | Fees can't be verified |
| Fee accumulator per block | ✓ | Model supports it |

### Privacy (TransferV1, SpendV1, BurnV1)

| Requirement | Status | Notes |
|-------------|--------|-------|
| Hidden values (Pedersen) | ✓ | |
| Hidden token IDs | ✓ | |
| Double-spend prevention (nullifiers) | ✓ | |
| Merkle inclusion proofs | ✓ | |
| Encrypted notes (AEAD) | ✓ | |
| Unlinkable burns | ✓ in circuit | Per-burn signature derivation |
| Value conservation (transfers) | ✓ Fixed | C4 from 2026-06-05 |
| Value conservation (spends) | ✗ Missing | H1: no conservation check |
| Output coin spendability | ✗ Broken | H2: Merkle tree not updated |

---

## Root Cause Analysis

The three CRITICAL findings share a common root cause: **the metadata extraction functions and the ZK circuit source files evolved independently without reconciliation testing**.

The ZK circuits were enhanced with:
- Cumulative supply chain constraints (mint_v1.zk: +2 constrain_instance calls)
- Fee value constraint (fee_v1.zk: +1 constrain_instance call)
- Per-burn signature derivation (burn_v1.zk: changed semantics of signature_public)

But the metadata extraction functions (`fee_get_metadata`, `pow_reward_get_metadata`, `transfer_get_metadata`) were not updated to match. The test harness tests proof creation (which works fine — the prover uses the correct instance count) but does NOT test the full verification pipeline (metadata extraction → host verification). This gap allowed the metadata/circuit misalignment to go undetected.

Additionally, the `BurnCallBuilder` discarded the proof's revealed values (`_revealed`) and reconstructed the `Input` from scratch using different keys, introducing the signature_public mismatch.

---

## Recommendations (Priority Order)

### Immediate (blocks functionality):
1. **Fix C2**: Add `fee` to `fee_get_metadata`'s public inputs (12th element)
2. **Fix C1**: Either (a) add cumulative supply coordinates to metadata, or (b) create separate circuits for transfer outputs vs coinbase
3. **Fix C3**: Use `revealed.signature_public` from the proof in BurnCallBuilder, not the ephemeral key

### High priority:
4. **Fix H1**: Add value conservation check to SpendV1 entrypoint
5. **Fix H2**: Add `merkle_add` to `apply_spend`
6. **Fix H3**: Change reward check to exact equality
7. **Add integration test**: End-to-end test that creates a proof, extracts metadata, and verifies the proof against metadata instances

### Medium priority:
8. **Fix M1**: Create separate `MintTransfer_V1` circuit without cumulative supply chain
9. **Remove dead code**: Delete `mint_v1` handler, `apply_mint`, and unify the MintV1 dispatch
10. **Add CI check**: Build script that verifies metadata instance counts match circuit `constrain_instance` counts

---

## Verification Status

| Test | What it tests | Catches CRITICAL bugs? |
|------|--------------|------------------------|
| `test_mint()` | Coin creation + serialization | No |
| `test_pow_reward_call_builder()` | Proof creation + serialization | No |
| `test_burn_call_builder()` | Proof creation (commented out) | No |
| `zk_audit.rs` | Circuit namespace decode | No |
| unit tests (`integration.rs`) | Enum + struct creation | No |

**None of the existing tests verify the metadata↔circuit instance alignment.** This is the root cause of all three CRITICAL findings going undetected.

---

*Audit conducted by Claude Opus 4.8 — independent adversarial review of ZK constraints and fitness for purpose. All findings verified against source code at the referenced line numbers.*
