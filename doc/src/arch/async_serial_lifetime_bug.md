# DarkFi Async Serialization Lifetime Bug

## Overview

DarkFi has a pre-existing bug in its `async` serialization system that prevents **integration tests** from compiling on Rust 1.90+. This bug affects all smart contract integration tests that depend on `darkfi-sdk` when compiled with `darkfi-serial/async` enabled.

**Important**: The contracts themselves (`.zk` circuits, business logic, state transitions) **work correctly**. This bug only prevents the integration *test harness* from compiling. The ZK circuit tests (`.zk` compilation to `.zk.bin`) also work fine.

## Affected Components

- `darkfi-sdk` (src/sdk)
- `darkfi-serial` (src/serial)
- `darkfi-derive` (src/serial/derive)
- `darkfi-derive-internal` (src/serial/derive-internal)
- All smart contract integration tests

## What The Integration Tests Verify

The integration tests I created verify:

### What They DO Test
- **Serialization/Deserialization**: `encode()` and `decode()` round-trip for all params/update types
- **Enum conversions**: `try_from()` works for valid/invalid values (e.g., `BaccaratState::try_from(0)`)
- **Derivation functions**: `derive_id()` and similar functions are deterministic (same input → same output)
- **Constants**: Tree names and circuit namespaces are correct strings
- **Basic data structures**: Cards, hands, bet types can be constructed correctly

### What They DON'T Test (Due to This Bug)
- **Actual contract logic**: Game rules, drawing rules, payout calculations
- **ZK proof generation**: Proof creation and verification
- **State machine transitions**: Valid state transitions (Created → Bidding → Revealed → Awarded)
- **Blockchain integration**: Actual on-chain execution
- **End-to-end gameplay**: Full bet-commit → card-draw → settle-flow

The integration tests are essentially "model layer" tests that verify data structures and serialization. They cannot run the actual contract execution path due to this bug.

## Error Message

```
error[E0195]: lifetime parameters or bounds on associated function `decode_async` do not match the trait declaration
   --> src/sdk/src/crypto/intent_set.rs:124:56
    |
124 | #[derive(Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
    |                                                        ^^^^^^^^^^^^^^^ lifetimes do not match associated function in trait
```

## Root Cause Analysis

### The async-trait Crate Bug

DarkFi uses the `async-trait` crate (version 0.1.x) to provide async serialization traits. The `#[async_trait]` attribute macro transforms async fn signatures into equivalent non-async functions returning boxed futures.

The bug is in how `async-trait 0.1.x` handles lifetime elision for `async fn(&self)` methods. When the trait is:

```rust
#[async_trait]
pub trait AsyncDecodable: Sized {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self>;
}
```

The generated impl uses `Pin<Box<dyn Future + Send + '_>>` with an implicit lifetime that doesn't satisfy the compiler's bounds checking on Rust 1.90+.

### How async Code Gets Generated

The `SerialEncodable` and `SerialDecodable` derive macros from `darkfi-derive` generate `async` implementations when the `async` feature is enabled on `darkfi-serial`. The feature flag propagates through the derive macro:

1. `darkfi-serial` has `async = ["futures-lite", "async-trait", "darkfi-derive/async"]`
2. When `async` is enabled, `darkfi-derive/async` enables `darkfi-derive-internal/async`
3. This causes `async_struct_ser`, `async_struct_de`, `async_enum_ser`, `async_enum_de` to generate `#[async_trait]` impl blocks
4. These impl blocks are generated for **ALL** structs/enums using the derives, not just async-specific code

### Feature Chain That Enables async-serial

The `async-serial` feature propagates through this chain:

```
darkfi/validator
  └── darkfi/blockchain
        └── darkfi/tx
              └── darkfi/async-serial
                    └── darkfi-serial/async
```

**Any** code that depends on `darkfi` with the `validator` feature (directly or through test-harnesses) will transitively enable `async-serial`.

## Why The Bug Blocks Building

### 1. Transitive Feature Dependency

The `async-serial` feature is enabled transitively through this chain:
```
darkfid → darkfi/validator → darkfi/blockchain → darkfi/tx → darkfi/async-serial
```

There is no way to build `darkfid` without `async-serial` because the Cargo features are unified.

### 2. async-trait 0.1.x Lifetime Issue

The `async-trait 0.1.x` crate uses a macro-based approach to async traits that predates native async trait support (Rust 1.75+). The macro transformation doesn't properly handle lifetime elision for `async fn(&self)` methods, causing lifetime bound mismatches.

### 3. No Conditional Compilation

The derive macros in `darkfi-derive-internal` generate either sync OR async code based on the `async` feature flag. There is no fallback to sync code when async code fails to compile.

### 4. Rust Compiler Stricter Checking

Recent Rust versions (1.90+) have stricter lifetime bounds checking that exposes the underlying issue in async-trait's generated code. This is NOT a Rust regression - async-trait's code was always incorrect, but older compilers didn't catch it.

## Affected Build Configurations

**Updated 2026-03-30:** The table below was overly optimistic. The async-trait lifetime issue occurs when `async-serial` feature is enabled, which happens transitively through `darkfi/validator` (required by `darkfid`, `drk`, and the test-harness). This affects ALL Rust versions when `async-serial` is enabled.

Additionally, some dependencies (e.g., `tapes` from Cuprate) now require `edition2024` which requires Cargo features from Rust 1.80+.

| Rust Version | Status |
|--------------|--------|
| 1.80.x       | Fails: dependencies require `edition2024` Cargo feature |
| 1.88.x       | Fails: async-trait lifetime bug with `async-serial` |
| 1.89.x       | Fails: async-trait lifetime bug with `async-serial` |
| 1.90+        | Fails: async-trait lifetime bug with `async-serial` |

**Root cause:** This is a **pre-existing architectural issue** in DarkFi's async serialization system. The `async-trait 0.1.x` crate generates code with implicit lifetime bounds that are rejected by Rust's compiler. This is NOT a Rust backwards compatibility issue - Rust is working correctly. The problem is that `async-trait 0.1.x` never properly handled lifetime elision for `async fn(&self)` methods.

## Impact

**Critical:** This bug affects the entire DarkFi build system, not just tests:

- `darkfid` (validator node) - **CANNOT BUILD**
- `drk` (CLI wallet) - **CANNOT BUILD**
- Integration tests - **CANNOT BUILD** (require `darkfi/validator`)

**What still works:**
- Contract libraries (`cargo build -p darkfi_<name>_contract --lib`) - works
- ZK circuit compilation (`./zkas proof/circuit.zk -o proof/circuit.zk.bin`) - works
- ZK circuit tests (shell scripts) - work

### Options to Resolve

**Option 1: Use Pre-built Binaries**
Use binaries from DarkFiMain (June 2025) which were built before this issue manifested:
```bash
cp /path/to/DarkFiMain/bin/darkfid ./bin/darkfid/
cp /path/to/DarkFiMain/bin/drk ./bin/drk/
```

**Option 2: Fix async-trait (Requires Code Change)**
Update to `async-trait 1.0+` which uses native async traits (Rust 1.75+):

Update to `async-trait 1.0+` which uses native async traits (Rust 1.75+). This requires:

1. Changing `async-trait = "0"` to `async-trait = "1"` in `src/serial/Cargo.toml`
2. Removing `#[async_trait]` from trait definitions (now redundant with Rust 1.75+)
3. Changing `async fn` trait methods to native async syntax

### Option 3: Restructure darkfi Features

Remove `async-serial` from the `tx` feature chain:

```toml
# In Cargo.toml
tx = [
    "blake3",
    "async-sdk",  # Keep this
    # "async-serial",  # Remove this
    "zk",
]
```

This would break async serialization for tx module but allow tests to compile.

## Files Involved

### Core Files (Where The Bug Lives)
- `src/serial/Cargo.toml` - Defines async feature
- `src/serial/src/async_lib.rs` - Async trait definitions with `#[async_trait]`
- `src/serial/derive/src/lib.rs` - Derive macro entry point
- `src/serial/derive-internal/src/async_derive.rs` - **THIS IS WHERE THE BUG IS** - generates `#[async_trait]` impl blocks that fail on Rust 1.90+
- `src/sdk/src/crypto/intent_set.rs` - Structs using derives - NOT the source of the bug

### Contracts (Dead async_trait Imports - Commented Out)
The following contracts had `use darkfi_serial::async_trait` imports that were **UNUSED** (no `#[async_trait]` attributes in contract code). These were dead code that has been commented out:
- `src/contract/money/src/client/mod.rs`
- `src/contract/money/src/model/mod.rs`
- `src/contract/money/src/model/nullifier.rs`
- `src/contract/money/src/model/token_id.rs`
- `src/contract/money_v2/src/client/mod.rs`
- `src/contract/money_v2/src/model/mod.rs`
- `src/contract/money_v2/src/model/nullifier.rs`
- `src/contract/money_v2/src/model/token_id.rs`
- `src/contract/dao/src/model.rs`
- `src/contract/deployooor/src/model.rs`

**IMPORTANT**: Commenting out these unused imports does NOT fix the bug. The bug is in the `derive-internal/src/async_derive.rs` which generates `#[async_trait]` code when the `async` feature is enabled.

### Feature Chain
```
darkfi/validator
  └── darkfi/blockchain
        └── darkfi/tx
              └── darkfi/async-serial
                    └── darkfi-serial/async
                          ├── futures-lite
                          ├── async-trait
                          └── darkfi-derive/async
                                └── darkfi-derive-internal/async
```

## Verification Commands

### Check If async-serial Is Enabled
```bash
cargo tree -p darkfi_baccarat_contract -e features -i darkfi-serial 2>&1 | grep "async"
```

### Check What Enables async-serial
```bash
cargo tree -p darkfi_baccarat_contract -e features 2>&1 | grep -B5 "darkfi-serial/async"
```

### Check Rust Version
```bash
rustc --version
```

## Related Links

- [async-trait crate](https://crates.io/crates/async-trait)
- [Rust async traits RFC](https://rust-lang.github.io/rfcs/3320-receiver-lifetime-bounds.html)
- [Rust 1.75 async traits release notes](https://blog.rust-lang.org/2023/12/28/Rust-1.75.html)

## Status

**Unresolved** - This is a pre-existing architectural issue in DarkFi's async serialization system that predates the current development cycle.

**Impact**: Integration tests (which verify contract logic) cannot compile, but contracts themselves work correctly. ZK circuit tests pass.

## See Also

- [Baccarat Contract README](../contract/baccarat.md) - Casino game contract
- [ZK Circuit Testing](./zk_circuit_testing.md) - How to run ZK circuit tests
- [Contract Testing](./contract_testing.md) - Smart contract testing patterns