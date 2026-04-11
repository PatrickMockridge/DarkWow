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

### The Fundamental Problem: ACL-Based Authorization

The upstream DarkFi architecture used an **Access Control List (ACL) model** for governance, which is mathematically unsound from a privacy-first perspective.

From [The Zero-Knowledge Authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization):

> *"I_min(p; grant) = log_2 |{ p' ∈ P : (p', r, s) ∈ L }|"*
>
> When authorization succeeds A(p,r,s) = 1, observers learn that **p is in the authorized set**. The anonymity set is bounded by the size of that set, not the total principals.

**This is the ACL Privacy Gap**: Traditional authorization reveals identity upon successful access.

### Why DAO v1 Was Mathematically Unsound

DAO v1 used **token-holder voting** - an ACL model where:
- Your **public key** = your **identity**
- Your **token balance** = your **voting weight**
- **Merkle proofs** = ACL membership proofs that reveal your holdings

```
Upstream DAO v1 Authorization (ACL Model):
┌─────────────────────────────────────────────────────────────┐
│  Vote(pubkey, proposal, amount)                            │
│                                                             │
│  1. Merkle proof: "pubkey ∈ MoneyMerkleTree"              │
│     → LEAKS: Your exact token balance to all observers      │
│                                                             │
│  2. Check: amount > proposal.quorum_threshold              │
│     → LEAKS: Your voting weight is now public              │
│                                                             │
│  Result: When your vote succeeds, observers learn:         │
│  - Your public key (who you are)                          │
│  - Your DARK token balance (how much power you have)      │
└─────────────────────────────────────────────────────────────┘
```

**The Math Doesn't Work for Privacy:**

| Property | ACL/DAO v1 Model | ZK Predicate Model |
|----------|-------------------|-------------------|
| Authorization result | Reveals `p ∈ L` | Reveals only "boolean true" |
| Information leakage | Identity + set size | Nothing |
| Vote weight | Public via Merkle proof | Private via ZK commitment |
| Coin ownership | Linked to public key | Hidden in Pedersen commitment |

### Problems with Money V1

1. **Merkle Tree Membership = Identity Linkage**: Money V1's coin model required proving `pubkey ∈ MerkleTree` to spend coins. This **permanently links your public key to your balance** on-chain.

2. **State Corruption Risk**: If Money V1's Merkle tree or nullifier SMT became corrupted, dependent contracts would fail catastrophically.

3. **No Clear Separation**: Governance and monetary policy were tightly coupled through the same contract state.

4. **The ACL-ZK Gap**: Money V1 couldn't escape its ACL roots - every transaction revealed coin ownership via Merkle proofs.

### Problems with DAO v1

1. **Inherited ACL Privacy Leakage**: DAO v1's governance depended on Money V1's Merkle proofs, meaning **every governance action revealed the participant's token balance**.

2. **Complex Governance**: Required multiple ZK circuits for propose/vote/exec lifecycle, but all circuits operated on public ACL data.

3. **Tightly Coupled**: Had hard dependency on Money V1 for governance tokens - couldn't separate governance from the token's ACL model.

4. **Inflexible**: Single governance mode, couldn't adapt to different DAO structures.

### The ZK Predicate Solution (Our Fork)

Our architecture uses **ZK predicate evaluation** instead of ACL membership:

```
Our DAO Escrow Authorization (ZK Predicate Model):
┌─────────────────────────────────────────────────────────────┐
│  Prove: "∃ w such that P(w) = true"                      │
│                                                             │
│  P(w) = "w is a valid deposit in DAO Escrow"            │
│                                                             │
│  Proof reveals: ONLY the boolean result (true/false)       │
│  Proof hides: depositor identity, deposit amount, witness │
└─────────────────────────────────────────────────────────────┘
```

**Key Differences:**
- **No Merkle proofs** for authorization - use Pedersen commitments instead
- **No public key → balance linkage** - balances are hidden in commitments
- **ZK circuits prove predicates**, not set membership
- **Vote weight is private** - only proven to exceed threshold, amount hidden

See: [The Zero-Knowledge Authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization) for the mathematical foundation.

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
