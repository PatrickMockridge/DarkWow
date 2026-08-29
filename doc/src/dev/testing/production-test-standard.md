# Production Test Standard

This document defines the standard that ALL tests in the DarkWow repository SHALL meet
when testing contract functions, consensus operations, and wallet capability construction.
It is normative — every test SHALL conform. A test that does not conform is a HAZOP
finding and SHALL be remediated.

This standard derives from a HAZOP review of 55 test functions across 7 test files and
28 contract integration test suites (July 2026). The review found that 40 of 55 tests
are false positives — they appear to test contract functions but never exercise them
through the production execution path.

## Test Outcome Taxonomy

Every heavyweight test produces one of four outcomes. This taxonomy is normative —
every assertion, integrity check, and diagnostic in the test suite SHALL use the
prefix convention defined below.

### INFRA-FAIL — Block-Proof Violation in Shared Infrastructure

A failure in a shared infrastructure module. The chain's consensus mechanism
rejected the block, produced non-deterministic results, or a shared integrity
check failed. These failures are NOT specific to the contract under test — they
affect all tests equally.

The single authoritative gate is `accept_block()` in `block_acceptor.rs`. An
INFRA-FAIL means: `accept_block` returned an error, determinism produced
different hashes from identical inputs, a nullifier replay was not rejected,
the genesis block is missing or corrupted, or the block hash chain is
discontinuous.

**Error prefix:** `INFRA-FAIL [module]:` — MUST name the module that failed.

### TEST-FAIL — Contract-Specific Failure

A failure specific to the contract under test. The contract's harness or spec
produced bad data — the generate closure returned an error, call_data was empty,
or the chain height did not advance after submitting this specific endpoint.

**Error prefix:** `TEST-FAIL [contract]:` — MUST name the contract (and endpoint
if applicable).

### WARN — Non-Blocking Diagnostic

The block was accepted by `accept_block` (mass balance passed, ZK proofs
verified, nullifiers checked). A derived metric or contract-internal state
query did not match expectations. WARNs are non-blocking: they report to
`eprintln!` and the test continues.

**Error prefix:** `WARN [module]:` — MUST name the originating module.

### PASS — All Block-Proof Checks Passed

Every endpoint's block was accepted by `accept_block`, all INFRA-FAIL and
TEST-FAIL checks passed, the determinism check matched, and any WARNs were
logged but did not block the test.

### Integrity Check Classification

| Check | ID | Module | Classification |
|-------|----|--------|----------------|
| Genesis block hash exists and non-zero | PI-1 | integrity_checks | INFRA-FAIL |
| Initial cumulative supply equals INITIAL_REWARD | PI-2 | integrity_checks | WARN |
| Contract exists in contracts tree at genesis | PI-3 | integrity_checks | INFRA-FAIL |
| Harness ZK coverage pre-check | PI-4 | integrity_checks | WARN |
| Block hash chain continuity | PI-5 | integrity_checks | INFRA-FAIL |
| Cumulative supply reconciliation | PI-6 | integrity_checks | WARN |
| Determinism — Pipeline B hash must match Pipeline A | PI-7 | determinism | INFRA-FAIL |
| accept_block rejection (mass balance, proofs, state) | — | block_submission | INFRA-FAIL |
| Nullifier replay must be rejected | — | nullifier_replay | INFRA-FAIL |
| Harness generate() closure returns error | — | uniform_runner | TEST-FAIL |
| call_data is empty after generate | — | uniform_runner | TEST-FAIL |
| Height must advance after accept_block | — | endpoint_exercise | TEST-FAIL |
| verify_state closure finds unexpected state | — | uniform_runner | WARN |

## Infrastructure Requirements

The test infrastructure SHALL be uniform — it is not cherry-picked per test.
Every block constructed by the test infrastructure SHALL follow the same path
regardless of which contract is under test.

### Block Structure

Every block submitted through `submit_single_call_block()` contains:

1. **Coinbase** (PoWRewardV1): opens the merkle tree, distributes block reward.
   Constructed by `build_coinbase_for_height()`.

2. **Contract call(s)**: the endpoint(s) under test.

3. **FeeCollectV1**: closes the merkle tree, collects accumulated fees.
   Appended conditionally by `with_fee_collect()`:
   - When FeeV1 calls exist in the block → FeeCollectV1 is appended as the
     final transaction (matches production miner at lib.rs:1358)
   - When no FeeV1 calls exist → FeeCollectV1 is omitted (zero-fee block)

Both cases are valid per consensus (validation.rs:376-387). Zero-fee blocks
are accepted by the validator — they match the production miner's behavior
for coinbase-only blocks. The fee pipeline is tested explicitly by
native_token (FeeV1 endpoint with coinbase coordination) and by the
fee integration tests (`test_fee_integration_full_lifecycle` /
`test_fee_integration_mempool_lifecycle` in `heavyweight_pipeline.rs`).

### Fee Mechanism

FeeV1 calls are added by the wallet during transaction construction, not by
the block constructor. Each user transaction includes a FeeV1 call (native_token
selector `0x00`) alongside its contract operation call. FeeV1 requires real
commitment data (Input, Output, FeeParamsV1 with Pedersen commitments, Merkle paths,
and ZK proofs) — structural FeeV1 stubs cannot pass block proof validation.

The test infrastructure does NOT inject synthetic FeeV1 calls. Instead:

- **native_token_spec** exercises the FeeV1→FeeCollectV1 path end-to-end with
  real proofs and coinbase coordination (§5.1 of heavyweight-spec.md)
- **fee integration tests** (`test_fee_integration_full_lifecycle` /
  `test_fee_integration_mempool_lifecycle`) test fee collection integration across
  multiple blocks with FeeV1-producing transactions
- **All other contract tests** produce structurally valid blocks that may be
  zero-fee (no FeeV1 calls → no FeeCollectV1). This is valid per consensus
  and matches the production miner's coinbase-only block path

### FeeCollectV1 Validation

The block structure validator (validation.rs:362-388) enforces four rules:

| FeeCollectV1 | FeeV1 fees | Result |
|-------------|-----------|--------|
| Present | Zero | REJECTED — zero-value replay attack (§3.13) |
| Absent | > 0 | REJECTED — fees stranded permanently |
| Present | > 0 | ACCEPTED — must be final transaction |
| Absent | Zero | ACCEPTED — valid zero-fee block |

The test infrastructure SHALL NOT construct FeeCollectV1 with zero fees.
The `with_fee_collect()` helper SHALL omit FeeCollectV1 when no FeeV1 calls
exist in the block, matching the production miner's `if let Some(fee_tx)`
pattern.

## 1. The Production Path

Every test that claims to exercise a contract function SHALL follow this path. A test
that skips any numbered step is not a test — it is a false positive.

```
1. Genesis:         init_genesis() — all contracts deployed via production path
2. Coinbase:        build_linear_coinbase() — real ZK proof + AEAD encryption
3. Contract call:   harness convenience method — proof + call_data
4. Format bridge:   raw call_data passed directly (no client-side wrapping).
                    The execution layer (execution.rs) extracts the DarkLeaf
                    call tree from the witness at accept_block time.
5. Witness:         build_witness(contract_id, &call_data, proofs)
                    → core tx serialized into witness bytes
6. Transaction:     build_contract_tx(contract_id, call_data)
                    .witness = witness_bytes
7. Block:           Block { header, transactions: vec![coinbase_tx, contract_tx] }
8. accept_block:    validate_block_structure
                    → execute_block (metadata → exec → apply)
                    → verify_core_tx_with_tables (ZK proof + signature verification)
                    → commit state
9. State check:     Query sled store for expected state change
```

### 1.1 Contract Function Test Template

```rust
#[test]
fn test_<contract>_<function>_through_accept_block() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // 1. Genesis — production path
        let har = GenesisHarness::new_without_contracts().unwrap();
        let keys_toml = "[node0]\nwallet_secret = \"0100...\"\n";
        let keys_path = /* tempfile */;
        let miner_mgr = AccountManager::open(&keys_path, Network::Testnet, "node0").unwrap();
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];
        let recipient_1 = MiningRecipient::from_account(&miner_mgr, BlockHeight::new(1)).unwrap();
        init_genesis(&har.chain_state, recipient_1, magic_bytes).await.unwrap();

        // 2. Build coinbase — production path
        let height = BlockHeight::new(2);
        let reward = expected_reward(height);
        let recipient = MiningRecipient::from_account(&miner_mgr, height).unwrap();
        let linear_zk = LinearPowRewardZk::new(har.chain_state.clone()).await.unwrap();
        let (coinbase, _pi, pow_call, _blind) =
            build_linear_coinbase(recipient, reward, &linear_zk, height).await.unwrap();

        let coinbase_tx = Transaction {
            contract_calls: vec![pow_call],
            nullifiers: vec![coinbase.nullifier],
            ..Default::default()
        };

        // 3. Build contract call via harness
        let harness = ContractHarness::spawn();
        let result = harness.some_function(/* params */).unwrap();

        // 4. call_data passes directly (execution layer extracts tree from witness)
        let call_data = result.call_data.clone();

        // 5-6. Build tx with witness
        let mut contract_tx = build_contract_tx(CONTRACT_ID, call_data.clone());
        contract_tx.witness = build_witness(CONTRACT_ID, &call_data, result.proofs.clone());

        // 7. Assemble block
        let txs = vec![coinbase_tx, contract_tx];
        let prev = har.chain_state.get_latest_block().unwrap();
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev);
        let header = BlockHeader {
            previous: prev_hash,
            merkle_root: compute_merkle_root(&txs),
            height,
            total_reward: reward,
            randomx_key: Miner::derive_key_from_height(height),
            target: BlockTarget::MAX,
            ..Default::default()
        };
        let block = Block { header, transactions: txs };

        // 8. accept_block — production path
        let vm = build_accept_vm(&block).unwrap();
        accept_block(&har.chain_state, &block, &[], &vm,
            BlockHeight::new(1), BlockTarget::MAX, None)
            .expect("accept_block must succeed");

        // 9. Verify state change
        let key = /* contract-specific sled key */;
        let stored = har.chain_state.get_contract_data(&key).unwrap();
        assert!(/* state changed as expected */);
    });
}
```

## 2. What SHALL NOT Appear in Any Test

### 2.1 Coinbase-only accept_block for contract function tests

If a test exercises a contract function, that function SHALL execute through
accept_block's WASM runtime. `exec_coinbase_only()` is valid ONLY for tests that
verify coinbase behavior — not for tests whose name or documentation claims to
test contract functions.

**Rationale:** 24 heavyweight tests call harness endpoint methods to generate
call_data, assert the call_data is non-empty, then call `exec_coinbase_only()`.
The contract function's call_data is never submitted to accept_block. The harness
method's ZK proof is never verified by the production verifier. The test proves
the harness compiles — nothing about the contract.

### 2.2 Proof-only verification

`assert!(!call_data.is_empty())` is not a test of contract function behavior.
Proofs SHALL be verified by the production verifier (`verify_core_tx_with_tables`).
Structural checks on proof size or presence are partition-A concerns (the compiler
proves the proof type is inhabited) and SHALL be removed.

### 2.3 strict_zk: Structural enforcement

`const STRICT_ZK: bool = true` is immutable and structural. ZK proof enforcement
uses `EndpointSpec::is_zk` (authoritative contract metadata, never a heuristic).
Empty proofs against ZK-gated functions SHALL be rejected by `submit_block()`.
No `strict_zk` field exists on `HeavyweightPipeline` — the field was removed
during Phase 1. There is no toggle to bypass enforcement.

### 2.4 Synthetic manifests for production-path tests

A test that stores a hand-crafted manifest TOML string directly into the wallet DB
tests the manifest parser, not the production pipeline. Production tests that claim
to witness the manifest-driven capability engine SHALL use manifests from genesis
seeding or from contract deployment — the same path the wallet uses in production.

Tests of the manifest parser itself (unit tests in `manifest.rs`) MAY use synthetic
TOML strings. This restriction applies to integration tests, not unit tests.

### 2.5 Format shortcuts

call_data SHALL be in the format the WASM entrypoint expects:

- **Non-NativeToken contracts:** The DarkLeaf call tree is extracted from the
  witness by `execution.rs` at accept_block time and passed to WASM. No
  client-side wrapping is needed — pass raw `[fn_code] + params` directly.
- **NativeToken:** Raw `[fn_code] + params`.

The harness produces raw `[fn_code] + params` format. The execution layer
(`extract_wasm_call_tree()` in `execution.rs`) handles the conversion from
raw chain-level call data to the WASM entrypoint's `Vec<DarkLeaf<ContractCall>>`
format. Every test that submits non-NativeToken contract calls through
accept_block SHALL pass raw call_data directly.

### 2.6 #[ignore] for pre-existing bugs

A test disabled due to a known bug SHALL have a tracking issue and a remediation
timeline. `#[ignore]` without a fix plan is acceptance of the bug.

**Status:** The previously-documented violation — `test_wallet_integration`
`#[ignore]` due to a halo2 plonk synthesis error in `build_native_transfer` — is
resolved: the test is active (`bin/dwowd/src/tests/wallet_integration.rs`). The two
remaining `#[ignore]` tests carry tracking IDs and are compliant: `H-TF-002`
(uncle-merkle proof validation not yet enforced in consensus) and `H-TF-003`
(harness-exercise test that does not go through `accept_block`), both in
`bin/dwowd/src/tests/heavyweight_pipeline.rs`.

### 2.7 Schnorr signature prohibition for ZK contracts

ZK contracts SHALL authenticate through ZK proofs and nullifiers ONLY (per
ocap.md §6.2: Exercise = ZK Proof, Verify = `Proof::verify`; per
contract-standards.md §3: Schnorr signatures PROHIBITED).

A contract's `metadata()` function SHALL return empty signature pubkeys:

```rust
let sigs: Vec<PublicKey> = vec![];
sigs.encode(&mut metadata)?;
```

A heavyweight test that exercises a ZK contract through accept_block SHALL
verify BOTH of:
- The contract's `metadata()` returns empty signature pubkeys for every function
- The witness's signatures field is empty for that contract's call

Tests SHALL NOT add Schnorr signatures to make `verify_core_tx_with_tables`
pass. `verify_sigs` trivially passes when both signatures and pubkeys are
empty (zero loop iterations → Ok(())). An agent that adds Schnorr signatures to
bypass a metadata-pubkey mismatch is producing a false positive and SHALL be
treated as an intentional HAZOP finding.

**Rationale:** Adding Schnorr signatures to make tests pass defeats the
production test standard. The ZK proof already proves secret key knowledge
(`ec_mul_base(secret, NULLIFIER_K)`) — a Schnorr signature adds no security
and actively harms privacy by deanonymizing the signer. The test standard
SHALL enforce the ZK-only authorization model.

## 3. Test Classification

Every test SHALL declare its partition per the A/B/C taxonomy
(testing/overview.md §"A/B/C Partition"):

### Partition A: Statically-proven interior

Facts the compiler or Lean proof assistant discharge. Tests here SHALL be removed —
the compiler IS the test. Examples:

- `assert_eq!(block.header.height, 42)` when `BlockHeight: PartialEq` is already
  compile-proven
- Checking that a `Proof` is non-empty when the type system already distinguishes
  `Proof` from `Option<Proof>`
- Testing that `u64::from(BlockHeight)` compiles — the compiler already enforces
  the `From` impl

### Partition B: Runtime enforcement at boundaries

The test SHALL exercise the production code path and verify that the boundary holds
against adversarial input. Every declared SHALL at a boundary SHALL have at least
one runtime witness test.

**This is where most contract function tests belong.** accept_block is the primary
boundary: bytes arrive from the network, the WASM runtime deserializes and executes
them, and the state is committed. A partition-B contract test verifies that this
boundary correctly enforces the contract's declared barbs.

### Partition C: Dynamic residue

Emergent runtime properties (timing, concurrency, economics, network topology).
These belong in the Docker pipeline (`test_pipeline.sh`). Partition C tests depend
on timing, concurrency, adversary behavior, or network topology. They SHALL NOT be
included in `cargo test` runs.

## 4. Wallet Capability Test Standard

The wallet is a generic capability engine (wallet.md §0). It constructs typed
capabilities at scan time from primitives + manifest + barb composition. A test
that claims to witness this engine SHALL verify ALL of:

1. **accept_block:** A contract function executes through accept_block with
   production WASM execution.
2. **Wallet scan:** The resulting block is scanned by `scan_block_linear` through
   the production scan path.
3. **Typed capability:** The wallet discovers a typed capability — manifest
   resolution succeeded and `resolve_capability_type` returned `Some`.
4. **Field verification:** The discovered capability has the correct:
   - `capability_name` (matches manifest `[[capabilities]].name`)
   - `capability_discriminant` (matches manifest declaration)
   - `resource` and `action` (set from manifest `[[actions]]`)
   - `primitives` (exact set matching manifest `[[capabilities]].primitives`)
   - `barbs` (exact union matching the primitive-to-barb composition table in
     `capability.rs`)
   - `contract_id` (matches the deployed contract's ID)

**What is insufficient:**

- `assert!(!barbs.is_empty())` — passes if ANY capability was discovered, even a
  native token output or a misconfigured manifest. Does not verify typed
  construction.
- `assert_eq!(contract_id, expected)` — verifies routing but not typing. A
  native token output to the wrong contract would fail, but a correctly-routed
  untyped capability would pass.
- Checking only `capabilities.len()` — confirms discovery count but not
  structure. Zero typed capabilities and one native fallback would pass.

### 4.1 Wallet capability test template

```rust
// Find capability by contract_id
let cap = scan_result.capabilities.iter()
    .find(|c| c.cap_record.contract_id == CONTRACT_ID)
    .expect("wallet must discover capability for contract");

let rec = &cap.cap_record;

// Manifest-driven type construction
assert_eq!(rec.capability_name.as_deref(), Some("coin"),
    "capability_name must match manifest declaration");
assert_eq!(rec.capability_discriminant, Some(0),
    "discriminant must match manifest declaration");
assert!(rec.resource.is_some(),
    "resource must be set from manifest [[actions]]");
assert!(rec.action.is_some(),
    "action must be set from manifest [[actions]]");

// Primitive composition
let expected_primitives: Vec<Primitive> = vec![
    Primitive::SecretKey, Primitive::Commitment, Primitive::Nullifier,
    Primitive::ContractId, Primitive::FuncId, Primitive::AssetId,
    Primitive::MerkleNode,
];
for p in &expected_primitives {
    assert!(rec.primitives.contains(p),
        "capability must contain primitive {:?}", p);
}
assert_eq!(rec.primitives.len(), expected_primitives.len(),
    "capability must have exactly {} primitives", expected_primitives.len());

// Barb composition — union of primitive barbs
let expected_barbs: Vec<Barb> = vec![
    Barb::Spend, Barb::Derive, Barb::Commit, Barb::Nullify,
    Barb::Dispatch, Barb::Gate, Barb::Denominate, Barb::ProveInclusion,
];
for b in &expected_barbs {
    assert!(rec.barbs.contains(b),
        "capability must contain barb {:?}", b);
}
assert_eq!(rec.barbs.len(), expected_barbs.len(),
    "capability must have exactly {} barbs", expected_barbs.len());

// Inflation guard
assert_eq!(rec.value, 0,
    "non-native capability must have zero DRKW value");
```

## 5. Harness Completeness Standard

A contract harness SHALL provide at minimum:

1. One convenience method per ZK circuit that produces `(call_data, Vec<Proof>)`
2. `call_data` in the `[fn_code] + params` format (raw — the test wraps it)
3. Proofs built using the contract's production client code (CallBuilder pattern)

A harness with zero proof methods (TIER C) is not a harness — it is a circuit
loader. Contracts with TIER C harnesses SHALL be upgraded to TIER A before any
test claims to exercise that contract's functions through accept_block.

### 5.1 Harness classification

| Tier | Proof methods | Encode-only | Verdict |
|------|--------------|-------------|---------|
| A | >= 1 per circuit | optional | Production-ready |
| B | Some circuits | some | Needs upgrade before accept_block testing |
| C | Zero | any | Circuit loader — not a harness |

## 6. Correctly-Scoped Tests (Preserve)

The following tests are correctly scoped and SHALL NOT be modified as part of
remediation. They may be extended but their existing assertions SHALL NOT be
weakened:

- **Genesis tests** (`genesis.rs`): determinism, block creation, sync
  materialization, tamper rejection, persistence roundtrip
- **Pipeline deployment tests** (`pipeline.rs`): Deployooor-based deployment,
  no ZK claimed
- **Merge mining tests** (`merge_mining.rs`): PowSource::Monero acceptance,
  determinism
- **Tripwire test** (`tripwire.rs`): architectural invariant enforcement
- **`test_wallet_coinbase_scan_only`** (`wallet_integration.rs`): Path 1
  coinbase production path, correctly scoped
- **`test_canonical_call_failure_rejects_block`** (`wallet_integration.rs`):
  strict-mode rejection, correctly scoped

## 7. References

- [Testing Overview](overview.md) — testing levels and MoC boundaries
- [Type System Specification](../type-system.md) — barb definitions, type rules
- [O-Cap: Emergent Types](../ocap.md) — capability composition
- [Wallet Architecture](../wallet.md) — capability engine design
- [Manifest Specification](../manifest.md) — manifest format and resolution
