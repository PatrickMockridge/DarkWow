# Contract Deployment Pipeline

## Overview

This document describes how `dwowd` initializes on the linear-master branch (Uncle Merkle consensus). Understanding this pipeline is helpful for debugging contract testing and deployment issues.

The startup sequence has two phases: genesis bootstrap (state initialization) and runtime consensus (block production and validation via Uncle Merkle).

## Architecture

`dwowd` is the DarkWow blockchain node. It handles:
- Consensus (Uncle Merkle PoW)
- Transaction validation
- Smart contract execution (ZK proofs + WASM state transitions)
- P2P networking

## Contract Types

### Native Contracts (at Genesis)

Only two contracts are deployed at genesis:

| Contract | Crate | ContractID | Deployed By |
|----------|-------|------------|-------------|
| Deployooor | `dwow_deployooor_contract` | `DEPLOYOOOR_CONTRACT_ID` | `dwowd` at startup |
| NativeToken | `dwow_native_token_contract` | `NATIVE_TOKEN_CONTRACT_ID` | `dwowd` at startup |

**Characteristics:**
- ContractID known at compile time (static constants)
- WASM binary embedded via `include_bytes!()` at compile time
- Deployed during genesis bootstrap via `deploy_native_contracts()`
- NativeToken handles all consensus-critical operations (block rewards, fees, minting, burning, transfers)

### WASM Contracts (Post-Genesis)

All other contracts are deployed post-genesis via the Deployooor contract:

| Contract | Crate | ContractID | Deployed By |
|----------|-------|------------|-------------|
| Money V3 | `dwow_money_v3_contract` | Derived from deployer's pubkey | Deployooor |
| DAO Escrow | `dwow_dao_escrow_contract` | Derived from deployer's pubkey | Deployooor |
| DEX | `dwow_dex_contract` | Derived from deployer's pubkey | Deployooor |
| Stablecoin | `dwow_stablecoin_contract` | Derived from deployer's pubkey | Deployooor |
| *(all others)* | `dwow_*_contract` | Derived from deployer's pubkey | Deployooor |

**Characteristics:**
- ContractID unknown until deployment (derived at runtime from deployer's public key)
- WASM binary deployed via transaction to Deployooor
- Deployable by any user — no special permissions required

---

## Genesis Bootstrap Sequence

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                         DWOWD GENESIS BOOTSTRAP                               │
└──────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 1: Create in-memory sled database                                        │
│         - Temporary sled instance for genesis state computation               │
│         - Used only during bootstrap, not for runtime state                   │
└──────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 2: Create BlockchainOverlay (genesis only)                               │
│         - Overlay on the in-memory sled for atomic genesis setup              │
│         - Allows computing the state root from the diff of deployed contracts │
└──────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 3: deploy_native_contracts()                                             │
│                                                                               │
│   Contracts hardcoded in dwowd (src/validator/utils.rs):                     │
│   ┌────────────────────────────────────────────────────────────────────┐     │
│   │  Name              │ ContractID              │ WASM Binary         │     │
│   ├────────────────────────────────────────────────────────────────────┤     │
│   │  Deployooor        │ DEPLOYOOOR_CONTRACT_ID  │ include_bytes!(...)  │     │
│   │  NativeToken       │ NATIVE_TOKEN_CONTRACT_ID│ include_bytes!(...)  │     │
│   └────────────────────────────────────────────────────────────────────┘     │
│                                                                               │
│   For each: Runtime::deploy() → WASM initialized with ContractID              │
│   Available immediately after startup                                         │
└──────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 4: Compute genesis state_root                                            │
│         - overlay.diff() → contract state changes                             │
│         - update_state_monotree(&diff) → state root hash                      │
│         - Stored in genesis block header                                      │
└──────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 5: Return ValidatorConfig                                                │
│         - Contains genesis block, PoW target, confirmation threshold          │
│         - Used identically by daemon and test harness                         │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Runtime Consensus

After genesis bootstrap, the node switches to linear Uncle Merkle consensus:

- **Block production**: Miners submit solved headers via stratum → `dwowd` assembles blocks
- **Block application**: `LinearBlockchain::apply_block()` validates PoW, processes transactions
- **State storage**: Plain sled trees (no overlay/diff rollback — state changes are final)
- **Fork handling**: Uncle blocks earn pin rewards rather than being orphaned
- **Sync**: Full P2P block sync via `LinearSyncHandler` (headers backward, blocks forward)

---

## NativeToken Contract Functions

The NativeToken contract handles all consensus-critical token operations:

| ID | Function | Purpose |
|----|----------|---------|
| 0x00 | FeeV1 | Pay network fees |
| 0x01 | MintV1 | Create new coins |
| 0x02 | BurnV1 | Destroy coins with nullifier |
| 0x03 | TransferV1 | Private transfers |
| 0x04 | SpendV1 | Spend with change output |
| 0x05 | PoWRewardV1 | Block rewards for miners |

---

## Key Differences from Upstream (Overlay-DAG)

The upstream overlay-DAG architecture deployed 4 native contracts (Money, DAO, Deployooor, MoneyV2) and used `BlockchainOverlay` for runtime consensus with speculative state and diff-based rollback.

This fork:
- Deploys only 2 native contracts (Deployooor + NativeToken) — governance and DeFi tokens are WASM contracts deployed post-genesis
- Uses `BlockchainOverlay` only during genesis bootstrap, not for runtime consensus
- Uses Uncle Merkle consensus for block production — deterministic, no speculative state
- Stores state in plain sled trees — every state change is final

---

## Related

- [Genesis Harness](./genesis_harness.md) — Test harness initialization
- [Uncle Merkle Consensus](./consensus/uncle_merkle.md) — Runtime consensus mechanism
- [Test Harness Guide](./test_harness_guide.md) — Writing contract integration tests
- [NativeToken Contract](../contract/native_token.md) — Contract overview
