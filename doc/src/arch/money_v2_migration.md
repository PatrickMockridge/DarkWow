# Money V2 Migration: Block Rewards and Genesis Block Changes

## Executive Summary

This document describes the architecture of DarkFi on this fork:

- **Money V1 is DEPRECATED and REMOVED** - Only Money V2 exists
- **DAO v1 is DEPRECATED and REMOVED** - Use `dao_escrow` contract instead
- **Money V2 is the ONLY money contract** - deployed as a native contract at genesis
- **Block rewards use Money V2** (`PoWRewardV2`)

This change constitutes a **hard fork** of the DarkFi protocol. Nodes running the old software will reject the new genesis block because:
1. Money V1 was not removed in the original protocol
2. DAO v1 was not removed in the original protocol
3. Block rewards used `MoneyFunction::PoWRewardV1` instead of `PoWRewardV2`

## Current Architecture (This Fork)

### Contract Status

| Contract | Status | Replacement |
|----------|--------|-------------|
| Money (v1) | **REMOVED** | Use Money V2 |
| DAO (v1) | **REMOVED** | Use `dao_escrow` |
| Money V2 | **ACTIVE** | Standard money contract |
| DAO Escrow | **ACTIVE** | Governance with endowment/treasury modes |

### Genesis Block (This Fork)

```
Genesis Block
    │
    ├── MoneyV2 Contract (MONEY_V2_CONTRACT_ID, index 0)
    │       └── PoWRewardV2 ← block rewards use this
    │
    └── Deployooor (DEPLOYOOOR_CONTRACT_ID, index 1)
            └── Deploy WASM contracts (DAO Escrow, and all other contracts)
```

Note: DAO Escrow is a **WASM contract** deployed via Deployooor, not a native contract.

### Before (Original DarkFi)

```
Genesis Block
    │
    ├── Money Contract (MONEY_CONTRACT_ID, index 0)
    │       └── PoWRewardV1 ← original block rewards
    │
    ├── DAO Contract (DAO_CONTRACT_ID, index 1)
    │
    └── Deployooor (DEPLOYOOOR_CONTRACT_ID, index 2)
            │
            └── MoneyV2 (deployed via WASM, NOT at genesis)
```

## Why Money V1 and DAO v1 Were Removed

### Problems with Money V1

1. **State Corruption Risk**: If Money V1's Merkle tree or nullifier SMT became corrupted, dependent contracts would fail catastrophically.

2. **No Clear Separation**: Governance and monetary policy were tightly coupled through the same contract state.

3. **Upgrade Path Blocked**: Upgrading Money meant potentially breaking DAO functionality since they share state.

### Problems with DAO v1

1. **Complex Governance**: Required multiple ZK circuits for propose/vote/exec lifecycle
2. **Tightly Coupled**: Had hard dependency on Money V1 for governance tokens
3. **Inflexible**: Single governance mode, couldn't adapt to different DAO structures

### Solution: Money V2 + DAO Escrow

- **Money V2**: Separate state, self-contained ZK proofs, no coupling issues
- **DAO Escrow**: Flexible modes (Escrow/Treasury/Endowment), uses Money V2 for funds

## Satoshi-Style Governance Model

This fork implements **Satoshi-style voluntary opt-in governance**:

1. **Proof of Work Consensus**: Block rewards are the primary sybil resistance mechanism, identical to Bitcoin/Satoshi's vision

2. **Voluntary Governance Participation**: No mandatory governance tokens. Anyone can participate in any DAO Escrow if they voluntarily deposit funds

3. **Opt-In Only**: Governance rights are attached to deposited funds, not to identity or token holdings. Users choose which DAOs to join

4. **No Pre-Mined Tokens**: No governance token airdrops or强迫. All value is earned through PoW mining or providing liquidity/services

5. **Minimal Attack Surface**: No token-based governance means no token-holder voting attacks (vote buying, whale manipulation, governance token concentration)

This differs fundamentally from:
- **Ethereum-style**: Validator deposits (bonded proof of stake) - mandatory if you want to validate
- **DAO Token-style**: Token-holder voting with airdropped/foundation tokens

Our model: **Mining → Earn DARK → Optionally deposit into DAO Escrow → Participate in governance**

## Implementation Details

### ContractId Derivation

ContractIds are derived using poseidon hash with a prefix and index:

```rust
// In darkfi_sdk::crypto::contract_id
pub static ref CONTRACT_ID_PREFIX: pallas::Base = pallas::Base::from(42);

pub static ref MONEY_V2_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(0)]));

pub static ref DEPLOYOOOR_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(1)]));
```

### Native Contracts at Genesis

The `deploy_native_contracts()` function deploys 2 native contracts:

```rust
let native_contracts = vec![
    ("MoneyV2 Contract", *MONEY_V2_CONTRACT_ID, include_bytes!("../contract/money_v2/darkfi_money_contract.wasm").to_vec(), vec![]),
    ("Deployooor Contract", *DEPLOYOOOR_CONTRACT_ID, include_bytes!("../contract/deployooor/darkfi_deployooor_contract.wasm").to_vec(), vec![]),
];
```

### VK Injection

Verification Keys are injected at genesis for all native contracts:

```rust
// In vks.rs::inject()
pub static ref NATIVE_CONTRACT_ZKAS_DB_NAMES: [[u8; 32]; 2] = [
    MONEY_V2_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
    DEPLOYOOOR_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
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

2. **ContractId Space**: Money V2 now occupies `ContractId` index 0 at genesis. Old nodes don't know about this contract.

3. **Transaction Validation**: Old nodes would reject `PoWRewardV2` transactions because:
   - They don't recognize `MONEY_V2_CONTRACT_ID`
   - They don't have `Mint_V2` circuit VKs

### Consensus Rules Changed

| Rule | Old | New |
|------|-----|-----|
| Money contract | Money V1 | Money V2 |
| Governance contract | DAO v1 | DAO Escrow (WASM) |
| ZK circuit namespace | `Mint_V1` | `Mint_V2` |
| Native contracts at genesis | 3 | 2 |

### Upstream Rejection

This fork is **incompatible with upstream DarkFi** because:

1. Upstream uses Money V1 for block rewards
2. Upstream does not have Money V2 as a native contract
3. Upstream's genesis block configuration is different

## Migration Path for Full Nodes

1. **Update software** to this fork version
2. **Sync from genesis** (no bootstrap from old nodes possible)
3. **Verify genesis block** matches the hardcoded values

```rust
// Genesis verification would check:
// - MONEY_V2_CONTRACT_ID exists at index 0
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
