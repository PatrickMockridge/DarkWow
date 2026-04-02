# Debugging FAQ and Common Issues

This document covers common issues and debugging strategies for DarkFi development, with a focus on DarkFi's unique implementation of Rust async patterns.

## Table of Contents

- [Async Serialization Issues](#async-serialization-issues)
- [Compilation Errors](#compilation-errors)
- [Runtime Issues](#runtime-issues)
- [ZK Circuit Debugging](#zk-circuit-debugging)

---

## Async Serialization Issues

DarkFi uses a custom async serialization system that can cause confusing compilation errors when not properly configured.

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

Contracts no longer need to import `async_trait` at use sites. The generated code in `src/serial/derive-internal/src/async_derive.rs` now uses `darkfi_serial::async_trait` directly.

**Related:**
- [Async Rust Fundamentals](../learn/dchat/async-rust-fundamentals.md) - Background on Rust async patterns

### Feature Flag Propagation

**Common pattern:**
When a contract enables `client` feature, it typically propagates to `darkfi-sdk/async` which enables `darkfi-serial/async`:

```toml
# In contract's Cargo.toml
[features]
client = [
    "darkfi-sdk/async",      # This enables darkfi-serial/async
    # ...
]
```

The `SerialEncodable`/`SerialDecodable` derives work automatically with the `async` feature. No additional imports are needed in contract code.

Example from a contract's Cargo.toml:

```toml
[features]
async = ["darkfi-sdk/async"]
client = [
    "darkfi",
    "async",                  # client depends on async
    "darkfi-sdk/async",
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
When `darkfi-serial/async` is enabled, the `SerialEncodable` and `SerialDecodable` derive macros generate async code that uses `#[async_trait]`. Previously, the generated code used `#[async_trait]` directly, expecting the macro to be in scope at the use site. However, `async_trait` is only available as a transitive dependency through `darkfi_serial`, not as a direct import at use sites in `darkfi_sdk`.

**Fix Applied:**
The fix was in `src/serial/derive-internal/src/async_derive.rs`:

1. Changed `#[async_trait]` to `#[#cratename::async_trait]` in generated code - this uses the fully qualified path through `darkfi_serial` where the trait is re-exported
2. Changed `s.write_all(&bytes).await?` to `#cratename::FutAsyncWriteExt::write_all(&mut *s, &bytes).await?` - uses fully qualified trait method

This makes the generated code self-contained and doesn't require contracts to import `async_trait` at use sites.

**Verification:**
```bash
cargo check -p darkfi --features "zk"  # Should compile without async_trait errors
cargo check -p darkfid  # Should compile
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

### MoneyV2 Contract Compilation Errors

The `darkfi_money_v2_contract` had several pre-existing issues preventing compilation:

**Error 1: Unresolved import `nullifier`**
```
error[E0432]: unresolved import `nullifier`
```

**Cause:** Module declared as `pub mod nullifier;` but re-exported incorrectly.

**Solution:** Use `self::` prefix for re-exports:
```rust
pub mod nullifier;
pub use self::nullifier::Nullifier;
```

**Error 2: Cannot find function `poseidon_hash`**
```
error[E0425]: cannot find function `poseidon_hash` in this scope
```

**Cause:** Import was outside the nested `crypto {}` block.

**Solution:** Ensure imports are in correct scope:
```rust
use darkfi_sdk::crypto::{
    // imports must be inside nested crypto block
    poseidon_hash,
    // ...
};
```

**Error 3: lazy_static ordering**
The `TokenId` struct must be defined before `DARK_TOKEN_ID` lazy_static that references it. Reorder so struct comes first.

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

DarkFi provides a generalized `contract.invoke` RPC endpoint for invoking any smart contract without requiring new API methods per function. See [Generalized Contract Invocation API](contract_invoke_api.md) for details.

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
- `contract.invoke` endpoint is implemented in `bin/darkfid/src/rpc/contract.rs`
- `ContractHandler` trait and `ContractRegistry` in `bin/darkfid/src/contract_registry.rs`
- DAO-Escrow handler with function selectors (0x00-0x06)
- Full ZK proof generation and transaction broadcasting is TODO

### Contract Handler Pattern

To add a new contract to the generalized invocation system:

1. Implement `ContractHandler` trait in `bin/darkfid/src/contract_handler/<contract>.rs`
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
- [Localnet Contract Testing](localnet_contract_testing.md)
- [Test Harness Guide](test_harness_guide.md)
- [Test Harness Guide](test_harness_guide.md)
