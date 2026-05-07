# Mining and Transacting on DarkWow Testnet

This guide covers setting up solo Proof-of-Work mining on DarkWow testnet using pre-built binaries. This is different from **merge mining** (which is for mainnet with Monero+p2pool).

## Prerequisites

- Pre-built DarkWow binaries from [DarkWowMain](https://codeberg.org/PatrickM123/darkwow)
- Wallet address (generate one first)
- ~2GB RAM per mining thread

> **Conda Users**: If using conda environments, deactivate conda before running DarkWow binaries with `conda deactivate`. Conda's Python may conflict with DarkWow's native dependencies. Alternatively, use a separate venv as described in [Using dnet](../learn/dchat/network-tools/using-dnet.md).

## Step 1: Generate a Wallet

```bash
# Initialize wallet
dww -c bin/dww/dww_config.toml -n testnet wallet initialize

# Generate keypair
dww -c bin/dww/dww_config.toml -n testnet wallet keygen

# Get your address
dww -c bin/dww/dww_config.toml -n testnet wallet address
```

Save the address - you'll need it for mining rewards.

## Step 2: Create dwowd Config

Create `/home/patrick/darkfi-testnet/devnet_dwowd.config.toml`:

```toml
# DarkWow testnet configuration for mining
network = "testnet"

[network_config."testnet"]
database = "~/.local/share/dwow/dwowd/testnet"
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
p2p_datastore = "~/.local/share/dwow/dwowd/testnet"
hostlist = "~/.local/share/dwow/dwowd/testnet/p2p_hostlist.tsv"
inbound = ["tcp+tls://0.0.0.0:8342"]
external_addrs = []
peers = []
seeds = ["tcp+tls://lilith0.darkwow.org:8342", "tcp+tls://lilith1.darkwow.org:8342"]
allowed_transports = ["tcp+tls"]
localnet = false
outbound_connections = 8
```

Replace `recipient` with your wallet address from Step 1.

## Step 3: Create dww Wallet Config

Create `/home/patrick/darkfi-testnet/devnet_drk.config.toml`:

```toml
# DarkWow CLI wallet configuration for testnet
network = "testnet"
fun = true

[network_config."testnet"]
cache_path = "~/.local/share/dwow/dww/testnet/cache"
wallet_path = "~/.local/share/dwow/dww/testnet/wallet.db"
wallet_pass = "test123"
endpoint = "tcp://127.0.0.1:8340"
history_path = "~/.local/share/dwow/dww/testnet/history.txt"
```

## Step 4: Start dwowd

```bash
dwowd -c /home/patrick/darkfi-testnet/devnet_dwowd.config.toml
```

Expected output:
```
[INFO] Initializing DarkWow node...
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
14:20:06 [INFO] Starting DarkWow Mining Daemon...
14:20:06 [INFO] Initializing a new mining daemon...
14:20:06 [INFO] Mining daemon initialized successfully!
14:20:06 [INFO] Starting mining daemon...
14:20:06 [INFO] Mining daemon started successfully!
```

## Step 6: Verify Mining Connection

When dwowd connects to minerd, you'll see:
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

After blocks are mined, sync your wallet to see the DRKW tokens:

```bash
dww -c /home/patrick/darkfi-testnet/devnet_drk.config.toml scan
dww -c /home/patrick/darkfi-testnet/devnet_drk.config.toml wallet balance
```

## Troubleshooting

### RPC Connection Issues

If minerd can't connect to dwowd:
1. Verify dwowd is running: `ss -tlnp | grep 8340`
2. Check `minerd_endpoint` in dwowd config matches minerd's `rpc_listen`

### Sync Issues

If dwowd won't sync:
1. Check peer connections: dwowd logs show `[INFO] Blocks received: X/XXXX`
2. Verify seeds are reachable: `tcp+tls://lilith0.darkwow.org:8342`

### Mining Not Starting

1. Ensure dwowd is fully synced before mining
2. Check dwowd logs for: `[INFO] Received request to mine block...`
3. Verify `recipient` address is valid in dwowd config

## File Locations

| Component | Path |
|-----------|------|
| dwowd binary | `/path/to/DarkWowMain/dwow/bin/dwowd/dwowd` |
| dww binary | `/path/to/DarkWowMain/dwow/bin/dww/dww` |
| minerd binary | `/path/to/DarkWowMain/dwow/bin/minerd/minerd` |
| dwowd config | `~/.config/dwow/dwowd_config.toml` or custom |
| dww config | `~/.config/dwow/dww_config.toml` or custom |
| dwowd data | `~/.local/share/dwow/dwowd/testnet/` |
| dww wallet | `~/.local/share/dwow/dww/testnet/wallet.db` |

## Common Commands

```bash
# Check wallet balance
dww -c bin/dww/dww_config.toml -n testnet wallet balance

# List tokens
dww -c bin/dww/dww_config.toml -n testnet token list

# List contracts
dww -c bin/dww/dww_config.toml -n testnet contract list

# Deploy contract
dww -c bin/dww/dww_config.toml -n testnet contract deploy <authority> <wasm-path> [deploy-ix]

# Mint custom token
dww -c bin/dww/dww_config.toml -n testnet token generate-mint
dww -c bin/dww/dww_config.toml -n testnet token mint <token-id> <amount> <recipient>
```

## Notes

- **Solo mining vs Merge mining**: This guide uses solo PoW mining. Merge mining (with Monero+p2pool) is for mainnet and provides additional security.
- **Testnet DRKW has no value**: Tokens earned on testnet are for testing only.
- **Mining difficulty**: The `pow_target` setting affects how quickly blocks are found. Lower = easier mining.

## Local Development Setup

For contract development, use the **linear-testnet** which provides a pre-funded developer wallet and instant block mining.

### Quick Start

```bash
# 1. Build dwowd with linear-testnet support
cargo build -p dwowd

# 2. Start the 5-node linear-testnet Docker stack
cd contrib/docker/linear-testnet
./scripts/start.sh

# 3. Check dev wallet address (auto-generated)
docker logs darkfi-linear-node0 2>&1 | grep "dev_wallet"

# 4. Mine some blocks to get DRKW for fees
./scripts/mine.sh 0 100000000

# 5. Deploy a contract
dww contract deploy <dev_secret_hex> --wasm path/to/contract.wasm | broadcast
```

### Docker Stack Overview

The linear-testnet runs 5 dwowd nodes with xmrig miners:

```
node0 (seed) ── xmrig0
node1         xmrig1
node2         xmrig2  ──>  darkfi-local network
node3         xmrig3
node4         xmrig4
```

**RPC Endpoints:**
- Node0: `http://localhost:28345`
- Node1: `http://localhost:28346`
- Node2: `http://localhost:28347`
- Node3: `http://localhost:28348`
- Node4: `http://localhost:28349`

### Developer Wallet Configuration

On first startup, a developer wallet is auto-generated with 100 DRKW:

```toml
# In node0.toml (or any node config)
[network_config."linear-testnet"]
dev_wallet_secret = "generate"  # or hex-encoded secret
dev_wallet_initial_balance = 100000000000  # 100 DRKW
```

To use a specific wallet, replace `"generate"` with the hex secret key.

Mining rewards automatically go to the dev wallet unless `mining_recipient` is set.

### Verifying Your Setup

```bash
# Check dev wallet balance
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"wallet.balance","params":[],"id":1}'

# Check blockchain height
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"blockchain.height","params":[],"id":1}'

# Check peer connections
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"p2p.get_info","params":[],"id":1}'
```

### Deploying a Contract

```bash
# 1. Create a deploy authority
dww --network linear contract generate-deploy
# Output: Deploy Authority Secret: <hex>
#         Contract ID: <contract_id>

# 2. Deploy WASM (takes hex secret, not contract ID)
dww --network linear contract deploy <secret_hex> \
  --wasm path/to/my_contract.wasm \
  --deploy-ix path/to/deploy_ix.bin | broadcast

# 3. Check deployment
dww --network linear contract list
```

### Using the Rust SDK

For tests, use `LinearTestnetSdk`:

```rust
use dwow_sdk::crypto::SecretKey;
use darkfi::tests::linear_sdk::{LinearTestnetSdk, DevWalletConfig};

async fn test_contract() -> Result<()> {
    // Create SDK with funded dev wallet
    let dev_config = DevWalletConfig::new_random();
    let mut sdk = LinearTestnetSdk::with_dev_wallet(dev_config);

    // Start network (deploys genesis contracts, creates genesis block)
    sdk.start()?;

    // Dev wallet has initial DRKW
    let dev_keypair = sdk.dev_wallet.keypair();

    // Deploy contract
    let wasm = include_bytes!("../../contract/my_contract.wasm").to_vec();
    let contract_id = sdk.deploy_contract(wasm, dev_keypair.secret).await?;

    // Mine blocks (rewards to dev wallet)
    sdk.mine_blocks(5)?;

    Ok(())
}
```

### Stopping the Stack

```bash
cd contrib/docker/linear-testnet
./scripts/stop.sh

# Or just docker-compose down
docker-compose down
```

## Linear-Testnet SDK

For advanced testing, use the `LinearTestnetSdk` directly:

```rust
use darkfi::tests::linear_sdk::LinearTestnetSdk;

let sdk = LinearTestnetSdk::new();
sdk.start()?;

// Mine blocks
sdk.mine_blocks(10)?;

// Deploy contract with ZK proofs
let tx = sdk.deploy_contract_with_proofs(wasm, dev_secret).await?;
```

See [Uncle Merkle Consensus](../../arch/consensus/uncle_merkle.md) for consensus specification.
