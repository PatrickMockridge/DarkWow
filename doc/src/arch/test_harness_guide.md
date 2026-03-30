# DarkFi Smart Contract Test Harness Guide

## Overview

This document describes the standards and best practices for writing and running test harnesses for DarkFi smart contracts, given the current technical limitations in the codebase.

## Types of Tests

DarkFi has two distinct levels of contract testing:

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

## See Also

- [ZK Circuit Testing](./zk_circuit_testing.md)
- [Async Serialization Lifetime Bug](./async_serial_lifetime_bug.md) (if using Rust 1.90+)
- [Contract Architecture](./sc/sc.md)
- [Baccarat Contract](./baccarat.md) (example)