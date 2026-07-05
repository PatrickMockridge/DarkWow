# Steps 4-5: AccountManager Section Lookup + Pipeline Cleanup

**Status:** Planning | **Date:** 2026-07-01

## Context

Steps 1-2 extracted `AccountManager` to a shared crate (`dwow-accounts`). Both the mining node and wallet now use it. Step 3 added auto-scan to the wallet daemon. But there's a critical bug preventing wallet-2 from importing the correct key, and the pipeline has redundant key reads.

## Problem: Wallet Section Lookup Bug

**Root cause:** `AccountManager::open()` at [dwow-accounts/src/lib.rs:102](crates/dwow-accounts/src/lib.rs#L102) uses `NODE_NAME` env var to select the `keys.toml` section:

```rust
let node_name = std::env::var("NODE_NAME").unwrap_or_else(|_| "node0".into());
```

This is correct for mining nodes (NODE_NAME=node0/node1). But the wallet's `import_from_keys_toml()` at [lib.rs:1078-1108](bin/dww/src/lib.rs#L1078-L1108) passes `wallet_name` as a parameter that's **never forwarded** to `AccountManager::open()`. The wallet name is only used for the log message at line 1104.

**Impact:**
- `wallet-1`: gets `[node0]` section — works accidentally (same key as `[wallet-1]`)
- `wallet-2`: gets `[node0]` section — **WRONG** key (should get `[wallet-2]` with secret `0000...0003`)

**The Python model already has the correct API** — `AccountManager.open()` at [wallet_model.py:402](contrib/model/wallet_model.py#L402) accepts `node_name: str = "node0"` and uses it for section lookup. The Rust version needs to catch up.

## Plan

### Change 1: Add `section_name` parameter to Rust `AccountManager::open()`

**File:** `crates/dwow-accounts/src/lib.rs`, line 76

Add `section_name: Option<&str>` parameter:

```rust
pub fn open(
    db: &sled::Db,
    localnet: bool,
    keys_toml: Option<&Path>,
    network: Network,
    section_name: Option<&str>,   // NEW — overrides NODE_NAME for wallets
) -> Result<Self, String> {
```

In the keys.toml section (line 100-102), use `section_name` if provided:

```rust
let node_name = if let Some(name) = section_name {
    name.to_string()
} else {
    std::env::var("NODE_NAME").unwrap_or_else(|_| "node0".into())
};
```

### Change 2: Update all call sites

**Mining node** — [dwowd/src/lib.rs:563](bin/dwowd/src/lib.rs#L563):
```rust
// Pass None — uses NODE_NAME env var (existing behavior)
let account_mgr = crate::accounts::AccountManager::open(
    sled_db, net_settings.localnet, keys_toml, network, None,
)?;
```

**Wallet** — [dww/src/lib.rs:1089](bin/dww/src/lib.rs#L1089):
```rust
// Pass wallet_name as section_name — selects the right [wallet-N] section
let mgr = dwow_accounts::AccountManager::open(
    &self.cache.db, true, Some(path),
    dwow_sdk::crypto::keypair::Network::Testnet,
    Some(wallet_name),   // selects [wallet-N] section
)?;
```

**Tests** (4 call sites in dwow-accounts/src/lib.rs, lines 837, 851, 864, 868):
- All pass `None` — tests don't use keys.toml

### Change 3: Pipeline — remove dead `mining_secret` file write

**File:** `contrib/docker/darkwow-testnet/entrypoint.sh`, lines 260-272

The `WALLET_SECRET` env var is written to `$DATADIR/mining_secret` but nothing reads that file. `AccountManager::open()` reads keys.toml directly. Remove the dead code block.

### Change 4: Pipeline — remove redundant Python parse in phase_04

**File:** `contrib/docker/darkwow-testnet/lib/phase_04_wallet.sh`, lines 37-91

The Python script that parses keys.toml and exports `WALLET_SECRET_*` env vars is unnecessary. Docker containers read keys.toml directly via AccountManager. The env vars only feed the dead mining_secret write. Remove the Python parse block and the env var exports.

**File:** `contrib/docker/darkwow-testnet/docker-compose.yml`
Remove `WALLET_SECRET=${WALLET_SECRET_0:-}` and `WALLET_SECRET=${WALLET_SECRET_1:-}` from node0 and node1 service definitions (the env vars they reference won't exist anymore).

## What We Don't Change

- **Python model** — already correct: `AccountManager.open()` accepts `node_name` parameter
- **Mining node behavior** — unchanged: continues using NODE_NAME env var
- **keys.toml file** — unchanged
- **phase_10 redundant import** — idempotent, left as-is

## Verification

1. `cargo check -p dwow-accounts -p dwowd -p dwow_wallet` — all compile
2. `cargo test -p dwow-accounts -p dwow_wallet` — existing tests pass
3. Pipeline: wallet-1 and wallet-2 both import correct keys
4. Auto-scan discovers correct balances for both wallets
