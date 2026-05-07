# DarkWow Smart Contract Test Harness Guide

## Overview

This document describes the standards and best practices for writing and running test harnesses for DarkWow smart contracts, given the current technical limitations in the codebase.

## Types of Tests

DarkWow has two distinct levels of contract testing:

### 1. ZK Circuit Tests (`.zk` compilation)

These verify that ZK circuits compile correctly to binary format.

**Location:** `src/contract/<name>/tests/zk_circuit_test.sh`

**What they test:**
- ZK circuit source files (`.zk`) compile to `.zk.bin`
- Witness generation is syntactically correct
- Constraint system is satisfied

**What they DON'T test:**
- Contract business logic
- State transitions
- Serialization

**Example:**
```bash
#!/bin/bash
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/baccarat/proof"
OUTPUT_DIR="src/contract/baccarat/proof"

echo "Compiling commit_bet_v1.zk..."
$ZKAS_BIN ${PROOF_DIR}/commit_bet_v1.zk -o ${OUTPUT_DIR}/commit_bet_v1.zk.bin

echo "Compiling settle_bet_v1.zk..."
$ZKAS_BIN ${PROOF_DIR}/settle_bet_v1.zk -o ${OUTPUT_DIR}/settle_bet_v1.zk.bin

# Verify binaries exist
[ -f "${OUTPUT_DIR}/commit_bet_v1.zk.bin" ] || exit 1
[ -f "${OUTPUT_DIR}/settle_bet_v1.zk.bin" ] || exit 1
```

**Run with:**
```bash
bash src/contract/baccarat/tests/zk_circuit_test.sh
```

### 2. Integration Tests (`integration.rs`)

These verify the contract's data model and serialization layer.

**Location:** `src/contract/<name>/tests/integration.rs`

**What they test:**
- Serialization/deserialization (`encode()`/`decode()` round-trips)
- Enum conversion functions (`try_from()`)
- Derivation functions are deterministic
- Constants have correct values
- Data structure construction

**What they DON'T test:**
- Actual contract business logic (game rules, payouts, etc.)
- ZK proof generation/verification
- State machine transitions
- Blockchain integration

**Why:** The full test harness requires `darkfi/validator` which enables `darkfi-serial/async`, causing compilation issues on Rust 1.90+ with the current codebase.

## Current Limitations

### The async-serial Issue

The full integration test harness (`darkfi-contract-test-harness`) depends on `darkfi/validator`. This transitively enables:

```
darkfi/validator
  └── darkfi/blockchain
        └── darkfi/tx
              └── darkfi/async-serial
                    └── darkfi-serial/async
```

When `darkfi-serial/async` is enabled, the derive macros generate async implementations that fail lifetime bounds checking on Rust 1.90+.

**Workaround:** Use Rust 1.89 with downgraded dependencies:
```bash
rustup install 1.89.0
rustup override set 1.89.0
cargo update typed-index-collections@3.4.0 --precise 3.3.0
```

**Note:** This is a pre-existing architectural issue, not a bug in your tests.

### What This Means For Test Authors

Your integration tests verify:
- ✅ Data model correctness
- ✅ Serialization layer
- ✅ Type conversions
- ❌ Business logic
- ❌ ZK proof generation
- ❌ End-to-end flows

This is still valuable - it catches type errors and serialization bugs early.

### 3. Full Darkfid Test Harness (`darkfi-contract-test-harness`)

This is the **full integration test harness** that tests:
- Full contract execution with darkfid
- ZK proof generation and verification
- State machine transitions
- Multi-holder workflows
- Transaction building with multiple contract calls

**Location:** `src/contract/test-harness/`

**What it tests:**
- ✅ Full contract business logic
- ✅ ZK proof generation and verification
- ✅ State transitions (apply_update)
- ✅ Multi-holder interactions
- ✅ Transaction building with ContractCall, TransactionBuilder
- ✅ Fee handling with append_fee_call()

**What it requires:**
- Running dwowd instance or embedded validator
- Compiled ZK proof binaries (`.zk.bin`)
- Contract WASM binaries (for WASM contracts)
- Full darkfi SDK with validator feature

**Why Two Levels of Testing?**

Integration tests (`integration.rs`) catch type errors and serialization bugs quickly without needing the full darkfi stack. The full test harness (`darkfi-contract-test-harness`) verifies actual contract behavior end-to-end.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Test Pyramid                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│                        dwowd                                   │
│                    (Full Node/Test Harness)                      │
│         - ZK proof generation/verification                       │
│         - Contract execution                                      │
│         - State transitions                                       │
│         - Multi-contract transactions                            │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│              darkfi-contract-test-harness                        │
│                   (Integration Tests)                             │
│         - Contract interaction API                                │
│         - Transaction building                                    │
│         - Wallet/Holder pattern                                 │
│         - Multi-holder workflows                                 │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│               Contract integration.rs                           │
│                  (Unit Tests)                                    │
│         - Model serialization                                     │
│         - Enum conversions                                       │
│         - Derivation functions                                    │
│         - Constants                                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## DarkWow Test Harness Architecture

### Core Components

#### TestHarness Struct

The `TestHarness` is the main entry point for testing contracts:

```rust
pub struct TestHarness {
    /// Initialized holders for this instance
    pub holders: HashMap<Holder, Wallet>,
    /// Ordered list of all holder keys (for broadcast operations)
    pub holder_keys: Vec<Holder>,
    /// Cached ProvingKeys for ZK proving
    pub proving_keys: HashMap<String, (ProvingKey, ZkBinary)>,
    /// The genesis block for this harness
    pub genesis_block: BlockInfo,
    /// Marker to know if we're supposed to include tx fees
    pub verify_fees: bool,
}
```

#### Holder Enum

Represents different participants in a test scenario:

```rust
pub enum Holder {
    Alice,    // Primary test participant
    Bob,      // Secondary participant
    Charlie,  // Tertiary participant
    Dao,      // DAO governance
    Rachel,   // Additional participant
}
```

#### Wallet Struct

Each holder has a `Wallet` containing their keypairs and state:

```rust
pub struct Wallet {
    /// Main holder keypair
    pub keypair: Keypair,
    /// Keypair for arbitrary token minting
    pub token_mint_authority: Keypair,
    /// Keypair for arbitrary contract deployment
    pub contract_deploy_authority: Keypair,
    /// Holder's Validator instance
    pub validator: ValidatorPtr,
    /// Holder's Money Merkle tree
    pub money_merkle_tree: MerkleTree,
    /// Holder's Money nullifiers SMT
    pub money_null_smt: SmtMemoryFp,
    /// Unspent OwnCoins from Money contract
    pub unspent_money_coins: Vec<OwnCoin>,
    // ... additional state
}
```

### Native vs WASM Contracts

DarkWow contracts come in two types:

#### Native Contracts

Native contracts are compiled into dwowd and have **static ContractIds** defined in the SDK:

```rust
// In darkfi_sdk::crypto::contract_id
pub static ref DEPLOYOOOR_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(0)]));
pub static ref MONEY_V2_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(1)]));
pub static ref NATIVE_TOKEN_CONTRACT_ID: ContractId =
    ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(2)]));
```

**Test Harness for Native Contracts:**
- VKs are injected at harness initialization (in `vks.rs::inject()`)
- Contract ID is known at compile time
- No deployment step needed

**Note:** NativeToken is the consensus-first native contract for block rewards (PoWRewardV1) and fees (FeeV1). MoneyV2 is deprecated.

#### WASM Contracts

WASM contracts are deployed via the `Deployooor` contract. Their **ContractId is derived from the deploy public key**:

```rust
let stablecoin_contract_id = ContractId::derive_public(deploy_public_key);
```

**Test Harness for WASM Contracts:**
- VKs are injected **after deployment** (not in `vks.rs::inject()`)
- Contract ID is only known after deployment
- Must deploy before use
- Use `Deployooor::Deploy` to deploy

## Creating a Test Harness

### For Native Contracts

**Step 1: Add Dependency**

In `src/contract/test-harness/Cargo.toml`:

```toml
darkfi_<contract>_contract = { path = "../<contract>", features = ["client", "no-entrypoint"] }
```

**Step 2: Add ZK Proof Bins**

In `src/contract/test-harness/src/vks.rs`, add to the `bins` vector:

```rust
&include_bytes!("../../<contract>/proof/circuit_v1.zk.bin")[..],
```

**Step 3: Add Namespace Handling**

In `vks.rs::inject()`, add a match arm for the new namespaces:

```rust
"<CONTRACT>_CONTRACT_ZKAS_NS" => {
    let key = serialize(&namespace.as_str());
    let value = serialize(&(bincode.clone(), vk.clone()));
    overlay.insert(&contract_db_name, &key, &value)?;
}
```

**Step 4: Create Harness Module**

Create `src/contract/test-harness/src/contract_<func>.rs`:

```rust
use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::contract_id::CONTRACT_ID, ContractCall,
};
use darkfi_serial::Encodable;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Contract::Function` transaction
    pub async fn contract_function(
        &mut self,
        holder: &Holder,
        // ... function-specific params
        block_height: u32,
    ) -> Result<(Transaction, FunctionParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);

        // Build contract params
        let params = FunctionParams { /* ... */ };

        // Build contract call
        let mut data = vec![FunctionEnum::FunctionV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *CONTRACT_ID, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        // Fee handling
        let mut fee_params = None;
        if self.verify_fees {
            // ... append fee call
        }

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[wallet.keypair.secret])?;
        tx.signatures = vec![sigs];

        Ok((tx, params, fee_params))
    }

    /// Execute a transaction
    pub async fn execute_contract_function_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        params: &FunctionParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);
        wallet.add_transaction("contract::function", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}
```

**Step 5: Add Module to lib.rs**

In `src/contract/test-harness/src/lib.rs`:

```rust
/// `Contract::Function` functionality
mod contract_function;
```

**Step 6: Create *_to_all() Convenience Method**

In `lib.rs`, add:

```rust
pub async fn contract_function_to_all(
    &mut self,
    // ... params
) -> Result<...> {
    let (tx, params, fee_params) = self.contract_function(/* ... */).await?;

    for h in &self.holder_keys {
        self.execute_contract_function_tx(h, tx.clone(), &params, &fee_params, block_height, true).await?;
    }

    self.assert_all_trees();
    Ok(...)
}
```

### For WASM Contracts

**Same as native, except:**

1. **VK injection is skipped** in `vks.rs::inject()` - WASM contract VKs are injected post-deployment

2. **Deploy first** - Add a `deploy_contract()` function:

```rust
pub async fn deploy_contract(
    &mut self,
    holder: &Holder,
    wasm_bincode: Vec<u8>,
    block_height: u32,
) -> Result<ContractId> {
    let deploy_public = self.wallet(holder).contract_deploy_authority.public;

    let (tx, _, fee_params) =
        self.deploy_contract(holder, wasm_bincode, block_height).await?;

    let contract_id = ContractId::derive_public(deploy_public);

    self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true).await?;

    Ok(contract_id)
}
```

3. **Track contract_id** - Store the derived contract ID and use it for subsequent calls

## Best Practices

### Writing Integration Tests

#### 1. Test the Function Enum

```rust
#[test]
fn test_baccarat_function_enum_valid() {
    assert!(BaccaratFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(BaccaratFunction::try_from(0x01).is_ok()); // CommitBetV1
    // ...
}

#[test]
fn test_baccarat_function_enum_invalid() {
    assert!(BaccaratFunction::try_from(0xFF).is_err());
    assert!(BaccaratFunction::try_from(0x06).is_err());
}
```

#### 2. Test State Enums

```rust
#[test]
fn test_bet_state_from_u8() {
    assert_eq!(BetState::try_from(0), Ok(BetState::Sealed));
    assert_eq!(BetState::try_from(1), Ok(BetState::Revealed));
    // Invalid states
    assert!(BetState::try_from(5).is_err());
    assert!(BetState::try_from(255).is_err());
}
```

#### 3. Test Serialization Round-Trips

```rust
#[test]
fn test_bet_encoding() {
    let bet = Bet {
        id: pallas::Base::from(1),
        // ... other fields
        state: BetState::Sealed,
    };

    let encoded = bet.encode().unwrap();
    let decoded = Bet::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, bet.id);
    assert_eq!(decoded.state, bet.state);
}
```

#### 4. Test Derivation Functions Are Deterministic

```rust
#[test]
fn test_bid_derive_id() {
    let tender_id = pallas::Base::from(1);
    let bidder_pubkey = /* ... */;
    let amount: u64 = 5000;
    let bid_nonce = pallas::Base::from(42);

    let id = Bid::derive_id(tender_id, &bidder_pubkey, amount, bid_nonce);
    let id2 = Bid::derive_id(tender_id, &bidder_pubkey, amount, bid_nonce);

    assert_eq!(id, id2); // Same input → Same output
}
```

#### 5. Test Constants

```rust
#[test]
fn test_constants() {
    assert_eq!(BACCARAT_CONTRACT_BETS_TREE, "bets");
    assert_eq!(BACCARAT_CONTRACT_NULLIFIERS_TREE, "nullifiers");
}
```

### Writing ZK Circuit Tests

#### Template

```bash
#!/bin/bash
set -e

ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/<name>/proof"
OUTPUT_DIR="src/contract/<name>/proof"

echo "=== <Name> Contract ZK Circuit Compilation Test ==="

# Compile each circuit
echo "[Test 1] Compiling <circuit>_v1.zk..."
$ZKAS_BIN ${PROOF_DIR}/<circuit>_v1.zk -o ${OUTPUT_DIR}/<circuit>_v1.zk.bin
echo "  ✓ <circuit>_v1.zk compiled successfully"

# Verify binaries
echo ""
echo "[Test N] Verifying compiled binaries..."
for circuit in <list>; do
    if [ -f "${OUTPUT_DIR}/${circuit}_v1.zk.bin" ]; then
        echo "  ✓ ${circuit}_v1.zk.bin exists"
    else
        echo "  ✗ ${circuit}_v1.zk.bin missing"
        exit 1
    fi
done

echo ""
echo "=== All <Name> circuit compilation tests passed ==="
```

## ZK Proof Client Module Implementation Status

The following contracts have full ZK proof client modules implemented for proof generation:

| Contract | Circuits | Module Location |
|----------|----------|-----------------|
| identity | 8 | `src/contract/identity/src/client/` |
| labor_market | 9 | `src/contract/labor_market/src/client/` |
| oracle | 5 | `src/contract/oracle/src/client/` |
| auction | 6 | `src/contract/auction/src/client/` |
| tender | 5 | `src/contract/tender/src/client/` |
| attestation | 8 | `src/contract/attestation/src/client/` |
| subscription | 3 | `src/contract/subscription/src/client/` |
| escrow | 4 | `src/contract/escrow/src/client/` |
| stablecoin | 5 | `src/contract/stablecoin/src/client/` |
| bridge | 6 | `src/contract/bridge/src/client/` |
| dex | 6 | `src/contract/dex/src/client/` |
| atomic_swap | 3 | `src/contract/atomic_swap/src/client/` |

Each client module provides:
- `*PublicInputs` struct with `to_vec()` for circuit public inputs
- `*CallData` struct with private/public input data  
- `compute_public_inputs()` method
- `to_witnesses()` method returning `Vec<Witness>` for `ZkCircuit`
- `*_proof()` function that creates `Proof`

## Verifying Your Tests

### Run ZK Circuit Tests

```bash
# Run all ZK circuit tests
for script in src/contract/*/tests/zk_circuit_test.sh; do
    bash "$script"
done

# Run specific contract
bash src/contract/baccarat/tests/zk_circuit_test.sh
```

### Run Integration Tests

Due to the async-serial limitation, you may need Rust 1.89:

```bash
rustup install 1.89.0
rustup override set 1.89.0
cargo update typed-index-collections@3.4.0 --precise 3.3.0

cargo test -p darkfi_baccarat_contract --test integration
```

### Verify Contract Builds

```bash
cargo build -p darkfi_baccarat_contract --lib
```

If this succeeds but integration tests fail, the issue is with the test harness environment, not your tests.

## File Structure

```
src/contract/<name>/
├── src/
│   ├── lib.rs          # Function enum, error types, constants
│   ├── model/mod.rs    # Data models, state types, params
│   ├── entrypoint.rs   # Contract execution logic
│   └── client/         # Client-side API
├── proof/              # ZK circuits
│   ├── circuit_v1.zk
│   └── witness_v1.zk
└── tests/
    ├── integration.rs  # Integration tests (serialization/model)
    └── zk_circuit_test.sh  # ZK circuit compilation test
```

## Common Patterns

### Importing Contract Types

```rust
use darkfi_baccarat_contract::{
    model::{
        Bet, BetState, BetType, Card, CommitBetParamsV1, CommitBetUpdateV1, Hand, Outcome,
        BACCARAT_CONTRACT_BETS_TREE, BACCARAT_CONTRACT_NULLIFIERS_TREE,
    },
    BaccaratFunction,
};
```

### Testing All Param/Update Types

For each function, test both params and update structures:

```rust
// Params
let params = CommitBetParamsV1 {
    proof: vec![1, 2, 3],
    bet_id: pallas::Base::from(1),
    // ... other fields
};
let encoded = params.encode().unwrap();
let decoded = CommitBetParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();
assert_eq!(decoded.bet_id, params.bet_id);

// Update
let update = CommitBetUpdateV1 {
    bet_id: pallas::Base::from(1),
};
let encoded = update.encode().unwrap();
let decoded = CommitBetUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();
assert_eq!(decoded.bet_id, update.bet_id);
```

## Troubleshooting

### "package ID specification did not match any packages"

The contract may not be in the workspace. Check `Cargo.toml` members list.

### "lifetime parameters or bounds do not match"

This is the async-serial issue. Use Rust 1.89 or verify the contract library builds without tests.

### ZK circuit compilation fails

Check that:
1. `zkas` binary exists at `./bin/zkas/zkas`
2. Circuit source files exist in `proof/` directory
3. The `.zk` files have valid syntax

### EcGetX: heap index error

```
EcGetX: heap index 6 >= heap.len() 5
```

**Root Cause**: Mixing money v1 contract IDs with money v2 ZK circuits.

When a transaction uses:
- `contract_id = MONEY_CONTRACT_ID` (v1)
- `function = TokenMintV2` (v2 function code)
- `zkbin namespace = "TokenMint_V2"` (v2 circuit)

The v1 WASM doesn't understand v2 function codes, causing heap layout mismatch.

**Fix**: Ensure contract IDs match contract versions:

```rust
// WRONG - v1 ID with v2 function
ContractCall { contract_id: *MONEY_CONTRACT_ID, data: vec![MoneyV2Function::TokenMintV2 as u8, ...] }

// CORRECT - v2 ID with v2 function
ContractCall { contract_id: *MONEY_V2_CONTRACT_ID, data: vec![MoneyV2Function::TokenMintV2 as u8, ...] }
```

### PositionNotMarked Error

**Cause**: Merkle tree not properly synchronized with wallet state.

When processing outputs, the wallet appends to its Merkle tree. If the tree gets out of sync with the actual blockchain state, `witness()` lookups fail.

**Fix**: Ensure `wallet.process_money_v2_outputs()` is called after every output-producing transaction.

## Money V1 and Money V2 are DEPRECATED (Native Token is Current)

**Money V1 and Money V2 are deprecated. Native Token is the current standard.**

| Component | Money V1 (DEPRECATED) | Money V2 (DEPRECATED) | Native Token (CURRENT) |
|-----------|----------------------|----------------------|-----------------------|
| Contract ID | `MONEY_CONTRACT_ID` (removed) | `MONEY_V2_CONTRACT_ID` | `NATIVE_TOKEN_CONTRACT_ID` |
| Design | ACL-based | ZK circuits | Consensus-first |
| DAO Coupling | Tight | Moderate | Decoupled |
| WASM Binary | None (deleted) | money_v2/*.wasm.bin | native_token/*.wasm.bin |

### Why Native Token Replaces Money V2

Native Token addresses fundamental issues:
- **Consensus-first**: Block rewards and fees are paramount
- **DAO-decoupled**: No ACL dependencies
- **Simple genesis**: Single GenesisMintV1 call
- **Minimal circuits**: Only essential ZK operations

### Wallet Processing Methods for Native Token

When writing tests use these Native Token methods:

```rust
// Process Native Token outputs
pub fn process_native_token_outputs(&mut self) -> Vec<OwnCoinNativeToken>

// Deploy Native Token contract
pub async fn deploy_native_token(&mut self, holder: &Holder, wasm_bincode: Vec<u8>, block_height: u32) -> Result<ContractId>

// Native Token transfers
pub async fn native_token_transfer(&mut self, ...) -> Result<...>
```

## See Also

- [ZK Circuit Testing](./zk_circuit_testing.md)
- [Async Serialization Lifetime Bug](./async_serial_lifetime_bug.md) (if using Rust 1.90+)
- [Contract Architecture](./sc/sc.md)
- [Genesis Harness](./genesis_harness.md) - NativeToken + Deployooor baseline only
- [Contract Testing Pipeline](./pipeline.md) - Unified binary check + genesis + deploy workflow
- [Baccarat Contract](../contract/baccarat.md) (example)