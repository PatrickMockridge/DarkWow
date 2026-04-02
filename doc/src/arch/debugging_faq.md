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

**Cause:**
The `SerialEncodable` and `SerialDecodable` derive macros from `darkfi-serial` internally use `async_trait::async_trait![]` when the `async` feature is enabled on `darkfi-serial`. However, the `async_trait` attribute macro must be imported into the scope where the derive is used.

**Solution:**
Add the following import to files that use `SerialEncodable` or `SerialDecodable` derives when the `async` feature is active:

```rust
#[cfg(feature = "async")]
use darkfi_serial::async_trait;
```

**Why this happens:**
DarkFi's contract architecture requires async serialization for wallet integration. When `darkfi-serial/async` is enabled (typically via `darkfi-sdk/async` or a contract's `client` feature), the serialization derives generate code that requires `async_trait` in scope.

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

If your contract uses `SerialEncodable`/`SerialDecodable` derives and enables the `client` feature, you must:

1. Add a local `async` feature that enables `darkfi-sdk/async`
2. Make your `client` feature depend on `async`
3. Add `#[cfg(feature = "async")] use darkfi_serial::async_trait;` to files using the derives

Example from `deployooor` contract:

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
error[E0195]: lifetime parameters or bounds on associated function `decode_async`
do not match the trait declaration
   --> src/sdk/src/crypto/intent_set.rs:124:56
    |
124 | #[derive(Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
    |                                                        ^^^^^^^^^^^^^^^ lifetimes do not match
```

**Cause:**
DarkFi uses the `async-trait` crate (version 0.1.x) to provide async serialization traits. The `#[async_trait]` attribute macro transforms async fn signatures into equivalent non-async functions returning boxed futures.

The bug is in how `async-trait 0.1.x` handles lifetime elision for `async fn(&self)` methods. When the trait is:

```rust
#[async_trait]
pub trait AsyncDecodable: Sized {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self>;
}
```

The generated impl uses `Pin<Box<dyn Future + Send + '_>>` with an implicit lifetime that doesn't satisfy the compiler's bounds checking on Rust 1.90+.

**Affected Components:**
- `darkfi-sdk` (src/sdk)
- `darkfi-serial` (src/serial)
- `darkfi-derive` (src/serial/derive)
- `darkfi-derive-internal` (src/serial/derive-internal)
- All smart contract integration tests when compiled with `async-serial` enabled

**Feature Chain:**
```
darkfi/validator
  └── darkfi/blockchain
        └── darkfi/tx
              └── darkfi/async-serial
                    └── darkfi-serial/async
```

**Options to Resolve:**

1. **Update to async-trait 1.0+** (requires Rust 1.75+):
   - Change `async-trait = "0"` to `async-trait = "1"` in `src/serial/Cargo.toml`
   - Remove `#[async_trait]` from trait definitions (now redundant with Rust 1.75+)

2. **Restructure darkfi Features:**
   Remove `async-serial` from the `tx` feature chain to break async serialization for tx module but allow tests to compile.

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

## See Also

- [Async Rust Fundamentals](../learn/dchat/async-rust-fundamentals.md)
- [Localnet Contract Testing](localnet_contract_testing.md)
- [Test Harness Guide](test_harness_guide.md)
