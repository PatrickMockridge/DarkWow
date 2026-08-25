# Fee System — Cross-Stack Coordination Safety

This document analyzes the unique safety challenges of the fee signalling
system: a universal coordination mechanism where every node, wallet, miner,
and contract MUST produce identical results from the same chain state.

The general testing taxonomy and safety patterns are defined in
`doc/src/dev/testing/overview.md` and `doc/src/arch/type-system.md`. This
document focuses on challenges specific to the fee system.

## 1. Sympatico Requirement

The fee signalling system is the **universal coordination mechanism** across
the DarkWow stack. It has no isolated components:

- The **wallet** computes fees from `fee_window_flags` in the block header
- The **mempool** admits transactions based on thresholds set by the miner
- The **miner** adjusts thresholds at window boundaries based on mempool
  queue depths, decrypts encrypted fees, and builds FeeCollectV1
- The **contract** (native_token) verifies the Pedersen accumulator matches
  the miner's claimed `total_fees` and resets it to Identity

A divergence in **any** of these components IS a consensus failure. A wallet
computing different CFs than the miner expected produces a fee below the
admission threshold → transaction rejected. A miner computing different
`total_fees` than another miner produces a different FeeCollectV1 →
different block hash → chain fork.

**No component has "local" fee parameters.** Every value that affects fee
computation must be derivable from chain state that all nodes agree on.
This is the architectural principle defined in `fee-spec.md` §13.1.

## 2. Genesis-Initiated, Window-Updated

All fee parameters start at genesis and evolve through the PID controller
every 20 blocks (`WINDOW_SIZE`). No node has private fee parameters.

This means:
- A node joining the network at height 1000 reads `fee_window_flags` from
  block 1000's header and derives the correct CFs — no "catch-up" logic needed
- A wallet that goes offline for 3 windows re-syncs to the current flags on
  its next block — the flags in the header ARE the current state
- A miner that restarts reads the `FeeWindowState` from its persisted sled
  database — the state was saved at the last window boundary

A node that cannot sync to the current window's parameters cannot participate.
There is no "offline mode" for fee computation — `FeeWindowFlags::default()`
(identity CF) is correct only at genesis.

## 3. Privacy-Preserving Dual Channel

Fees travel through two channels with different visibility:

**Public channel — `fee_window_flags` (block header).** Encodes congestion
direction (hold/+10%/-10%) for both circuit execution CF and WASM storage CF.
All nodes can read these. They are advisory signalling, not consensus-validated
(G5: `accept_block` does not reject invalid flags). They are excluded from the
block hash to prevent circular dependency (flags depend on mempool state, block
hash depends on header).

**Private channel — `encrypted_fee_value` (FeeParamsV2).** The exact fee
amount encrypted to the miner's per-block public key via ECDH + ChaCha20-Poly1305.
68-byte format: `[ephemeral_public(32)] [nonce(12)] [ciphertext+tag(24)]`.
Only the miner can decrypt. The miner's key is per-block derived
(`derive_instance(NATIVE_TOKEN, height)`) so encrypted fees cannot be
correlated across blocks by public key.

**The private channel MUST be functional.** An empty `encrypted_fee_value` is
a malformed transaction (fee-spec.md SPEC-5). Without it:
- The miner cannot learn exact fee amounts → `total_fees` is wrong →
  FeeCollectV1 Pedersen check fails → block rejected
- Fee privacy collapses: all fees are either revealed (if the fallback is
  used) or unknown (if neither the encrypted channel nor the fallback works)

**Testing the dual channel requires:**
- L1 unit: encrypt/decrypt roundtrip with real AEAD (G2 test)
- L1.5 bridge: FeeV2 with non-empty `encrypted_fee_value` through accept_block
- L3 Docker: wallet encrypts → miner decrypts → FeeCollectV1 verifies

## 4. Risk Transfer — Miners Underwrite Execution Risk

The `RiskFactor` system transfers contract execution risk from users to miners.
A contract with "self_declared" status (no attestation, no endowment) pays a
1.5× risk multiplier on its circuit component. A genesis contract pays 1.0×.
Miners are compensated for underwriting execution risk through higher fees.

This requires:
- **Per-contract CostProfile resolution.** The miner must look up each
  contract's `[[cost_profiles]]` from its manifest to determine
  `circuit_difficulty`, `k_value`, and `wasm_kb`.
- **Risk factor tracking.** The `ContractRiskTracker` records observed-vs-declared
  cost deviations per contract per window. Contracts that systematically
  under-declare circuit difficulty face escalating risk factors (1.25× → 1.5× →
  2.0× capped).
- **Dynamic escalation.** Risk factors update at window boundaries based on
  the tracker's `evaluate_window()` output.

**Current state (2026-08):** The `RiskFactor` type is specified in
`type-system.md` §2.3.1. `compute_total_fee()` (the risk-aware formula) is
implemented and tested in `fee_window.rs` but has zero production call sites.
`ContractRiskTracker` exists only in the Python reference model
(`contrib/model/fee_window_model.py`). The miner uses hardcoded
`compute_fee(&[1000], 1, ...)` for all thresholds. This is tracked as
red team findings H8, H9, H10, M2.

## 5. Loud Failures — Diagnostic Surface

Every divergence from expected behavior in the fee system SHALL produce a
diagnostic. Silent failures are the primary attack vector identified by the
2026-08 red team audit.

**Anti-pattern:** `decrypt_fee_for_miner() -> Option<u64>`
All failure modes (empty ciphertext, wrong key, corrupted data, AEAD tag
mismatch) collapse to `None` with zero diagnostic information. The caller
cannot distinguish "fee encrypted to a different miner" from "wallet hasn't
wired encryption yet" from "ciphertext corrupted in transit."

**Required pattern:** `decrypt_fee_for_miner() -> Result<u64, FeeDecryptError>`
with distinct variants:
- `EmptyCiphertext` — `encrypted_fee_value.len() < 68` (not yet wired)
- `InvalidEphemeralKey` — cannot parse ephemeral public key
- `DecryptionFailed` — AEAD tag verification failed (wrong key or corrupted)
- `InvalidFeeBytes` — decrypted bytes cannot be parsed as u64

The caller logs: `warn!("FeeV2 decrypt failed for tx {}: {:?}", tx_hash, err)`

**Other diagnostic requirements (fee-spec.md SPEC-3):**
- `FeeParamsV2::decode` failure → `warn!` with transaction hash
- Congestion measurement returning 0 due to lock contention → `warn!`
- `verify_threshold_proof()` rejection → reason logged (threshold mismatch,
  proof verification failure, malformed params)
- `FeeCollectV1` accumulator mismatch → hard error with expected vs actual

## 6. Testing Strategy

The fee system requires testing at every level of the taxonomy
(`doc/src/dev/testing/overview.md`):

| Level | What It Tests | Fee-Specific Concerns |
|-------|--------------|----------------------|
| **Python Model** | Executable specification — 69 tests in `fee_window_model.py` | Full lifecycle scenarios, feedback loop, edge cases |
| **L1 (Unit)** | Pure functions: `compute_fee()`, `CongestionFactor`, `WindowSignalling`, `encrypt_fee_for_miner()` | Deterministic integer arithmetic, no floats |
| **L1.5 (Bridge)** | Production path: real ZK proofs + AEAD + accept_block + wallet scan | Full fee lifecycle: wallet→mempool→miner→FeeCollectV1 |
| **L2 (Heavyweight)** | Multi-block chain: window boundaries, cross-window CF propagation, multi-contract fee differential | 20+ blocks to trigger window boundary, real ZK coinbases |
| **L3 (Docker)** | End-to-end: wallet container → mining nodes → block production → wallet scan | Real RandomX, real P2P, 120s block times |
| **Benchmark** | Proof timing: FeeThreshold_V1, Fee_V2, FeeCollect_V1 | Confirm proof generation < window boundary deadline |

**The Python model is the specification** (`python-model-is-the-spec`).
Every Rust implementation SHALL match a Python model scenario. Changes to
fee logic SHALL update the Python model first, then the Rust implementation.

**The L1.5 bridge is the MoC gate.** All L1.5 tests SHALL pass before any
code proceeds to the Docker pipeline. They enforce that the production code
path (real ZK proofs, real AEAD, real accept_block, real wallet scan) is
functional before introducing real networking and real PoW.

## 7. Red Team Findings (2026-08 Summary)

A combined wallet + miner red team audit identified 3 CONSENSUS-CRITICAL,
~10 HIGH, ~7 MEDIUM, and ~5 LOW anti-patterns. The root cause across all
findings: **compile-time constants substituting for chain-synced values,
silent fallbacks on consensus-critical paths, and functionality defined
but unwired with fallback defaults.**

Full findings are documented in the implementation plan. Key remediation
items:

1. Wire `encrypted_fee_value` — paired wallet + miner change (C1)
2. Remove `#[cfg(feature = "fee-window")]` feature gate (C3)
3. Unify fee estimate paths — single chain-derived value (C2)
4. Replace `try_lock().unwrap_or(0)` congestion measurement (H4)
5. Add diagnostic surface to `decrypt_fee_for_miner()` (H1, H3)
6. Wire `compute_total_fee()` and `resolve_cost_profile()` (H8, H9)
7. Implement `extract_tx_wasm_kb()` for DeployV1 (H5)
8. Port `ContractRiskTracker` from Python (M2)

## 8. SetMembership Public-Input Soundness (HAZOP)

**Date:** 2026-08-16
**Scope:** `oracle/proof/push_value_commitment.zk` — the only circuit using the
`set_membership` zkas opcode. Its heavyweight test fails at `accept_block` with
`invalid proof: call[0] namespace 'PushValueCommitmentV2'`.

### 8.1 Top event

The proof is **created** successfully (`plonk::create_proof`) but **fails
verification** (`verify_zkp`). A real prover rejects unsatisfied constraints, so
"creates but fails to verify" signals a prover/verifier desync, not a witness-value bug.

### 8.2 Guide-word table

| # | Deviation | Verdict |
|---|-----------|---------|
| H1 | Wrong witnesses (commitment/path/data_root) | RULED OUT |
| H2 | Mock/non-enforcing prover | RULED OUT — `Params::new(k)` is deterministic (`hash_to_curve("Halo2-Parameters")`) and functional |
| H3 | Stale `.zk.bin` (PK ≠ VK) | RULED OUT — harness + WASM both `include_bytes!` the same file |
| H4 | Host SMT hasher ≠ circuit hasher | RULED OUT — both `P128Pow5T3, ConstantLength<2>, (3,2)` |
| H5 | `SetMembership` constrains an extra public input | CONFIRMED — `vm.rs:1219-1224` |
| H6 | Redundant explicit `constrain_instance(data_root)` | CONFIRMED — `.zk:129` |
| H7 | Public-input ORDER mismatch | RULED OUT — bytecode dump + diagnostic prove order matches |
| H8 | `constrain_equal_base` compiled to no-op | RULED OUT — opcode present in bytecode |

### 8.3 Definitive findings

- Bytecode dump (`zkas -e`): circuit public inputs are
  `[data_root(set_membership), oracle_id, commitment, data_root(explicit),
  tx_binding, tx_nonce]` — six values.
- Diagnostic (`eprintln` in harness + `verify_zkp`): the proof's `to_vec()` and the
  verifier's `instances` **match exactly** (all values equal).
- `Params::new(k)` (`vendor/halo2/halo2_proofs/src/poly/commitment.rs:38`) is
  **deterministic**, so prover and verifier share the same SRS.

### 8.4 Root cause (partial) and unresolved remainder

The `set_membership` opcode internally constrains its `expected_root` argument as a
public input — a surprising, undocumented extra instance the client/metadata must
duplicate (H5/H6). That is a real maintainability/soundness hazard.

**However** — aligning the public inputs (both a 6-value `set_membership` version and
a 5-value `sparse_merkle_root` rewrite) does **not** resolve the `invalid proof`.
With matching public inputs, correct witnesses, a deterministic SRS, and matching
hashers, the proof still fails verification. The remaining cause is an unresolved
prover/verifier desync specific to the `SparseMerklePath` / merkle-opcode path and is
**still open** — it needs a halo2 MockProver constraint trace or a VM-level trace,
outside this fix's scope (the VM is off-limits).

### 8.5 Remediation

- `push_value_commitment.zk` rewritten to use `sparse_merkle_root` + explicit
  `constrain_equal_base(computed_root, data_root)` (the green `bridge` pattern)
  instead of `set_membership`, reducing public inputs 6 → 5 and removing the
  undocumented extra instance.
- Client `to_vec()` (`client/push_value_commitment.rs`) + `get_metadata`
  (`entrypoint.rs`) aligned to 5 values.

### 8.6 Verification result (verbatim)

```
accept_block at height 5: L2 proof verify ... invalid proof: call[0] namespace 'PushValueCommitmentV2'
test tests::heavyweight_pipeline::test_heavyweight_oracle ... FAILED
```

### 8.7 Soundness note — Merkle membership is a box/purse pattern

Merkle membership (`set_membership` / `sparse_merkle_root` over a `SparseMerklePath`) is
a **box/purse pattern**: use it only when the contract actually maintains the tree it is
proving membership in. In `push_value_commitment` the "data tree" was never contract
state (apply was a no-op; no `data` tree exists in `init_contract`), so membership against
a caller-supplied `data_root` proved nothing. **Resolution:** the membership proof was
removed entirely (Option A) — the circuit now proves only commitment-correctness
(`commitment == poseidon_hash(4, value, nonce)`) and staker authorization. Before adding
a membership proof to any contract, confirm the tree is real contract state.

### 8.8 Deterministic ZK mode is a zero-knowledge disabler (DZ-4)

The heavyweight determinism check (PI-7) requires byte-identical proofs, achieved by
seeding the prover RNG (`StdRng::seed_from_u64(0)`) under a `deterministic_zk` flag. That
seed makes the blinding factors deterministic and publicly known, so the proofs are
**not zero-knowledge** — an observer who knows the seed can unblind commitments and
recover the witness (secret key, private value).

Safe only because the mode is **compile-time gated** (`heavyweight-spec.md` §7.4 DZ-4):
`enable_deterministic_zk()` and the flag live behind the `deterministic-zk` cargo feature
which only the test harness enables; the wallet and WASM never enable it, and in those
builds `deterministic_zk_enabled()` always returns `false`.

The legacy un-gated `pub fn enable_deterministic_zk()` pattern (in `bridge` and the other
swept contracts) violates DZ-4 and SHALL be remediated across all contracts — especially
genesis contracts.

## 9. Attestation sweep — `metadata-decode-zkp` root cause + over-engineering removal (2026-08)

### 9.1 The `fn_code` in the error is a red herring

`accept_block` reported `fn_code=0x01` (looked like `RevokeAttestationV1`) at the
`metadata-decode-zkp` stage. The `fn_code` printed there is `job.call_data.first()`
(`src/linear/src/execution.rs`), which for a non-native-token call is the **first byte of
the serialized `DarkLeaf` call tree** — i.e. the `VarInt(1)` length prefix, **not** the
function selector. The actually-failing function was `DelegateAttestationV1` (0x08).
Lesson: a diagnostic that prints `call_data.first()` as `fn_code` is only valid for
`native_token` (raw call data); every other contract receives the wrapped call tree, so
the first byte is the tree's length prefix.

### 9.2 Root cause — stale fixed-size guard in a params decode

`DelegateAttestationParamsV1::decode` guarded `data.len() < fixed_start + 266`, but its
own `encode` writes only `fixed_start + 233` bytes (12 fixed fields). The decode therefore
always failed, `get_metadata` fell into its error branch (`set_return_data(&vec![])`), and
the host decoded empty metadata → `UnexpectedEof`. This is the same class as the other
V1→V2 encode/decode drift bugs: a length constant left stale when fields were stripped.

### 9.3 Over-engineering removal (same principle as §8.7)

The revocation-tree / delegation-chain merkle machinery had already been stripped from the
V2 circuits (`set_membership`/`sparse_merkle_root` gone); only dead caller-supplied params
remained. These guard nothing (the contract has no revocation/chain merkle tree), so they
were removed, not patched:

- `VerifyClaimParamsV1.revocation_root`
- `DelegateAttestationParamsV1` `revocation_root`/`chain_root`/`chain_depth`/`max_depth`/
  `delegator_stake`/`delegatee_stake`
- `VerifyChainParamsV1` `chain_root`/`current_depth`/`max_depth`
- `UpdateDelegationParamsV1` `current_depth`/`max_depth`/`delegator_stake`/`delegatee_stake`

**Correction:** `CheckNotRevokedParamsV1.revocation_root` is NOT dead — the exec uses it
for replay protection (`proof_hash = poseidon_hash([nonce, revocation_root])`). A removal
list that does not audit each field's exec usage is unsafe; verify per-field before deleting.

## 10. Multisig H-5 — deletion is a tombstone, and the test reader must match (2026-08)

`db_del` (contract) → host `db_remove` does **not** actually remove the key; it writes an
empty value (`insert(&ck, &[])`) as a deletion tombstone. The backend `db_get`/
`db_contains_key` treat empty values as "not found", so replay protection works in the
contract. The heavyweight test's `query_contracts_tree` read the sled tree directly and
returned `Some(empty)` for the tombstone, so `verify_state`'s `is_some()` check wrongly saw
the consumed signature as still present (HAZOP H-5).

**Fix:** `query_contracts_tree` now maps empty values to `None`, mirroring the backend's
empty-as-deletion semantics. Lesson: any test-side state reader MUST replicate the
backend's tombstone semantics (`empty == absent`), or deletion-verifying `verify_state`
closures will report false positives.

## 11. Native Token HAZOP + WYSIWYG Spec→Code→Test Traceability (2026-08)

### 11.1 Baseline (verbatim)

- `test_heavyweight_native_token` — **FAIL**: `accept_block at height 3 … fn_code=0x02 …
  ContractError(Custom(14))` = `TransferMerkleRootNotFound`. FeeV2 (height 2) accepted; the
  BurnV1 commitment spends a commitment whose on-chain merkle root is not reproduced by the harness.
- `test_heavyweight_fee_v2` (+`_box`, `_deploy`) — **PASS** (3/3).
- `fee_extractor` — **PASS** (19/19). `nt_unit` — **PASS** (34/34).
- `cargo test` (without `--lib`) — **pre-existing doctest failure** `E0463 can't find crate for
  dwow_chain / dwow_mempool` in `bin/dwowd/src/lib.rs` doctests (unrelated to native_token).

### 11.2 Spec→Code→Test Traceability Matrix

Verdict = PASS (code implements AND a test asserts at ≥ the required L1/L1.5/L2/L3),
WARN (implemented but untested/under-level), FAIL (not implemented / wrong).

| Invariant | Verdict | Code anchor | Test anchor |
|-----------|---------|-------------|-------------|
| FI-GEN-1 genesis fee params | PASS | `src/linear/src/fee_window.rs` FeeWindowState | `fee_integration_spec.rs` IT-1 |
| FI-GEN-2 no compile-time fee consts | PASS | (CI grep) | grep gate |
| FI-COLLECT-1 accumulator lifecycle | PASS (L2) | `entrypoint/mod.rs` fee_v2/apply_fee/fee_collect | `heavyweight_pipeline.rs` test_heavyweight_fee_v2 |
| FI-COLLECT-2 supply neutrality | PASS (L2) | `apply_fee_collect` (no supply write) | fee_v2 + fee_integration |
| FI-COLLECT-3 accumulator state machine | PASS (L1.5) | `model/mod.rs` AccumulatorPoint/State | `fee_extractor.rs` test_accumulator_* |
| FI-COLLECT-4 overlay visibility | PASS (L2) | overlay (execution.rs) | fee_v2 multi-FeeV2 |
| FI-COLLECT-5 byte encoding | PASS (L1.5) | `model/mod.rs` AccumulatorPoint | `fee_extractor.rs` test_accumulator_* |
| FI-ENCRYPT-1 mandatory ciphertext | **WARN** | client `fee.rs` 68-byte zero placeholder (real AEAD in `bin/dww` fee_builder) | fee_integration IT-1 (real AEAD) |
| FI-ENCRYPT-2 per-block key rotation | PASS | `bin/dww` fee_builder | `fee_extractor.rs` test_per_block_key_rotation |
| FI-ENCRYPT-3 no silent decrypt fallback | PASS | `bin/dwowd/src/lib.rs` decrypt_fee_for_miner | `fee_extractor.rs` test_g2_encrypt_decrypt_roundtrip |
| FI-ADMIT-1 two-tier admission | PASS | mempool | fee_integration IT-1/2 |
| FI-ADMIT-2 FCFS | PASS | mempool | fee_integration |
| FI-ADMIT-3 nullifier replay | PASS | mempool + chain_state | fee_integration |
| FI-FLAG-1 flags chain-synced | PASS | BlockHeader + fee_window | fee_integration |
| FI-FLAG-2 flags excluded from hash | PASS | BlockHeader | (structural) |
| FI-FLAG-3 flags advisory | PASS | accept_block | (structural) |
| FI-WINDOW-1..7 (+I1..I8) | PASS | `fee_window.rs` | `fee_extractor.rs` L1.5-FW-* |
| FI-RISK-1..6 | PASS | `src/linear/src/contract_risk.rs` | heavyweight_pipeline risk tests |
| FI-WASM-1..2 | PASS | `fee_window.rs` extract_tx_wasm_kb | heavyweight_pipeline |
| FI-TIME-1 proof timing | PASS (bench) | wallet fee_threshold | `fee_extractor.rs` test_fi_time1 |

**Contract entrypoints vs heavyweight test** (`native_token_spec.rs`):

| Entrypoint | Verdict | Note |
|-----------|---------|------|
| FeeV2 (0x08) | PASS | merkle root + sk_H + add_fee fixed |
| FeeCollectV1 (0x06) | PASS | exercised structurally by with_fee_collect |
| MintV1 (0x01) | PASS | rejection placeholder (walled off) |
| BurnV1 (0x02) | **FAIL** | merkle tree reproduction (see §11.4) |
| TransferV1 (0x03) | **FAIL** | merkle tree reproduction |
| SpendV1 (0x04) | **FAIL** | merkle tree reproduction |

### 11.3 HAZOP Guide-Word Table (consensus-critical paths)

| # | Deviation | Verdict |
|---|-----------|---------|
| H1 | Fee accumulator NOT reset at block start | RULED OUT — `apply_pow_reward` writes Identity (FI-COLLECT-1) |
| H2 | FeeCollectV1 claims MORE than accumulated | RULED OUT — Pedersen equality check C2 (Theorem 2) |
| H3 | Accumulator reset from Active bypassing FeeCollectV1 | RULED OUT — AccumulatorPoint has no public reset (FI-COLLECT-3) |
| H4 | Nullifier double-spend | RULED OUT — `db_contains_key` before spend + mempool replay (FI-ADMIT-3) |
| H5 | Commitment minted twice (duplicate commitment) | RULED OUT — `db_contains_key(commitment_set)` (P8/C3) |
| H6 | Reward over/under emission | RULED OUT — `expected_reward` equality (HAZOP F1) |
| H7 | FeeV2 fee exposed in clear text | RULED OUT — Pedersen commitment, no clear fee (SPEC-5) |
| H8 | Threshold bypassed (fee < threshold) | RULED OUT — FeeThreshold_V1 `range_check(64, fee−threshold)` |
| H9 | encrypted_fee_value empty/short | **CONFIRMED** — client placeholder is 68 zero bytes, not real AEAD (§11.4) |
| H10 | Commitment merkle root mismatch | **CONFIRMED** — heavyweight Burn/Transfer/Spend don't reproduce the accumulated tree (§11.4) |

### 11.4 Findings + Remediation

- **F1 (FAIL) — heavyweight BurnV1/TransferV1/SpendV1.** Two distinct defects:
  1. **BurnV1**: the contract coin tree accumulates *every* minted leaf (coinbase + FeeV2 change +
     FeeCollect fee + transfer/spend outputs); the harness rebuilds only the coinbase history, so
     the spent commitment's leaf position/path are wrong (`TransferMerkleRootNotFound`).
  2. **TransferV1/SpendV1**: the harness spends a commitment that does not exist on-chain — hardcoded
     `value=500, asset_id=1, secret=[2;32], coin_blind=6, leaf_position=0, merkle_path=[0;32]` — so
     the input commitment never matches any minted leaf. These endpoints need a real minted commitment + correct
     path (a full test redesign, mirroring the escrow `notes` setup), not a one-line patch.
- **F2 (WARN) — FI-ENCRYPT-1 client placeholder.** `client/fee.rs` emits a 68-byte zero
  `encrypted_fee_value` instead of real AEAD. The real `encrypt_fee_for_miner` lives in the wallet
  and is exercised by `fee_integration_spec.rs` IT-1/2/3. The contract-client placeholder is a test
  simplification; reconcile so the heavyweight FeeV2 path also produces a real ciphertext, or
  document the dispensation.
- **F3 (WARN) — README selector discrepancy.** `src/contract/native_token/README.md` labels the fee
  entrypoint `0x00`; fee-spec §10 says FeeV1 `0x00` is REMOVED and FeeV2 is `0x08`. Align README.
- **F4 (WARN) — dead constants.** `NATIVE_TOKEN_CONTRACT_MERKLE_TREE` (`"merkle"`) and the
  `genesis_root`/`miner_pubkey` info-tree keys are defined but never read. Remove or justify.

### 11.5 Remediation outcome — commitment-transfer + full recipient support (2026-08-17)

A deeper HAZOP of the commitment-transfer path (following `fee-spec.md` §2.3 tree growth, `mint.zk` C1/C2
the M8 fix, `burn.zk` signature derivation, and `dev/contracts/native_token.md:64-91` "transfer to a
fresh recipient") surfaced four further root causes beyond F1. All fixed from the spec, not from the
next red test line:

- **F5 — Transfer/Spend mint `spend_secret` model (CONFIRMED→FIXED).** `TransferCallBuilder::build`
  reused the spender's secret as the output `spend_secret` (`transfer/mod.rs:225-227`), while
  `mint.zk:56-62` constrains `coin_public == from_secret(spend_secret)`. Every output was therefore a
  self-change commitment — a real transfer to a different recipient was impossible. Fix (full recipient
  support): `build` now generates a fresh per-output `SecretKey::random(rng)` and passes it as the
  mint `spend_secret`; `create_transfer_mint_proof` derives the commitment public key from `spend_secret`
  (not `output.public_key`); the `NativeToken` note carries `spend_secret` so the recipient can
  compute the nullifier and spend.
- **F6 — Burn `signature_public` mismatch (CONFIRMED→FIXED).** `create_burn_proof` derives
  `signature_secret = poseidon(SIGNATURE_SECRET, spend_secret, nullifier)` (`burn.rs:111`) but
  `BurnCallBuilder::build` serialised `Input.signature_public` from the ephemeral input, so the
  proof's public input and the params' value disagreed. `create_burn_proof` now returns the derived
  `signature_secret`; `build` emits `revealed.signature_public`.
- **F7 — Non-determinism (DZ-4) (CONFIRMED→FIXED).** `burn.rs` blinds+proof and the harness
  `transfer()`/`spend()` used un-gated `OsRng`. Gated behind `deterministic_zk_enabled()` with
  `StdRng::seed_from_u64(0)`.
- **F8 — `uniform_runner` chain-B determinism replay (CONFIRMED→FIXED).** The replay loop only called
  `generate`, skipping `generate_with_coinbase` endpoints, so PI-7 compared blocks with different
  transaction sets. Chain B now replays `generate_with_coinbase` (prefetch + `submit_with_coinbase`).

Verification (verbatim, 2026-08-17):

- `test_heavyweight_native_token` — **PASS** (all endpoints FeeV2/BurnV1/TransferV1/SpendV1 + MintV1
  rejection accepted; PI-7 chain-A/B block hashes equal).
- `test_heavyweight_fee_v2` (+`_box`, `_deploy`) — **PASS** (3/3).
- `fee_extractor` — **PASS** (19/19). `nt_unit` — **PASS** (34/34).
- `fee_integration` — **10/10 PASS** (after F9 below).

### 11.6 Nullifier tracking — claim vs spend (F9, 2026-08-17)

`test_fee_integration_full_lifecycle` exposed a nullifier-tracking bug: the mempool rejected a
legitimate FeeV2 spend with `Double-spend: nullifier already confirmed on-chain`. Root cause:
`chain_state.rs` `connect_block`'s in-memory cache tracked *every* `tx.nullifiers` entry as a spend
nullifier, but the coinbase/FeeCollectV1 transactions place their **claim** nullifier in
`tx.nullifiers` (the test harness `build_coinbase_inner` and the production genesis/miner path both
do). The claim nullifier IS the future spend nullifier (fee-spec §17.4), so tracking it as spent
made the coinbase/fee coin born-unspendable.

Fix: `connect_block` now records the PoWRewardV1 and FeeCollectV1 claim nullifiers (`is_spend=false`,
the maturity `nullifier_set`) and **skips** them in the `tx.nullifiers` spend-tracking loop, so they
land only in `nullifier_set`, never `spent_nullifiers`. Verification: `fee_integration` 10/10,
`test_heavyweight_native_token` and `test_heavyweight_fee_v2` unchanged (PASS).
