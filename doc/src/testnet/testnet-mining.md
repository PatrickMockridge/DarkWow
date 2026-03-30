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
drk wallet --initialize

# Generate keypair
drk wallet --keygen

# Get your address
drk wallet --address
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
drk -c /home/patrick/darkfi-testnet/devnet_drk.config.toml wallet --balance
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
drk wallet --balance

# List tokens
drk token list

# List contracts
drk contract list

# Deploy contract
drk contract deploy <authority> <wasm-path> <deploy-ix>

# Mint custom token
drk token generate-mint
drk token mint <token-id> <amount> <recipient>
```

## Notes

- **Solo mining vs Merge mining**: This guide uses solo PoW mining. Merge mining (with Monero+p2pool) is for mainnet and provides additional security.
- **Testnet DARK has no value**: Tokens earned on testnet are for testing only.
- **Mining difficulty**: The `pow_target` setting affects how quickly blocks are found. Lower = easier mining.
