# Local Devnet Setup

## Overview

A local development network (devnet) for DarkFi testing, funded via block mining rather than a broken faucet. Uses RandomX PoW mining against the local darkfid node's stratum server to generate DARK tokens for testing.

## Quick Start

```bash
# Terminal 1: Start darkfid with localnet config
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml

# Terminal 2: Mine blocks to your wallet
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine

# Terminal 3: Check balance
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

## How It Works

1. **darkfid** runs a stratum mining server on port 48347 (configured in localnet toml)
2. **drk mine** connects via TCP, logs in with your wallet address as recipient
3. darkfid sends mining jobs (RandomX blob + target)
4. drk mines RandomX hashes in a background thread
5. Shares found are submitted back to stratum server
6. Accepted shares = mined blocks = PoW rewards (20 DARK per block)
7. Wallet scanning discovers the coins

## Key Components

### darkfid (daemon)
- Stratum server: `127.0.0.1:48347`
- RPC endpoint: `127.0.0.1:48345`
- Config: `contrib/localnet/darkfid-single-node/darkfid.toml`
- `pow_fixed_difficulty=1` makes mining fast for testing

### drk (CLI wallet)

**Global flags:**
```
-c, --config <config>      Configuration file to use
-n, --network <network>    Blockchain network to use [default: testnet]
-f, --fun                  Flag for fun
-v                         Increase verbosity (-vvv supported)
```

**Wallet subcommands:**
```
drk wallet address            Get the default address
drk wallet addresses          Print all addresses
drk wallet balance            Query known balances
drk wallet coins              Print all coins
drk wallet default-address    Set default address
drk wallet import-secrets     Import secret keys from stdin
drk wallet initialize         Initialize wallet database
drk wallet keygen             Generate new keypair
drk wallet mining-config      Print wallet address mining configuration
drk wallet secrets            Print all secret keys
drk wallet tree               Print Merkle tree
```

**Contract subcommands:**
```
drk contract deploy <auth> <wasm-path> [deploy-ix]    Deploy a smart contract
drk contract export-data <tx-hash>                     Export wasm bincode + deploy ix
drk contract generate-deploy                          Generate new deploy authority
drk contract list [contract-id]                        List deploy authorities
drk contract lock <deploy-auth>                        Lock a smart contract
```

## Mining Details

- **Algorithm:** RandomX (rx/0)
- **Difficulty:** 1 (very low, shares found frequently)
- **Block reward:** 20 DARK per mined block
- **Blob structure:** 43 bytes = [2 byte padding][40 byte header with nonce at offset 39]
- **Target:** 8 bytes MSB of 32-byte target, padded with zeros to 32 bytes for comparison
- **Nonce:** 4 bytes at blob byte offset 39 (little-endian u32)

## Full Workflow

### 1. Initialize wallet (first time only)
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet initialize
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet keygen
```

### 2. Start localnet
```bash
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml
```

### 3. Mine blocks
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine
# Press Ctrl+C when sufficient DARK accumulated
```

### 4. Check balance
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

### 5. Scan blockchain
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet scan
# Or reset and rescan from block 0:
./target/release/drk -c bin/drk/drk_config.toml -n localnet scan --reset 0
```

### 6. List known coins
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet coins
```

### 7. Deploy a contract
```bash
# Generate deploy authority if needed
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy

# Deploy contract (pipe output to broadcast)
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract deploy <contract-id> <wasm-path> | ./target/release/drk -c bin/drk/drk_config.toml -n localnet broadcast
```

### 8. Verify deployment
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract list
```

## Network Ports

| Service | Port | Purpose |
|---------|------|---------|
| darkfid RPC | 48345 | JSON-RPC for wallet commands |
| darkfid stratum | 48347 | Stratum server for block mining |

## Troubleshooting

```bash
# If "Resource temporarily unavailable" error on wallet db:
# Kill any running drk processes
pkill -f "drk.*mine"

# Then retry wallet commands
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

## CLI Quirks

### scan is a top-level subcommand, not wallet scan

The `scan` command is not under `wallet` - it's a top-level subcommand:
```bash
drk scan                    # Correct - scan blockchain
drk wallet scan             # Wrong - this doesn't exist
```

This differs from other wallet operations which are under `drk wallet <subcommand>`.

### Config file must be passed explicitly

There is no default config file location. Every command requires `-c`:
```bash
drk -c bin/drk/drk_config.toml -n localnet wallet balance  # Correct
drk -n localnet wallet balance                              # Wrong - fails
```

### --reset uses space, not equals

The `--reset` flag for scan uses space-separated syntax:
```bash
drk scan --reset 0     # Correct - space
drk scan --reset=0    # Wrong - equals sign doesn't work
```

### broadcast reads base64 from stdin

The `broadcast` command reads a base64-encoded transaction from stdin:
```bash
drk contract deploy <auth> <wasm> | drk broadcast  # Pipe output to broadcast
```

### balance shows unspent only

`drk wallet balance` shows only unspent balances. Spent coins are not included in the balance calculation.

### coin values are in raw units

Coin values in `drk wallet coins` output are shown as raw values (e.g., `2000000000`) with a formatted version in parentheses (e.g., `(20)`). The DARK token has 8 decimal places.

### contract list without args lists all authorities

```bash
drk contract list              # Lists ALL deploy authorities
drk contract list <contract>  # Shows history for specific contract
```

The history lookup requires the deployment transaction hash (tx-hash), not the contract ID.

## Contract Deployment Testing (2026-04-07)

Tested WASM contract deployment on localnet.

### Successfully Deployed Contracts

| Contract | WASM Size | Status |
|----------|------------|--------|
| darktoshi_dice | 196KB | ✅ Deployed |
| baccarat | 199KB | ✅ Deployed |
| dao | 320KB | ✅ Deployed |
| dao_escrow | 66KB | ✅ Deployed |
| money | 496KB | ✅ Deployed |
| money_v2 | 496KB | ✅ Deployed |
| escrow | 177KB | ✅ Deployed |
| lottery | 228KB | ✅ Deployed |
| roulette | 239KB | ✅ Deployed (2026-04-07) |
| betting_stake | 171KB | ✅ Deployed (2026-04-07) |
| drain_protection | 224KB | ✅ Deployed (2026-04-07) |

### Contracts That Previously Failed to Deploy

| Contract | WASM Size | Previous Error | Status |
|----------|------------|-------|--------|
| roulette | 375B → 239KB | Gas estimation failed | ✅ Fixed |
| betting_stake | 380B → 171KB | Gas estimation failed | ✅ Fixed |
| drain_protection | 383B → 224KB | Gas estimation failed | ✅ Fixed |
| bridge | 227KB | Money v1/v2 composition | Needs Money v1 refactor (testing only) |
| darkbet_exchange | 313KB | Money v1/v2 composition | Needs Money v1 refactor (testing only) |
| dex | 208KB | Money v1/v2 composition | Needs Money v1 refactor (testing only) |
| pool_stake | 212KB | Money v1/v2 composition | Needs Money v1 refactor (testing only) |
| stablecoin | 85KB | Money v1/v2 composition | Needs Money v1 refactor (testing only) |
| relayer_endowment | 181KB | Money v1/v2 composition | Needs Money v1 refactor (testing only) |

### Common Betting Contract Bug Patterns

Roulette, betting_stake, and Lottery are concept-wise >90% the same - privacy-preserving betting games with state machines. Comparing implementations revealed **identical bug patterns** across roulette and betting_stake:

#### Bug 1: Missing `msg` Import
Both roulette and betting_stake had `msg` missing from imports:
```rust
// WRONG - msg not imported
use darkfi_sdk::{wasm, ContractCall};

// CORRECT - msg imported from darkfi_sdk
use darkfi_sdk::{msg, wasm, ContractCall};
```

Also: `wasm::msg!` should be just `msg!` (macro is exported from darkfi_sdk, not wasm module)

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

// CORRECT - msg! is exported directly from darkfi_sdk
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
use darkfi_sdk::{crypto::ContractId, ...};

// CORRECT - need both pallas and PublicKey for helper functions
use darkfi_sdk::{crypto::{poseidon_hash, ContractId, PublicKey}, pasta::pallas, ...};
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
1. Is `msg` imported from `darkfi_sdk`?
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
drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy

# Deploy (pipe to broadcast)
drk -c bin/drk/drk_config.toml -n localnet contract deploy <auth> <wasm> | \
  drk -c bin/drk/drk_config.toml -n localnet broadcast
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

**Root Cause**: When `drk contract deploy <auth> <wasm>` is used without a deploy-ix file, it sends an empty `vec![]`. The `.decode()` call on empty data fails.

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

For contracts that expect init data, create binary files using `serialize()` from `darkfi_serial`:

```rust
// Example: bridge deploy instruction
use darkfi_serial::serialize;
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
AUTH=$(drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy)

# bridge
drk -c bin/drk/drk_config.toml -n localnet contract deploy $AUTH \
  target/wasm32-unknown-unknown/release/darkfi_bridge_contract.wasm bridge_deploy_ix.bin | \
  drk -c bin/drk/drk_config.toml -n localnet broadcast
```

**Group 2** (underscore contracts, fixed with db_lookup guards):
```bash
# darkbet_exchange
drk -c bin/drk/drk_config.toml -n localnet contract deploy $AUTH \
  target/wasm32-unknown-unknown/release/darkfi_darkbet_exchange_contract.wasm | \
  drk -c bin/drk/drk_config.toml -n localnet broadcast
```

### Current Status (2026-04-07)

**Completed**: Code fixes applied to all 6 contracts. WASM verified at proper sizes (84KB-314KB).

**Blocked**: Deployment debugging halted by Money v1/v2 contract composition issue.

**Root Cause Identified**: The `drk` CLI wallet uses Money v1 for the native DARK token, but these contracts (bridge, dex, stablecoin) are written to use Money v2 ZK circuit patterns (specifically `constrain_equal_base` from `money/burn_v1.zk`).

**Error Observed**:
```
thread 'main' panicked at bin/drk/src/money.rs:1300:75:
called `Result::unwrap()` on an `Err` value: PositionNotMarked(Position(18))
```

This occurs when the wallet tries to spend a coin for gas - the Merkle tree lookup fails because DARK tokens exist on Money v1 but contracts expect Money v2's tree structure.

**Next Steps**: Refactor contracts to use Money v1 patterns instead of Money v2 patterns.

---

## Money v1 vs v2 Contract Composition Issue

### The Problem

DarkFi maintains two versions of the money contract:
- **`money` (v1)**: Original DarkFi money contract - used by `drk` CLI for DARK token
- **`money_v2`**: Secure version with self-contained circuit design - NEW standard

Contracts like bridge, dex, and stablecoin are written using Money v2 patterns:
- Use `constrain_equal_base` for binding public inputs to witnesses
- Reference `money/burn_v1.zk` patterns
- Expect Money v2 Merkle tree structure

But the `drk` CLI wallet only knows about Money v1 for the native DARK token.

### Impact

When deploying these contracts or when these contracts need to interact with DARK tokens:
1. Transaction construction uses Money v1 Merkle tree structure
2. Contract expects Money v2 structure
3. `PositionNotMarked` error occurs at Merkle tree lookup

### Files Affected

| Contract | Money Version Used | Issue |
|----------|-------------------|-------|
| bridge | v2 | Uses `constrain_equal_base` pattern from money v2 |
| dex | v2 | Uses `constrain_equal_base` pattern from money v2 |
| stablecoin | v2 | Uses `constrain_equal_base` pattern from money v2 |
| darkbet_exchange | ? | Likely same issue |
| pool_stake | ? | Likely same issue |
| relayer_endowment | ? | Likely same issue |

### Solution

**This is a TESTING accommodation, not a production recommendation.**

These contracts need to be refactored to use Money v1 patterns for localnet testing, where `drk` CLI manages DARK tokens using Money v1.

**Important caveats**:
- Money v2 is the more robust contract engineering-wise
- The v1 vs v2 question for mainnet is still an open decision
- This refactoring is purely for localnet testing compatibility
- Production deployment should revisit whether to use v1 or v2 patterns based on the network's money contract version

See: [Money Version Bridge Decision](../../arch/money-version-bridge.md)

---

## Deployment Debugging Notes (2026-04-07)

While fixing contract code was successful, deployment revealed infrastructure issues:

### Symptom: Mining Shares Rejected

```
Share rejected: {"id":1,"result":{"status":"rejected"},"jsonrpc":"2.0"}
```

**Likely Causes**:
1. Wallet database lock held by another process
2. darkfid/stratum server in bad state
3. Block template became stale

**Resolution Steps**:
```bash
# Kill all drk mining processes
pkill -f "drk.*mine"

# Kill darkfid
pkill darkfid

# Wait for ports to clear
sleep 2

# Restart darkfid
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml &

# Wait for startup
sleep 3

# Check wallet (may need to reset)
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

### Symptom: "Failed to calculate transaction's gas"

**Cause**: Wallet had DARK but not enough confirmed balance for gas estimation.

**Resolution**: Mine more blocks to accumulate confirmed DARK.

### Deployment vs Code Bugs

Two distinct categories of contract failures:

1. **Code Bugs** (roulette, betting_stake, drain_protection): Fixed by correcting Rust code
2. **Deployment Bugs** (bridge, dex, stablecoin, darkbet_exchange, pool_stake, relayer_endowment): Code correct, deployment infrastructure issues

### Key Insight

Not all `ParseFailed: Requires deploy instruction` errors are the same:
- Some need actual deploy instruction data (Group 1)
- Some need the `db_lookup` guard pattern fixed (Group 2)

## File References

- `bin/darkfid/src/rpc/miner.rs` - darkfid stratum server implementation
- `bin/darkfid/src/lib.rs` - `DarkfiNode::is_localnet()` guard
- `bin/drk/src/main.rs` - Subcommand definitions and handlers
- `bin/drk/src/rpc.rs` - `miner_mine()` stratum client
- `bin/drk/drk_config.toml` - Network configuration
- `contrib/localnet/darkfid-single-node/darkfid.toml` - Localnet config