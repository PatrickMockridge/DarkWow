# Mining and Transacting on DarkWow Testnet

> **See also:** [Level 3: Containerized Localnet](../dev/testing/level-3-localnet.md)
> for Docker-based multi-node testnet mining, and the
> [darkwow-testnet README](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/contrib/docker/darkwow-testnet/README.md)
> for the containerized devnet with pre-configured wallet and xmrig mining.

This guide covers joining the DarkWow testnet (`darkwow-testnet`) as a mining
node and using the wallet to transact. Mining uses **xmrig** connecting to
dwowd's built-in stratum server (RandomX `rx/0`).

For **merge mining** with Monero+p2pool, see [merge-mining.md](merge-mining.md).

**Scope boundary:** DarkWow-native mining pools (DRKW reward distribution
without Monero merge mining) are an ecosystem concern — this repo provides
the node software; pool protocols and reward distribution schemes use the
same stratum interface but are not bundled here.

## Prerequisites

- Built binaries: `dwowd`, `dww` (`cargo build -p dwowd -p dww --release`)
- xmrig installed (`xmrig` in PATH)

## Step 1: Generate a Wallet

See [Wallet Architecture](../arch/wallet.md) for initialization, keygen, and
address retrieval. Save the address — you'll need it for mining rewards.

## Step 2: Create dwowd Config

Create `dwowd_config.toml`:

```toml
# DarkWow testnet configuration for mining
network = "darkwow-testnet"

[network_config."darkwow-testnet"]
database = "~/.local/share/dwow/dwowd/darkwow-testnet"
threshold = 3
pow_target = 120
recipient = "YOUR_WALLET_ADDRESS"
skip_sync = false
skip_fees = false
txs_batch_size = 50

[network_config."darkwow-testnet".rpc]
rpc_listen = "tcp://127.0.0.1:31345"

[network_config."darkwow-testnet".stratum_rpc]
rpc_listen = "tcp://127.0.0.1:31347"

[network_config."darkwow-testnet".finality]
# Finality mode: "always" (default) | "native" | "signaled"
# mode = "always"
# Enable Caribina Arweave anchoring (default: true)
# caribina_enabled = true
# Enable Monero p2pool anchoring (default: false, requires p2pool)
# monero_enabled = false
# Monero confirmations required before finality (default: 3)
# monero_min_confirmations = 3
# monerod JSON-RPC URL for full anchor verification (optional)
# monerod_url = "http://127.0.0.1:18081/json_rpc"

[network_config."darkwow-testnet".net]
localnet = false
active_profiles = ["tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:31342"]
hostlist = "~/.local/share/dwow/dwowd/darkwow-testnet/hostlist.tsv"
seeds = ["tcp+tls://lilith0.dark.fi:31340", "tcp+tls://lilith1.dark.fi:31340"]
allowed_transports = ["tcp+tls"]
outbound_connections = 8
```

Replace `recipient` with your wallet address from Step 1.

The `[finality]` section enables Caribina Arweave anchoring by default —
every mined block is timestamped on Arweave and cannot be reorganized.
Set `mode = "native"` or pass `--finality-mode native` to disable all
finality (useful for local testing where Arweave HTTP calls add latency).
Monero p2pool anchoring can be enabled with `monero_enabled = true` or
`--finality-enable-monero` for merge mining setups.

## Step 3: Create dww Wallet Config

Create `dww_config.toml`:

```toml
# DarkWow CLI wallet configuration
network = "darkwow-testnet"

[network_config."darkwow-testnet"]
wallet_path = "~/.local/share/dwow/dww/darkwow-testnet/wallet.db"
wallet_pass = "test123"
endpoint = "tcp://127.0.0.1:31345"
```

## Step 4: Start dwowd

```bash
dwowd -c dwowd_config.toml
```

Expected output:
```
[INFO] Initializing DarkWow node...
[INFO] Node is configured to run with PoW mining
[INFO] Initializing Validator
[INFO] Initializing Blockchain
...
[INFO] Blocks received: XXXX/XXXX
...
[INFO] Blockchain synced!
```

## Step 5: Start xmrig Mining

In a separate terminal:

```bash
xmrig -o stratum+tcp://127.0.0.1:31347 \
      -u YOUR_WALLET_ADDRESS \
      -a rx/0 \
      -t 1
```

When dwowd receives a connection from xmrig, you'll see:
```
[INFO] [RPC] Server accepted conn from tcp://127.0.0.1:XXXXX/
```

Mining progress:
```
[INFO] Mining block HASH for target: DIFFICULTY
[INFO] Mined block HASH with nonce: NONCE
```

If you see this error, it's normal (happens when a new block arrives and the
current mining task is cancelled):
```
[ERROR] Mining block HASH failed: Miner task stopped
```

## Step 6: Sync Wallet

After blocks are mined, sync your wallet to see DRKW tokens:

```bash
dww -c dww_config.toml scan
dww -c dww_config.toml wallet balance
```

## Docker-Based Mining

The quickest way to get a mining node running is with the containerized testnet.
All builds and tests go through a single pipeline entry point:

```bash
cd contrib/docker/darkwow-testnet

# Local 3-node devnet (build + start + verify)
./test_pipeline.sh --mode native
./test_pipeline.sh --mode merge

# Join public testnet as a single node (build + validate + verify)
./test_pipeline.sh --mode join-native
./test_pipeline.sh --mode join-merge

# Join the public testnet for real (no verification — just launch)
./join-testnet.sh --mode native
./join-testnet.sh --mode merge
```

`test_pipeline.sh` is the single entry point for all builds and tests. Run
`./test_pipeline.sh --help` for full documentation of all 5 modes, phases,
and environment variables. Every phase runs sequentially — one thing at a time
for reproducible results.

`join-testnet.sh` launches the actual node without the test harness. Use it
after the pipeline passes. See `join-testnet.sh --help` for all options.

The node remembers peers across restarts via a persistent hostlist file in its
data directory — mount a volume to preserve it.

See the [darkwow-testnet README](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/contrib/docker/darkwow-testnet/README.md) for full details.

## Troubleshooting

### RPC Connection Issues

If xmrig can't connect to dwowd:
1. Verify dwowd is running: `ss -tlnp | grep 31347`
2. Check stratum is enabled in dwowd config: `[network_config."darkwow-testnet".stratum_rpc]`

### Sync Issues

If dwowd won't sync:
1. Check peer connections: dwowd logs show `[INFO] Blocks received: X/XXXX`
2. Verify seeds are reachable

### Mining Not Starting

1. Ensure dwowd is fully synced before mining will produce blocks
2. Check dwowd logs for stratum connection from xmrig
3. Verify `recipient` address is valid in dwowd config

## File Locations

| Component | Path |
|-----------|------|
| dwowd binary | `target/release/dwowd` |
| dww binary | `target/release/dww` |
| dwowd config | `~/.config/dwow/dwowd_config.toml` or custom |
| dww config | `~/.config/dwow/dww_config.toml` or custom |
| dwowd data | `~/.local/share/dwow/dwowd/darkwow-testnet/` |
| dww wallet | `~/.local/share/dwow/dww/darkwow-testnet/wallet.db` |

## Common Commands

```bash
# Check wallet balance
dww wallet balance

# Scan for received coins
dww scan

# Send a transfer
dww transfer <amount> <recipient_address> | broadcast

# List contracts
dww contract list

# Deploy a contract via Deployooor
dww contract deploy <wasm_path>
```

## Notes

- **Testnet DRKW has no value**: Tokens earned on testnet are for testing only.
- **docker compose vs test_pipeline.sh**: `docker compose up` gives a running
  stack; `test_pipeline.sh --mode <mode>` adds health checks, block production
  verification, and automated teardown.
