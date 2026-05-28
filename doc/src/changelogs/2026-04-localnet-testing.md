# Localnet Contract Testing Logs (April 2026)

> **Historical**: Development testing records from 2026-04-07 to 2026-04-08.
> These are raw debugging notes, not user-facing documentation.
> See [Local Devnet Setup](../localnet-dev.md) for the current guide.

## Contract Deployment Testing (2026-04-07)

Tested WASM contract deployment on localnet.

### Successfully Deployed Contracts

| Contract | WASM Size | Status |
|----------|------------|--------|
| darktoshi_dice | 196KB | ✅ Deployed |
| baccarat | 199KB | ✅ Deployed |
| promissory_note | 496KB | ✅ Deployed (DeFi tokens contract; NativeToken handles fees/rewards) |
| escrow | 177KB | ✅ Deployed |
| lottery | 228KB | ✅ Deployed |
| roulette | 239KB | ✅ Deployed (2026-04-07) |
| betting_stake | 171KB | ✅ Deployed (2026-04-07) |
| drain_protection | 224KB | ✅ Deployed (2026-04-07) |
| bridge | 227KB | ✅ Deployed |
| darkbet_exchange | 313KB | ✅ Deployed |
| dex | 208KB | ✅ Deployed |
| pool_stake | 212KB | ✅ Deployed |
| stablecoin | 85KB | ✅ Deployed |
| relayer_endowment | 181KB | ✅ Deployed |
| dao_escrow | 169KB | ✅ Deployed (2026-04-08) |

> **Note**: Money V1 and Money V2 have been **deprecated**. Native Token is the current native token contract. DAO Escrow replaces DAO V1 for governance.

### Contracts That Previously Failed to Deploy

| Contract | WASM Size | Previous Error | Status |
|----------|------------|-------|--------|
| roulette | 375B → 239KB | Gas estimation failed | ✅ Fixed |
| betting_stake | 380B → 171KB | Gas estimation failed | ✅ Fixed |
| drain_protection | 383B → 224KB | Gas estimation failed | ✅ Fixed |
| bridge | 227KB | Deployed (tx broadcast) | Pending confirmation |
| darkbet_exchange | 313KB | Deployed (tx broadcast) | Pending confirmation |
| dex | 208KB | Deployed (tx broadcast) | Pending confirmation |
| pool_stake | 212KB | Deployed (tx broadcast) | Pending confirmation |
| stablecoin | 85KB | Deployed (tx broadcast) | Pending confirmation |
| relayer_endowment | 181KB | Deployed (tx broadcast) | Pending confirmation |
| identity | N/A | SDK API incompatibility | Needs significant refactoring |

### Common Betting Contract Bug Patterns

Roulette, betting_stake, and Lottery are concept-wise >90% the same - privacy-preserving betting games with state machines. Comparing implementations revealed **identical bug patterns** across roulette and betting_stake:

#### Bug 1: Missing `msg` Import
Both roulette and betting_stake had `msg` missing from imports:
```rust
// WRONG - msg not imported
use dwow_sdk::{wasm, ContractCall};

// CORRECT - msg imported from dwow_sdk
use dwow_sdk::{msg, wasm, ContractCall};
```

Also: `wasm::msg!` should be just `msg!` (macro is exported from dwow_sdk, not wasm module)

#### Bug 2: Error Type in TryFrom (both roulette and betting_stake)
```rust
// WRONG - TryFrom returns (), ? operator fails
let func = BettingStakeFunction::try_from(self_.data[0])?;

// CORRECT - map error to ContractError
let func = BettingStakeFunction::try_from(self_.data[0]).map_err(|_| BettingStakeError::InvalidFunction)?;
```

#### Bug 3: `process_instruction` Return Type Mismatch
```rust
// WRONG - process_instruction returned data, process_update returned ()
fn process_instruction(...) -> ContractResult {
    ...
    Ok(serialize(&update))  // Wrong!
}
fn process_update(...) -> ContractResult {
    ...
    Ok(())  // Wrong!
}

// CORRECT - process_instruction returns data for set_return_data
fn process_instruction(...) -> ContractResult {
    ...
    wasm::util::set_return_data(&serialize(&update))?;
    Ok(serialize(&update))  // Returns data
}
fn process_update(...) -> ContractResult {
    ...
    Ok(())  // process_update returns ()
}
```

#### Bug 4: State Machine Not Updated in `process_update`
```rust
// WRONG - SettleBetsUpdateV1 missing state field
pub struct SettleBetsUpdateV1 {
    pub house_new_capital: u64,
    // MISSING: pub state: RouletteTableState,
}

// CORRECT - Update struct includes state
pub struct SettleBetsUpdateV1 {
    pub house_new_capital: u64,
    pub state: RouletteTableState,  // Added
}

// WRONG - process_update didn't update table state
table.house_capital = update.house_new_capital;

// CORRECT - process_update applies state
table.house_capital = update.house_new_capital;
table.state = update.state;
```

#### Bug 5: Missing State Validation on Transitions
```rust
// WRONG - house_close didn't validate state
if params.house_pub != table.house_pub { ... }

// CORRECT - validate state before allowing operation
if table.state != RouletteTableState::Spun && table.state != RouletteTableState::Settled {
    return Err(RouletteError::InvalidTableState.into())
}
```

#### Bug 6: `wasm::msg!` vs `msg!` (betting_stake)
```rust
// WRONG - wasm::msg! doesn't exist
wasm::msg!("[betting_stake::stake] Staking {}", params.amount);

// CORRECT - msg! is exported directly from dwow_sdk
msg!("[betting_stake::stake] Staking {}", params.amount);
```

#### Bug 7: `_get_metadata` vs `get_metadata` (betting_stake)
```rust
// WRONG - underscore prefix breaks define_contract! macro lookup
fn _get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}

// CORRECT - must be exact name match
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}
```

#### Bug 8: Type Mismatches (betting_stake)
```rust
// WRONG - multiplying u64 by u32 fails
let house_edge_earnings = (params.payout_amount * table.house_edge_bp) / 10000;

// CORRECT - cast to u64
let house_edge_earnings = (params.payout_amount * table.house_edge_bp as u64) / 10000;

// WRONG - pallas::Base in format string
msg!("Stake {} created", stake_id);  // stake_id is pallas::Base

// CORRECT - don't use pallas::Base in format strings
msg!("Stake created");
```

#### Bug 9: Missing Imports (betting_stake)
```rust
// WRONG - missing pallas and PublicKey
use dwow_sdk::{crypto::ContractId, ...};

// CORRECT - need both pallas and PublicKey for helper functions
use dwow_sdk::{crypto::{poseidon_hash, ContractId, PublicKey}, pasta::pallas, ...};
```

### Bug Pattern Root Cause

All three contracts (roulette, betting_stake) were written by someone who:
1. Read SDK docs but missed the `msg` import pattern
2. Implemented logic correctly but didn't follow lottery's state machine pattern
3. Forgot that `process_update` must return `Ok(())` and state via Update structs
4. Didn't validate state transitions in transition functions
5. Used wrong module prefix (`wasm::msg!` instead of `msg!`)
6. Used underscore prefix that breaks macro lookup (`_get_metadata`)

### New Betting Contract Checklist

Before declaring a betting contract "done", verify against lottery:
1. Is `msg` imported from `dwow_sdk`?
2. Does `TryFrom` error map to `ContractError`?
3. Does `process_instruction` return `Ok(serialize(&update))` and call `set_return_data`?
4. Does `process_update` return `Ok(())`?
5. Does every `*UpdateV1` struct have a `state: StateEnum` field?
6. Does `process_update` apply `model.state = update.state`?
7. Does every state transition function validate the current state?

---

### Deployment Command

```bash
# Generate authority
dwow_wallet -c bin/drk/dww_config.toml -n localnet contract generate-deploy

# Deploy (pipe to broadcast)
dwow_wallet -c bin/drk/dww_config.toml -n localnet contract deploy <auth> <wasm> | \
 dwow_wallet -c bin/drk/dww_config.toml -n localnet broadcast
```

All three previously-failed contracts (roulette, betting_stake, drain_protection) are now deployed with proper WASM sizes (171KB-239KB).

---

## Second Wave: bridge, dex, stablecoin, darkbet_exchange, pool_stake, relayer_endowment (2026-04-07)

After fixing the first three contracts, a second wave of six contracts also failed with `ParseFailed: Requires deploy instruction`. Analysis revealed **two distinct problem patterns**:

### Problem Groups

#### Group 1: Contracts Expecting Init Data (bridge, dex, stablecoin)

These contracts use `ix: &[u8]` (no underscore) and call `.decode()` on the init data:

| Contract | Init Signature | Expected Params |
|----------|---------------|-----------------|
| bridge | `init_contract(cid, ix)` | `UpdateConfigParams { deposit_fee, withdrawal_fee, min_confirmations, max_deposit, max_withdrawal }` |
| dex | `init_contract(cid, ix)` | `InitializeParams { timeout, fee, trusted_money_merkle_root, transparency_config }` |
| stablecoin | `init_contract(cid, ix)` | `UpdateConfigParams { min_collateralization_ratio, liquidation_threshold }` |

**Root Cause**: When `dwow_wallet contract deploy <auth> <wasm>` is used without a deploy-ix file, it sends an empty `vec![]`. The `.decode()` call on empty data fails.

**Fix**: Create a properly encoded binary deploy instruction file with valid params.

#### Group 2: Contracts Using Underscore (darkbet_exchange, pool_stake, relayer_endowment)

These contracts use `_ix: &[u8]` (underscore, ignore init data) but are missing proper `db_lookup` guards:

| Contract | Trees Initialized | Issue |
|----------|-------------------|-------|
| darkbet_exchange | MARKETS, BACK_ORDERS, LAY_ORDERS, MATCHES, POSITIONS, LP_SHARES, NULLIFIERS | Missing INFO_TREE, no db_lookup guard |
| pool_stake | REGISTRY, MEMBERS, ALLOCATIONS | Missing INFO_TREE, no db_lookup guard |
| relayer_endowment | REGISTRY, DEPLOYMENTS | Missing INFO_TREE, no db_lookup guard |

**Root Cause**: `db_init` on an existing tree may fail, AND they lack the INFO_TREE pattern that working contracts (like money) have.

**Fix**: Add INFO_TREE constant and `db_lookup` guard pattern before each `db_init`.

### The db_lookup Guard Pattern

Reference: money contract uses safe redeployment pattern:

```rust
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize INFO_TREE with redeployment guard
    let _info_db = match wasm::db::db_lookup(cid, MONEY_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, MONEY_INFO_TREE)?,
    };

    // Initialize other trees with same pattern
    if wasm::db::db_lookup(cid, MONEY_CONTRACT_TREE).is_err() {
        wasm::db::db_init(cid, MONEY_CONTRACT_TREE)?;
    }
    Ok(())
}
```

**Key Pattern**: `if wasm::db::db_lookup(cid, TREE).is_err() { wasm::db::db_init(cid, TREE)?; }`

This allows the contract to be redeployed safely - if the tree already exists, `db_lookup` succeeds and we skip `db_init`.

### Code Changes Made

#### darkbet_exchange (src/contract/darkbet_exchange/src/)

**lib.rs** - Added INFO_TREE constant:
```rust
pub const DARKBET_EXCHANGE_INFO_TREE: &str = "darkbet_info";
```

**entrypoint.rs** - Added db_lookup guards:
```rust
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    let _info_db = match wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DARKBET_EXCHANGE_INFO_TREE)?,
    };
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    }
    // ... similar guards for other trees
    Ok(())
}
```

#### pool_stake (src/contract/pool_stake/src/)

**lib.rs** - Added INFO_TREE constant:
```rust
pub const POOL_STAKE_INFO_TREE: &str = "pool_stake_info";
```

**entrypoint.rs** - Added db_lookup guards similar pattern.

#### relayer_endowment (src/contract/relayer_endowment/src/)

**lib.rs** - Added INFO_TREE constant:
```rust
pub const RELAYER_ENDOWMENT_INFO_TREE: &str = "relayer_endowment_info";
```

**entrypoint.rs** - Added db_lookup guards similar pattern.

### Deploy Instruction Encoding (Group 1)

For contracts that expect init data, create binary files using `serialize()` from `dwow_serial`:

```rust
// Example: bridge deploy instruction
use dwow_serial::serialize;
let params = UpdateConfigParams {
    deposit_fee: 0,
    withdrawal_fee: 0,
    min_confirmations: 1,
    max_deposit: u64::MAX,
    max_withdrawal: u64::MAX,
};
let data = serialize(&params);
// Write to file: bridge_deploy_ix.bin
```

### Deployment Commands (Second Wave)

**Group 1** (with deploy instruction files):
```bash
AUTH=$(dwow_wallet -c bin/drk/dww_config.toml -n localnet contract generate-deploy)

# bridge
dwow_wallet -c bin/drk/dww_config.toml -n localnet contract deploy $AUTH \
  target/wasm32-unknown-unknown/release/dwow_bridge_contract.wasm bridge_deploy_ix.bin | \
 dwow_wallet -c bin/drk/dww_config.toml -n localnet broadcast
```

**Group 2** (underscore contracts, fixed with db_lookup guards):
```bash
# darkbet_exchange
dwow_wallet -c bin/drk/dww_config.toml -n localnet contract deploy $AUTH \
  target/wasm32-unknown-unknown/release/darkfi_darkbet_exchange_contract.wasm | \
 dwow_wallet -c bin/drk/dww_config.toml -n localnet broadcast
```

### Current Status (2026-04-07)

**Completed**: Code fixes applied to all 6 contracts. WASM verified at proper sizes (84KB-314KB). Deployments broadcast successfully.

**Root Cause (Refined)**: The `PositionNotMarked` error was a wallet Merkle tree sync issue, NOT a Money v1/v2 composition issue. After restarting dwowd and rescanning, deployments proceeded successfully.

**Actual Issue**: Mining instability caused shares to be rejected when block template changed. This prevented timely block confirmation of deployment transactions.

**Deployments (2026-04-07)**: All 6 contracts' transactions were broadcast:
- darkbet_exchange: d38125600ebfd718faf463f371e436c2a24c0fe483d8bac9f2ff392189409823
- pool_stake: 117d4f5aba39470ff9fea88f5cab659997c87ed7bfebd027b1868d57e9499702
- relayer_endowment: cf77ff3ea4c4cb916c1befce0281ee52ce535815e8fda043df18eed430a87ea5
- bridge: a5a3394d2dd9f95a6f8c45126e1dfd2d856671aaa58f8196b644720f3c255ab3
- dex: 5e1573f2e587c8aced4989cc3c2808a29f3af75c87497881b6b289ffb0b5daa2
- stablecoin: b0be85b5927aa5023dc95af843d178325038b98a6c031e592eba35943f45be35

**Next Steps**: Need stable mining to confirm deployment transactions.

---

## DAO Escrow Contract Implementation (2026-04-08)

### Overview

The DAO Escrow contract manages endowment funds for DAOs with three modes:
- **Escrow**: Members pay premiums to endowment, owner withdraws
- **Treasury**: DAO governance controls withdrawals
- **TreasuryEndowment**: Combination with endowment-style deposits

### Implemented Functions

| Function | Code | Description | Status |
|----------|------|-------------|--------|
| InitializeV1 | 0x00 | Create new endowment | ✅ Implemented |
| UpdateV1 | 0x01 | Update endowment parameters | ✅ Implemented |
| PayPremiumV1 | 0x02 | Member pays premium, receives membership | ✅ Implemented |
| WithdrawV1 | 0x03 | Owner withdraws from endowment | ✅ Implemented |
| EndowmentWithdrawV1 | 0x04 | DAO governance withdrawal | Stub |
| TreasurySpendV1 | 0x05 | Treasury spending | Stub |
| EnableDrainProtectionV1 | 0x06 | Enable fund drain protection | ✅ Implemented |

### Model Changes (2026-04-08)

**InitializeUpdateV1** - Added fields:
- `owner_pubkey: PublicKey` - Owner public key for withdrawal authorization
- `bulla_blind: BaseBlind` - Bulla blind factor

**PayPremiumParamsV1** - Added field:
- `member_pubkey: PublicKey` - Verified in ZK proof

**PayPremiumUpdateV1** - Added fields:
- `member_pubkey: PublicKey` - Member public key
- `token_id: pallas::Base` - Token ID
- `expiry: u64` - Membership expiry block

### Deployment Details

```
Contract ID: Dgwt5X8nWh8DFKHKjao1gvHD92DfxqzaUWyJBsGxFkVV
Transaction: aabb0985c73096c6001bcbed144f86743bf83ea9036f8b566eb2663e1e0ad56a
WASM Size: 169KB
Status: ✅ Deployed and confirmed
```

### Key Trees

| Tree | Purpose |
|------|---------|
| BULLAS_TREE | Tracks all endowment bullet |
| ENDOWMENT_TREE | Stores endowment state |
| MEMBERSHIP_TREE | Stores membership notes |
| INFO_TREE | Metadata (for redeployment guards) |

### Implementation Pattern

```rust
// Initialize - creates endowment
fn initialize_v1(cid: ContractId, params: model::InitializeParamsV1) -> ContractResult {
    let bullas_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;
    if wasm::db::db_contains_key(bullas_db, &params.dao_bulla.to_repr())? {
        return Err(DaoEscrowError::DaoEscrowAlreadyExists(...).into())
    }
    let update = model::InitializeUpdateV1 { ... };
    wasm::util::set_return_data(&serialize(&update))
}

fn initialize_apply_v1(cid: ContractId, update: model::InitializeUpdateV1) -> ContractResult {
    wasm::db::db_set(bullas_db, &update.bulla.to_repr(), &[])?;
    wasm::db::db_set(endowments_db, &update.bulla.to_repr(), &serialize(&endowment))?;
    Ok(())
}
```

---

## Money Contracts: Current Status

- **`money` (v1)**: DEPRECATED — Original DarkFi money contract
- **`money_v2`**: DEPRECATED — Replaced by promissory_note (EC heap bugs)
- **`promissory_note`**: CURRENT — DeFi token contract (ERC-20, stablecoins, wrapped tokens)
- **`native_token`**: CURRENT — Consensus-native token (PoW rewards, fees)

### Migration Path

| Old Contract | Status | Replacement |
|-------------|--------|-------------|
| Money V1 | DEPRECATED | promissory_note (DeFi) or Native Token (consensus) |
| Money V2 | DEPRECATED | promissory_note (DeFi) or Native Token (consensus) |

Native Token provides:
- Consensus-first design (fees, rewards work reliably)
- DAO-decoupled (no ACL dependencies)
- Simple genesis mint
- Minimal ZK circuit complexity

### Historical Context

The Money V1 → Money V2 migration addressed:
- `EcGetX` heap indexing bugs
- ACL-based authorization issues
- Circuit complexity problems

The Money V2 → Native Token migration addresses:
- Tight DAO coupling via ACL
- Complex multi-step token authorization
- Consensus-first design philosophy

---

## Identity Contract (2026-04-08)

### Status

The identity contract **cannot compile** - it uses SDK APIs that don't exist.

### Issues Found

**102 compilation errors** due to API incompatibility:

1. **Missing SDK modules**: Imports from non-existent paths:
   - `dwow_sdk::bridge` (doesn't exist)
   - `dwow_sdk::contract` (doesn't exist)
   - `dwow_sdk::runtime` (doesn't exist)

2. **Undefined constants**: Uses `IDENTITY_CONTRACT_*` constants that aren't defined in lib.rs

3. **Wrong API pattern**: Uses `rt.create_tree()` which doesn't exist in current SDK

### Root Cause

The identity contract was written for a **future/different SDK version** that hasn't been implemented yet. It uses APIs (`Runtime::create_tree()`, `BridgeCall::decode()`) that don't exist in the current `dwow-sdk` crate.

### Solution

The identity contract needs to be refactored to use the **current SDK API patterns**:
- Use `wasm::db::db_init()` instead of `rt.create_tree()`
- Use `wasm::util::*` instead of `Runtime::*`
- Define `IDENTITY_CONTRACT_*` constants in lib.rs following the pattern of other contracts

This is a **significant refactoring effort** - the contract's architecture would need to be adapted to match the current SDK design.

See: [Promissory Note Migration(./contract/promissory_note_migration.md)

---

## Deployment Debugging Notes (2026-04-07)

While fixing contract code was successful, deployment revealed infrastructure issues:

### Symptom: Mining Shares Rejected

```
Share rejected: {"id":1,"result":{"status":"rejected"},"jsonrpc":"2.0"}
```

**Likely Causes**:
1. Wallet database lock held by another process
2. dwowd/stratum server in bad state
3. Block template became stale

**Resolution Steps**:
```bash
# Kill all dwow_wallet mining processes
pkill -f "dwow_wallet.*mine"

# Kill dwowd
pkill dwowd

# Wait for ports to clear
sleep 2

# Restart dwowd
./target/release/dwowd -c contrib/localnet/dwowd-single-node/dwowd.toml &

# Wait for startup
sleep 3

# Check wallet (may need to reset)
./target/release/dwow_wallet -c bin/drk/dww_config.toml -n localnet wallet balance
```

### Symptom: "Failed to calculate transaction's gas"

**Cause**: Wallet had DRKW but not enough confirmed balance for gas estimation.

**Resolution**: Mine more blocks to accumulate confirmed DRKW.

### Deployment vs Code Bugs

Two distinct categories of contract failures:

1. **Code Bugs** (roulette, betting_stake, drain_protection): Fixed by correcting Rust code
2. **Deployment Bugs** (bridge, dex, stablecoin, darkbet_exchange, pool_stake, relayer_endowment): Code correct, deployment infrastructure issues

### Key Insight

Not all `ParseFailed: Requires deploy instruction` errors are the same:
- Some need actual deploy instruction data (Group 1)
- Some need the `db_lookup` guard pattern fixed (Group 2)

## File References

- `bin/dwowd/src/rpc/miner.rs` - dwowd stratum server implementation
- `bin/dwowd/src/lib.rs` - `DarkfiNode::is_localnet()` guard
- `bin/drk/src/main.rs` - Subcommand definitions and handlers
- `bin/drk/src/rpc.rs` - `miner_mine()` stratum client
- `bin/drk/dww_config.toml` - Network configuration
- `contrib/localnet/dwowd-single-node/dwowd.toml` - Localnet config
