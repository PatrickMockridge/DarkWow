# Migration Plan: money_v2 → PromissoryNote

> **Date**: 2026-04-16 (completed 2026-05)
> **Status**: Completed — money_v2 directory removed, contracts migrated to promissory_note
> **Contracts Affected**: dex, dao_escrow, game_room, subscription

---

## Overview

This document details the migration path from deprecated `money_v2` to `promissory_note` for contracts that depend on it. The migration is required because `money_v2` contains EC heap bugs in 4 of 5 circuits.

### Key Technical Difference

| Aspect | money_v2 | promissory_note |
|--------|----------|----------|
| **Function codes** | TransferV2=0x03, OtcSwapV2=0x04 | transfer_v1=0x04 |
| **Value commitment** | `pallas::Point` (EC Pedersen) | `pallas::Base` (Poseidon) |
| **Signature public** | `PublicKey` (EC point) | `pallas::Base` (Poseidon hash) |
| **Security** | EC heap bugs | Poseidon-only (safe) |

**CRITICAL**: money_v2 `TransferV2` (0x03) and promissory_note `transfer_v1` (0x04) use **different function codes**. Contracts must update validation from `0x03` to `0x04`.

---

## Phase 1: Simple Transfer Migration (dao_escrow, game_room, subscription)

These contracts use `money::TransferV2` which maps directly to `promissory_note::transfer_v1`.

### Step 1.1: Update Function Code Validation

#### dao_escrow/src/entrypoint.rs

**WithdrawV1** (lines ~379-398):
```rust
// BEFORE
if child_call.data[0] != 0x03 { ... }  // TransferV2
"[WithdrawV1] Error: Expected money::TransferV2 (0x03)"

// AFTER
if child_call.data[0] != 0x04 { ... }  // transfer_v1 (promissory_note)
"[WithdrawV1] Error: Expected promissory_note::transfer_v1 (0x04)"
```

**EndowmentWithdrawV1** (lines ~504-523):
```rust
// BEFORE
if child_call.data[0] != 0x03 { ... }
"[EndowmentWithdrawV1] Error: Expected money::TransferV2 (0x03)"

// AFTER
if child_call.data[0] != 0x04 { ... }
"[EndowmentWithdrawV1] Error: Expected promissory_note::transfer_v1 (0x04)"
```

**TreasurySpendV1** (lines ~594-613):
```rust
// BEFORE
if child_call.data[0] != 0x03 { ... }
"[TreasurySpendV1] Error: Expected money::TransferV2 (0x03)"

// AFTER
if child_call.data[0] != 0x04 { ... }
"[TreasurySpendV1] Error: Expected promissory_note::transfer_v1 (0x04)"
```

#### game_room/src/entrypoint/claim.rs

**Claim function** (lines ~63-84):
```rust
// BEFORE
if child_call.data[0] != 0x03 { ... }
"[Claim] Error: Expected money::TransferV2 (0x03)"

// AFTER
if child_call.data[0] != 0x04 { ... }
"[Claim] Error: Expected promissory_note::transfer_v1 (0x04)"
```

#### subscription/src/entrypoint.rs

**DaoControlV1** (lines ~528-548):
```rust
// BEFORE
if child_call.data[0] != 0x03 { ... }
"[DaoControlV1] Error: Expected money::TransferV2 (0x03)"

// AFTER
if child_call.data[0] != 0x04 { ... }
"[DaoControlV1] Error: Expected promissory_note::transfer_v1 (0x04)"
```

### Step 1.2: Update Error Types

#### dao_escrow/src/error.rs

```rust
// BEFORE
#[error("Invalid children indexes: expected money::TransferV2 call")]
#[error("Invalid child call: expected money::TransferV2")]

// AFTER
#[error("Invalid children indexes: expected promissory_note::transfer_v1 call")]
#[error("Invalid child call: expected promissory_note::transfer_v1")]
```

### Step 1.3: Add promissory_note Dependency

Update Cargo.toml for each contract:

#### dao_escrow/Cargo.toml
```toml
[dependencies]
dwow_promissory_note_contract = { path = "../promissory_note", optional = true }

[features]
default = []
client = [
    "darkfi",
    "darkfi-serial/async",
    "rand",
    "chacha20poly1305",
    "tracing",
    "halo2_proofs",
    "dwow_promissory_note_contract/client",  # Add this
]
```

#### game_room/Cargo.toml
Same changes as dao_escrow

#### subscription/Cargo.toml
Same changes as dao_escrow

### Step 1.4: Update Client Code

The client code that constructs child calls must use promissory_note's `TransferCallBuilder` instead of money_v2.

**Before (money_v2)**:
```rust
use dwow_money_v2_contract::client::transfer_v1::TransferCallBuilder;
// ...
let child_call = TransferCallBuilder {
    inputs: ...,
    outputs: ...,
    signature_keypair,
    proof_builder: ...,
}.build()?;
```

**After (promissory_note)**:
```rust
use dwow_promissory_note_contract::client::transfer_v1::TransferCallBuilder;
// ...
let child_call = TransferCallBuilder {
    inputs: ...,    // Now uses pallas::Base for value_commit
    outputs: ...,   // Same structure
    signature_keypair,
    proof_builder: ...,
}.build()?;
```

### Step 1.5: Verification

```bash
# Build each contract
cargo build --release -p dwow_dao_escrow_contract
cargo build --release -p dwow_game_room_contract
cargo build --release -p dwow_subscription_contract

# Run pipeline tests
CONTRACT_NAME=dao_escrow cargo test --package dwowd test_pipeline
CONTRACT_NAME=game_room cargo test --package dwowd test_pipeline
CONTRACT_NAME=subscription cargo test --package dwowd test_pipeline
```

---

## Phase 2: DEX OtcSwap Migration (Complex)

**Problem**: `promissory_note` does NOT have an `OtcSwapV2` equivalent. The DEX contract uses `OtcSwapV2` (0x04) to perform atomic token swaps between two parties.

### Option A: Implement `otc_swap_v1` in promissory_note (Recommended)

#### Step 2A.1: Add OtcSwapV1 to promissory_note

**New file**: `promissory_note/src/entrypoint/otc_swap_v1.rs`

Add to `promissory_note/src/lib.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAsProxy)]
pub enum PromissoryNoteFunction {
    // ... existing functions ...
    OtcSwapV1 = 0x05,  // NEW
}
```

Add to `promissory_note/src/entrypoint/mod.rs`:
```rust
match func {
    // ... existing matches ...
    PromissoryNoteFunction::OtcSwapV1 => money_otcswap_get_metadata_v1(cid, call_idx, calls)?,
}
```

#### Step 2A.2: Implement Circuit

The `otc_swap_v1` circuit should:
- Accept exactly 2 inputs and 2 outputs
- Enforce cross-token swap: inputs[0].token → outputs[1].token, inputs[1].token → outputs[0].token
- Enforce value equality: inputs[0].value = outputs[1].value, inputs[1].value = outputs[0].value
- Use existing `burn_v1` and `mint_v1` proof structures

**New ZK circuit file**: `promissory_note/proof/otc_swap_v1.zk`

#### Step 2A.3: Update DEX Child Call Validation

**File**: `dex/src/entrypoint/execute_swap_v1.rs` (lines ~88-100)

```rust
// BEFORE
if self_.children_indexes.len() != 2 { ... }
if child_call.data[0] != 0x04 { ... }  // OtcSwapV2
"[ExecuteSwapV1] Error: Expected 2 child calls (money::OtcSwapV2)"

// AFTER
if self_.children_indexes.len() != 2 { ... }
if child_call.data[0] != 0x05 { ... }  // OtcSwapV1 (promissory_note)
"[ExecuteSwapV1] Error: Expected 2 child calls (promissory_note::otc_swap_v1)"
```

#### Step 2A.4: Update DEX Error Types

**File**: `dex/src/error.rs`

```rust
// BEFORE
#[error("Invalid children indexes: expected money::OtcSwapV2 calls")]

// AFTER
#[error("Invalid children indexes: expected promissory_note::otc_swap_v1 calls")]
```

### Option B: Hashlock Pattern (Alternative)

If implementing OtcSwapV1 is too complex, redesign DEX to use hashlock:

1. Party A creates secret `s`, publishes `H(s)` as lock
2. Party B creates secret `r`, publishes `H(r)` as lock
3. Two separate `transfer_v1` calls bundled:
   - A's transfer to B is only claimable with `s`
   - B's transfer to A is only claimable with `r`
4. Atomicity through bundled execution

**This requires significant DEX contract redesign and is NOT recommended for initial migration.**

---

## Phase 3: DEX Integration

### Step 3.1: Add promissory_note Dependency to DEX

**dex/Cargo.toml**:
```toml
[dependencies]
dwow_promissory_note_contract = { path = "../promissory_note", optional = true }

[features]
client = [
    "darkfi",
    "darkfi-serial/async",
    "rand",
    "chacha20poly1305",
    "tracing",
    "halo2_proofs",
    "dwow_promissory_note_contract/client",  # Add this
]
```

### Step 3.2: Update Client Swap Construction

**Files**: `dex/src/client/execute_swap_v1.rs`, `accept_swap_v1.rs`

Must use promissory_note's `OtcSwapCallBuilder` (after implementation) instead of money_v2's internal logic.

### Step 3.3: Verification

```bash
cargo build --release -p dwow_dex_contract
CONTRACT_NAME=dex cargo test --package dwowd test_pipeline
```

---

## Implementation Order

```
Phase 1a: dao_escrow (3 functions need updating)
Phase 1b: game_room (1 function)
Phase 1c: subscription (1 function)
         ↓
Phase 2a: Implement otc_swap_v1 in promissory_note
Phase 2b: Update DEX child call validation
Phase 2c: Update DEX client code
         ↓
Phase 3:  Full integration testing
```

---

## Files to Modify

| File | Phase | Change |
|------|-------|--------|
| `dao_escrow/src/entrypoint.rs` | 1a | 0x03 → 0x04 validation |
| `dao_escrow/src/error.rs` | 1a | Error messages |
| `dao_escrow/Cargo.toml` | 1a | Add promissory_note dependency |
| `game_room/src/entrypoint/claim.rs` | 1b | 0x03 → 0x04 validation |
| `game_room/Cargo.toml` | 1b | Add promissory_note dependency |
| `subscription/src/entrypoint.rs` | 1c | 0x03 → 0x04 validation |
| `subscription/Cargo.toml` | 1c | Add promissory_note dependency |
| `promissory_note/src/lib.rs` | 2a | Add OtcSwapV1 function |
| `promissory_note/src/entrypoint/mod.rs` | 2a | Add otc_swap_v1 entrypoint |
| `promissory_note/proof/otc_swap_v1.zk` | 2a | New ZK circuit |
| `dex/src/entrypoint/execute_swap_v1.rs` | 2b | 0x04 → 0x05 validation |
| `dex/src/error.rs` | 2b | Error messages |
| `dex/Cargo.toml` | 2c | Add promissory_note dependency |
| `dex/src/client/*.rs` | 2c | Use promissory_note builders |

---

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Function code collision | HIGH | Explicit validation updates |
| ZK proof incompatibility | CRITICAL | Regenerate all proofs with promissory_note circuits |
| Client SDK changes | MEDIUM | Update all client libraries |
| OtcSwapV1 complexity | HIGH | Implement in promissory_note, not DEX |

---

## Testing

### Unit Tests
- Each entrypoint validates correct function code (0x04 or 0x05)
- Invalid function codes are rejected
- Malformed payloads are rejected

### Integration Tests
```bash
# Deploy promissory_note fresh (not money_v2)
# Execute transactions with child calls to promissory_note
# Verify state transitions correctly

# Heavyweight tests (full ZK proofs)
export RUST_MIN_STACK=16777216
cargo test --package dwowd --release test_dao_escrow_heavyweight
cargo test --package dwowd --release test_game_room_heavyweight
cargo test --package dwowd --release test_subscription_heavyweight
cargo test --package dwowd --release test_dex_heavyweight
```

---

## Rollback Plan

If issues are found during migration:

1. **Revert function code changes** - Switch back to 0x03/0x04
2. **Keep money_v2 deployed** alongside promissory_note temporarily
3. **Use feature flags** to toggle between money_v2 and promissory_note

---

## Success Criteria

1. All 4 contracts build successfully with promissory_note dependencies
2. All pipeline tests pass: `CONTRACT_NAME=<contract> cargo test --package dwowd test_pipeline`
3. No references to `money_v2` in contract source code (grep should return empty)
4. Heavyweight tests pass with real ZK proofs
