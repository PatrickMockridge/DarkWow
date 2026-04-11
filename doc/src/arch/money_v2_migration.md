# Money V2 Migration: Block Rewards and Genesis Block Changes

## Executive Summary

This document describes a critical architectural change in DarkFi: **block rewards are now paid through Money V2 instead of Money V1**, and Money V2 is deployed as a **native contract at genesis** with a hardcoded `ContractId`.

This change constitutes a **hard fork** of the DarkFi protocol. Nodes running the old software will reject the new genesis block because:
1. Money V2 was not a native contract in the original protocol
2. Block rewards used `MoneyFunction::PoWRewardV1` instead of `PoWRewardV2`

## Background: The Money V1 / DAO Coupling Problem

### Original Architecture

In the original DarkFi design:

| Contract | Type | ContractId | Purpose |
|----------|------|------------|---------|
| Money | Native | Hardcoded `MONEY_CONTRACT_ID` (index 0) | DARK token, PoWRewardV1 |
| DAO | Native | Hardcoded `DAO_CONTRACT_ID` (index 1) | Governance |
| Deployooor | Native | Hardcoded `DEPLOYOOOR_CONTRACT_ID` (index 2) | Deploy WASM contracts |
| MoneyV2 | WASM | Derived at runtime | Next-gen token (initially deployed via Deployooor) |

### The Coupling Issue

The DAO contract has a critical dependency on the Money contract:

```rust
// In contract_graph.rs
Contract::Dao => vec![Contract::Money], // DAO uses Money for governance token
```

This dependency exists because:

1. **DAO Governance Uses DARK Token**: DAO proposals can include `AuthMoneyTransfer` calls that move DARK tokens held by the DAO treasury. This requires the DAO to call into the Money contract.

2. **Token Minting**: DAOs can mint new governance tokens via `DaoFunction::Mint`, which creates tokens tracked in the Money contract's state.

3. **Voting Weight**: DAO voting weight is tied to DARK token holdings managed by the Money contract.

### Why This Coupling Was Problematic

The Money V1 contract had several issues that affected the DAO:

1. **State Corruption Risk**: If Money V1's Merkle tree or nullifier SMT became corrupted, the DAO would fail catastrophically since it couldn't process governance transactions.

2. **No Clear Separation**: Governance (DAO) and monetary policy (Money) were tightly coupled through the same contract state.

3. **Upgrade Path Blocked**: Upgrading Money meant potentially breaking DAO functionality since they share state.

## Money V2: The Solution

### What is Money V2?

Money V2 is a separate contract that implements the same token functionality as Money V1:

| Feature | Money V1 | Money V2 |
|---------|----------|----------|
| DARK Token | Yes | Yes |
| PoWReward | `PoWRewardV1` | `PoWRewardV2` |
| Fee handling | `FeeV1` | `FeeV2` |
| Transfer | `TransferV1` | `TransferV2` |
| Deployment | Native at genesis | **Native at genesis (new)** |

### Key Differences

1. **Separate State**: Money V2 has its own Merkle tree, nullifiers, and coins database. It does NOT share state with Money V1.

2. **New ZK Circuits**: Money V2 uses `Mint_V2`, `Fee_V2`, `Burn_V2` circuit namespaces instead of `Mint_V1`, `Fee_V1`, etc.

3. **PoWRewardV2**: The new block reward function uses `MONEY_CONTRACT_ZKAS_MINT_NS_V2` for ZK proofs.

## The Migration: Block Rewards Switch

### Before (Original Protocol)

```
Genesis Block
    │
    ├── Money Contract (MONEY_CONTRACT_ID, index 0)
    │       └── PoWRewardV1 ← block rewards
    │
    ├── DAO Contract (DAO_CONTRACT_ID, index 1)
    │
    └── Deployooor (DEPLOYOOOR_CONTRACT_ID, index 2)
            │
            └── MoneyV2 (deployed via WASM, NOT at genesis)
```

Block reward transaction:
```rust
let call = ContractCall {
    contract_id: *MONEY_CONTRACT_ID,  // Money V1
    data: vec![MoneyFunction::PoWRewardV1 as u8, ...],
};
```

### After (This Fork)

```
Genesis Block
    │
    ├── Money Contract (MONEY_CONTRACT_ID, index 0)
    │       └── (still exists for backward compatibility)
    │
    ├── DAO Contract (DAO_CONTRACT_ID, index 1)
    │
    ├── Deployooor (DEPLOYOOOR_CONTRACT_ID, index 2)
    │
    └── MoneyV2 Contract (MONEY_V2_CONTRACT_ID, index 3) ← NEW
            └── PoWRewardV2 ← block rewards NOW use this
```

Block reward transaction:
```rust
let call = ContractCall {
    contract_id: *MONEY_V2_CONTRACT_ID,  // Money V2
    data: vec![MoneyFunction::PoWRewardV2 as u8, ...],
};
```

## Implementation Details

### ContractId Derivation

ContractIds are derived using poseidon hash with a prefix and index:

```rust
// In darkfi_sdk::crypto::contract_id
pub static ref CONTRACT_ID_PREFIX: pallas::Base = pallas::Base::from(42);

pub static ref MONEY_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(0)]));

pub static ref DAO_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(1)]));

pub static ref DEPLOYOOOR_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(2)]));

// NEW: Money V2 at index 3
pub static ref MONEY_V2_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(3)]));
```

### Native Contracts at Genesis

The `deploy_native_contracts()` function now deploys 4 native contracts:

```rust
let native_contracts = vec![
    ("Money Contract", *MONEY_CONTRACT_ID, include_bytes!("../contract/money/darkfi_money_contract.wasm").to_vec(), vec![]),
    ("DAO Contract", *DAO_CONTRACT_ID, include_bytes!("../contract/dao/darkfi_dao_contract.wasm").to_vec(), vec![]),
    ("Deployooor Contract", *DEPLOYOOOR_CONTRACT_ID, include_bytes!("../contract/deployooor/darkfi_deployooor_contract.wasm").to_vec(), vec![]),
    ("Money V2 Contract", *MONEY_V2_CONTRACT_ID, include_bytes!("../contract/money_v2/darkfi_money_contract.wasm").to_vec(), vec![]),  // NEW
];
```

### VK Injection

Verification Keys are injected at genesis for all native contracts:

```rust
// In vks.rs::inject()
pub static ref NATIVE_CONTRACT_ZKAS_DB_NAMES: [[u8; 32]; 4] = [
    MONEY_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
    DAO_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
    DEPLOYOOOR_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
    MONEY_V2_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),  // NEW
];
```

## Mining Rewards

### How Mining Rewards Work

1. **Miner RPC** (`miner.mine`): Called to mine a block locally (localnet only)

2. **PoWRewardV2 Transaction**: Creates a transaction that mints new DARK tokens as block reward

3. **ZK Proof**: Generated using `Mint_V2` circuit with `PoWRewardCallBuilder`

```rust
// Simplified miner.mine flow
let debris = PoWRewardCallBuilder {
    signature_keypair: block_signing_keypair,
    block_height,
    fees: 0,
    recipient: None,  // Reward goes to block signing key
    spend_hook: None,
    user_data: None,
    mint_zkbin: zkbin,  // MONEY_CONTRACT_ZKAS_MINT_NS_V2
    mint_pk: pk,
}
.build()
.unwrap();

// Transaction uses MoneyV2 contract
let call = ContractCall {
    contract_id: *MONEY_V2_CONTRACT_ID,  // NOT MONEY_CONTRACT_ID
    data: vec![MoneyFunction::PoWRewardV2 as u8, ...],
};
```

### Reward Value

Block reward value is calculated from the block height:

```rust
let expected_reward = expected_reward(block_height) + paid_fee;
```

The `expected_reward` function returns a predefined schedule that decreases over time (deflationary emission).

## Why This Is a Hard Fork

### Technical Reasons

1. **Genesis Block Hash Change**: The new genesis block includes Money V2's state and VKs, producing a completely different state root hash.

2. **ContractId Space**: Money V2 now occupies `ContractId` index 3 at genesis. Old nodes don't know about this contract.

3. **Transaction Validation**: Old nodes would reject `PoWRewardV2` transactions because:
   - They don't recognize `MONEY_V2_CONTRACT_ID`
   - They don't have `Mint_V2` circuit VKs

### Consensus Rules Changed

| Rule | Old | New |
|------|-----|-----|
| Block reward contract | Money V1 | Money V2 |
| ZK circuit namespace | `Mint_V1` | `Mint_V2` |
| Native contracts at genesis | 3 | 4 |

### Upstream Rejection

This fork is **incompatible with upstream DarkFi** because:

1. Upstream uses Money V1 for block rewards
2. Upstream does not have Money V2 as a native contract
3. Upstream's genesis block configuration is different

## Backward Compatibility

### Money V1 Still Exists

Money V1 is **not removed** and still functions for:

- Existing DARK tokens in Money V1 state
- DAO contract dependencies (DAO still uses Money V1 for governance tokens)
- Any legacy transactions that reference Money V1

### Dual Token State

The blockchain now has **two separate token states**:

```
Blockchain State
├── Money V1 State
│   ├── Coins Merkle Tree
│   ├── Nullifiers SMT
│   └── Fees Accumulator
│
└── Money V2 State
    ├── Coins Merkle Tree
    ├── Nullifiers SMT
    └── Fees Accumulator
```

## Migration Path for Full Nodes

1. **Update software** to this fork version
2. **Sync from genesis** (no bootstrap from old nodes possible)
3. **Verify genesis block** matches the hardcoded values

```rust
// Genesis verification would check:
// - MONEY_V2_CONTRACT_ID exists at index 3
// - VKs for Mint_V2 are injected
// - State root matches expected value
```

## Testing

### Localnet

The test harness supports Money V2:

```rust
// In test-harness/src/money_pow_reward.rs
let debris = PoWRewardCallBuilder {
    signature_keypair: block_signing_keypair,
    block_height,
    // ... uses MoneyV2 types
}
.build()
.unwrap();
```

### Build Verification

```bash
# Build the node
cargo build --package darkfid

# Build the test harness
cargo build --package darkfi-contract-test-harness
```

## Related Documentation

- [Contract Deployment Pipeline](./darkfid_contract_pipeline.md) - How native contracts are deployed
- [Test Harness Guide](./test_harness_guide.md) - Testing architecture
- [Contract Graph](./contract_graph.md) - Contract dependencies

## Changelog

- **2026-04-10**: Initial implementation
  - Added `MONEY_V2_CONTRACT_ID` to SDK
  - Added Money V2 to `deploy_native_contracts()`
  - Updated miner RPC to use `PoWRewardV2`
  - Updated test harness for Money V2 support
  - Removed deprecated faucet module
