# Smart Contract Testing and Debugging

This guide covers testing strategies and common debugging techniques for DarkFi smart contracts.

## Testing Overview

DarkFi contracts use integration tests to verify:
- Data structure encoding/decoding
- Function enum validity
- State transitions
- Update serialization

## Running Tests

```bash
# Test a specific contract
cargo test -p darkfi_<name>_contract --test integration

# Test all contracts
cargo test --workspace

# With output
cargo test -p darkfi_oracle_contract --test integration -- --nocapture
```

## Test Structure

Integration tests follow a consistent pattern:

```rust
use darkfi_serial::{deserialize, serialize};
use darkfi_sdk::pasta::pallas;
use darkfi_<contract>_contract::{
    model::{ParamsV1, UpdateV1},
    ContractFunction,
};

#[test]
fn test_params_encoding() {
    let params = ParamsV1 {
        id: pallas::Base::from(1),
        value: pallas::Base::from(100),
    };

    // Serialize
    let encoded = serialize(&params);

    // Deserialize
    let decoded: ParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, params.id);
    assert_eq!(decoded.value, params.value);
}

#[test]
fn test_function_enum_valid() {
    assert!(ContractFunction::try_from(0x00).is_ok());
    assert!(ContractFunction::try_from(0x01).is_ok());
}

#[test]
fn test_function_enum_invalid() {
    assert!(ContractFunction::try_from(0xFF).is_err());
    assert!(ContractFunction::try_from(0x10).is_err());
}
```

## Common Test Patterns

### Testing State Enums

State enums return `Result` from `try_from`:

```rust
#[test]
fn test_state_from_u8() {
    // Use .unwrap() since TryFrom returns Result
    assert_eq!(MyState::try_from(0).unwrap(), MyState::Created);
    assert_eq!(MyState::try_from(1).unwrap(), MyState::Active);
    assert!(MyState::try_from(255).is_err());
}
```

### Testing Struct Encoding

```rust
#[test]
fn test_struct_encoding() {
    let obj = MyStruct {
        id: pallas::Base::from(1),
        value: pallas::Base::from(100),
        data: vec![1, 2, 3],
    };

    let encoded = serialize(&obj);
    let decoded: MyStruct = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, obj.id);
    assert_eq!(decoded.value, obj.value);
}
```

### Testing Derive Functions

```rust
#[test]
fn test_derive_id() {
    let pub_x = pallas::Base::from(1);
    let pub_y = pallas::Base::from(2);

    let id = MyStruct::derive_id(pub_x, pub_y, "title");

    // Verify determinism
    let id2 = MyStruct::derive_id(pub_x, pub_y, "title");
    assert_eq!(id, id2);
}
```

## Test Helper Functions

Many contracts use helper functions to create consistent test data:

```rust
use darkfi_sdk::crypto::pasta_prelude::{Field, Group};

/// Create a public key for testing
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from(pallas::Base::from_u64(seed));
    PublicKey::from_secret(secret)
}

/// Create a dummy subscription for testing
fn create_dummy_subscription(id: SubscriptionId) -> Subscription {
    let keypair = darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng);
    let subscriber_pubkey = darkfi_sdk::crypto::PublicKey::from_secret(keypair.secret);
    Subscription {
        id,
        subscriber_pubkey,
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        token_id: Field::zero(),
        value_commit: Group::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: Field::zero(),
        created_at: 50000,
        dao_escrow_bulla: None,
        dao_membership_note: None,
        uses_allowed: 100,
        rate_period: 1000,
        period_uses: 5,
        last_access_block: 50000,
        uses_remaining: 95,
    }
}
```

## Correct Serialization API

Use `serialize()`/`deserialize()` NOT `encode()`/`decode()`:

```rust
// CORRECT
let encoded = serialize(&params);
let decoded: ParamsV1 = deserialize(&encoded).unwrap();

// WRONG (old API)
let encoded = params.encode().unwrap();
let decoded = ParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();
```

## Correct Field Constructors

```rust
// CORRECT
pallas::Base::zero()    // not pallas::Base::ZERO
pallas::Base::one()      // not pallas::Base::ONE
Group::identity()         // not pallas::Point::identity()

// PublicKey from secret
let pubkey = PublicKey::from_secret(secret_key);

// From u64 to Base
pallas::Base::from_u64(42)  // for u64 values
```

## Debugging Failed Tests

### Compilation Errors

#### Missing Traits

```
error: no method named `encode` found for struct `Foo`
help: trait `Encodable` which provides `encode` is not in scope
```

**Fix**: Use `darkfi_serial::{serialize, deserialize}` instead of `.encode()`/`.decode()`:

```rust
// Wrong
let encoded = params.encode().unwrap();
let decoded = ParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

// Correct
let encoded = serialize(&params);
let decoded: ParamsV1 = deserialize(&encoded).unwrap();
```

#### Missing Types

```
error[E0433]: failed to resolve: could not find `pallas` in `wasm`
```

**Fix**: Use `pasta::pallas::` prefix:

```rust
// Wrong
let id = pallas::Base::from(1);

// Correct
let id = pasta::pallas::Base::from(1);
```

#### Type Mismatches

```
error[E0308]: mismatched types
  --> expected `u32`, found `Option<Vec<u8>>`
```

**Fix**: `db_get` returns `Option<Vec<u8>>`, not the struct directly:

```rust
// Wrong
let obj: MyStruct = wasm::db::db_get(db, &key)?;

// Correct
let data = wasm::db::db_get(db, &key)?;
let obj: MyStruct = match data {
    Some(bytes) => deserialize(&bytes)?,
    None => return Err(ContractError::NotFound.into()),
};
```

### Runtime Test Failures

#### Assertion Failures on Result Comparison

```
error: binary operation `==` cannot be applied to type `Result<State, ContractError>`
```

**Fix**: Use `.unwrap()` on Result types:

```rust
// Wrong
assert_eq!(State::try_from(0), Ok(State::Active));

// Correct
assert_eq!(State::try_from(0).unwrap(), State::Active);
```

#### Struct Field Mismatches

```
error[E0560]: struct `MyStruct` has no field named `pubkey`
```

**Fix**: Update test to match current struct fields. Fields may have changed from `pubkey: PublicKey` to `pub_x: pallas::Base, pub_y: pallas::Base`.

## Debugging Contract Issues

### Library vs Test Issues

First determine if the issue is in library code or test code:

```bash
# Build just the library
cargo build -p darkfi_<name>_contract

# If this succeeds, the issue is in tests
# If this fails, the issue is in library code
```

### Common Library Fixes

#### process_update Pattern

The `process_update` function must use function enum matching, NOT DarkLeaf iteration:

```rust
// WRONG - DarkLeaf cannot be indexed
fn process_update(cid: ContractId, updates: &[u8]) -> ContractResult {
    let updates: Vec<DarkLeaf<pallas::Base>> = deserialize(updates)?;
    for update in updates {
        match update.data[0] {  // ERROR: data is pallas::Base, not indexable
```

```rust
// CORRECT - use function enum directly
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MyFunction::try_from(update_data[0])? {
        MyFunction::DoSomethingV1 => {
            let update: DoSomethingUpdateV1 = deserialize(&update_data[1..])?;
            Ok(())
        }
    }
}
```

#### Import Correctness

```rust
// CORRECT imports
use darkfi_sdk::{
    crypto::{pasta_prelude::*, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta, ContractCall,
    wasm,
};
use darkfi_serial::{deserialize, serialize};
```

### Debugging with cargo check

```bash
# Full check with all features
cargo check -p darkfi_<name>_contract --all-features

# Check specific target
cargo check -p darkfi_<name>_contract --target wasm32-unknown-unknown
```

## Test Coverage

View test coverage reports:

```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Generate coverage
make coverage

# View report
open target/llvm-cov/html/index.html
```

## Contract-Specific Testing Notes

### Oracle Contract

Tests use `pallas::Base::from()` for field elements:

```rust
let oracle = Oracle {
    id: pallas::Base::from(1),
    oracle_pub_x: pallas::Base::from(2),
    oracle_pub_y: pallas::Base::from(3),
    name: "Price Feed".to_string(),
    data_type: "price".to_string(),
    value: pallas::Base::from(50000),
    updated_at: 50000,
    is_active: true,
};
```

### Auction Contract

Auction uses coordinate pairs for public keys:

```rust
let auction = Auction {
    id: pallas::Base::from(1),
    seller_pub_x: params.seller_pub_x,
    seller_pub_y: params.seller_pub_y,
    // ...
};
```

### Tender Contract

Tender's `derive_id` takes x/y coordinates directly:

```rust
let id = Tender::derive_id(
    requester_pub_x,
    requester_pub_y,
    title,
    specification,
    // ...
);
```

## Continuous Integration

All contracts should pass tests before merging:

```bash
# Pre-submit checklist
cargo build -p darkfi_<name>_contract
cargo test -p darkfi_<name>_contract --test integration
cargo check -p darkfi_<name>_contract --all-features
```

## Known Limitations

### ZK Proof Verification

`wasm::zk::verify_zk_proof()` is not available in the SDK. ZK verification happens at validator runtime. Contract code should comment out these calls:

```rust
// ZK verification happens at validator runtime
// wasm::zk::verify_zk_proof(cid, NAMESPACE)?;
```

### Block Height

`wasm::chain::get_block_height()` does not exist. Use:

```rust
let current_block = wasm::util::get_verifying_block_height()? as u64;
```