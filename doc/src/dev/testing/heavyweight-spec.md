# Heavyweight Testing Specification: Level 2 Pre-Production Standard

This document defines the non-negotiable criteria that ALL Level 2 heavyweight tests
in the DarkWow project SHALL satisfy. It SHALL be read in conjunction with the
[Testing Overview](overview.md), [Wallet Architecture Specification](../arch/wallet.md),
and [Type System Specification](../arch/type-system.md).

It uses SHALL, MUST, SHALL NOT, MUST NOT per RFC 2119.

## 0. Foundation — The Role of Heavyweight Tests

Heavyweight tests occupy Level 2 of the DarkWow testing taxonomy:

```
Level 1 (Lightweight)        Fast, no ZK, no P2P, single process
    |
    +-- Level 1.5 (Bridge)   Production-path integration: real ZK + accept_block + wallet scan
    |
    +-- Level 2 (Heavyweight)  Adds ZK proofs, real execution, still single process
            |
            +-- Level 3 (Localnet)    Adds P2P networking, Docker, multiple nodes
                    |
                    +-- Level 4 (Devnet)     Adds multi-machine, LAN/internet, public access
```

### 0.1 What Heavyweight Tests Witness

Per the A/B/C partition ([type-system.md §10.5](../arch/type-system.md)), heavyweight tests
are **partition B witnesses**: they verify runtime enforcement at the contract entrypoint
boundary — the `accept_block` production path.

A heavyweight test makes exactly one claim: **"This contract's function `F`, dispatched
through `accept_block` with real ZK proofs, produces state transition `S_old → S_new`."**

### 0.2 The 32 Contracts

| Cohort | Count | Deployment |
|--------|-------|-----------|
| Genesis | 9 | Height 1, static ContractId |
| WASM-deployed | 23 | Via deployooor at test setup |

All 9 genesis contracts are standalone — verified from entrypoint code: zero cross-contract
child calls exist in any genesis entrypoint. Each genesis contract test is self-contained.

---

## 1. Contract Categories

### 1.1 Verified Architecture

Cross-contract dependencies verified from entrypoint code (not assumed):

| Contract | Category | Functions | Circuits | Dependencies |
|----------|---------|-----------|----------|-------------|
| native_token | Consensus-Critical | 7 (1 disabled) | 3 | None |
| deployooor | Infrastructure | 2 | 0 | None |
| box | L1 O-Cap Primitive | 3 | 2 | None |
| purse | L1 O-Cap Primitive | 4 | 3 | None |
| oracle | Standalone Oracle | 6 | 5 | None |
| multisig | O-Cap Authorization | 4 | 3 | None |
| promissory_note | DeFi Capability Primitive | 6 | 5 | None |
| identity | O-Cap Authorization | 9 | 3 | None (stores BOX_CONTRACT_ID; Box::Put/Take is architectural intent, not yet implemented) |
| attestation | O-Cap Authorization | 13 | 10 | None (manifest declares deps on promissory_note + native_token_v1 but entrypoint shows zero child calls — likely manifest error) |

### 1.2 Category Definitions

**Consensus-Critical (native_token):** The single bespoke citizen ([wallet.md §0.1](../arch/wallet.md)).
The only contract crate the wallet depends on directly. Handles block rewards, fee payment,
value transfer. Deliberately rock-dumb: no multi-token, no auth, no freezing, no business logic.
No manifest — it IS the consensus. A bug here halts the chain.

**Infrastructure (deployooor):** The second sanctioned citizen. Deploys WASM contracts,
marks them immutable. Pure WASM, zero ZK circuits. No manifest. Without deployooor,
manifest discovery is impossible.

**L1 O-Cap Primitives (box, purse):** Minimal capability containers. Depend on nothing.
Other contracts compose from them. Box: linear delegation (Put/Take). Purse: fungible
container (Deposit/Withdraw/Balance).

**O-Cap Authorization (identity, attestation, multisig):** Authorization primitives.
Identity: credentials, ZK claims (5 modes via unified circuit), capabilities.
Attestation: attestations, claims, delegation chains, slashing, fee schedules.
MultiSig: threshold signature groups.

**DeFi Capability Primitive (promissory_note):** Currency plumbing — not a token, a DeFi
capability. Multi-token creation, backing capability proofs, private transfers with nullifier
link-breaking, atomic OTC swaps, redemption with zero-value receipts. Does NOT compose from
native_token — completely separate contract, separate blast radius. Bugs in PN cannot halt
consensus. Emits spend_hook callbacks TO other contracts.

**Standalone Oracle (oracle):** Data feeds with ZK-proof-authenticated values.
Register, push, attest, aggregate.

**WASM-Deployed:** 23 contracts deployed via deployooor after genesis. Have manifests.
May compose from genesis contracts via spend_hook or child calls.

### 1.3 Key Architectural Facts

- **All 9 genesis contracts are standalone.** Zero cross-contract child calls in any entrypoint.
- **native_token** has no manifest. Every block depends on it structurally via coinbase + FeeCollect.
- **deployooor** has no manifest, no ZK. All WASM contracts start here.
- **promissory_note** is a DeFi capability, not a token. Completely separate from native_token.
  PN has nothing to do with fee payment. Fee payment is exclusively native_token's domain.
- **box** and **purse** depend on nothing. Other contracts depend on THEM.
- **identity** stores BOX_CONTRACT_ID but does not yet construct child calls to it.

---

## 2. Definitions

### 2.1 Key Terms

| Term | Definition |
|------|-----------|
| **accept_block** | The production block acceptance path at `src/linear/src/block_acceptor.rs`. All five mining entry points route through this single path. A test that does not exercise `accept_block` is NOT a heavyweight test. |
| **ContractHarness** | A struct implementing the `ContractHarness` trait, providing `spawn()`, `circuits()`, `get_zkbin()`, `get_pk()`, `verify_zk_coverage()`, and endpoint methods. Each method SHALL map to exactly one variant in the contract's function enum. |
| **HeavyweightPipeline** | Shared chain state. Owns a temp sled DB, `CChainState`, cached ZK coinbase keys, deterministic test mining key. Created once per test. |
| **HeavyweightBlock** | Fluent per-block builder. Accumulates contract calls and uncles, seals, and submits through `accept_block`. |
| **strict_zk** | `const STRICT_ZK: bool = true` — immutable and structural. ZK gating is enforced by the uniform runner's `submit_block()` function using `EndpointSpec::is_zk` (authoritative contract metadata, never a heuristic). `with_call()` is a data accumulation method — it accepts proofs without validating whether they are required. No `strict_zk` field exists on `HeavyweightPipeline`. |
| **FeeCollectV1** | Function code `0x06` on native_token. The final transaction in every production block, closing the coin merkle tree. |
| **function enum** | The `#[repr(u8)]` enum declared in the contract's `lib.rs` mapping opcode to variant name. This IS the authoritative list of endpoints. |
| **manifest** | The TOML file at `src/contract/<name>/manifest.toml` declaring functions, circuits, trees, and capabilities. |

### 2.2 "Passing" — The Formal Definition

A heavyweight test SHALL be considered **passing** if and only if ALL of the
following positive requirements are met AND no INFRA-FAIL or TEST-FAIL outcome
occurs (see [production-test-standard.md](production-test-standard.md) "Test
Outcome Taxonomy" for the classification of every integrity check):

1. **INFRA-FAIL/TEST-FAIL gate:** No block-proof violation (accept_block
   rejection, determinism mismatch, nullifier replay failure, genesis
   corruption, hash chain breakage) and no contract-specific failure
   (harness generate error, empty call_data, height not advancing).

2. **WARN tolerance:** WARN outcomes (cumulative supply mismatch, verify_state
   discrepancy, ZK coverage gaps) are logged as diagnostics but do not prevent
   the test from passing. The block was accepted — the chain is valid.

The positive requirements below define what the test SHALL exercise:

1. **Full-path execution:** Every function in the contract's function enum SHALL be exercised
   through `accept_block`: generate proofs → `with_call()` → `with_fee_collect()` → `submit()`.

2. **State transition verification:** The test SHALL verify that contract state transitioned
   correctly, not merely that `height > before`.

3. **Real ZK proofs:** Every `requires_proof = true` function SHALL use real `Proof` objects.
   ZK-proof-only (proof generated but never submitted) SHALL NOT satisfy this criterion.

4. **FeeCollectV1 presence:** Every block SHALL include FeeCollectV1 unconditionally.

5. **Nullifier replay rejection:** At least one nullifier-producing endpoint SHALL be tested
   for replay rejection.

6. **Determinism:** Same inputs → identical block hashes on repeated runs.

7. **No suppression patterns:** Zero match-Err-skip, ZK-proof-only, comment-deferred,
   explicit-skip, strict_zk toggling, or `println!("skipped")`.

### 2.3 Test Subtypes

| Subtype | Scope | Deployment |
|---------|-------|-----------|
| Genesis-Standalone | Single genesis contract, all function enum variants | Static ContractId |
| WASM-Deployed | Single contract, all function enum variants | `chain.deploy()` |
| Block-Execution | Block production machinery | Mixed |
| Cross-Contract | Multi-contract orchestration | Mixed |

---

## 3. Non-Negotiable Criteria

Every Level 2 heavyweight test SHALL meet these criteria. No exception without an MoC review.

### 3.1 Endpoint Exhaustiveness

**Criterion:** The test SHALL exercise EVERY variant of the contract's function enum through `accept_block`.

A variant whose circuit is known-broken SHALL produce a test failure, not a skip comment.
If a variant is genuinely not ready for testing, the test SHALL be marked `#[ignore]` with
a tracking issue reference.

### 3.2 Full Production Path — accept_block

**Criterion:** Every contract function invocation SHALL be submitted through `accept_block`.
Proof generation alone SHALL NOT satisfy endpoint coverage.

### 3.3 State Transition Verification

**Criterion:** After every `accept_block` submission, the test SHALL verify at least one
state transition beyond "height advanced." The verification SHALL query the contract's
sled trees and assert expected key-value pairs.

### 3.4 Real ZK Proofs for ZK-Gated Functions

**Criterion:** Every function with `requires_proof = true` SHALL be exercised with real
`Proof` objects from the harness's `ProvingKey`. `empty_witnesses()` SHALL NOT be used
in any harness method.

### 3.5 FeeCollectV1 in Every Block

**Criterion:** Every block SHALL include FeeCollectV1 unconditionally. `with_fee_collect()`
SHALL NOT silently skip when no FeeV1 calls exist.

### 3.6 Nullifier Replay Rejection

**Criterion:** Every test for a contract with at least one ZK-gated function SHALL verify
nullifier replay rejection.

### 3.7 Deterministic Execution

**Criterion:** Two independent `HeavyweightPipeline` instances executing the same scenario
SHALL produce identical block hashes.

---

## 4. Anti-Patterns — Prohibited Patterns

### 4.1 match-Err-skip

```rust
// PROHIBITED:
match harness.deposit(...) {
    Ok(d) => { /* accept_block */ }
    Err(e) => println!("    deposit proof skipped: {}", e),
}
```

**Required:** Unwrap the result. If the operation fails, the test SHALL fail.

### 4.2 ZK-proof-only

```rust
// PROHIBITED:
let _pv = harness.push_value(...)?;  // result discarded
println!("=== push_value proof OK ===");
```

**Required:** Every harness call result SHALL feed into `with_call()` + `submit()`.

### 4.3 Comment-Deferred

```rust
// PROHIBITED:
// BurnV1 accept_block routing is deferred until harness adds call_data encoding.
```

**Required:** Either implement the accept_block path or mark the test `#[ignore]` with a tracking issue.

### 4.4 Explicit Skip

```rust
// PROHIBITED:
println!("  Test: CreateClaimDAG (skipped — pre-existing circuit bug)");
```

**Required:** The endpoint SHALL be exercised. If it fails, the test fails.

### 4.5 strict_zk Toggling

**Background:** `const STRICT_ZK: bool = true` is immutable and structural. No `strict_zk` field
exists on `HeavyweightPipeline` — the field was removed during Phase 1 per PR-1. ZK proof
enforcement is structural: the uniform runner's `submit_block()` checks `EndpointSpec::is_zk`
(authoritative contract metadata, never a heuristic) and rejects empty proofs BEFORE calling
`with_call()`. There is no toggle to bypass this enforcement. Non-ZK functions SHALL be
declared with `EndpointSpec::is_zk = false`.

### 4.6 Single-Block Batching Without Per-Call Error Isolation

**Required:** Each endpoint SHALL be submitted in its own block. Exception: cross-contract
and block-execution tests that explicitly test multi-call ordering.

### 4.7 println!("skipped")

**Required:** grep for `skipped` SHALL return zero hits across all heavyweight test sources.

### 4.8 Early-Return on Harness Failure

```rust
// PROHIBITED:
let result = match harness.call() {
    Ok(d) => d,
    Err(e) => { println!("skipped"); return Ok(()); }
};
```

**Required:** Harness failure SHALL fail the test. No early return on error.

### 4.9 Asserting Known Security Bugs Pass

```rust
// PROHIBITED:
assert!(result.is_ok(), "Uncle application should succeed (validation not yet enforced)");
```

**Required:** Tests SHALL assert correct behavior. Known-missing validation SHALL be
documented with a tracking issue and the assertion SHALL reflect the CORRECT behavior
(expected rejection), gated behind `#[ignore]` until fixed.

### 4.10 Temporary Compatibility Shims

```rust
// PROHIBITED:
#[deprecated(note = "Migrate to uniform runner")]
pub fn with_call_compat(&mut self, cid, harness, call_data, proofs) -> Result<&mut Self> {
    let is_zk = !proofs.is_empty();  // HEURISTIC — not authoritative
    self.with_call(cid, harness, call_data, proofs, is_zk)
}
```

**Why prohibited:** A compatibility shim created during a refactor serves no purpose in the
final codebase. It exists only to bridge between old and new API signatures — a gap that
SHALL NOT exist in committed code (all call sites SHALL be updated in the same commit as
a signature change, per PR-6). Temporary shims: (a) are not traceable to any spec section,
(b) use heuristics where authoritative metadata is required (see §4.5, RG-21), (c) become
permanent because "temporary" code is never prioritized for removal.

**Required:** The heavyweight test infrastructure SHALL NOT contain temporary compatibility
methods, migration bridges, adapter layers, or any code whose sole purpose is bridging old
code to new during a refactor. `#[deprecated]` SHALL NOT be used on code at the moment of
its creation. API evolution SHALL be additive (new methods added, old methods removed only
after all callers migrated) or atomic (signature change + all call site updates in one commit).

### 4.11 False Positives — Zero-Knowledge Proofs That Prove Nothing

```rust
// PROHIBITED — proof verifies but constrains nothing about contract logic:
let witnesses = empty_witnesses(zkbin)?;
let circuit = ZkCircuit::new(witnesses, zkbin);
let proof = Proof::create(pk, &[circuit], &[], OsRng)?;
```

**Why prohibited:** An `empty_witnesses` proof is a valid halo2 proof — the proof system accepts it.
But the proof constrains nothing about the contract's function parameters. The circuit's `constrain_instance`
calls bind instance values from the witness, and empty witnesses set all values to zero. The resulting
proof attests to a trivial statement ("zero equals zero") rather than the contract's intended predicate
("the holder knows the credential secret" or "the input coin exists in the Merkle tree").

A test that submits an empty-witness proof through `accept_block` will pass if the contract verifies
the proof but does not validate that the proof's public inputs match the function's expected parameters.
This is a **false positive**: the test passes, the block is accepted, but the contract's ZK security
— its entire authorization model — was never exercised.

**The severity:** This is not a gap in coverage. It is a **false signal of coverage**. A passing test
with empty_witnesses gives the operator confidence that the endpoint is verified, when in fact the ZK
circuit — the contract's primary security mechanism — was never tested with real data. This is worse
than an untested endpoint because it actively conceals the gap.

**Required:** Every ZK proof submitted to `accept_block` SHALL be generated from real witnesses
derived from the function's actual parameters. `empty_witnesses()` SHALL NOT be used in any harness
method's proof generation path. Harnesses using empty_witnesses SHALL be classified as STUB and their
spec endpoints SHALL be marked `#[ignore = "tracking: URL — empty_witnesses proofs"]`.

**Detection:** The anti-pattern scanner (`contrib/ci/scan_heavyweight_antipatterns.sh`) SHALL detect
`empty_witnesses` in harness method bodies (outside `spawn()`) and `Proof::create` with empty public
inputs (`&[]`). CI SHALL fail if either pattern is found.

### 4.12 Silenced Failures — Tests That Always Pass

A test SHALL be considered **fraudulent** if it can pass without exercising every declared endpoint
through `accept_block`. The following patterns SHALL cause the anti-pattern scanner to return a
failure:

- **match-Err-skip**: `match harness.method() { Ok(d) => { ... }, Err(e) => println!("skipped") }`
- **ZK-proof-only**: `let _ = harness.method()?; println!("proof OK");` — proof generated, never submitted
- **comment-deferred**: `// accept_block routing deferred until harness adds call_data encoding`
- **explicit-skip**: `println!("Test: X (skipped — pre-existing circuit bug)");`
- **early-return**: `Err(e) => { println!("skipped"); return Ok(()); }`

Any test containing these patterns SHALL NOT be counted as "passing" in coverage metrics. The test
SHALL be fixed (endpoint exercised through accept_block) or marked `#[ignore = "tracking: URL"]`
with a concrete remediation plan.

**The rule:** A test that cannot exercise an endpoint through accept_block SHALL fail. No exceptions.
"Pre-existing bug" is not a justification for silence — it is a reason to fix the bug.

---

## 5. Category-Specific Requirements

### 5.1 Consensus-Critical — native_token

**Role:** The single bespoke citizen. Block rewards, fee payment, value transfer.
No manifest. 7 functions: FeeV2 (0x08), MintV1 (0x01, disabled), BurnV1 (0x02),
TransferV1 (0x03), SpendV1 (0x04), PoWRewardV1 (0x05), FeeCollectV1 (0x06).
FeeV1 (0x00) is REMOVED — returns InvalidFunction.
3 ZK circuits: MintV2, BurnV2, FeeV2.

**Test SHALL:**
- Use `NATIVE_TOKEN_CONTRACT_ID` — never `chain.deploy()`
- Verify MintV1 returns `FunctionDisabled` (walled off behind PoWRewardV1)
- Route BurnV1, FeeV2, TransferV1, SpendV1 each through accept_block with real proofs
- FeeV2: privacy-preserving with dual ZK proofs (Fee_V2 + FeeThreshold_V1).
  Merkle root from production tree (tree.root(0)), never recomputed manually.
  Call data: `[0x08][FeeParamsV2]` — NO clear-text fee bytes.
- Verify cumulative supply after every value-moving operation
- Verify block hash chain continuity across all blocks
- Verify FeeCollectV1 plate state after fee collection
- Verify PoWRewardV1 coinbase reward equals `expected_reward(height)` per block

**PoWRewardV1 and FeeCollectV1** are exercised structurally by every block's coinbase
and `with_fee_collect()`. The native_token test SHALL additionally verify their state
effects explicitly (reward amount, AEAD encryption, nullifier insertion, fee plate).

### 5.2 Infrastructure — deployooor

**Role:** Second sanctioned citizen. Deploys WASM, locks contracts. 2 functions:
DeployV1 (0x00), LockV1 (0x01). Zero ZK circuits.

**Test SHALL:**
- Use `DEPLOYOOOR_CONTRACT_ID` — never `chain.deploy()`
- DeployV1: use a real valid WASM binary, verify WASM appears in contracts tree
- LockV1: verify deployed contract becomes immutable
- No ZK proofs — `circuits()` is empty
- Verify state: deployed WASM exists in contracts tree, lock flag is set

### 5.3 L1 O-Cap Primitives — box, purse

**box:** 3 functions: InitializeV1 (0x00), PutV1 (0x01), TakeV1 (0x02). 2 ZK circuits.

**Test SHALL:**
- InitializeV1 first (non-ZK, sets up merkle trees)
- PutV1: verify merkle leaf insertion and root update
- TakeV1: verify merkle leaf consumption and nullifier insertion
- Nullifier replay: TakeV1 SHALL reject duplicate nullifier
- Determinism: same operations → identical merkle roots

**purse:** 4 functions: InitializeV1 (0x00), DepositV1 (0x01), WithdrawV1 (0x02),
BalanceV1 (0x03). 3 ZK circuits.

**Test SHALL:**
- InitializeV1 first
- DepositV1: verify balance increase
- WithdrawV1: verify balance decrease, nullifier insertion
- BalanceV1: read-only — verify correct balance returned, no state mutation
- Nullifier replay: WithdrawV1 SHALL reject duplicate nullifier

### 5.4 O-Cap Authorization — identity

**Role:** Credentials, ZK claims (5 modes via unified CreateClaimV2 circuit),
capabilities. 9 functions, 3 ZK circuits. Stores BOX_CONTRACT_ID at init
(architectural intent for Box::Put/Take — not yet implemented in entrypoint).

**Test SHALL:**
- Use `IDENTITY_CONTRACT_ID`
- Bootstrap order: RegisterIssuerV1 (0x08) → IssueCredentialV1 (0x01) → CreateClaimV1 (0x03)
- InitializeV1 (0x00): verify circuit registration, tree creation
- IssueCredentialV1: real ZK proof, verify credential stored, nullifier inserted
- RevokeCredentialV1 (0x02): verify issuer signature check, credential marked revoked
- CreateClaimV1: real ZK proof, 5 claim modes (basic/threshold/ratio/multi/DAG)
- RegisterCapabilityV1 (0x04), IssueCapabilityV1 (0x05): verify capability lifecycle
- VerifyCapabilityV1 (0x06): real ZK proof, verify capability verified
- RevokeCapabilityV1 (0x07): verify revocation
- RegisterIssuerV1 (0x08): verify issuer stored
- All 5 CreateClaim modes SHALL be tested (not skipped)
- Verify BOX_CONTRACT_ID stored in info tree at initialization

### 5.5 O-Cap Authorization — attestation

**Role:** Attestation framework. 13 functions, 10 ZK circuits. Entrypoint shows zero
cross-contract child calls. Manifest dependencies unsubstantiated by code.

**Test SHALL:**
- Use `ATTESTATION_CONTRACT_ID`
- All 13 functions exercised through accept_block:
  0x00 CreateAttestationV1, 0x01 RevokeAttestationV1, 0x02 ExpireAttestationV1,
  0x03 CreateClaimV1, 0x04 VerifyClaimV1, 0x05 ConsumeClaimV1, 0x06 ValidateClaimV1,
  0x07 CheckNotRevokedV1, 0x08 DelegateAttestationV1, 0x09 VerifyChainV1,
  0x0a UpdateDelegationV1, 0x0b AttestSlashV1, 0x0c CommitFeeScheduleV1
- Delegation chain: CreateAttestation → DelegateAttestation → VerifyChain
- Claim lifecycle: CreateClaim → VerifyClaim → ConsumeClaim
- Non-ZK functions (0x01, 0x02, 0x06): submit with empty proofs
- All ZK functions: real proofs from correct circuit

### 5.6 O-Cap Authorization — multisig

**Role:** Threshold signature factory. 4 functions, 3 ZK circuits.

**Test SHALL:**
- Use `MULTISIG_CONTRACT_ID`
- Bootstrap: CreateGroupV1 (0x01) → SignV1 (0x02) → FinalizeV1 (0x03)
- InitializeV1 (0x00): non-ZK, circuit registration
- CreateGroupV1: real ZK proof, verify group stored with correct threshold
- SignV1: real ZK proof, verify partial signature stored
- FinalizeV1: real ZK proof, verify approval capability produced
- Nullifier replay on FinalizeV1

### 5.7 DeFi Capability Primitive — promissory_note

**Role:** Currency plumbing. Multi-token creation, minting with backing proofs,
private transfers, OTC swaps, redemption. 6 functions, 5 ZK circuits.
Does NOT compose from native_token — completely separate.

**Test SHALL:**
- Use `PROMISSORY_NOTE_CONTRACT_ID`
- Token lifecycle: RegisterTypeV1 (0x00) → IssueV1 (0x02) → TransferV1 (0x04) → RedeemV1 (0x01)
- RegisterTypeV1: real ZK proof, verify token_id and token_auth_parent stored in registry
- IssueV1: real ZK proof, verify mint_public matches stored token_auth_parent, coin created
- RevokeV1 (0x03): real ZK proof, verify nullifier inserted, coin destroyed
- TransferV1: real ZK proof, verify value conservation via Pedersen homomorphism
- OtcSwapV1 (0x05): real ZK proof, exactly 2 inputs/2 outputs, per-token_commit conservation
- RedeemV1: real ZK proof, verify receipt coin has value=0 (is_notequal gate)
- BlindOutput_V1: every Transfer/OtcSwap output SHALL carry valid blind output proof
- Token registry: duplicate RegisterTypeV1 SHALL be rejected
- Spend hooks: verify spend_hook routing when non-zero
- Nullifier replay on RevokeV1

### 5.8 Standalone Oracle — oracle

**Role:** Data feeds. 6 functions, 5 ZK circuits.

**Test SHALL:**
- Use `ORACLE_CONTRACT_ID`
- RegisterOracleV1 (0x00): real ZK proof, verify oracle stored
- PushValueV1 (0x01): real ZK proof through accept_block
- AttestValueV1 (0x02): real ZK proof through accept_block
- PushValueCommitmentV1 (0x03): real ZK proof through accept_block
- AggregateV1 (0x04): real ZK proof through accept_block
- SetOracleActiveV1 (0x05): non-ZK, through accept_block
- All 5 ZK endpoints SHALL go through accept_block (no ZK-proof-only)

### 5.9 WASM-Deployed Contracts

**Test SHALL:**
- Deploy via `chain.deploy()` with `include_bytes!` WASM
- Verify WASM in contracts tree after deployment
- Exercise all function enum variants through accept_block
- Verify state isolation from other contracts
- `harness.verify_zk_coverage()` SHALL pass pre-deploy

---

## 6. State Verification Requirements

After every `accept_block` submission, the test SHALL verify at minimum:

| Function Pattern | State Tree | Key Verification |
|-----------------|-----------|-----------------|
| Create/Register | Entity tree | Entity exists with expected fields |
| Update/Execute | Entity tree | Entity fields match new values |
| Cancel/Revoke | Entity tree | Entity status is cancelled/revoked |
| Mint/Issue | Balance or supply tree | Balance increased by expected amount |
| Burn/Destroy | Balance or supply tree | Balance decreased by expected amount |
| Transfer | Balance trees (sender + receiver) | Sender decreased, receiver increased |

Additionally:
- **ST-1:** Height advanced: `assert!(chain.height() > height_before)`
- **ST-2:** Contract state tree was written (query + assert)
- **ST-3:** Nullifier was inserted (for ZK-gated functions)
- **ST-4:** Cumulative supply is consistent (after mint/burn)

### Pre-Test Integrity Checks

- **PI-1:** Genesis block hash verified against known constant
- **PI-2:** Initial cumulative supply equals `INITIAL_REWARD`
- **PI-3:** Contract exists at genesis height (for genesis contracts)
- **PI-4:** ZK coverage pre-check passes

### Post-Test Integrity Checks

- **PI-5:** Block hash chain continuity from height 2 through final
- **PI-6:** Cumulative supply reconciliation
- **PI-7:** State tree root determinism across two independent pipelines

---

## 7. Structural Requirements

### 7.1 ContractHarness

Every harness SHALL:
- **CR-1:** Load all circuits declared in contract manifest
- **CR-2:** Build ProvingKey with `zkbin.k`, not hardcoded constant
- **CR-3:** Expose state query methods for post-call verification
- **CR-4:** Map 1:1 to contract function enum
- **CR-5:** Return typed result structs with `call_data: Vec<u8>` and `proofs: Vec<Proof>`
- **CR-6:** Use real client proof functions — never `empty_witnesses()`
- **CR-7:** Use consistent circuit naming: `FunctionNameV2` (no underscore before V2)

### 7.2 HeavyweightPipeline

- **PR-1:** `strict_zk` SHALL be immutable (`true` always)
- **PR-2:** `with_fee_collect()` SHALL be unconditional — always appends FeeCollectV1
- **PR-3:** Expose state inspection API: `query_contract_tree()`, `cumulative_supply()`
- **PR-4:** Per-function ZK gating enforced by the uniform runner's `submit_block()` (function-level, not harness-level). `with_call()` is a data accumulation method, not a security gate. `is_zk` SHALL come from `EndpointSpec::is_zk` — authoritative contract metadata, never a heuristic.
- **PR-5:** Expose `block_hash()` post-submission for determinism verification
- **PR-6:** `with_call()` SHALL have exactly one signature. ZK gating SHALL NOT be performed in `with_call()`. `with_call()` SHALL accept proofs without validating whether they are required.

### 7.3 HeavyweightBlock

- **BR-1:** Support single-endpoint-per-block by default
- **BR-2:** Emit structured diagnostics (height, tx count, outcome, timing)
- **BR-3:** Store block hash post-submission
- **BR-4:** SHALL NOT acquire adapter, bridge, compat, shim, or temporary methods. Every method on `HeavyweightBlock` SHALL have a permanent role traceable to a spec section. (§4.10)

---

## 8. Block Execution Tests

| Test | What It Verifies |
|------|-----------------|
| canonical_exec | Block with canonical transactions accepted |
| uncle_exec | Block with one uncle accepted |
| mixed_exec | Canonical + uncle transactions, correct ordering |
| multi_uncle | Multiple uncles (>1), uncle-merkle root verified |
| uncle_depth | Uncles at different depths, correct reward adjustments |
| empty_uncle | Uncle with no transactions |
| invalid_uncle_proof | Invalid merkle proof → block rejection |
| coinbase_rejects_wrong_reward | Wrong reward → block rejection |

Requirements:
- **BE-1:** Real ZK coinbases via `build_linear_coinbase()`
- **BE-2:** FeeCollectV1 in every block
- **BE-3:** Uncles constructed from real competing blocks
- **BE-4:** Gas tracking verified per block
- **BE-5:** Uncle reward = `base_reward / (2^depth)`

---

## 9. Per-Contract Test Template

```
For contract <Name> of category <Category>:

Architecture (from entrypoint):
  - Dependencies: [none, verified from entrypoint]
  - Functions: [count, list ZK vs non-ZK]
  - Circuits: [count, list]
  - Trees: [list from manifest]

Test sequence:
  1. Pre-test: genesis block hash, initial supply, contract exists at height 1
  2. Contract initialization (InitializeV1 if present)
  3. For each function F in <Name>Function enum:
     a. [If ZK-gated] Generate proof with real witnesses
     b. Build block: with_call(cid, &harness, &call_data, proofs)
     c. with_fee_collect() — unconditional
     d. submit() through accept_block
     e. Assert height advanced
     f. Verify state transition per function pattern (§6)
  4. [If has ZK functions] Nullifier replay rejection
  5. Determinism: two independent pipelines, identical hashes
  6. Post-test: hash chain continuity, supply reconciliation
```

---

## 10. Compliance Checklist

### Per-Test

- [ ] CHK-01: All function enum variants exercised through accept_block
- [ ] CHK-02: All ZK-gated functions use real proofs
- [ ] CHK-03: FeeCollectV1 in every block
- [ ] CHK-04: State transition verified per block
- [ ] CHK-05: Nullifier replay rejection verified
- [ ] CHK-06: Determinism verified
- [ ] CHK-07: Zero suppressed failures (grep for `skipped` returns zero)
- [ ] CHK-08: Pre-test integrity checks pass
- [ ] CHK-09: Post-test integrity checks pass
- [ ] CHK-10: One endpoint per block (exception: cross-contract, block-execution)
- [ ] CHK-11: Genesis contracts not re-deployed (use static ContractId)

### Per-Harness

- [ ] CHK-12: All manifest circuits loaded
- [ ] CHK-13: ProvingKey::build uses zkbin.k
- [ ] CHK-14: State query methods exposed
- [ ] CHK-15: 1:1 function enum mapping
- [ ] CHK-16: Typed result structs with call_data and proofs
- [ ] CHK-17: No empty_witnesses() in any method
- [ ] CHK-18: Circuit names use `FunctionNameV2` convention

### Per-Pipeline

- [ ] CHK-19: strict_zk immutable (always true)
- [ ] CHK-20: FeeCollectV1 unconditional
- [ ] CHK-21: State inspection API exists
- [ ] CHK-22: Block hash exposed for determinism

---

## References

- [Type System Specification](../arch/type-system.md)
- [Wallet Architecture Specification](../arch/wallet.md)
- [Testing Overview](overview.md)
- [Promissory Note](../contract/promissory_note.md)
- [Contract Safety Patterns](contracts/safety.md)
- Bradner, S. (1997). "Key words for use in RFCs to Indicate Requirement Levels." RFC 2119.
