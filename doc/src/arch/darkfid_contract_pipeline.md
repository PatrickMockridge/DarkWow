# DarkFi Contract Deployment Pipeline

## Overview

This document explains how darkfid initializes, deploys contracts, and how the test harness integrates with the system. Understanding this pipeline is critical for debugging contract testing issues.

## DarkFi Node Architecture

darkfid is the DarkFi blockchain node software. It handles:
- Consensus (Proof-of-Work)
- Transaction validation
- Smart contract execution (ZK proofs + state transitions)
- P2P networking

## Contract Types

DarkFi has two distinct contract types with fundamentally different deployment models:

### Native Contracts

| Contract | Package | ContractID | Deployed By |
|----------|---------|------------|-------------|
| Money | `darkfi_money_contract` | Hardcoded `MONEY_CONTRACT_ID` | darkfid at startup |
| DAO | `darkfi_dao_contract` | Hardcoded `DAO_CONTRACT_ID` | darkfid at startup |
| Deployooor | `darkfi_deployooor_contract` | Hardcoded `DEPLOYOOOR_CONTRACT_ID` | darkfid at startup |

**Characteristics:**
- ContractID known at compile time (static)
- WASM binary embedded via `include_bytes!()` at compile time
- VKs (Verification Keys) injected at genesis during initialization
- No deployment transaction needed - available immediately

### WASM Contracts

| Contract | Package | ContractID | Deployed By |
|----------|---------|------------|-------------|
| MoneyV2 | `darkfi_money_v2_contract` | Derived from deployer's pubkey | Deployooor contract |
| Stablecoin | `darkfi_stablecoin_contract` | Derived from deployer's pubkey | Deployooor contract |
| Identity | `darkfi_identity_contract` | Derived from deployer's pubkey | Deployooor contract |
| DEX | `darkfi_dex_contract` | Derived from deployer's pubkey | Deployooor contract |

**Characteristics:**
- ContractID unknown until deployment (derived at runtime)
- WASM binary deployed via transaction to Deployooor
- VKs CANNOT be pre-injected (contract ID unknown)
- VKs MUST be injected post-deployment (not currently implemented)

---

## Full Pipeline Diagram

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          DARKFID STARTUP SEQUENCE                             │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 1: Load/create sled database                                             │
│         - Blockchain state stored in sled                                     │
│         - Multiple trees per contract (nullifiers, values, metadata, etc.)   │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 2: Load cached PKs and VKs from disk                                     │
│         - pks.bin: ProvingKeys for ZK proof generation                        │
│         - vks.bin: VerificationKeys for ZK proof verification                 │
│         - Hash validated to detect staleness                                   │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 3: Create BlockchainOverlay                                               │
│         - In-memory overlay on top of sled                                     │
│         - Allows atomic state changes                                          │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 4: vks::inject() - Inject VKs into overlay                               │
│                                                                                │
│   For each (bincode, namespace, vk) in vks.bin:                               │
│                                                                                │
│   ┌─ Native contracts (Money, DAO) ─────────────────────────────────────┐    │
│   │  namespace matches MONEY_CONTRACT_ZKAS_*_V1                        │    │
│   │  → Inject VK into money_db_name                                     │    │
│   │                                                                     │    │
│   │  namespace matches DAO_CONTRACT_ZKAS_*                            │    │
│   │  → Inject VK into dao_db_name                                       │    │
│   └─────────────────────────────────────────────────────────────────────┘    │
│                                                                                │
│   ┌─ WASM contracts (Stablecoin, Identity, DEX, MoneyV2, etc.) ───────┐    │
│   │  namespace matches WASM contract constants                           │    │
│   │  → SKIP with debug log: "WASM contract, injected post-deployment"  │    │
│   │                                                                     │    │
│   │  WHY: ContractID unknown until deployment.                          │    │
│   │       Cannot inject VKs without knowing the database key.          │    │
│   └─────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ STEP 5: deploy_native_contracts()                                             │
│                                                                                │
│   Contracts hardcoded in darkfid:                                             │
│   ┌────────────────────────────────────────────────────────────────────┐      │
│   │  Name              │ ContractID            │ WASM Binary          │      │
│   ├────────────────────────────────────────────────────────────────────┤      │
│   │  Money Contract    │ MONEY_CONTRACT_ID     │ include_bytes!(...)   │      │
│   │  DAO Contract      │ DAO_CONTRACT_ID       │ include_bytes!(...)   │      │
│   │  Deployooor       │ DEPLOYOOOR_CONTRACT_ID│ include_bytes!(...)   │      │
│   └────────────────────────────────────────────────────────────────────┘      │
│                                                                                │
│   For each: Runtime::deploy() → WASM initialized with ContractID              │
│   Available immediately after startup                                          │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           NODE READY                                          │
│                                                                                │
│   - Native contracts at hardcoded IDs with VKs injected                       │
│   - WASM contracts NOT deployed (deploy individually via Deployooor)          │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## WASM Contract Deployment (The Missing Piece)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│               WASM CONTRACT DEPLOYMENT VIA DEPLOYOOOR                         │
└──────────────────────────────────────────────────────────────────────────────┘

  User/Test                    TestHarness                  Deployooor              Blockchain
     │                              │                            │                     │
     │  deploy_money_v2()            │                            │                     │
     │──────────────────────────────►│                            │                     │
     │                              │                            │                     │
     │                              │  deploy_contract()         │                     │
     │                              │────────────────────────────►│                     │
     │                              │                            │                     │
     │                              │                    Derive ContractID           │
     │                              │                    from deployer's pubkey        │
     │                              │                            │                     │
     │                              │     tx, deploy_params      │                     │
     │                              │◄───────────────────────────│                     │
     │                              │                            │                     │
     │                              │  execute_deploy_tx()       │                     │
     │                              │────────────────────────────►│                     │
     │                              │                            │                     │
     │                              │                      Runtime.deploy()           │
     │                              │                      ContractID known now      │
     │                              │                            │                     │
     │◄─────────────────────────────│                            │                     │
     │  contract_id                  │                            │                     │
     │                              │                            │                     │
     │         ⚠️ VK INJECTION NEVER HAPPENS!                      │                     │
     │                              │                            │                     │
     │  use money_v2()              │                            │                     │
     │──────────────────────────────►                            │                     │
     │                              │                            │                     │
     │                              │                 generate_proof()              │
     │                              │                 (uses PK from pks.bin)        │
     │                              │                            │                     │
     │                              │                 verify_proof()              │
     │                              │                 (looks up VK in blockchain)  │
     │                              │                            │                     │
     │                              │              💥 VK NOT FOUND - PANIC!          │
     │                              │                            │                     │
```

---

## The Verification Key (VK) System

### How VKs Are Used

1. **ZK Proof Generation** (client-side):
   - Uses ProvingKey (PK) from `pks.bin`
   - Generates proof that satisfies circuit constraints
   - Proof attached to transaction

2. **ZK Proof Verification** (darkfid):
   - Receives transaction with proof
   - Looks up VerificationKey (VK) from contract's database
   - Verifies proof against VK
   - If VK missing → `vm.rs:936` panic (EcGetX type mismatch)

### VK Namespace Matching

Every ZK circuit has a **namespace** (string identifier):

| Contract | Circuit File | Namespace | Used By |
|----------|-------------|-----------|---------|
| MoneyV1 | `fee_v1.zk` | `Fee_V1` | Native Money contract |
| MoneyV1 | `mint_v1.zk` | `Mint_V1` | Native Money contract |
| MoneyV1 | `burn_v1.zk` | `Burn_V1` | Native Money contract |
| MoneyV2 | `fee_v1.zk` | `Fee_V2` | MoneyV2 WASM contract |
| MoneyV2 | `mint_v1.zk` | `Mint_V2` | MoneyV2 WASM contract |

The namespace MUST match between:
- What the `.zk` binary file declares (circuit name)
- What the contract entrypoint references (namespace constant)
- What the VK is stored under in the database

---

## Test Harness Integration

### TestHarness::new() Flow

```rust
pub async fn new(holders: &[Holder], verify_fees: bool) -> Result<Self> {
    // 1. Load cached PKs/VKs
    let (pks, vks) = vks::get_cached_pks_and_vks()?;

    // 2. Create blockchain overlay
    let overlay = BlockchainOverlay::new(&Blockchain::new(&sled_db)?)?;

    // 3. Inject VKs (same as darkfid startup)
    vks::inject(&overlay, &vks)?;

    // 4. Deploy native contracts (Money, DAO, Deployooor)
    deploy_native_contracts(&overlay, POW_TARGET).await?;

    // 5. Create wallets for each holder
    for holder in holders {
        // - Generate keypairs
        // - Create Validator instance
        // - Initialize Merkle trees
    }
}
```

### Why Native Contracts Work in Tests

```
TestHarness::new()
       │
       ├── vks::inject() → Money VKs → money_db_name
       │                      DAO VKs → dao_db_name
       │
       └── deploy_native_contracts() → Money, DAO, Deployooor deployed
                                        with hardcoded IDs

th.generate_block_all() → Uses Money contract (native, VKs present) ✅
th.transfer_to_all()     → Uses Money contract (native, VKs present) ✅
```

### Why WASM Contracts Fail in Tests

```
TestHarness::new()
       │
       ├── vks::inject() → WASM VKs → SKIPPED (IDs unknown)
       │
       └── deploy_native_contracts() → Money, DAO, Deployooor ONLY
                                        (MoneyV2 NOT deployed)

th.deploy_money_v2() → Deploys MoneyV2 via Deployooor
       │              → ContractID derived (e.g., 0x4A7B...)
       │              → VKs NOT injected
       │
       th.money_v2_mint() → Generates proof using PK
               │          → Verifies against VK in blockchain
               │          → VK NOT FOUND 💥
               ▼
        vm.rs:936 panic: EcGetX try_into() failed
```

---

## Local Testing vs Mainnet Deployment

### Local Testing (TestHarness)

| Aspect | Native Contracts | WASM Contracts |
|--------|-----------------|----------------|
| Deployment | At harness init | Via `deploy_*()` calls |
| VK Injection | At init (vks::inject) | **NEVER** (bug) |
| ContractID | Hardcoded | Derived at runtime |
| Works? | ✅ Yes | ❌ No (VK missing) |

### Mainnet Deployment (darkfid + drk)

| Aspect | Native Contracts | WASM Contracts |
|--------|-----------------|----------------|
| Deployment | At darkfid startup | Via Deployooor transaction |
| VK Injection | At startup (vks::inject) | Via `drk contract deploy`? |
| ContractID | Hardcoded | Derived at runtime |
| Works? | ✅ Yes | ❓ Unknown (untested) |

---

## The Fix Required

For WASM contracts to work, post-deployment VK injection is needed:

```rust
pub async fn deploy_money_v2(&mut self, holder: &Holder, wasm_bincode: Vec<u8>, block_height: u32) -> Result<ContractId> {
    // ... existing deployment code ...

    // NEW: Inject VKs after deployment
    let contract_id = ContractId::derive_public(deploy_public);
    let db_name = contract_id.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);

    // Open the contract's zkas database
    overlay.open_tree(&db_name, false)?;

    // Inject matching VKs
    for (bincode, namespace, vk) in vks.iter() {
        if namespace matches MONEY_V2_* {
            overlay.insert(&db_name, &serialize(namespace), &serialize((bincode, vk)))?;
        }
    }

    Ok(contract_id)
}
```

---

## Current Status

| Contract Type | VK Injection | Status |
|--------------|---------------|--------|
| Native (MoneyV1, DAO) | At genesis | ✅ Works |
| WASM (MoneyV2, Stablecoin, Identity, DEX) | Never | ❌ Fails |

---

## Related Documentation

- [Test Harness Guide](./test_harness_guide.md) - Detailed test harness architecture
- [Localnet Contract Testing](./localnet_contract_testing.md) - Testing workflows
- [Contract Overview](./sc/sc.md) - Smart contract architecture
- [ZKas System](../zkas/index.md) - ZK proof system