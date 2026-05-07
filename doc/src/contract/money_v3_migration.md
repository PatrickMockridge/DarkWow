# Money V3 Migration: Privacy-First DeFi Token Contract

## Executive Summary

This document describes the architecture of DarkWow on this fork:

- **Money V1/V2 are not used on this fork** — Replaced by Money V3 (DeFi tokens) and NativeToken (consensus operations)
- **DAO v1 is not used on this fork** — Replaced by [dao_escrow](./dao_escrow.md)
- **NativeToken is the native DRKW token contract** — Handles fees and consensus
- **Money V3 is a WASM contract** — DeFi tokens (ERC-20 style) with full privacy
- **Money V3 uses Poseidon-only design** — Avoids EC operations implicated in heap corruption

This change constitutes a **hard fork** of the DarkWow protocol. Nodes running the old software will reject the new genesis block because:

1. Money V1/V2 were not removed in the original protocol
2. DAO v1 was not removed in the original protocol
3. Block rewards used `MoneyFunction::PoWRewardV1` instead of NativeToken's `PoWRewardV1`
4. Original protocol used EC-based signatures, we use Poseidon-only Schnorr-style

## Current Architecture (This Fork)

### Contract Status

| Contract | Type | Status | Purpose |
|----------|------|--------|---------|
| NativeToken | Native | **ACTIVE** | DRKW token, fees, PoW rewards |
| Money V3 | WASM | **ACTIVE** | DeFi tokens (stablecoins, wrapped assets) |
| DAO Escrow | WASM | **ACTIVE** | Governance with endowment/treasury modes |
| Deployooor | Native | **ACTIVE** | Deploy WASM contracts |

### Genesis Block (This Fork)

```
Genesis Block
    │
    ├── NativeToken Contract (NATIVE_TOKEN_CONTRACT_ID)
    │       ├── PoWRewardV1 ← block rewards use this
    │       ├── FeeV1 ← network fee payment
    │       ├── MintV1 ← DRKW token minting
    │       └── BurnV1 ← DRKW token burning
    │
    └── Deployooor (DEPLOYOOOR_CONTRACT_ID)
            └── Deploy WASM contracts (Money V3, DAO Escrow, etc.)
```

Note: Money V3 is a **WASM contract** deployed via Deployooor, not a native contract.

### Before (Original DarkWow)

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

## Why Money V1/V2 Were Replaced with Money V3

### The Fundamental Problem: EC-Based Circuits and Heap Bugs

Money V1 and V2 used **Elliptic Curve (EC) operations** in their ZK circuits:

| Circuit | EC Operations | Heap Bug Risk |
|---------|---------------|---------------|
| Money V1 | 4 | YES - Pedersen commitment handling |
| Money V2 | 4 | YES - Same EC issues |
| Money V3 | **0** | **NO** - Poseidon-only |

**The Heap Bug Problem**: EC operations require careful memory management in halo2 circuits. Bugs in point compression/decompression can lead to:
- Invalid proof acceptance
- Coin duplication via double-spend
- Total state corruption

### Money V3: Poseidon-Only Design

Money V3 eliminates EC operations entirely:

```
Money V3 Circuit Design:
┌─────────────────────────────────────────────────────────────┐
│  All operations use Poseidon hash (no EC)                  │
│                                                             │
│  Coin = poseidon_hash(pub, value, token_id, ...)         │
│  Nullifier = poseidon_hash(secret, coin)                   │
│  Value Commitment = poseidon_hash(value, blind)            │
│  Token ID = poseidon_hash(auth_parent, user_data, blind)  │
│  Public Key = poseidon_hash(secret)  ← Schnorr-style       │
│                                                             │
│  Signature = hash(message || pubkey) * secret              │
│  Verification: check signature against poseidon_hash(secret)│
└─────────────────────────────────────────────────────────────┘
```

**Benefits:**
- **Zero heap bugs** - No EC point handling
- **Simpler circuits** - Faster proving
- **Full privacy** - Token IDs are hidden commitments

### Token ID Privacy: 100% Fungibility

Money V2 vs Money V3 token ID comparison:

| Aspect | Money V2 | Money V3 |
|--------|----------|----------|
| Token ID visibility | Revealed plaintext | Hidden commitment |
| Token traceability | Traceable via token_id | Untraceable |
| Fungibility | Partial | **100%** |
| EC Operations | 4 | 0 |

```
Money V2 (traceable):
  token_id = plaintext_identifier  ← Can be traced on-chain

Money V3 (private):
  token_id = poseidon_hash(auth_parent, user_data, blind)  ← Hidden commitment
```

## Why DAO v1 Was Replaced

### The ACL Privacy Gap

DAO v1 used **token-holder voting** - an ACL model where:
- Your **public key** = your **identity**
- Your **token balance** = your **voting weight**
- **Merkle proofs** = ACL membership proofs that reveal your holdings

```
DAO v1 Authorization (ACL Model):
┌─────────────────────────────────────────────────────────────┐
│  Vote(pubkey, proposal, amount)                           │
│                                                              │
│  1. Merkle proof: "pubkey ∈ MoneyMerkleTree"             │
│     → LEAKS: Your exact token balance to all observers     │
│                                                              │
│  2. Check: amount > proposal.quorum_threshold             │
│     → LEAKS: Your voting weight is now public             │
└─────────────────────────────────────────────────────────────┘
```

**The ACL-ZK Gap**: Traditional authorization reveals identity upon successful access.

### Solution: DAO Escrow with ZK Predicates

Our architecture uses **ZK predicate evaluation** instead of ACL membership:

```
DAO Escrow Authorization (ZK Predicate Model):
┌─────────────────────────────────────────────────────────────┐
│  Prove: "∃ w such that P(w) = true"                       │
│                                                              │
│  P(w) = "w is a valid deposit in DAO Escrow"             │
│                                                              │
│  Proof reveals: ONLY the boolean result (true/false)        │
│  Proof hides: depositor identity, deposit amount, witness  │
└─────────────────────────────────────────────────────────────┘
```

## NativeToken vs Money V3 Separation

### Design Philosophy: CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD

| Priority | NativeToken | Money V3 |
|----------|-------------|----------|
| 1. Consensus | **PoWRewardV1** - Block rewards | N/A |
| 2. Network Fees | **FeeV1** - Deterministic fee payment | N/A |
| 3. Privacy | **MintV1/BurnV1** - Basic token operations | **Full privacy** |

### NativeToken (DRKW - Native Token)

Purpose: Consensus and fees
- Block rewards (PoWRewardV1)
- Network fee payment (FeeV1)
- Basic DRKW token operations (MintV1, BurnV1)

**Key Properties:**
- Token ID = 0 (DRKW is the native token)
- No token registry needed
- Simple, hardened circuits

### Money V3 (DeFi Tokens)

Purpose: Privacy-first DeFi tokens
- Token creation (TokenMintV1)
- Authorized minting (AuthTokenMintV1)
- Token minting (MintV1)
- Token burning (BurnV1)
- Private transfers (TransferV1)

**Key Properties:**
- Token IDs are hidden commitments
- 100% fungibility
- spend_hook for cross-contract calls

## Implementation Details

### ContractId Derivation

ContractIds are derived using poseidon hash with a prefix and index:

```rust
// In dwow_sdk::crypto::contract_id
pub static ref CONTRACT_ID_PREFIX: pallas::Base = pallas::Base::from(42);

pub static ref NATIVE_TOKEN_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(0)]));

pub static ref DEPLOYOOOR_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(1)]));
```

Note: Money V3 ContractId is set at deployment time (user-deployed WASM contract).

### Native Contracts at Genesis

The `deploy_native_contracts()` function deploys 2 native contracts:

```rust
let native_contracts = vec![
    ("NativeToken Contract", *NATIVE_TOKEN_CONTRACT_ID, include_bytes!("../contract/native_token/darkfi_native_token_contract.wasm").to_vec(), vec![]),
    ("Deployooor Contract", *DEPLOYOOOR_CONTRACT_ID, include_bytes!("../contract/deployooor/dwow_deployooor_contract.wasm").to_vec(), vec![]),
];
```

### VK Injection

Verification Keys are injected at genesis for all native contracts:

```rust
// In vks.rs::inject()
pub static ref NATIVE_CONTRACT_ZKAS_DB_NAMES: [[u8; 32]; 2] = [
    NATIVE_TOKEN_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
    DEPLOYOOOR_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
];
```

## Mining Rewards

### How Mining Rewards Work

1. **Miner RPC** (`miner.mine`): Called to mine a block locally

2. **PoWRewardV1 Transaction**: Creates a transaction that mints new DRKW tokens as block reward

3. **ZK Proof**: Generated using `Mint_V1` circuit with `PoWRewardCallBuilder`

```rust
// Simplified miner.mine flow
let debris = PoWRewardCallBuilder {
    signature_keypair: block_signing_keypair,
    block_height,
    fees: 0,
    recipient: None,  // Reward goes to block signing key
    spend_hook: None,
    user_data: None,
    mint_zkbin: zkbin,  // NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1
    mint_pk: pk,
}
.build()
.unwrap();

// Transaction uses NativeToken contract
let call = ContractCall {
    contract_id: *NATIVE_TOKEN_CONTRACT_ID,
    data: vec![NativeTokenFunction::PoWRewardV1 as u8, ...],
};
```

### Reward Value

Block reward value is calculated from the block height:

```rust
let expected_reward = expected_reward(block_height) + paid_fee;
```

The `expected_reward` function returns a predefined schedule that decreases over time (deflationary emission).

## dww Wallet Integration

The `dww` command-line wallet supports Money V3 with full functionality:

| Feature | Status | Implementation |
|---------|--------|----------------|
| Coin Scanning | ✅ | `apply_tx_money_data()` in rpc.rs |
| Coin Storage | ✅ | `coins` and `coin_merkle_proofs` tables |
| Transfer | ✅ | `transfer()` with FeeV1 attachment |
| Token Creation | ✅ | `create_token()` via TokenMintV1 |
| Token Minting | ✅ | `mint_tokens()` via AuthTokenMintV1 + MintV1 |

### Transfer Flow

```
1. Select unspent coin with sufficient value
2. Retrieve Merkle proof from wallet database
3. Decode secret key from wallet
4. Build TransferCallBuilder with:
   - Input: coin data + Merkle proof
   - Output: recipient + change
5. Generate ZK proofs (Burn_V1 + Mint_V1)
6. Select DRKW coin for fee payment
7. Build NativeToken::FeeV1 for fee attachment
8. Combine into final transaction using TransactionBuilder
```

## Why This Is a Hard Fork

### Technical Reasons

1. **Genesis Block Hash Change**: The new genesis block includes NativeToken's state and VKs, producing a completely different state root hash.

2. **ContractId Space**: NativeToken now occupies `ContractId` index 0 at genesis. Old nodes don't know about this contract.

3. **Transaction Validation**: Old nodes would reject `PoWRewardV1` transactions because:
   - They don't recognize `NATIVE_TOKEN_CONTRACT_ID`
   - They don't have `Mint_V1` circuit VKs

4. **Circuit Design**: Original DarkWow used EC-based circuits (Pedersen commitments). We use Poseidon-only circuits.

### Consensus Rules Changed

| Rule | Old | New |
|------|-----|-----|
| Money contract | Money V1/V2 | **Money V3** (WASM) |
| Native token | N/A | **NativeToken** (native) |
| Governance | DAO v1 | DAO Escrow (WASM) |
| ZK circuits | EC-based | **Poseidon-only** |
| Token IDs | Revealed | **Hidden commitments** |
| EC Operations | 4+ per tx | **0** |

### Upstream Rejection

This fork is **incompatible with upstream DarkWow** because:

1. Upstream uses Money V1/V2 for tokens
2. Upstream does not have NativeToken as a native contract
3. Upstream's genesis block configuration is different
4. Upstream uses EC-based circuits, we use Poseidon-only

## Migration Path for Full Nodes

1. **Update software** to this fork version
2. **Sync from genesis** (no bootstrap from old nodes possible)
3. **Verify genesis block** matches the hardcoded values

```rust
// Genesis verification would check:
// - NATIVE_TOKEN_CONTRACT_ID exists at index 0
// - VKs for Mint_V1 are injected
// - State root matches expected value
```

## Related Documentation

- [Contract Deployment Pipeline](./dwowd_contract_pipeline.md) - How native contracts are deployed
- [Testing Overview](../dev/testing/overview.md) - Four-level testing taxonomy
- [Contract Graph](./contract_graph.md) - Contract dependencies
- [Money V3 Contract](../dev/contracts/money_v3.md) - Detailed Money V3 specification
- [NativeToken Contract](../dev/contracts/native_token.md) - NativeToken specification

## Changelog

- **2026-04-13**: Money V3 migration complete
  - Money V1/V2 removed, Money V3 added (WASM)
  - NativeToken contract added (native, for DRKW)
  - All ZK circuits converted to Poseidon-only
  - Token IDs now hidden commitments (100% fungibility)
  - dww wallet updated with full Money V3 support
  - Transfer, token creation, token minting all functional
