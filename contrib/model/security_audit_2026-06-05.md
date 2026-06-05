# Smart Contract Security Audit — Double-Spend & Infinity-Mint Hardening

**Date:** 2026-06-05
**Scope:** All smart contracts under `src/contract/` (30 contracts, 144 ZK circuits)
**Method:** 7-dimensional adversarial audit (finder + verifier agent per dimension)
**Result:** 9 confirmed bugs (4 CRITICAL, 4 HIGH, 1 MEDIUM)

---

## CRITICAL Bugs

### C1: PromissoryNote Mint_V1 circuit — `mint_public` unconstrained

**File:** `src/contract/promissory_note/proof/mint_v1.zk:36`
**Entrypoint:** `src/contract/promissory_note/src/entrypoint/mod.rs:526-533`

The `mint_public` witness is exposed via `constrain_instance` but has NO circuit-level constraint proving `mint_public = poseidon_hash(backing_secret)`. The circuit comment on line 14 states this constraint but the code does not implement it — there is no `backing_secret` witness at all.

The entrypoint check compares `params.mint_public != stored_auth`. But `stored_auth` is publicly readable from the on-chain token registry (`db_get(token_registry_db, token_id)`). Since `mint_public` is unconstrained, any prover can:
1. Read `stored_auth` from the token registry
2. Set `mint_public = stored_auth` as a witness
3. Generate a valid ZK proof
4. Bypass the authorization check entirely

**Exploit:** Mint unlimited tokens of ANY registered token type without knowing the backing secret.

**Root cause:** The ZK circuit was written assuming the `poseidon_hash(backing_secret)` constraint existed but it was never implemented. The Rust client (`client/mint_v1.rs:112`) computes `mint_public = poseidon_hash([self.input.mint_secret])` off-circuit and passes the result as a free witness.

**Fix:** Add `backing_secret` witness to the circuit and constrain `mint_public = poseidon_hash(backing_secret)`.

---

### C2: NativeToken FeeV1 ZK circuit — no `output_value = input_value - fee` constraint

**File:** `src/contract/native_token/proof/fee_v1.zk`
**Entrypoint:** `src/contract/native_token/src/entrypoint/mod.rs:444-500`

The FeeV1 ZK circuit has zero constraint linking `input_value` to `output_value`. The fee subtraction is computed off-circuit in the Rust client (`client/fee_v1.rs:288`). The circuit uses `input_value` in the input coin hash and `output_value` in the output coin hash, but imposes no relationship between them. There is no `fee` witness at all in the circuit.

The FeeV1 entrypoint has a 1-in-1-out structure but no cross-proof value conservation check (unlike PromissoryNote's `verify_value_conservation()`).

**Exploit:** Create an output coin with `output_value = input_value + 1,000,000` — the ZK proof verifies, the entrypoint has no mechanism to detect the inflation.

**Root cause:** The circuit was designed assuming the 1-in-1-out structure provided implicit conservation. But without a circuit-level constraint or cross-proof Pedersen sum check, the values are independent.

**Fix:** Either (a) add `output_value = input_value - fee` constraint in the ZK circuit, or (b) add cross-proof value conservation check in the entrypoint using Pedersen additive homomorphism (as PromissoryNote does).

---

### C3: NativeToken MintV1 — no authority check, no supply tracking

**Files:**
- `src/contract/native_token/src/entrypoint/mod.rs:616-632` (mint_v1)
- `src/contract/native_token/src/entrypoint/mod.rs:890-908` (apply_mint)
- `src/contract/native_token/proof/mint_v1.zk` (circuit)

The `mint_v1` function performs exactly ONE check: the coin doesn't already exist in the coins DB. There is no authority check, no supply cap check, and no emission schedule enforcement. `apply_mint` never touches `NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY`.

Compare with `pow_reward_v1` which enforces:
- Token ID == DRKW_TOKEN_ID
- Value ≥ expected_reward(height)
- Cumulative supply == expected_cumulative_supply

**Exploit:** Generate a valid `Mint_V1` ZK proof for a coin of arbitrary value. The proof only constrains well-formedness (coin hash, value commitment, 64-bit range check). The entrypoint accepts it and adds the coin to the Merkle tree. TOTAL_SUPPLY is never updated, making the inflation invisible to supply auditing.

**Fix:** Either remove MintV1 if it's not intended for production use, or add: (a) authority check (who can mint), (b) supply tracking (update TOTAL_SUPPLY), (c) supply cap enforcement.

---

### C4: NativeToken TransferV1 — no value conservation check

**File:** `src/contract/native_token/src/entrypoint/mod.rs:506-561`

NativeToken's TransferV1 does NOT verify `sum(input value commits) == sum(output value commits)`. Each burn proof and each mint proof verifies independently, but there is no cross-proof sum check using Pedersen additive homomorphism.

PromissoryNote's `verify_value_conservation()` (entrypoint/mod.rs:435-476) implements this check correctly. NativeToken does not.

**Exploit:** A prover with one input coin (any value) can create multiple output coins. The burn proof proves the input existed, each mint proof proves output well-formedness, but nothing links input sum to output sum.

**Fix:** Implement `verify_value_conservation()` in NativeToken's TransferV1 (same pattern as PromissoryNote).

---

## HIGH Bugs

### H1: Same-block double-spend via isolated execution overlays

**File:** `bin/dwowd/src/execution.rs:113-135, 265-272`

Every contract call in a block receives `base_overlay.clone()` — an independent copy of the pre-block state. No call sees any other call's state changes during execution. Diffs are merged post-hoc with `main_overlay.add_diff(diff)` which silently overwrites duplicate keys.

The mempool (`bin/dwowd/src/mempool.rs:74`) only deduplicates by exact transaction hash, not by nullifier or coin.

**Exploit:** Submit two distinct transactions spending the same coin. Both pass exec-phase nullifier checks (base state shows nullifier unspent). Both writes land in the merge. The same coin is effectively spent twice.

**Fix:** (a) Add semantic deduplication to the mempool (reject transactions with duplicate nullifiers), or (b) add conflict detection in the merge phase (reject blocks with conflicting diffs), or (c) switch to a shared-overlay execution model where each call sees prior calls' writes.

---

### H2: Independent coin_secret and signature_secret in burn circuits

**Files:**
- `src/contract/native_token/proof/burn_v1.zk` (lines 15, 30, 38, 82)
- `src/contract/promissory_note/proof/burn_v1.zk` (lines 14, 29, 35, 79)

Both burn circuits have separate `coin_secret` (for nullifier) and `signature_secret` (for transaction signing) witnesses with no cross-constraint. The coin owner and the transaction signer can be different entities.

In native_token: `pub = ec_mul_base(coin_secret, K)` and `signature_public = ec_mul_base(signature_secret, K)` — no constraint linking `pub` to `signature_public`.

In promissory_note: `pub = poseidon_hash(coin_secret)` and `signature_public = poseidon_hash(signature_secret)` — no constraint linking `pub` to `signature_public`.

**Exploit:** A prover who knows secret_A (coin owner) can authorize a burn while someone with secret_B signs the transaction. The coin owner's authorization is effectively delegated without on-chain evidence.

**Fix:** Add `constrain_equal(coin_secret, signature_secret)` in the burn circuits, or derive both from a single master secret.

---

### H3: BearerBond IssueStakeV1 — no issuer authority check

**File:** `src/contract/bearer_bond/src/entrypoint/mod.rs:574-596`

The `issue_stake_v1` function checks only: (a) the series exists, (b) the coin doesn't already exist. It does NOT verify that the caller is the series issuer. `BondSeriesInfo` has an `issuer_contract: ContractId` field (model/mod.rs:164) but it's never compared against any caller-derived value.

Additionally, `total_staked` is never incremented when stake coins are issued, distorting interest calculations in `request_interest_v1` (line 702-706).

**Exploit:** Anyone who knows a valid `token_id` for an existing series can mint unlimited stake coins for that series.

**Fix:** Add issuer verification (compare caller's contract ID against `series_info.issuer_contract`) and increment `total_staked` when issuing.

---

### H4: Bridge WithdrawV1 — no Merkle root verification

**Files:**
- `src/contract/bridge/src/entrypoint.rs:797-799` (on-chain: `_deposits_db` assigned but unused)
- `src/contract/bridge/proof/withdraw_v1.zk:41-43` (circuit: `merkle_root_val` is not `constrain_instance`d)

The on-chain code looks up the deposits DB but assigns it to `_deposits_db` (underscore = unused). There is no `db_contains_key` check verifying the deposit commitment exists. The comment at line 798 says "In production, we would verify the merkle proof here."

The ZK circuit computes `computed_root = sparse_merkle_root(leaf_index, merkle_path, deposit_leaf)` and constrains `constrain_equal_base(computed_root, merkle_root_val)`. But `merkle_root_val` is a prover-provided witness — it is never `constrain_instance`d. The prover can set `merkle_root_val = computed_root` and the constraint always passes.

**Exploit (moderated by child transfer requirement):** The attacker must own tokens to burn in the child PromissoryNote transfer. The bridge's deposit↔withdrawal accounting is the unverifiable link.

**Fix:** (a) Add `constrain_instance(merkle_root_val)` in the ZK circuit, (b) verify `db_contains_key(deposits_db, &serialize(&computed_root))` on-chain, (c) store and verify the merkle root against the bridge's on-chain deposit tree root.

---

## MEDIUM Bugs

### M1: Stablecoin AccrueInterestV1 — old_total_debt not validated against on-chain state

**File:** `src/contract/stablecoin/src/entrypoint.rs` (AccrueInterestV1 handler)
**Circuit:** `src/contract/stablecoin/proof/accrue_interest_v1.zk`

The `old_total_debt` value used in interest computation is not validated against the on-chain `STABLECOIN_SUPPLY` value. The ZK proof computes `new_total_debt = old_total_debt + interest` but `old_total_debt` is a prover-provided witness. If the prover supplies a stale (lower) `old_total_debt`, the interest accrued would be computed on a smaller base than reality.

**Fix:** Add `constrain_instance` for `old_total_debt` and verify it matches on-chain `STABLECOIN_SUPPLY` in the entrypoint.

---

## False Positives (verified safe)

| Finding | Why Safe |
|---------|----------|
| D1.3 PromissoryNote registry root replay | Root freshness check at line 541 is effective against proof replay |
| D1.4 TokenMintV1 token_auth_parent binding | `constrain_instance(token_auth_parent)` is present at circuit line 37 |
| D1.2 GenesisMintV1 dead code | Not callable (no function ID, no dispatch arm) — dead code, not attack surface |
| D2.3 BurnV1 pure destruction | Correctly gated by nullifier + Merkle root + ZK ownership proof |
| D3.1 Subscription nullifier write | The write is a subscription-ID placeholder, not a security nullifier |
| D3.3 Non-token Merkle root checks | Other contracts use flat DB lookups, not Merkle trees — equivalent security |
| D4.1 Stablecoin child call validation | Triple defense: function ID check (line 868) + contract ID check (line 875) + value commit (line 884) |
| D4.3 Game contract mint authority | Game contracts use transfer child calls, never mint — hold no mint authority |
| D4.4 Spend hook callback | Composability by design; target contracts self-verify caller (Stablecoin line 505) |
| D5.3 signature_public constraint | `signature_public = poseidon_hash(signature_secret)` IS computed in-circuit (burn_v1.zk:79) |
| D6.2 PromissoryNote coin count tracking | Correct for privacy model (values hidden behind Pedersen commitments) |
| D7.2 Stablecoin apply-phase re-check | Effective defense-in-depth within single call, but ineffective against cross-call attacks |
| D7.3 apply_mint re-verification | No race possible — exec and apply share the same thread and overlay |
| D7.4 Merkle root update timing | All three writes execute atomically within a single synchronous WASM call |

---

## Severity Summary

| ID | Bug | Severity | Exploitable Today? |
|----|-----|----------|--------------------|
| C1 | PromissoryNote mint_public unconstrained | CRITICAL | Yes — any registered token |
| C2 | NativeToken FeeV1 no value constraint | CRITICAL | Yes — every fee payment |
| C3 | NativeToken MintV1 no authority/supply | CRITICAL | Yes — any valid proof |
| C4 | NativeToken TransferV1 no conservation | CRITICAL | Yes — every transfer |
| H1 | Same-block double-spend (isolated overlays) | HIGH | Yes — submit two txs |
| H2 | Independent coin/signature secrets | HIGH | Yes — any burn |
| H3 | BearerBond IssueStakeV1 no authority | HIGH | Yes — known token_id |
| H4 | Bridge WithdrawV1 no Merkle verification | HIGH | Yes (moderated by child call) |
| M1 | Stablecoin old_total_debt unvalidated | MEDIUM | Yes — stale debt base |
