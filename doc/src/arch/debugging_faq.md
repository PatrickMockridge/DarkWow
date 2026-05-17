# Debugging FAQ and Common Issues

This document covers common issues and debugging strategies for DarkWow development, with a focus on DarkWow's unique implementation of Rust async patterns.

## Table of Contents

- [Async Serialization Issues](#async-serialization-issues)
- [Compilation Errors](#compilation-errors)
- [Runtime Issues](#runtime-issues)
- [ZK Circuit Debugging](#zk-circuit-debugging)

---

## Async Serialization Issues

DarkWow uses a custom async serialization system that can cause confusing compilation errors when not properly configured.

### The `async_trait` Import Error

**Error message:**
```
error: cannot find attribute `async_trait` in this scope
  --> src/contract/money/src/model/nullifier.rs:24:45
   |
24 | #[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
   |                                             ^^^^^^^^^^^^^^^
```

**Status: Resolved**

This issue was fixed by updating the derive macros to use fully qualified paths:
- `#[#cratename::async_trait]` instead of `#[async_trait]`
- `#cratename::FutAsyncWriteExt::write_all()` instead of unqualified method calls

Contracts no longer need to import `async_trait` at use sites. The generated code in `src/serial/derive-internal/src/async_derive.rs` now uses `dwow_serial::async_trait` directly.

**Related:**
- [Async Rust Fundamentals](../learn/dchat/async-rust-fundamentals.md) - Background on Rust async patterns

### Feature Flag Propagation

**Common pattern:**
When a contract enables `client` feature, it typically propagates to `dwow-sdk/async` which enables `darkfi-serial/async`:

```toml
# In contract's Cargo.toml
[features]
client = [
    "dwow-sdk/async",      # This enables darkfi-serial/async
    # ...
]
```

The `SerialEncodable`/`SerialDecodable` derives work automatically with the `async` feature. No additional imports are needed in contract code.

Example from a contract's Cargo.toml:

```toml
[features]
async = ["dwow-sdk/async"]
client = [
    "darkfi",
    "async",                  # client depends on async
    "dwow-sdk/async",
    "darkfi-serial/async",
    # ...
]
```

### Async Serialization Lifetime Bug

**Error message:**
```
error[E0433]: failed to resolve: could not find `async_trait` in the list of imported crates
  --> src/sdk/src/crypto/intent_set.rs:124:56
    |
124 | #[derive(Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
    |                                       ^^^^^^^^^^^^^^^ could not find `async_trait`
```

**Cause:**
When `darkfi-serial/async` is enabled, the `SerialEncodable` and `SerialDecodable` derive macros generate async code that uses `#[async_trait]`. Previously, the generated code used `#[async_trait]` directly, expecting the macro to be in scope at the use site. However, `async_trait` is only available as a transitive dependency through `dwow_serial`, not as a direct import at use sites in `dwow_sdk`.

**Fix Applied:**
The fix was in `src/serial/derive-internal/src/async_derive.rs`:

1. Changed `#[async_trait]` to `#[#cratename::async_trait]` in generated code - this uses the fully qualified path through `dwow_serial` where the trait is re-exported
2. Changed `s.write_all(&bytes).await?` to `#cratename::FutAsyncWriteExt::write_all(&mut *s, &bytes).await?` - uses fully qualified trait method

This makes the generated code self-contained and doesn't require contracts to import `async_trait` at use sites.

**Verification:**
```bash
cargo check -p darkfi --features "zk"  # Should compile without async_trait errors
cargo check -p dwowd  # Should compile
```

**Related:**
- [Async Rust Fundamentals](../learn/dchat/async-rust-fundamentals.md)
- [async-trait crate](https://crates.io/crates/async-trait)
- [Rust 1.75 async traits release notes](https://blog.rust-lang.org/2023/12/28/Rust-1.75.html)

---

## Compilation Errors

### Opcode Match Exhaustiveness

**Error message:**
```
error[E0004]: non-exhaustive patterns: `Opcode::IsEqualBase`,
`Opcode::LessThanOrEqual`, `Opcode::NotBase` and 1 more not covered
   --> src/validator/fees.rs:65:26
```

**Cause:**
New opcodes were added to the `Opcode` enum but the fee calculation match statement wasn't updated.

**Solution:**
Add the missing opcode arms with appropriate gas costs:

```rust
Opcode::IsEqualBase => 100,
Opcode::LessThanOrEqual => 100,
Opcode::NotBase => 20,
Opcode::BaseLtStrict => 100,
```

### Borrow Checker Errors in WASM Runtime

**Error message:**
```
error[E0502]: cannot borrow `*env` as mutable because it is also
borrowed as immutable
   --> src/runtime/import/util.rs:636:5
```

**Cause:**
Creating a reference to a field (`let cid = &env.contract_id;`) while later requiring a mutable borrow of the same struct (`env.subtract_gas(...)`).

**Solution:**
Clone the value instead of borrowing:

```rust
let cid = env.contract_id.clone();  // Clone, don't reference
```

### Method Argument Type Mismatch with Layouter

**Error message:**
```
error[E0308]: mismatched types
   --> src/zk/vm.rs:1263:44
    |
1263 | .is_eq_with_output(layouter.namespace(|| "is_equal_base"), lhs, rhs)?;
    |                          ----------------- ^^^^^^^^^^^^^^^^^^^^^^
    |                          expected `&mut _`, found `NamespacedLayouter`
```

**Cause:**
`layouter.namespace()` returns a `NamespacedLayouter` by value, but some methods require a mutable reference.

**Solution:**
Use `&mut` to pass a mutable reference:

```rust
.is_eq_with_output(&mut layouter.namespace(|| "is_equal_base"), lhs, rhs)?;
```

### MoneyV3 Contract Compilation

The `darkfi_money_v3_contract` is the current DeFi token contract. If you encounter compilation issues:

**Common issue: Missing ZK circuit binaries**
```
error: failed to read circuit binary
```

**Solution:** Regenerate circuit binaries:
```bash
cd src/contract/money_v3
make clean && make all
```

**Error 2: Cannot find function `poseidon_hash`**
```
error[E0425]: cannot find function `poseidon_hash` in this scope
```

**Cause:** Import was outside the nested `crypto {}` block.

**Solution:** Ensure imports are in correct scope:
```rust
use dwow_sdk::crypto::{
    // imports must be inside nested crypto block
    poseidon_hash,
    // ...
};
```

**Error 3: lazy_static ordering**
The `TokenId` struct must be defined before `DRKW_TOKEN_ID` lazy_static that references it. Reorder so struct comes first.

**Error 4: Type annotations needed**
```
error[E0282]: type annotations needed
```

**Cause:** Closure parameter type not inferred.

**Solution:** Add explicit type annotation:
```rust
.update.nullifiers.iter().map(|n: &Nullifier| n.inner())
```

---

## Runtime Issues

### Intent Expiry Not Being Checked

Intents have an `expiry` block height field. The `EXPIRE` transition doesn't update state - consumers must check expiry before consuming.

See [`PrivateIntent::is_expired_at`](../src/sdk/src/crypto/intent.rs) for the implementation.

### Nullifier Replay Protection

The `IntentSetIndexV1` tracks consumed nullifiers in memory. On restart, this state is rebuilt from the blockchain. Ensure your integration properly replays nullifiers from on-chain state.

---

## DAO Escrow Contract

### Contract Overview

The DAO Escrow contract (`src/contract/dao_escrow/`) manages endowment funds for DAOs with three modes:
- **Escrow**: Members pay premiums to endowment, owner withdraws
- **Treasury**: DAO governance controls withdrawals
- **TreasuryEndowment**: Combination with endowment-style deposits

### Function Codes

| Function | Code | Description |
|----------|------|-------------|
| InitializeV1 | 0x00 | Create new endowment |
| UpdateV1 | 0x01 | Update endowment parameters |
| PayPremiumV1 | 0x02 | Member pays premium, receives membership |
| WithdrawV1 | 0x03 | Owner withdraws from endowment |
| EndowmentWithdrawV1 | 0x04 | DAO governance withdrawal (not implemented) |
| TreasurySpendV1 | 0x05 | Treasury spending (not implemented) |
| EnableDrainProtectionV1 | 0x06 | Enable fund drain protection |

### Common Implementation Patterns

**InitializeV1** - Creates a new DAO-Escrow endowment:
```rust
fn initialize_v1(cid: ContractId, params: model::InitializeParamsV1) -> ContractResult {
    // Verify endowment doesn't already exist
    let bullas_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;
    if wasm::db::db_contains_key(bullas_db, &params.dao_bulla.to_repr())? {
        return Err(DaoEscrowError::DaoEscrowAlreadyExists(...).into())
    }
    // Derive endowment bulla and create update
    let update = model::InitializeUpdateV1 { ... };
    wasm::util::set_return_data(&serialize(&update))
}

fn initialize_apply_v1(cid: ContractId, update: model::InitializeUpdateV1) -> ContractResult {
    // Store in bullas tree and endowment tree
    wasm::db::db_set(bullas_db, &update.bulla.to_repr(), &[])?;
    wasm::db::db_set(endowments_db, &update.bulla.to_repr(), &serialize(&endowment))?;
    Ok(())
}
```

**WithdrawV1** - Endowment owner withdrawals:
```rust
fn withdraw_v1(cid: ContractId, params: model::WithdrawParamsV1) -> ContractResult {
    // Verify caller is endowment owner
    if endowment.owner_pubkey != params.recipient_pubkey {
        return Err(DaoEscrowError::NotAuthorizedToWithdraw.into())
    }
    // Verify sufficient balance
    if endowment.total_endowment < params.value {
        return Err(DaoEscrowError::InsufficientEndowment.into())
    }
    // Create update with new total
    Ok(())
}
```

### Model Update Structs

The Update structs returned by instruction handlers must include all fields needed by the apply handler:

```rust
// WRONG - missing fields in update
pub struct WithdrawUpdateV1 {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub value: u64,
    // Missing: total_endowment needed by apply handler!
}

// CORRECT - apply handler has everything it needs
pub struct WithdrawUpdateV1 {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub value: u64,
    pub total_endowment: u64,  // Updated balance after withdrawal
}
```

### Key Trees

| Tree | Purpose |
|------|---------|
| BULLAS_TREE | Tracks all endowment bullet |
| ENDOWMENT_TREE | Stores endowment state |
| MEMBERSHIP_TREE | Stores membership notes |
| INFO_TREE | Metadata (for redeployment guards) |

### Redeployment Guard Pattern

To allow safe redeployment, use db_lookup before db_init:
```rust
let _info_db = match wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE) {
    Ok(v) => v,
    Err(_) => wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?,
};
```

---

## Subscription Contract

### Contract Overview

The Subscription contract (`src/contract/subscription/`) manages recurring subscriptions with DAO escrow integration and rate limiting.

### Function Codes

| Function | Code | Description |
|----------|------|-------------|
| InitializeV1 | 0x00 | Create subscription plan |
| SubscribeV1 | 0x01 | User subscribes to a plan |
| CancelV1 | 0x02 | Cancel active subscription |
| RenewV1 | 0x03 | Renew an expired subscription |
| VerifyAccessV1 | 0x04 | Verify subscriber access |
| DaoControlV1 | 0x05 | DAO controls subscription |
| UpdateUsageV1 | 0x06 | Update usage counters |

### State Machine

Subscriptions follow this state flow:
- `Created` → `Active` (on SubscribeV1)
- `Active` → `Cancelled` (on CancelV1)
- `Active` → `Expired` (if not renewed past `lock_until_block`)
- `Cancelled`/`Expired` → `Active` (on RenewV1)

### Common Implementation Issues

**State not persisting**: The `process_update` function must write the full `Subscription` object to the database:
```rust
fn subscribe_apply_v1(cid: ContractId, update: SubscribeUpdateV1) -> ContractResult {
    let subscription_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    // Must serialize the full subscription, not just the ID
    wasm::db::db_set(subscription_db, &update.subscription.id.to_repr(), &serialize(&update.subscription))?;
    Ok(())
}
```

**Key serialization**: Use `to_repr()` not `to_bytes()` for pallas::Base keys:
```rust
// CORRECT
wasm::db::db_set(db, &subscription.id.to_repr(), &serialize(&subscription)?);

// WRONG - to_bytes() doesn't exist on pallas::Base
wasm::db::db_set(db, &subscription.id.to_bytes(), &serialize(&subscription)?);
```

**Block height casting**: `get_verifying_block_height()` returns `u32`, cast to `u64`:
```rust
// CORRECT
let current_block = wasm::util::get_verifying_block_height()? as u64;

// WRONG - type mismatch
let current_block = wasm::util::get_verifying_block_height()?;
```

### Model Update Structs

Update structs must carry full objects, not just IDs:

```rust
// WRONG - apply handler needs full subscription
pub struct SubscribeUpdateV1 {
    pub subscription_id: SubscriptionId,
}

// CORRECT - apply handler has everything it needs
pub struct SubscribeUpdateV1 {
    pub subscription: Subscription,
}
```

---

## ZK Circuit Debugging

### Gadget Errors

When a ZK gadget fails, check:
1. Input assignments are within field arithmetic constraints
2. Witness values match expected ranges
3. Layout constraints are satisfied

### Common Halo2 Errors

- **Excess bits**: Ensure field elements don't exceed pallas::Base bit width
- **Constraint unsatisfied**: Check witness values match public inputs
- **Assignment errors**: Use `AssignedCell::value()` to inspect actual values

---

## RPC and Wallet Integration

### Generalized Contract Invocation

DarkWow provides a generalized `contract.invoke` RPC endpoint for invoking any smart contract without requiring new API methods per function. See [Generalized Contract Invocation API](contract_invoke_api.md) for details.

**RPC Endpoint:** `contract.invoke`

**Request:**
```json
{
  "method": "contract.invoke",
  "params": {
    "contract_id": "dao_escrow",
    "function": "InitializeV1",
    "params": {"enable_drain_protection": true},
    "dry_run": true
  }
}
```

**Current Status:**
- `contract.invoke` endpoint is implemented in `bin/dwowd/src/rpc/contract.rs`
- `ContractHandler` trait and `ContractRegistry` in `bin/dwowd/src/contract_registry.rs`
- DAO-Escrow handler with function selectors (0x00-0x06)
- Full ZK proof generation and transaction broadcasting is TODO

### Contract Handler Pattern

To add a new contract to the generalized invocation system:

1. Implement `ContractHandler` trait in `bin/dwowd/src/contract_handler/<contract>.rs`
2. Register the handler in `ContractRegistry::register_default_handlers()`
3. Add function selectors matching the contract's `define_contract_function!` macro

Example function selectors from DAO-Escrow:
- `InitializeV1` = 0x00
- `UpdateV1` = 0x01
- `PayPremiumV1` = 0x02
- `WithdrawV1` = 0x03
- `EndowmentWithdrawV1` = 0x04
- `TreasurySpendV1` = 0x05
- `EnableDrainProtectionV1` = 0x06

---

## See Also

- [Async Rust Fundamentals](../learn/dchat/async-rust-fundamentals.md)
- [Generalized Contract Invocation API](contract_invoke_api.md)
- [Testing Overview](../dev/testing/overview.md) — Four-level taxonomy
