# Genesis Harness - DarkFi Contract Testing Foundation

## Overview

The `GenesisHarness` provides a reusable baseline chain setup for DarkFi contract testing. It initializes NativeToken (consensus-first token) and Deployooor (WASM contract deployer), allowing you to test any WASM contract on top of a functioning chain.

**Location:** `bin/darkfid/src/tests/genesis.rs`

## Why GenesisHarness?

Previously, contract tests required manually setting up:
- Blockchain overlay
- Native contracts deployment
- Fork initialization
- Block generation with PoW rewards
- Contract deployment via Deployooor

This was error-prone and duplicated across tests. `GenesisHarness` wraps all of this into a single struct.

## Quick Start

```rust
use crate::tests::genesis::GenesisHarness;

async fn test_my_contract() -> Result<()> {
    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(BigUint::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18440".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18441".to_string(),
    };

    let mut genesis = GenesisHarness::new(config, &ex).await?;

    // Generate 3 genesis blocks (mints NativeToken via PoW)
    genesis.generate_genesis_blocks(3).await?;

    // Deploy your WASM contract via Deployooor
    let wasm_bincode = include_bytes!("../../my_contract/darkfi_my_contract.wasm").to_vec();
    let contract_id = genesis.deploy_contract(wasm_bincode, "MyContract").await?;

    // Now test your contract...
    Ok(())
}
```

## GenesisHarness API

### Struct

```rust
pub struct GenesisHarness {
    pub harness: Harness,                        // The underlying darkfid test harness
    pub fork: Fork,                              // Current fork for block production
    pub keypair: Keypair,                        // Default keypair for signing
    pub deployed_contracts: Vec<ContractId>,      // Track deployed contract IDs
}
```

### Methods

#### `new(config, ex) -> Result<Self>`

Initialize the genesis harness:
- Creates `Harness` with alice/bob nodes
- Initializes fork from alice's consensus
- Sets up default keypair

```rust
let mut genesis = GenesisHarness::new(config, &ex).await?;
```

#### `generate_genesis_blocks(n) -> Result<()>`

Generate `n` blocks with NativeToken PoW rewards:
- Each block mints new NativeToken via `PoWRewardCallBuilder`
- Creates ZK proofs and attaches `zkbin_data` for verification
- Verifies and applies each block to the fork

```rust
// Generate 5 genesis blocks
genesis.generate_genesis_blocks(5).await?;
```

#### `deploy_contract(wasm_bincode, name) -> Result<ContractId>`

Deploy a WASM contract via Deployooor:
- Creates `DeployV1` transaction
- Attaches the WASM binary
- Verifies and applies the deploy block
- Returns the derived `ContractId`

```rust
let dex_wasm = include_bytes!("../../dex/darkfi_dex_contract.wasm").to_vec();
let dex_id = genesis.deploy_contract(dex_wasm, "DEX").await?;
```

#### `verify_and_apply(block) -> Result<()>`

Verify a block using sync module and apply it:
- Calls `sync::verify_block()` with the block's `zkbin_data`
- Calls `sync::apply_block()`
- Appends to fork

```rust
let custom_block = create_custom_block(&mut genesis.fork)?;
genesis.verify_and_apply(&custom_block).await?;
```

#### `block_height() -> Result<u32>`

Get current block height:

```rust
let height = genesis.block_height()?;
```

## Helper Functions

### `generate_native_block(fork, keypair) -> Result<BlockInfo>`

Generate a NativeToken PoW reward block:
- Uses `PoWRewardCallBuilder` to create mint transaction
- Derives ZK proof using `include_bytes!` for zkbin
- Attaches `zkbin_data` for verification

### `generate_deploy_tx(deploy_keypair, wasm_bincode) -> Result<Transaction>`

Generate a Deployooor deploy transaction:
- No ZK proofs (Deployooor has no circuits)
- Uses `DeployParamsV1` with WASM binary

### `generate_deploy_block(fork, deploy_keypair, wasm_bincode) -> Result<BlockInfo>`

Generate a block containing a deploy transaction.

## Test Examples

### `test_genesis` - Basic Chain Setup

```rust
#[test]
fn test_genesis() -> Result<()> {
    smol::block_on(async {
        let config = HarnessConfig { /* ... */ };
        let mut genesis = GenesisHarness::new(config, &ex).await?;
        genesis.generate_genesis_blocks(3).await?;
        tracing::info!("Height: {}", genesis.block_height()?);
        Ok(())
    })
}
```

### `test_native_token_deployoor` - Full Deployment

```rust
#[test]
fn test_native_token_deployoor() -> Result<()> {
    smol::block_on(async {
        let config = HarnessConfig { /* ... */ };
        let mut genesis = GenesisHarness::new(config, &ex).await?;

        // Set up baseline
        genesis.generate_genesis_blocks(2).await?;

        // Deploy MoneyV2 via Deployooor
        let wasm = include_bytes!("../../money_v2/darkfi_money_contract.wasm").to_vec();
        let contract_id = genesis.deploy_contract(wasm, "MoneyV2").await?;

        tracing::info!("Deployed: {:?}", contract_id);
        Ok(())
    })
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    GenesisHarness                            │
├─────────────────────────────────────────────────────────────┤
│  harness: Harness                                          │
│    ├── alice: DarkfiNodePtr  (validator + p2p + registry)   │
│    └── bob: DarkfiNodePtr                                    │
│                                                             │
│  fork: Fork                                                 │
│    ├── overlay: BlockchainOverlay                           │
│    ├── diffs: Vec<StateDiff>                               │
│    └── module: ConsensusModule                             │
│                                                             │
│  keypair: Keypair  (default, for signing)                  │
│  deployed_contracts: Vec<ContractId>                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  Native Contracts (Pre-deployed)             │
├─────────────────────────────────────────────────────────────┤
│  NATIVE_TOKEN_CONTRACT_ID  - PoWRewardV1, FeeV1, BurnV1    │
│  DEPLOYOOOR_CONTRACT_ID    - DeployV1, LockV1              │
│  MONEY_V2_CONTRACT_ID      - (deprecated)                  │
└─────────────────────────────────────────────────────────────┘
```

## Flow: Testing a New WASM Contract

```
1. GenesisHarness::new()
   └── Creates Harness with alice/bob nodes
   └── Deploys native contracts to overlay

2. genesis.generate_genesis_blocks(n)
   └── For each block:
       ├── PoWRewardCallBuilder creates mint tx
       ├── generate_native_block() creates BlockInfo
       ├── verify_block() validates ZK proofs
       └── apply_block() updates state

3. genesis.deploy_contract(wasm, name)
   └── generate_deploy_tx() creates DeployV1 tx
   └── generate_deploy_block() wraps in BlockInfo
   └── verify_block() (empty zkbin_data for Deployooor)
   └── apply_block() deploys WASM
   └── Returns ContractId derived from deploy pubkey

4. Your test uses the deployed contract
   └── TransactionBuilder with ContractCallLeaf
   └── Custom proofs if needed
   └── verify_and_apply() for each block
```

## Integration with Sync Module

GenesisHarness uses the stateless sync verification:

```rust
// verify_block uses zkbin_data from block
verify_block(&block, &previous, &block.zkbin_data).await?;

// zkbin_data format: (contract_id, zkas_ns, zkbin_bytes, instances)
block.zkbin_data = vec![(
    *NATIVE_TOKEN_CONTRACT_ID,
    "Mint_V1".to_string(),
    zkbin_bytes,
    public_inputs,
)];
```

This means **no sled VK lookup** - VKs are derived from `zkbin_bytes` at verification time.

## Testing WASM Contracts After Genesis

Once you have `genesis` setup:

```rust
async fn test_stablecoin() -> Result<()> {
    let mut genesis = GenesisHarness::new(config, &ex).await?;
    genesis.generate_genesis_blocks(3).await?;

    // Deploy Stablecoin via Deployooor
    let stablecoin_wasm = include_bytes!("../../stablecoin/darkfi_stablecoin_contract.wasm").to_vec();
    let stablecoin_id = genesis.deploy_contract(stablecoin_wasm, "Stablecoin").await?;

    // Now use Stablecoin client API to interact with it
    let open_result = stablecoin::client::open_position_v1::OpenPositionCallBuilder {
        // ... params
    }.build()?;

    // Create transaction with Stablecoin call
    let mut data = vec![StablecoinFunction::OpenPositionV1 as u8];
    open_result.params.encode(&mut data)?;
    let call = ContractCall { contract_id: stablecoin_id, data };

    // ... build and verify transaction
    Ok(())
}
```

## File Locations

| File | Purpose |
|------|---------|
| `bin/darkfid/src/tests/genesis.rs` | GenesisHarness implementation |
| `bin/darkfid/src/tests/mod.rs` | Module exports (contains `pub mod genesis`) |
| `bin/darkfid/src/tests/harness.rs` | Underlying Harness struct |
| `src/validator/sync/verify.rs` | `verify_block` function |
| `src/validator/sync/apply.rs` | `apply_block` function |

## Related Documentation

- [Sync Module](./sync.md) - Stateless block verification
- [ZK Verification](./zk_verification.md) - Pure ZK proof verification
- [Deployooor Contract](../spec/contract/deploy/deploy.md) - WASM deployment
- [NativeToken Contract](./money_v3_migration.md) - Consensus-first token

## See Also

- [Test Harness Guide](./test_harness_guide.md) - Full testing overview
- [DarkFi Contract Pipeline](./darkfid_contract_pipeline.md) - How contracts are deployed
- [DEX Documentation](./dex.md) - Example WASM contract