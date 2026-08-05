# Uniform Heavyweight Test Runner — Design Document

Spec: heavyweight-spec.md. Guardrails: RG-0 through RG-15.

## 1. HeavyweightPipeline API (Post-Refactor)

All changes relative to current `bin/dwowd/src/tests/blockchain.rs`.

### Constants

```rust
/// Strict ZK enforcement — immutable. Per spec §7.2 PR-1, §4.5.
const STRICT_ZK: bool = true;
```

### Struct

```rust
pub struct HeavyweightPipeline {
    pub db: Arc<sled::Db>,
    pub chain_state: Arc<CChainState>,
    pub linear_zk: Arc<LinearPowRewardZk>,
    keys_path: PathBuf,
    // REMOVED: pub strict_zk: bool  (replaced by const STRICT_ZK)
}
```

### Public Methods

| Method | Signature | Spec Ref | Change from Current |
|--------|-----------|----------|-------------------|
| `new()` | `async fn new() -> Result<Self>` | — | Remove `strict_zk: true` from init |
| `init_genesis()` | `async fn init_genesis(&self) -> Result<()>` | — | Unchanged |
| `deploy()` | `async fn deploy(&self, harness, name, wasm) -> Result<ContractId>` | RG-7 | ADD genesis ContractId rejection |
| `deploy_with_ix()` | `async fn deploy_with_ix(&self, harness, name, wasm, ix) -> Result<ContractId>` | RG-7 | ADD genesis ContractId rejection |
| `height()` | `fn height(&self) -> BlockHeight` | — | Unchanged |
| `block()` | `fn block(&self) -> Result<HeavyweightBlock>` | — | Unchanged |
| `build_coinbase_for_height()` | `async fn build_coinbase_for_height(&self, h, r) -> Result<CoinbaseResult>` | — | Unchanged |
| `expected_target()` | `fn expected_target(&self, h) -> BlockTarget` | — | Unchanged |
| `query_contract_tree()` | `fn query_contract_tree(&self, cid, tree, key) -> Result<Option<Vec<u8>>>` | RG-8, spec §7.2 PR-3 | **NEW** |
| `cumulative_supply()` | `fn cumulative_supply(&self) -> u64` | RG-8, spec §7.2 PR-3 | **NEW** |
| `block_hash_chain_continuous()` | `fn block_hash_chain_continuous(&self) -> bool` | RG-8 | **NEW** |
| `block_hash_at()` | `fn block_hash_at(&self, height: BlockHeight) -> Option<blake3::Hash>` | RG-8 | **NEW** |

### `query_contract_tree()` Implementation

```rust
pub fn query_contract_tree(
    &self,
    contract_id: ContractId,
    tree_name: &str,
    key: &[u8],
) -> Result<Option<Vec<u8>>> {
    let tree = self.chain_state.store
        .contract_tree(contract_id, tree_name)
        .map_err(|e| Error::Custom(format!("tree lookup: {}", e)))?;
    tree.get(key)
        .map_err(|e| Error::Custom(format!("tree get: {}", e)))
        .map(|opt| opt.map(|iv| iv.to_vec()))
}
```

### Genesis Deploy Rejection

```rust
const GENESIS_CONTRACT_IDS: &[ContractId] = &[
    *NATIVE_TOKEN_CONTRACT_ID,
    *DEPLOYOOOR_CONTRACT_ID,
    *IDENTITY_CONTRACT_ID,
    *ATTESTATION_CONTRACT_ID,
    *MULTISIG_CONTRACT_ID,
    *ORACLE_CONTRACT_ID,
    *PROMISSORY_NOTE_CONTRACT_ID,
    *PURSE_CONTRACT_ID,
    *BOX_CONTRACT_ID,
];

// In deploy():
if GENESIS_CONTRACT_IDS.contains(&contract_id) {
    return Err(Error::Custom(format!(
        "Cannot deploy genesis contract '{}' — use its static ContractId", name
    )));
}
```

Note: deploy() derives contract_id from name BEFORE the check. For genesis contracts, the derived test ContractId differs from the static one, so the check should compare against the name, not the derived ID. Alternative: check name against a known list of genesis contract names.

Revised approach:
```rust
const GENESIS_CONTRACT_NAMES: &[&str] = &[
    "native_token", "deployooor", "identity", "attestation", "multisig",
    "oracle", "promissory_note", "purse", "box",
];

if GENESIS_CONTRACT_NAMES.contains(&name) {
    return Err(Error::Custom(format!(
        "Cannot deploy genesis contract '{}' — use its static ContractId", name
    )));
}
```

## 2. HeavyweightBlock API (Post-Refactor)

### Struct

```rust
pub struct HeavyweightBlock<'c> {
    chain: &'c HeavyweightPipeline,
    height: BlockHeight,
    reward: BlockReward,
    contract_txs: Vec<Transaction>,
    uncles: Vec<UncleBlock>,
    block_hash: Option<blake3::Hash>,  // NEW — stored after submit
}
```

### Public Methods

| Method | Signature | Spec Ref | Change from Current |
|--------|-----------|----------|-------------------|
| `with_call()` | `fn with_call(&mut self, cid, harness, call_data, proofs, is_zk) -> Result<&mut Self>` | RG-5, spec §7.2 PR-4 | ADD `is_zk: bool` parameter; REMOVE strict_zk field read |
| `with_uncle()` | `fn with_uncle(&mut self, uncle) -> &mut Self` | — | Unchanged |
| `with_uncles()` | `fn with_uncles(&mut self, uncles) -> &mut Self` | — | Unchanged |
| `with_fee_collect()` | `fn with_fee_collect(&mut self) -> Result<&mut Self>` | RG-6, spec §7.2 PR-2 | UNCONDITIONAL — always appends FeeCollectV1 |
| `build_coinbase()` | `async fn build_coinbase(&self) -> Result<CoinbaseResult>` | — | Unchanged |
| `submit()` | `async fn submit(&mut self) -> Result<BlockHeight>` | RG-8 | Store block_hash after submission |
| `submit_with_coinbase()` | `async fn submit_with_coinbase(&mut self, tx) -> Result<BlockHeight>` | — | Store block_hash |
| `block_hash()` | `fn block_hash(&self) -> Option<blake3::Hash>` | RG-8, spec §7.2 PR-5 | **NEW** |

### `with_call()` — Per-Function ZK Gating

```rust
pub fn with_call(
    &mut self,
    contract_id: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<Proof>,
    is_zk_function: bool,  // NEW parameter
) -> Result<&mut Self> {
    // ZK gating: per-function, not per-harness
    if is_zk_function && proofs.is_empty() {
        return Err(Error::Custom(format!(
            "ZK function on contract '{}' requires proofs (got 0)", harness.name()
        )));
    }

    let mut tx = build_contract_tx(contract_id, call_data.to_vec());
    tx.witness = build_witness(contract_id, call_data, proofs);
    self.contract_txs.push(tx);
    Ok(self)
}
```

### `with_fee_collect()` — Unconditional

```rust
pub fn with_fee_collect(&mut self) -> Result<&mut Self> {
    let fee_txs: Vec<Transaction> = self.contract_txs.iter()
        .filter(|tx| tx.contract_calls.iter().any(|c|
            c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
            && c.data.first() == Some(&0x00)
        ))
        .cloned()
        .collect();

    let mgr = AccountManager::open(/* ... */)?;
    let recipient = MiningRecipient::from_account(&mgr, self.height)?;
    drop(mgr);

    // Always call build_fee_collect_tx — it handles empty fee_txs internally
    // (produces FeeCollectV1 with zero fees to collect)
    let fee_collect_tx = build_fee_collect_tx(
        &recipient, &fee_txs, self.height, &self.chain.linear_zk,
    ).map_err(|e| Error::Custom(format!("build_fee_collect_tx: {}", e)))?;

    // Always append — even if fee_collect_tx collects zero fees
    self.contract_txs.push(fee_collect_tx.unwrap_or_else(|| {
        // If build_fee_collect_tx returns None for empty fee_txs, build a zero-fee tx
        build_zero_fee_collect_tx(&recipient, self.height, &self.chain.linear_zk)
    }));
    Ok(self)
}
```

## 3. ContractTestSpec and EndpointSpec

```rust
/// Specification for a single contract endpoint.
pub struct EndpointSpec<'a> {
    /// Function name (matches function enum variant)
    pub name: &'static str,
    /// Whether this function requires a ZK proof
    pub is_zk: bool,
    /// Produces call_data + proofs for this endpoint
    pub generate: Box<dyn Fn() -> Result<EndpointResult> + 'a>,
    /// State tree to verify after submission
    pub state_tree: &'static str,
    /// Key to query in the state tree
    pub state_key_fn: Box<dyn Fn() -> Vec<u8> + 'a>,
}

/// Result of generating proofs + call_data for one endpoint.
pub struct EndpointResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Full specification for a contract's heavyweight test.
pub struct ContractTestSpec<'a> {
    /// Contract name (matches directory name)
    pub name: &'static str,
    /// Whether this is a genesis contract (uses static ContractId)
    pub is_genesis: bool,
    /// ContractId — static for genesis, derived for WASM
    pub contract_id: ContractId,
    /// The contract harness
    pub harness: &'a dyn ContractHarness,
    /// WASM bytes — None for genesis, Some(include_bytes!) for WASM
    pub wasm_bytes: Option<&'a [u8]>,
    /// Whether this contract has an InitializeV1 function
    pub has_initialize: bool,
    /// Generate InitializeV1 call_data (if has_initialize)
    pub initialize: Option<Box<dyn Fn() -> Result<EndpointResult> + 'a>>,
    /// All endpoints in function enum order
    pub endpoints: Vec<EndpointSpec<'a>>,
    /// Whether any endpoint is ZK-gated
    pub has_zk_functions: bool,
    /// Index of the first ZK endpoint for nullifier replay testing
    pub nullifier_replay_index: Option<usize>,
    /// State tree names for verification
    pub state_trees: &'static [&'static str],
}

impl<'a> ContractTestSpec<'a> {
    /// Verify the spec is internally consistent before running the test.
    pub fn validate(&self) -> Result<()> {
        // Genesis contracts must not provide wasm_bytes
        if self.is_genesis && self.wasm_bytes.is_some() {
            return Err(Error::Custom("Genesis contract must not have wasm_bytes".into()));
        }
        // WASM contracts must provide wasm_bytes
        if !self.is_genesis && self.wasm_bytes.is_none() {
            return Err(Error::Custom("WASM contract must have wasm_bytes".into()));
        }
        // Nullifier replay index must point to a ZK endpoint
        if let Some(idx) = self.nullifier_replay_index {
            if idx >= self.endpoints.len() {
                return Err(Error::Custom("nullifier_replay_index out of bounds".into()));
            }
            if !self.endpoints[idx].is_zk {
                return Err(Error::Custom("nullifier_replay_index must point to ZK endpoint".into()));
            }
        }
        // has_initialize must match initialize fn presence
        if self.has_initialize != self.initialize.is_some() {
            return Err(Error::Custom("has_initialize must match initialize fn".into()));
        }
        Ok(())
    }
}
```

## 4. run_heavyweight_test() Pseudocode

```rust
/// The single uniform test runner. Every heavyweight test calls this.
/// Enforces heavyweight-spec.md structurally.
pub async fn run_heavyweight_test(spec: &ContractTestSpec<'_>) -> Result<()> {
    // Validate spec
    spec.validate()?;

    // ── Pipeline A (primary) ─────────────────────────────────────────
    let chain_a = HeavyweightPipeline::new().await?;
    chain_a.init_genesis().await?;

    // ── Pre-test integrity checks (spec §5.2) ────────────────────────
    verify_genesis_block_hash(&chain_a)?;          // PI-1
    verify_initial_supply(&chain_a)?;               // PI-2
    if spec.is_genesis {
        verify_contract_at_genesis(&chain_a, spec.contract_id)?; // PI-3
    }
    spec.harness.verify_zk_coverage()?;             // PI-4

    // ── Deploy if WASM ──────────────────────────────────────────────
    let cid = if spec.is_genesis {
        spec.contract_id
    } else {
        chain_a.deploy(spec.harness, spec.name, spec.wasm_bytes.unwrap())?
    };

    // ── Initialize (if contract has InitializeV1) ────────────────────
    let mut height_before = chain_a.height();
    if let Some(init_fn) = &spec.initialize {
        let result = init_fn()?;
        assert!(!result.call_data.is_empty());
        let new_height = submit_block(&chain_a, cid, spec.harness,
            &result.call_data, result.proofs, false).await?; // is_zk=false
        assert!(new_height > height_before, "height must advance after InitializeV1");
        height_before = new_height;
    }

    // ── Exercise every endpoint (one per block) ─────────────────────
    for (i, endpoint) in spec.endpoints.iter().enumerate() {
        let result = (endpoint.generate)()?;
        assert!(!result.call_data.is_empty(),
            "{}: call_data must not be empty", endpoint.name);

        let new_height = submit_block(&chain_a, cid, spec.harness,
            &result.call_data, result.proofs, endpoint.is_zk).await?;

        assert!(new_height > height_before,
            "{}: height must advance after accept_block", endpoint.name);

        // State verification (spec §6)
        let key = (endpoint.state_key_fn)();
        let value = chain_a.query_contract_tree(cid, endpoint.state_tree, &key)?;
        assert!(value.is_some(),
            "{}: state tree '{}' must contain key after accept_block",
            endpoint.name, endpoint.state_tree);

        height_before = new_height;
    }

    // ── Nullifier replay rejection (spec §3.6) ──────────────────────
    if let Some(idx) = spec.nullifier_replay_index {
        let endpoint = &spec.endpoints[idx];
        let result = (endpoint.generate)()?;
        // First submission: already done above (succeeds)
        // Second submission: must fail
        let replay_result = submit_block(&chain_a, cid, spec.harness,
            &result.call_data, result.proofs, endpoint.is_zk).await;
        assert!(replay_result.is_err(),
            "{}: nullifier replay MUST be rejected", endpoint.name);
    }

    // ── Post-test integrity checks (spec §5.3) ──────────────────────
    assert!(chain_a.block_hash_chain_continuous()?, "hash chain must be continuous"); // PI-5

    // ── Determinism (spec §3.7) ─────────────────────────────────────
    // Pipeline B: replay identical scenario
    let chain_b = HeavyweightPipeline::new().await?;
    chain_b.init_genesis().await?;
    /* ... replay all steps identically ... */
    // Compare final block hashes
    let hash_a = chain_a.block_hash_at(chain_a.height())?;
    let hash_b = chain_b.block_hash_at(chain_b.height())?;
    assert_eq!(hash_a, hash_b, "determinism: block hashes must match"); // PI-7

    Ok(())
}

/// Uniform block submission helper.
async fn submit_block(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<Proof>,
    is_zk: bool,
) -> Result<BlockHeight> {
    chain.block()?
        .with_call(cid, harness, call_data, proofs, is_zk)?
        .with_fee_collect()?   // unconditional
        .submit().await
}
```

## 5. Contract Fit Matrix

How each genesis contract maps to the uniform runner steps. "APPLIES" means the step runs normally. "N/A" means the step is skipped with `has_*: false` or `Option::None` in the spec. "SPECIAL" means the step requires contract-specific handling that must be documented.

| Contract | Deploy | Init | Endpoint Loop | FeeCollect | Nullifier Replay | Determinism | Post-Checks |
|----------|--------|------|---------------|------------|------------------|-------------|-------------|
| box | N/A (genesis) | APPLIES (InitializeV1) | APPLIES (3 endpoints: Init, Put, Take) | APPLIES | APPLIES (TakeV1 nullifier) | APPLIES | APPLIES |
| purse | N/A (genesis) | APPLIES (InitializeV1) | APPLIES (4 endpoints: Init, Deposit, Withdraw, Balance) | APPLIES | APPLIES (WithdrawV1 nullifier) | APPLIES | APPLIES |
| multisig | N/A (genesis) | APPLIES (InitializeV1) | APPLIES (4 endpoints: Init, CreateGroup, Sign, Finalize) | APPLIES | APPLIES (FinalizeV1 nullifier) | APPLIES | APPLIES |
| oracle | N/A (genesis) | N/A (no Init) | APPLIES (6 endpoints) | APPLIES | APPLIES (PushValueV1 nullifier) | APPLIES | APPLIES |
| promissory_note | N/A (genesis) | N/A (genesis-init) | APPLIES (6 endpoints: RegisterType, Redeem, Issue, Revoke, Transfer, OtcSwap) | APPLIES | APPLIES (RevokeV1 nullifier) | APPLIES | APPLIES |
| identity | N/A (genesis) | APPLIES (InitializeV1) | APPLIES (9 endpoints) | APPLIES | APPLIES (CreateClaimV1 nullifier) | APPLIES | APPLIES |
| attestation | N/A (genesis) | N/A (genesis-init) | APPLIES (13 endpoints) | APPLIES | APPLIES (ConsumeClaimV1 nullifier) | APPLIES | APPLIES |
| deployooor | N/A (genesis) | N/A (no Init) | APPLIES (2 endpoints: DeployV1, LockV1) | APPLIES | N/A (zero ZK functions) | APPLIES | APPLIES |
| native_token | N/A (genesis) | N/A (genesis-init) | SPECIAL (7 endpoints, FeeV1/BurnV1 need coinbase params) | APPLIES | APPLIES (BurnV1 nullifier) | APPLIES | APPLIES |

### native_token SPECIAL Handling

FeeV1 and BurnV1 require real coinbase coin parameters (coin_commitment, nullifier, coin_blind, recipient). The current `build_coinbase()` on HeavyweightBlock provides these via `CoinbaseResult`. The native_token test shall:

1. Build coinbase first: `let cb = chain.block()?.build_coinbase().await?;`
2. Use `cb.coin_commitment`, `cb.nullifier`, `cb.coin_blind` in FeeV1/BurnV1 proof generation
3. Submit with pre-built coinbase: `chain.block()?.with_call(...)?.with_fee_collect()?.submit_with_coinbase(cb.tx).await?`

The EndpointSpec for FeeV1 and BurnV1 will access the coinbase through a closure that captures the pre-built coinbase.

MintV1 (disabled): The endpoint spec shall generate call_data for MintV1, submit it, and the test shall assert that accept_block returns an error (FunctionDisabled). This is a SPECIAL endpoint that expects REJECTION, not acceptance.

## 6. Verification Steps (Before Code)

- [x] Design document exists with all 5 sections
- [ ] contract fit matrix validated against all 9 entrypoint files (verify function counts)
- [ ] native_token SPECIAL handling prototyped and confirmed feasible
- [ ] deployooor verify_zk_coverage() with empty circuits() confirmed working
- [ ] All ContractTestSpec types compilable (struct definitions complete)
