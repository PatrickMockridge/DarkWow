# Mining and Transacting on DarkFi Testnet

This guide covers setting up solo Proof-of-Work mining on DarkFi testnet using pre-built binaries. This is different from **merge mining** (which is for mainnet with Monero+p2pool).

## Prerequisites

- Pre-built DarkFi binaries from [DarkFiMain](https://github.com/darkrenaissance/darkfi)
- Wallet address (generate one first)
- ~2GB RAM per mining thread

> **Conda Users**: If using conda environments, deactivate conda before running DarkFi binaries with `conda deactivate`. Conda's Python may conflict with DarkFi's native dependencies. Alternatively, use a separate venv as described in [Using dnet](../learn/dchat/network-tools/using-dnet.md).

## Step 1: Generate a Wallet

```bash
# Initialize wallet
drk -c bin/drk/drk_config.toml -n testnet wallet initialize

# Generate keypair
drk -c bin/drk/drk_config.toml -n testnet wallet keygen

# Get your address
drk -c bin/drk/drk_config.toml -n testnet wallet address
```

Save the address - you'll need it for mining rewards.

## Step 2: Create darkfid Config

Create `/home/patrick/darkfi-testnet/devnet_darkfid.config.toml`:

```toml
# DarkFi testnet configuration for mining
network = "testnet"

[network_config."testnet"]
database = "~/.local/share/darkfi/darkfid/testnet"
threshold = 6
minerd_endpoint = "tcp://127.0.0.1:28467"
pow_target = 120
recipient = "YOUR_WALLET_ADDRESS_HERE"
skip_sync = false
skip_fees = false
txs_batch_size = 50

[network_config."testnet".rpc]
rpc_listen = "tcp://127.0.0.1:8340"
rpc_disabled_methods = ["p2p.get_info"]

[network_config."testnet".net]
p2p_datastore = "~/.local/share/darkfi/darkfid/testnet"
hostlist = "~/.local/share/darkfi/darkfid/testnet/p2p_hostlist.tsv"
inbound = ["tcp+tls://0.0.0.0:8342"]
external_addrs = []
peers = []
seeds = ["tcp+tls://lilith0.dark.fi:8342", "tcp+tls://lilith1.dark.fi:8342"]
allowed_transports = ["tcp+tls"]
localnet = false
outbound_connections = 8
```

Replace `recipient` with your wallet address from Step 1.

## Step 3: Create drk Wallet Config

Create `/home/patrick/darkfi-testnet/devnet_drk.config.toml`:

```toml
# DarkFi CLI wallet configuration for testnet
network = "testnet"
fun = true

[network_config."testnet"]
cache_path = "~/.local/share/darkfi/drk/testnet/cache"
wallet_path = "~/.local/share/darkfi/drk/testnet/wallet.db"
wallet_pass = "test123"
endpoint = "tcp://127.0.0.1:8340"
history_path = "~/.local/share/darkfi/drk/testnet/history.txt"
```

## Step 4: Start darkfid

```bash
darkfid -c /home/patrick/darkfi-testnet/devnet_darkfid.config.toml
```

Expected output:
```
[INFO] Initializing DarkFi node...
[INFO] Node is configured to run with fixed PoW difficulty: 1
[INFO] Initializing a Darkfi daemon...
[INFO] Initializing Validator
[INFO] Initializing Blockchain
...
[INFO] Blocks received: XXXX/XXXX
...
[INFO] Blockchain synced!
```

## Step 5: Start minerd

In a separate terminal:

```bash
minerd
```

Expected output:
```
14:20:06 [INFO] Starting DarkFi Mining Daemon...
14:20:06 [INFO] Initializing a new mining daemon...
14:20:06 [INFO] Mining daemon initialized successfully!
14:20:06 [INFO] Starting mining daemon...
14:20:06 [INFO] Mining daemon started successfully!
```

## Step 6: Verify Mining Connection

When darkfid connects to minerd, you'll see:
```
[INFO] [RPC] Server accepted conn from tcp://127.0.0.1:XXXXX/
```

Mining progress:
```
[INFO] Mining block HASH for target: DIFFICULTY
[INFO] Mined block HASH with nonce: NONCE
```

If you see this error, it's normal (happens when a new block arrives):
```
[ERROR] minerd::rpc: Failed mining block HASH with error: Miner task stopped
```

## Step 7: Sync Wallet

After blocks are mined, sync your wallet to see the DARK tokens:

```bash
drk -c /home/patrick/darkfi-testnet/devnet_drk.config.toml scan
drk -c /home/patrick/darkfi-testnet/devnet_drk.config.toml wallet balance
```

## Troubleshooting

### RPC Connection Issues

If minerd can't connect to darkfid:
1. Verify darkfid is running: `ss -tlnp | grep 8340`
2. Check `minerd_endpoint` in darkfid config matches minerd's `rpc_listen`

### Sync Issues

If darkfid won't sync:
1. Check peer connections: darkfid logs show `[INFO] Blocks received: X/XXXX`
2. Verify seeds are reachable: `tcp+tls://lilith0.dark.fi:8342`

### Mining Not Starting

1. Ensure darkfid is fully synced before mining
2. Check darkfid logs for: `[INFO] Received request to mine block...`
3. Verify `recipient` address is valid in darkfid config

## File Locations

| Component | Path |
|-----------|------|
| darkfid binary | `/path/to/DarkFiMain/darkfi/bin/darkfid/darkfid` |
| drk binary | `/path/to/DarkFiMain/darkfi/bin/drk/drk` |
| minerd binary | `/path/to/DarkFiMain/darkfi/bin/minerd/minerd` |
| darkfid config | `~/.config/darkfi/darkfid_config.toml` or custom |
| drk config | `~/.config/darkfi/drk_config.toml` or custom |
| darkfid data | `~/.local/share/darkfi/darkfid/testnet/` |
| drk wallet | `~/.local/share/darkfi/drk/testnet/wallet.db` |

## Common Commands

```bash
# Check wallet balance
drk -c bin/drk/drk_config.toml -n testnet wallet balance

# List tokens
drk -c bin/drk/drk_config.toml -n testnet token list

# List contracts
drk -c bin/drk/drk_config.toml -n testnet contract list

# Deploy contract
drk -c bin/drk/drk_config.toml -n testnet contract deploy <authority> <wasm-path> [deploy-ix]

# Mint custom token
drk -c bin/drk/drk_config.toml -n testnet token generate-mint
drk -c bin/drk/drk_config.toml -n testnet token mint <token-id> <amount> <recipient>
```

## Notes

- **Solo mining vs Merge mining**: This guide uses solo PoW mining. Merge mining (with Monero+p2pool) is for mainnet and provides additional security.
- **Testnet DARK has no value**: Tokens earned on testnet are for testing only.
- **Mining difficulty**: The `pow_target` setting affects how quickly blocks are found. Lower = easier mining.

## Local Linear-Testnet SDK

For local development, you can use the `LinearTestnetSdk` to spin up a funded testnet.

### Rust SDK Usage

```rust
use darkfi_sdk::{crypto::SecretKey, pasta::pallas};
use darkfi::tests::linear_sdk::LinearTestnetSdk;

// Create SDK with a funded dev wallet
let dev_secret = SecretKey::random(&mut OsRng);
let mut sdk = LinearTestnetSdk::with_dev_wallet(DevWalletConfig::new(dev_secret, 100_000_000_000));

// Start the testnet
sdk.start()?;

// Dev wallet already has DARK from genesis
let dev_pubkey = sdk.dev_pubkey();

// Mine blocks (rewards go to dev wallet by default)
sdk.mine_blocks(10)?;

// Deploy a contract
let wasm = std::fs::read("my_contract.wasm")?;
let contract_id = sdk.deploy_contract(wasm, dev_secret).await?;
```

### Docker Setup

The Docker Compose stack auto-generates a dev wallet on first startup:

```bash
cd contrib/docker/linear-testnet

# Start the stack (dev wallet is auto-generated)
./scripts/start.sh

# Check which wallet received initial funds
docker logs darkfi-linear-node0 2>&1 | grep "dev_wallet"

# Mine some blocks to get more DARK
./scripts/mine.sh 0 100000000
```

### Configuration

In `node*.toml`, configure the dev wallet:

```toml
# Developer wallet configuration
dev_wallet_secret = "generate"  # "generate" = create new, or hex-encoded secret
dev_wallet_initial_balance = 100000000000  # 100 DARK in smallest unit

# Mining recipient (defaults to dev_wallet)
# mining_recipient = "Dz..."
```

The dev wallet receives initial DARK at genesis, and mining rewards automatically go to it unless overridden.

### Verifying Setup

```bash
# Check dev wallet balance via RPC
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"wallet.balance","params":[],"id":1}'

# Check node sync status
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"blockchain.height","params":[],"id":1}'
```

See [Uncle Merkle Consensus](../../arch/uncle_merkle.md) for consensus specification.
