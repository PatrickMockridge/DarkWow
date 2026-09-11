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
- The **mempool** admits transactions by plain comparison against tier prices
- The **miner** adjusts tier prices at window boundaries based on mempool
  queue depths, and builds FeeCollectV1 from the plaintext fee pot
- The **contract** (native_token) verifies the claimed `total_fees` matches
  the plaintext pot `fees_db[height]` and zeroes it

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

## 3. Plaintext Fee Channel

The dual-channel design (public `fee_window_flags` + private
`encrypted_fee_value`) was collapsed to a single plaintext channel in 2026-09
(fee-spec.md §14.4):

**Public channel — `fee_window_flags` (block header).** Encodes congestion
direction (hold/+10%/-10%) for both circuit execution CF and WASM storage CF.
All nodes can read these. They are advisory signalling, not consensus-validated
(G5: `accept_block` does not reject invalid flags). They are excluded from the
block hash to prevent circular dependency (flags depend on mempool state, block
hash depends on header).

**Fee amounts — plaintext `fee` in FeeParamsV3.** The exact fee rides in the
clear in the call data (`[0x08][FeeParamsV3]`); the block's fee total is
public by design (privacy-model.md §2). The miner computes `total_fees` as the
plain sum of the block's FeeV2 fees; FeeCollectV1 checks
`total_fees == fees_db[height]`.

**Testing the plaintext channel requires:**
- L1 unit: FeeParamsV3 encode/decode roundtrip with plaintext fee + tier
- L1.5 bridge: FeeV2 with a clear fee through accept_block, fee pot accumulation verified
- L3 Docker: wallet pays a clear fee → miner sums the pot → FeeCollectV1 verifies

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

**Anti-pattern:** `read_fee(tx) -> Option<u64>`
All failure modes (missing fee bytes, malformed params, corrupted data)
collapse to `None` with zero diagnostic information. The caller cannot
distinguish "fee malformed" from "not a fee transaction at all."

**Required pattern:** `extract_fee(tx) -> Result<FeeAmount, FeeExtractError>`
with distinct variants:
- `MissingFeeBytes` — call data shorter than the fee field
- `MalformedParams` — `FeeParamsV3::decode` failed
- `WrongSelector` — call data does not start with `0x08`

The caller logs: `warn!("FeeV2 fee extraction failed for tx {}: {:?}", tx_hash, err)`

**Other diagnostic requirements (fee-spec.md SPEC-3):**
- `FeeParamsV3::decode` failure → `warn!` with transaction hash
- Congestion measurement returning 0 due to lock contention → `warn!`
- Tier admission rejection → reason logged (fee below tier price, malformed params)
- `FeeCollectV1` pot mismatch → hard error with expected vs actual

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
| **Benchmark** | Proof timing: Fee_V2 | Confirm proof generation < window boundary deadline |

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

1. ~~Wire `encrypted_fee_value` — paired wallet + miner change (C1)~~ — RULED OUT:
   the encrypted-fee channel was removed in FeeV3 (fee-spec.md §14.4)
2. Remove `#[cfg(feature = "fee-window")]` feature gate (C3)
3. Unify fee estimate paths — single chain-derived value (C2)
4. Replace `try_lock().unwrap_or(0)` congestion measurement (H4)
5. Add diagnostic surface to `extract_fee()` (H1, H3)
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
| FI-COLLECT-1 fee pot lifecycle | PASS (L2) | `entrypoint/mod.rs` fee_v2/apply_fee/fee_collect | `heavyweight_pipeline.rs` test_heavyweight_fee_v2 |
| FI-COLLECT-2 supply neutrality | PASS (L2) | `apply_fee_collect` (no supply write) | fee_v2 + fee_integration |
| FI-COLLECT-3 fee pot state machine | PASS (L1.5) | `entrypoint/mod.rs` `fees_db` writes | `fee_extractor.rs` fee-collect tests |
| FI-COLLECT-4 overlay visibility | PASS (L2) | overlay (execution.rs) | fee_v2 multi-FeeV2 |
| FI-COLLECT-5 byte encoding | PASS (L1.5) | `model/mod.rs` `FeeParamsV3` | `fee_extractor.rs` fee-params tests |
| FI-ENCRYPT-1..3 encrypted-fee channel | RULED OUT | channel removed in FeeV3 (fee-spec.md §14.4) | — |
| FI-ADMIT-1 three-tier admission | PASS | mempool | fee_integration IT-1/2 |
| FI-ADMIT-2 FCFS | PASS | mempool | fee_integration |
| FI-ADMIT-3 nullifier replay | PASS | mempool + chain_state | fee_integration |
| FI-FLAG-1 flags chain-synced | PASS | BlockHeader + fee_window | fee_integration |
| FI-FLAG-2 flags excluded from hash | PASS | BlockHeader | (structural) |
| FI-FLAG-3 flags advisory | PASS | accept_block | (structural) |
| FI-WINDOW-1..7 (+I1..I8) | PASS | `fee_window.rs` | `fee_extractor.rs` L1.5-FW-* |
| FI-RISK-1..6 | PASS | `src/linear/src/contract_risk.rs` | heavyweight_pipeline risk tests |
| FI-WASM-1..2 | PASS | `fee_window.rs` extract_tx_wasm_kb | heavyweight_pipeline |
| FI-TIME-1 proof timing | RULED OUT | no threshold proofs in FeeV3 | — |

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
| H1 | Fee pot NOT seeded at block start | RULED OUT — `apply_pow_reward` seeds `fees_db[H+1] = 0` (FI-COLLECT-1) |
| H2 | FeeCollectV1 claims MORE than accumulated | RULED OUT — plaintext equality check `total_fees == fees_db[height]` (C2) |
| H3 | Fee pot reset bypassing FeeCollectV1 | RULED OUT — `fees_db[height]` is written only by `apply_fee` and `apply_fee_collect` (FI-COLLECT-3) |
| H4 | Nullifier double-spend | RULED OUT — `db_contains_key` before spend + mempool replay (FI-ADMIT-3) |
| H5 | Commitment minted twice (duplicate commitment) | RULED OUT — `db_contains_key(commitment_set)` (P8/C3) |
| H6 | Reward over/under emission | RULED OUT — `expected_reward` equality (HAZOP F1) |
| H7 | FeeV2 fee in clear text | BY DESIGN — FeeV3 plaintext fee (fee-spec.md §14.4); SPEC-5 encrypted channel removed |
| H8 | Tier price bypassed (fee < tier price) | RULED OUT — plain comparison in mempool admission (fee-spec.md §12.8.1) |
| H9 | encrypted_fee_value empty/short | RULED OUT — field removed from FeeParamsV3 (fee-spec.md §14.4) |
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
- **F2 (RULED OUT) — FI-ENCRYPT-1 client placeholder.** `client/fee.rs` carries
  no `encrypted_fee_value` — the fee is plaintext in `FeeParamsV3` — so this
  finding does not apply.
- **F3 (ADDRESSED) — README selector discrepancy.** `src/contract/native_token/README.md`
  labels `0x08` as the fee entrypoint and marks `0x00` as returning
  `InvalidFunction` (fee-spec §10: FeeV2 selector `0x08`).
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
