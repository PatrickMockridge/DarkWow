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
- The **miner** derives tier prices at window boundaries from the completed window's
  applied FeeV3 counts (chain data, not its mempool), and builds FeeCollectV1 from the
  plaintext fee pot
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
- A node joining at height 1000 re-derives the exact CFs from its own copy of the
  completed window's applied FeeV3 counts. The header's `fee_window_flags` carry the
  **direction**, not the factors (`FI-FLAG-1`); reading only the flags is not enough.
- A wallet that goes offline for 3 windows derives the current CFs from the chain data it
  syncs, not from a cached flag value.
- A miner that restarts reads the persisted per-boundary snapshot from its sled database;
  a node that reorgs restores the snapshot the canonical chain implies.

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
block hash to prevent circular dependency (flags depend on the completed window's chain
data, block hash depends on header).

**Fee amounts — plaintext `fee` in FeeParamsV3.** The exact fee rides in the
clear in the call data (`[0x08][FeeParamsV3]`); the block's fee total is
public by design (privacy-model.md §2). The miner computes `total_fees` as the
plain sum of the block's FeeV3 fees; FeeCollectV1 checks
`total_fees == fees_db[height]`.

**Testing the plaintext channel requires:**
- L1 unit: FeeParamsV3 encode/decode roundtrip with plaintext fee + tier
- L1.5 bridge: FeeV3 with a clear fee through accept_block, fee pot accumulation verified
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

**Current state (2026-11-01):** `ContractRiskTracker` exists in Rust
(`src/linear/src/contract_risk.rs`); `chain_state.rs:1476-1520` records deviations on
apply and evaluates them at the boundary, and mempool admission reads the factor in
`rederived_minimum_fee` (`crates/dwow-mempool/src/lib.rs:855-860`).
`compute_fee_v3(gas, cf, tier, risk)` multiplies the `RiskFactor`; there is no
`compute_total_fee()` production call site because that name does not exist. The old
"2026-08" text described the Python model as the only home for the tracker; the Rust
tracker is now the admission input.

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

The caller logs: `warn!("FeeV3 fee extraction failed for tx {}: {:?}", tx_hash, err)`

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
| **L1 (Unit)** | Pure functions: `compute_fee_v3()`, `CongestionFactor`, `WindowSignalling` | Deterministic: the log is the integer `ilog2`; the α coefficients are `f64` but determinism survives by what they touch (FI-WINDOW-2) |
| **L1.5 (Bridge)** | Production path: real ZK proofs + AEAD + accept_block + wallet scan | Full fee lifecycle: wallet→mempool→miner→FeeCollectV1 |
| **L2 (Heavyweight)** | Multi-block chain: window boundaries, cross-window CF propagation, multi-contract fee differential | 20+ blocks to trigger window boundary, real ZK coinbases |
| **L3 (Docker)** | End-to-end: wallet container → mining nodes → block production → wallet scan | Real RandomX, real P2P, 120s block times |
| **Benchmark** | Proof timing: Fee_V3 | Confirm proof generation < window boundary deadline |

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
6. Wire the risk multiplier — `compute_fee_v3(gas, cf, tier, risk)` is implemented and
   admission reads `ContractRiskTracker`; `resolve_cost_profile()` exists and is tested (H8, H9)
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

- `push_value_commitment.zk` **no longer carries a membership proof**. The "data tree" was
  never contract state, so the `set_membership`/`sparse_merkle_root` path was removed
  entirely (§8.7). The circuit now proves commitment-correctness
  (`commitment == poseidon_hash(4, value, nonce)`) and staker authorization.
- Client `to_vec()` (`client/push_value_commitment.rs`) and `get_metadata`
  (`entrypoint.rs`) publish the circuit's instances; there is no 6→5 reduction to record —
  the previous version of this bullet described a fix that was superseded by the removal.

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

**Correction (2026-10-22):** the endpoint this note defended was itself removed. Its replay use
(`proof_hash = poseidon_hash([nonce, revocation_root])`) was real, but the call returned
`is_not_revoked: true` unconditionally and its circuit bound only the tx pair; `revocation_root` was
never checked against any state. The lesson stands and is sharper: auditing per-field exec usage is
necessary but not sufficient — a field can be read and still be part of a verdict the contract does
not compute. Identity's `VerifyCapabilityV1` reads the revocation marker and replaces the claim.

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
  ContractError(Custom(14))` = `TransferMerkleRootNotFound`. FeeV3 (height 2) accepted; the
  BurnV1 commitment spends a commitment whose on-chain merkle root is not reproduced by the harness.
- `test_heavyweight_fee_v3` (+`_box`, `_deploy`) — **PASS** (3/3).
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
| FI-COLLECT-1 fee pot lifecycle | PASS (L2) | `entrypoint/mod.rs` fee_v3/apply_fee/fee_collect | `heavyweight_pipeline.rs` test_heavyweight_fee_v3 |
| FI-COLLECT-2 supply neutrality | PASS (L2) | `apply_fee_collect` (no supply write) | fee_v3 + fee_integration |
| FI-COLLECT-3 fee pot state machine | PASS (L1.5) | `entrypoint/mod.rs` `fees_db` writes | `fee_extractor.rs` fee-collect tests |
| FI-COLLECT-4 overlay visibility | PASS (L2) | overlay (execution.rs) | fee_v3 multi-FeeV3 |
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
| FeeV3 (0x08) | PASS | merkle root + sk_H + add_fee fixed |
| FeeCollectV1 (0x06) | PASS | exercised structurally by with_fee_collect |
| MintV1 (0x01) | PASS | rejection placeholder (walled off) |
| BurnV1 (0x02) | PASS | remediation landed; `test_heavyweight_native_token` passes (§11.5) |
| TransferV1 (0x03) | PASS | remediation landed; the FAIL below was the dated baseline |
| SpendV1 (0x04) | PASS | remediation landed; the FAIL below was the dated baseline |

### 11.3 HAZOP Guide-Word Table (consensus-critical paths)

| # | Deviation | Verdict |
|---|-----------|---------|
| H1 | Fee pot NOT seeded at block start | RULED OUT — `apply_pow_reward` seeds `fees_db[H+1] = 0` (FI-COLLECT-1) |
| H2 | FeeCollectV1 claims MORE than accumulated | RULED OUT — plaintext equality check `total_fees == fees_db[height]` (C2) |
| H3 | Fee pot reset bypassing FeeCollectV1 | RULED OUT — `fees_db[height]` is written only by `apply_fee` and `apply_fee_collect` (FI-COLLECT-3) |
| H4 | Nullifier double-spend | RULED OUT — `db_contains_key` before spend + mempool replay (FI-ADMIT-3) |
| H5 | Commitment minted twice (duplicate commitment) | RULED OUT — `db_contains_key(commitment_set)` (P8/C3) |
| H6 | Reward over/under emission | RULED OUT — `expected_reward` equality (HAZOP F1) |
| H7 | FeeV3 fee in clear text | BY DESIGN — FeeV3 plaintext fee (fee-spec.md §14.4); SPEC-5 encrypted channel removed |
| H8 | Tier price bypassed (fee < tier price) | RULED OUT — plain comparison in mempool admission (fee-spec.md §12.8.1) |
| H9 | encrypted_fee_value empty/short | RULED OUT — field removed from FeeParamsV3 (fee-spec.md §14.4) |
| H10 | Commitment merkle root mismatch | **CONFIRMED** — heavyweight Burn/Transfer/Spend don't reproduce the accumulated tree (§11.4) |

### 11.4 Findings + Remediation

- **F1 (FAIL) — heavyweight BurnV1/TransferV1/SpendV1.** Two distinct defects:
  1. **BurnV1**: the contract coin tree accumulates *every* minted leaf (coinbase + FeeV3 change +
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
  `InvalidFunction` (fee-spec §10: FeeV3 selector `0x08`).
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

- `test_heavyweight_native_token` — **PASS** (all endpoints FeeV3/BurnV1/TransferV1/SpendV1 + MintV1
  rejection accepted; PI-7 chain-A/B block hashes equal).
- `test_heavyweight_fee_v3` (+`_box`, `_deploy`) — **PASS** (3/3).
- `fee_extractor` — **PASS** (19/19). `nt_unit` — **PASS** (34/34).
- `fee_integration` — **10/10 PASS** (after F9 below).

### 11.6 Nullifier tracking — claim vs spend (F9, 2026-08-17)

`test_fee_integration_full_lifecycle` exposed a nullifier-tracking bug: the mempool rejected a
legitimate FeeV3 spend with `Double-spend: nullifier already confirmed on-chain`. Root cause:
`chain_state.rs` `connect_block`'s in-memory cache tracked *every* `tx.nullifiers` entry as a spend
nullifier, but the coinbase/FeeCollectV1 transactions place their **claim** nullifier in
`tx.nullifiers` (the test harness `build_coinbase_inner` and the production genesis/miner path both
do). The claim nullifier IS the future spend nullifier (fee-spec §17.4), so tracking it as spent
made the coinbase/fee coin born-unspendable.

Fix: `connect_block` now records the PoWRewardV1 and FeeCollectV1 claim nullifiers (`is_spend=false`,
the maturity `nullifier_set`) and **skips** them in the `tx.nullifiers` spend-tracking loop, so they
land only in `nullifier_set`, never `spent_nullifiers`. Verification: `fee_integration` 10/10,
`test_heavyweight_native_token` and `test_heavyweight_fee_v3` unchanged (PASS).

## 12. Consensus integrity — one commit broke both coinbase key-binding and sync bootstrap (2026-09)

Commit `55a04076c9` ("bind the coinbase note's value and the uncle note's spend key") added two consensus
checks without updating the corresponding production paths; both surfaced in `daemon_sync_integration`.

**12.1 Coinbase key-binding — the built-in miner never set `header.miner`.** The check
(`bin/dwowd/src/block_acceptor.rs:341-347`) requires `header.miner ==` the coinbase note's `pk_H` for every
non-genesis block (so a reward note is spendable only by the declared miner). The built-in miner's
`create_block_with_uncles` (`src/linear/src/block.rs:683`) left `miner: [0u8;32]` (comment: "miner sets reward
public key") and no later step filled it; only the stratum/mm_rpc template path
(`bin/dwowd/src/registry/model.rs:724`) set it. Result: every built-in-miner block was rejected
(`note.key ≠ [0u8;32]`).
Fix: `Miner::mine` (`src/linear/src/miner.rs`) now takes the recipient `pk_H` and writes `block.header.miner`,
on both the built-in `miner_task` and the stratum RPC path; the `daemon_sync` test's two manual block
constructions bind the same key. Verified 6/10 green (the specific error is gone).

**12.2 Sync handshake — fail-closed on genesis broke bootstrap.** The handshake
(`src/linear/src/sync_connection.rs:452-458`) rejected a client presenting `genesis_hash = None`. A
bootstrapping node has no genesis yet, so it sends `None`; the authority (which holds genesis) rejected it, so
the node could never join — the three sync timeouts (`0 vs 2`, 900s, "never pulled") were this one gate's
symptom, and the reorg test's own `SyncPeer::dial(…, None, …)` hit it too.
Fix: admit `None` (bootstrap) while still rejecting a *different* hash (chain identity) —
`.unwrap_or(false)` → `.unwrap_or(true)`. A `None` peer's chain identity is enforced downstream by the pinned
`genesis_hash.txt` (`check_genesis_pin`, `bin/dwowd/src/lib.rs:890,907`).

Principle (both): a consensus rule SHALL be introduced together with the change on every path that must
satisfy it — the barrier without the upstream prevention rejects legitimate production output.

## 13. Genesis, join, and heaviest-chain HAZOP (2026-11)

### 13.1 Baseline (verbatim)

**Top event.** Two chains exist, or the pinned genesis is not the chain every node adopts, or the
chain a node adopts is not the heaviest by accumulated work. The audit covers the three paths a node
can take: create genesis, adopt it from a peer, and choose between chains, plus the persistent peer
bond that makes the second path repeatable. The verbatim run record is in §13.5.

**Baseline (pre-fix).** F1–F6 were established by reading the code against the live control
surface (R6). F1 was then reproduced at runtime: with the boundary check planted out, the new
control adopts a foreign genesis and fails (§13.5). All controls pass with the fixes in place. No
Docker devnet run or umbrella sweep was performed (out of scope; see §13.5).

### 13.2 Spec→Code→Test Traceability Matrix

Verdict = PASS (code implements AND a test asserts at the required level), WARN (implemented but the
control is structural/deployment-only), FAIL (not implemented / wrong).

| Invariant | Verdict | Code anchor | Test anchor |
|-----------|---------|-------------|-------------|
| I1 one creator (role, env, marker) | WARN | `bin/dwowd/src/main.rs:287-292`; `task/consensus_linear.rs:59-87` | deployment gate `phase_09_blocks.sh:200-288`; observer panic has no dedicated failing-before test |
| I2 one block per node, no in-place re-creation | PASS | `bin/dwowd/src/lib.rs:936-1015` (stored-genesis pin hoisted) | `genesis_pin_rejects_a_wrong_hash`, `genesis_pin_is_current` |
| I3 sync adoption pinned | PASS | `task/consensus_linear.rs:549-575` | `the_adopt_path_refuses_a_second_genesis` |
| I4 broadcast adoption pinned | PASS (fixed here) | `bin/dwowd/src/block_acceptor.rs:162` | `accept_block_refuses_a_foreign_genesis_at_the_boundary` |
| I5 joiner converges to the authority's genesis and tip | PASS | `task/consensus_linear.rs:340-640` | `test_daemon_pull_sync_converges` |
| I6 caught-up before mining | PASS | `task/consensus_linear.rs:631-640` | `test_sync_state_gates_mining_until_caught_up` |
| I7 heaviest chain is the default rule | PASS | `src/linear/src/chain_state.rs:1846-1861`; `consensus_linear.rs:275` | `test_detect_reorg_*`, `test_sync_path_reorg_to_heavier_chain` |
| I8 finality vetoes only a verified anchor | PASS (fixed here) | `src/linear/src/chain_state.rs:1876`; `block_acceptor.rs:923` | `test_displaced_range_is_finalized` |
| I9 peer bond (seed/peer discovery, hostlist, reconnect) | WARN | `src/net/session/refine_session.rs:74-118`; `outbound_session.rs:335,610,739` | none — durability limitation, G23 |

### 13.3 HAZOP Guide-Word Table

| # | Deviation | Verdict |
|---|-----------|---------|
| G1 | NO genesis authority in the network | DEPLOYMENT-ONLY — no code control; the network simply has no block 1. `contrib/docker/darkwow-testnet/docker-compose.yml` names exactly one `NODE_ROLE=genesis` |
| G2 | MORE than one genesis authority | RULED OUT for a *different* genesis — the acceptance boundary checks the pin before any genesis commits (`bin/dwowd/src/block_acceptor.rs:162`), so a different-key authority fails its own startup ceremony. Two authorities with the same declared key produce the identical block; uniqueness remains deployment-owned |
| G3 | Observer AS WELL AS genesis | RULED OUT — startup panic (`bin/dwowd/src/main.rs:287-292`); the panic has no dedicated failing-before test |
| G4 | Genesis received via broadcast OTHER THAN the pinned hash | **CONFIRMED → FIXED** — the broadcast rail reached `accept_block` with no pin comparison. The check now sits at the shared acceptance boundary (`bin/dwowd/src/block_acceptor.rs:162`), control `accept_block_refuses_a_foreign_genesis_at_the_boundary` |
| G5 | Genesis REVERSED out by a heavier competing chain | **CONFIRMED → FIXED by G4's root** — a foreign genesis can no longer be stored as a competing block at height 1, so the height-2 extension that would have made it canonical is never reached |
| G6 | Second genesis stored as a competing block at height 1 | **CONFIRMED → FIXED** — same boundary. The competing-at-height path admits two blocks with `previous = [0u8;32]`; the block is now refused before `connect_block` can see it |
| G7 | Pin NOT checked on restart of a non-authority | **CONFIRMED → FIXED** — the stored-genesis check is hoisted out of `create_genesis` (`bin/dwowd/src/lib.rs:936`). A non-authority restart now verifies its stored genesis and adopts the stored hash as its identity |
| G8 | Every node rebroadcasts genesis AS WELL AS the creator | **CONFIRMED → FIXED** — the startup broadcast is gated on `config.genesis_authority.is_some()` (`bin/dwowd/src/lib.rs:1125`). Before it, the code did the opposite of its comment and every node at height ≥ 1 re-broadcast the multi-MB block |
| G9 | Joiner presents `None` in the handshake | BY DESIGN — `src/linear/src/sync_connection.rs:468-475`; the pin downstream now covers both adoption rails, not only the pull path |
| G10 | Path B plurality lets `None` outvote `Some` | RULED OUT — Rust's `(count, is_some)` key matches the model's sort and its equal-count re-tie-break (`src/linear/src/sync_boundary.rs:160-176`); a strict `None` majority wins in both, by design |
| G11 | Path B selects a minority genesis | RULED OUT — the received block is pinned; a wrong plurality is refused and the peer scored, so no wrong chain is adopted |
| G12 | `Relaxed`/`Off` reachable in production | RULED OUT — `const GENESIS_FILTER: GenesisFilterMode = Strict` (`bin/dwowd/src/task/consensus_linear.rs:397`) |
| G13 | Genesis-less node declared CaughtUp at 0 | PARTIAL — `caught_up` is local (`bin/dwowd/src/task/consensus_linear.rs:631`), but mining also needs a peer or authority, and a block at height 0 cannot be mined. `skip_sync` sets CaughtUp outright; it is false in the devnet compose and is a test/legacy escape hatch |
| G14 | Authority mines MORE than its pinned chain permits | RULED OUT — block acceptance binds to the local genesis; creation and stored genesis are pinned |
| G15 | Accumulated work NOT recomputed on restart | RULED OUT — recomputed `1..=height` with the sled cache corrected (`src/linear/src/chain_state.rs:281-320`) |
| G16 | Recomputation disagrees at target 0 or across a chain gap | RESIDUAL — unreachable from validated blocks; `BlockTarget::chain_work` returns 0 for target 0 while the restart recompute uses `max(1)` |
| G17 | Equal-work longer chain loses | RULED OUT — strict `>` in both comparison sites (`src/linear/src/chain_state.rs:1846-1861`; `bin/dwowd/src/task/consensus_linear.rs:275`) |
| G18 | Finality veto blocks a heavier chain by default | BY DESIGN — the veto requires a **verified** anchor proof; an unanchored block is displaceable (`src/linear/src/chain_state.rs:1827-1831`) |
| G19 | Finality NOT applied on the general sync reorg | **CONFIRMED → FIXED** — `displaced_range_is_finalized` is checked at the shared reorg executor (`bin/dwowd/src/block_acceptor.rs:923`), control `test_displaced_range_is_finalized` |
| G20 | Reorg depth LESS than the heavier chain needs | DECLARED — `MAX_REORG_DEPTH = 100` (`bin/dwowd/src/task/consensus_linear.rs:129`) |
| G21 | Failed reconnect leaves the chain truncated | **CONFIRMED → FIXED** — the disconnected canonical blocks are reconnected before the error returns (`bin/dwowd/src/block_acceptor.rs:1085,1127`), control `test_failed_reorg_restores_the_disconnected_chain` |
| G22 | Fork-choice changed by a config flag | RULED OUT — no flag selects height over work; finality is a separate veto layer |
| G23 | Wrong-genesis peer persists in hostlist/greylist | LIMITATION — the sync-rail score is per-process and the hostlist is saved only on graceful stop; the spec says *skip* a mismatched peer, and `seeds`/`peers` re-bootstrap, so retention costs a retry rather than a lock-out |
| G24 | Wallet adopts the first peer's genesis | OUT OF SCOPE — the wallet follows the daemon's reported tip; recorded as a client posture gap, not a node rule |

### 13.4 Findings and remediation

**F1 — the genesis pin did not cover the broadcast rail (CONFIRMED → FIXED).** `handle_receive_block`
(`bin/dwowd/src/proto/linear_broadcast.rs:401-548`) accepted a height-1 block at local height 0 through
`accept_block_with_mempool` without the pin comparison the pull loop performs
(`bin/dwowd/src/task/consensus_linear.rs:549-573`), and without the magic-byte check. Genesis PoW is
`BlockTarget::MAX`, so a forged genesis costs no mining; the pin is its only authenticity. The root cause
is the principle at the foot of §12: the rule was introduced on one caller rather than on the boundary
every caller shares. The fix moves it to `accept_block_locked` (`bin/dwowd/src/block_acceptor.rs:162`),
which is reached by pull sync, broadcast, stratum, mm_rpc and the built-in miner alike. The pull loop's
own early call remains, because it also scores the peer before the block is handed over.

**F2 — a foreign genesis could be made canonical (CONFIRMED → FIXED by F1).** A foreign genesis stored as
a competing block at height 1 (`previous = [0u8;32]` matches the canonical sibling) followed by a
height-2 block made `detect_reorg` return `Heavier`; `accept_block` then called
`activate_best_chain(..., fork_height.pred() = 0, ...)`, which disconnected the canonical genesis and
connected the forged one. With F1 the block never reaches the competing store, so the extension has no
parent to build on. No separate genesis-immutability guard is added: the identity check at the boundary
is the guard, and the sync-path reorg already requires a common ancestor above genesis
(`reorg_to_heavier_chain` returns `Failed` when `fork_point` is zero).

**F3 — a non-authority restart adopted an unverified stored genesis (CONFIRMED → FIXED).** The stored
genesis was re-hashed and pinned only inside the `create_genesis` branch
(`bin/dwowd/src/lib.rs:940-978` before the fix). A joining node restarting on a poisoned datadir never
looked at the pin, and its `genesis_hash()` then became the filter's definition of "compatible". The
check is hoisted (`bin/dwowd/src/lib.rs:936`), and a joining node now records the stored hash as its
identity instead of the all-zero sentinel. No new control is added: the comparison itself is already
covered by `genesis_pin_rejects_a_wrong_hash`; the change is *when* it runs, and `init_linear` is too
heavy for the fast tier.

**F4 — every node rebroadcast genesis (CONFIRMED → FIXED).** The comment said "if this node created it";
the condition was only `height >= GENESIS` (`bin/dwowd/src/lib.rs` before the fix). Every node at height
≥ 1 re-broadcast the multi-MB genesis block, and a node holding a foreign genesis amplified it. The
broadcast is now authority-gated (`bin/dwowd/src/lib.rs:1125`).

**F5 — a failed reorg truncated the chain (CONFIRMED → FIXED).** `activate_best_chain` disconnects the
canonical blocks before it knows the competing chain will connect. A peer can supply a chain that is
heavier by accumulated work (the sync walk validates each block's own PoW, not the difficulty rule) but
fails full validation on connect; the old failure paths logged and returned, leaving the chain at the
fork point. The failure paths now reconnect the disconnected blocks in ascending order
(`bin/dwowd/src/block_acceptor.rs:1085,1127`) and report a loud truncation if restoration also fails.

**F6 — the general sync reorg ignored finality (CONFIRMED → FIXED).** The depth-1 path consulted the
finality predicate before reorging (`src/linear/src/chain_state.rs:1827-1831`); `reorg_to_heavier_chain`
did not, and `activate_best_chain` disconnects before `connect_block`'s finality check can see the
displaced block. The shared executor now refuses a reorg whose displaced range contains a verified
anchor (`displaced_range_is_finalized`, `src/linear/src/chain_state.rs:1876`; call at
`bin/dwowd/src/block_acceptor.rs:923`).

**F7 — adjudicated, no code change.** G10's `None` majority is the model's rule, not a Rust divergence
(the model's guard re-tie-breaks equal counts only); G23's hostlist retention is a durability limitation
bounded by re-bootstrap; G2's one-authority uniqueness is deployment-owned and the pin is its
defence-in-depth; G24 is a wallet posture gap outside the node's fork-choice rule.

`§12.2` said a `None` peer's identity "is enforced downstream by the pinned `genesis_hash.txt`". Before
F1 that was true only of the pull path; with the boundary check it is now true of both adoption rails.

### 13.5 Verification

The controls are named in §13.3 and §13.4. Run 2026-11-01, debug profile (`-j 8`,
`RAYON_NUM_THREADS=10`, `--test-threads 4`, `DWOW_TEST_LOGS=1`, output in full under `/tmp/`). The
plan asked for `--release`; the debug binary was the one already built and the release tree did not
exist on this host (43 GiB free, swap exhausted), so the runs were made in debug. The assertions
are profile-independent; only duration differs.

1. `cargo test -p dwow_chain --lib --features pow,sync-p2p -- test_genesis_filter --test-threads 4`
   → `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 313 filtered out; finished in 0.00s`
   (Path A identity, Path B plurality, tie-break, Off).
2. `cargo test -p dwow_chain --lib --features pow -- detect_reorg --test-threads 4`
   → `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 303 filtered out; finished in 2.28s`
   (strict-greater work, finalized-tip veto).
3. `cargo test -p dwow_chain --lib --features pow -- test_displaced_range_is_finalized --test-threads 4`
   → `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 307 filtered out; finished in 1.38s`
   (F6/G19; positive and negative arms in one test).
4. `cargo test -p dwowd --lib -- genesis --test-threads 4`
   → `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 139 filtered out; finished in 1384.18s`
   (pin, determinism, tampered-WASM refusal, second-genesis refusal, boundary refusal, sync
   materialisation, the nine contract ids).
   The boundary control alone:
   `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 149 filtered out; finished in 4.03s`
5. `cargo test -p dwowd --lib -- test_failed_reorg_restores_the_disconnected_chain --test-threads 4`
   → `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 149 filtered out; finished in 1438.35s`
   (F5/G21).
6. `cargo test -p dwowd --lib -- daemon_sync_integration --test-threads 4`
   → `test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 138 filtered out; finished in 1398.13s`
   (pull convergence, caught-up gating, broadcast propagation, the reorg family, and F5).

**Negative control for the F1 boundary check (R8).** The genesis admission at
`bin/dwowd/src/block_acceptor.rs:162` was planted out (`if false && height == GENESIS`), the tree
rebuilt, and control (4) re-run:
`test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 149 filtered out; finished in 666.99s`
with
`Error: TEST-FAIL [genesis::accept_block]: a genesis that is not this build's genesis MUST be refused at the acceptance boundary, before it can be stored as a competing block`.
The plant was reverted, the tree rebuilt, and the control re-run:
`test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 149 filtered out; finished in 2.22s`.
The check can fail, and it fails on the pre-fix behaviour.

**Not run (R7).**
- The Docker devnet pipeline and the umbrella sweep: out of scope, require a separate request.
- The observer+genesis panic has no dedicated failing-before test (I1 is WARN; deployment gate only).
- Hostlist durability has no test (I9 is WARN; G23 records the limitation).
- The F5 restore control was not executed against a reverted pre-fix tree. Its assertions (the
  "canonical chain restored" error text and the unchanged height/tip) fail by construction under
  the old truncating path; the executed negative control was for the F1 boundary check, the other
  new admission check.
