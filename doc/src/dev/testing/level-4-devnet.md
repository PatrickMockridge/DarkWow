# Level 4: Containerized Devnet Node

A set of self-contained Docker images that turn any Linux machine into a DarkWow
devnet mining node. Pre-built images are published to Docker Hub — `docker pull`
+ `docker run` and you're mining within minutes, with zero local build
requirements.

**Location:** `contrib/docker/darkwow-testnet/`

## Image Architecture

Four images, all inheriting from a shared base:

```
darkwow-base:24.04
├── darkwow-testnet:latest     (dwowd + lilith + xmrig)
├── darkwow-monerod:latest     (monerod)
└── darkwow-p2pool:latest      (p2pool + xmrig)
```

| Image | Contents | Build method |
|-------|----------|-------------|
| `darkwow-base:24.04` | Ubuntu 24.04 + apt deps + Rust nightly + xmrig + b3sum | Pre-baked once, inherited by all others |
| `darkwow-testnet` | dwowd fullnode, lilith seed, 4 contracts — 3 genesis (deployooor, native_token, promissory_note) + 1 user-deployed (baccarat) | Source: git clone → cargo build |
| `darkwow-monerod` | monerod v0.18.5.0 | Pre-built binary (checksum-verified) |
| `darkwow-p2pool` | p2pool v4.14 | Pre-built binary (checksum-verified) |

All pre-built binaries are SHA-256 checksum-verified at build time for
reproducibility.

## Public Testnet Quick Start

The Docker image runs the **node** (dwowd). The **wallet** (`dwow_wallet`) is a separate
native binary you run on your host — it scans the blockchain, decrypts coinbase
notes, and shows your balance.

### Step 1: Build the wallet binary (host, one-time)

```bash
git clone https://codeberg.org/PatrickM123/darkwow.git
cd darkwow
cargo build -p dwow_wallet --release
DRK="./target/release/dwow_wallet"
NETWORK="darkwow-testnet"
```

### Step 2: Generate a keypair

See [Wallet Architecture](../../arch/wallet.md) for wallet initialization and keygen.
Use `-n darkwow-testnet` for the public testnet.

### Step 3: Write the secret to a secure file

```bash
echo -n "<hex-secret>" > /tmp/dwow_mining_secret
chmod 600 /tmp/dwow_mining_secret
```

### Step 4: Start the node

```bash
docker pull darkwow-testnet:latest
docker run -d --name dwow-node --network=host \
  -e ROLE=dwowd \
  -e WALLET_ADDRESS="<bs58-address>" \
  -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
  -e SEED_ADDR=lilith0.dark.fi:31340,lilith1.dark.fi:31340 \
  -e MAGIC_BYTES=68,82,75,87 \
  -v /data/dwowd:/root/.local/share/dwow/dwowd \
  -v /tmp/dwow_mining_secret:/run/secrets/mining_secret:ro \
  darkwow-testnet:latest

rm -f /tmp/dwow_mining_secret
```

### Step 5: Wait for blocks, then collect rewards

```bash
curl -s http://127.0.0.1:31345 -X POST \
  -H 'Content-Type: application/json' \
  -d '{"method":"blockchain.info","params":[],"id":1}'

$DRK -n $NETWORK scan
$DRK -n $NETWORK wallet balance
```

### Merge mining variant

Add merge mining via Monero testnet + p2pool:

```bash
# Build/pull all three images, then:
docker compose --profile join-merge up -d
```

See the [darkwow-testnet README] for the full join-testnet.sh flow,
environment variable reference, and merge mining configuration.

[darkwow-testnet README]: https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/contrib/docker/darkwow-testnet/README.md

## Network Parameters

| Parameter | Value |
|-----------|-------|
| Network name | `darkwow-testnet` |
| Magic bytes | `[68, 82, 75, 87]` ("DRKW") |
| Block time | 120 seconds |
| Initial difficulty | 255 (auto-adjusting) |
| PoW algorithm | RandomX (rx/0) |
| Consensus threshold | 3 |
| `localnet` | `false` (full TLS cert validation) |
| `skip_fees` | `false` |
| P2P seed nodes | `lilith0.dark.fi:31340`, `lilith1.dark.fi:31340` |

## Mining

xmrig mines RandomX via the local stratum server. `MINING_THREADS` defaults to `2`
(~0.5–1 kH/s on modern hardware). xmrig will saturate every thread you give it —
a machine running at full core count will become unresponsive.

| Setting | Threads | Use case |
|---------|---------|----------|
| Default | `2` | Background mining, ~0.5–1 kH/s |
| Recommended | `4` | Dedicated mining without risking lockup |
| Maximum | all physical cores | Dedicated miner, expect UI freezes |

Block reward follows an exponential-decay emission schedule starting at ~13.84 DRKW
at height 1, with a tail emission floor of ~0.80 DRKW. Total supply cap is
21,000,000 DRKW.

## Key Environment Variables

| Variable | Default | Description |
|---|---|---|
| `ROLE` | `dwowd` | `lilith` (P2P seed) or `dwowd` (fullnode) |
| `NETWORK` | `darkwow-testnet` | Network name (determines P2P isolation) |
| `P2P_PORT` | `31342` | P2P inbound listen port |
| `RPC_PORT` | `31345` | JSON-RPC port |
| `STRATUM_PORT` | `31347` | Stratum mining port |
| `SEED_ADDR` | (empty) | Comma-separated seed `host:port` for P2P bootstrap |
| `EXTERNAL_ADDR` | (empty) | Public `host:port` for internet-facing nodes |
| `MAGIC_BYTES` | auto | 4 comma-separated bytes (auto-derived from NETWORK if unset) |
| `MINING_THREADS` | `2` | xmrig thread count |
| `TARGET_BLOCK_TIME` | `120` | Block time target in seconds |
| `THRESHOLD` | `3` | Confirmation threshold in blocks |
| `LOCALNET` | `false` | P2P localnet flag |
| `WALLET_ADDRESS` | auto | Mining payout address (auto-generated if unset) |
| `WALLET_SECRET_FILE` | (empty) | Path to file containing hex-encoded secret key (preferred) |
| `MERGE_MINING` | `false` | Enable merge mining via Monero p2pool |
| `MM_RPC_PORT` | `31348` | Merge mining JSON-RPC port |
| `FINALITY_MODE` | `always` | Finality mode: `always`, `never`, or `auto` |
| `FINALITY_ENABLE_MONERO` | `false` | Enable Monero finality anchors (auto-enabled with merge mining) |
| `MONEROD_RPC_URL` | (empty) | Monero daemon JSON-RPC URL for finality verification |
| `MONERO_MIN_CONFIRMATIONS` | `3` | Minimum Monero confirmations before accepting anchor |

## Compose Profiles

For local devnet testing, the docker-compose.yml provides four profiles:

| Profile | Services | Use case |
|---------|----------|----------|
| `native` | lilith, node0, node1 | 3-node local devnet with native RandomX mining |
| `merge` | native + monerod, p2pool | 3-node local devnet with Monero merge mining |
| `bridge` | native + bridge-node | Full bridge deposit→withdraw→execute lifecycle test |
| `join-merge` | dwowd-join, monerod-join, p2pool-join | Single-node merge mining stack joining public testnet |

```bash
# Local 3-node devnet
docker compose --profile native up -d

# Local 3-node with merge mining
docker compose --profile merge up -d
```

## Data Persistence

Blockchain data is stored inside the container. Mount a host volume to persist
across restarts:

```bash
docker run --network=host \
  -v /data/dwowd:/root/.local/share/dwow/dwowd \
  ... \
  darkwow-testnet:latest
```

The hostlist file at `/data/dwowd/darkwow-testnet/hostlist.tsv` persists peer
addresses across restarts, so the node remembers peers it has connected to.

## Multi-Machine LAN Deployment

Each machine runs one container with host networking. The seed must be reachable
from all other machines.

| Machine | Role | Key env vars |
|---------|------|-------------|
| Any | Seed | `ROLE=lilith`, `EXTERNAL_ADDR=<ip>:31340` |
| Any | Miner | `ROLE=dwowd`, `SEED_ADDR=<seed-ip>:31340` |
| Any | More miners | Same as above |

All nodes must use the same `MAGIC_BYTES` so they discover each other on the P2P
network.

## Opening to the Internet

1. **Set up port forwarding** on your router for the P2P port (default 31342)
2. **Set `EXTERNAL_ADDR`** to your public IP:
   ```bash
   docker run --network=host \
     -e ROLE=dwowd \
     -e EXTERNAL_ADDR=<your-public-ip>:31342 \
     darkwow-testnet:latest
   ```
3. **External participants** join by pointing `SEED_ADDR` to your public IP

The recommended approach for external participants is the `join-testnet.sh` script,
which auto-detects the public IP and sets sensible defaults. See the
[darkwow-testnet README] for details.

## Building from Source

Pre-built images are preferred for deployment. To build from source:

```bash
# Build the base image once
./contrib/docker/darkwow-testnet/build-base.sh

# Build the main image (30–60 min)
docker build -t darkwow-testnet:latest \
  -f contrib/docker/darkwow-testnet/Dockerfile .

# Or use the build script
./contrib/docker/darkwow-testnet/build-and-push.sh
```

## Differences from Level 3

| Aspect | Level 3 (Localnet) | Level 4 (Devnet) |
|--------|-------------------|-----------------|
| Topology | Fixed 3-container on one machine | Single container per machine |
| Networking | Bridge (`dwow-local`) | Host (`--network=host`) |
| Scale | 1 machine, 3 containers | N machines, 1 container each |
| Discovery | Internal bridge DNS | Real LAN IPs |
| Internet access | Not designed for | Built-in via `EXTERNAL_ADDR` |
| Use case | Local testing | Multi-machine LAN, public testnet joining |
| Image source | Same Dockerfile as Level 4 | Pre-built or source-built |
| Compose profiles | `native`, `merge`, `bridge` | `native`, `merge`, `join-merge` |

## Dwow-Devnet Variant

A lighter variant lives at `contrib/docker/dwow-devnet/` with relaxed parameters
for rapid local iteration:

| Feature | `darkwow-testnet` | `dwow-devnet` |
|---------|-------------------|---------------|
| `localnet` | `false` | `false` |
| Magic bytes | `[68, 82, 75, 87]` | auto-derived |
| Threshold | 3 | 1 |
| `pow_target` | 120 | 120 |
| `fixed_difficulty` | auto-adjusting | 1 (instant blocks) |
| `skip_fees` | `false` | `true` |
| `skip_sync` | `false` | `true` |

Use `dwow-devnet` for fast local contract testing. Use `darkwow-testnet` when
you need parameters matching the public testnet.

## File Locations

| Component | Path |
|-----------|------|
| Base image | `contrib/docker/darkwow-testnet/Dockerfile.base` |
| Main Dockerfile | `contrib/docker/darkwow-testnet/Dockerfile` |
| Monerod Dockerfile | `contrib/docker/darkwow-testnet/Dockerfile.monero` |
| p2pool Dockerfile | `contrib/docker/darkwow-testnet/Dockerfile.p2pool` |
| Entrypoint (dwowd/lilith) | `contrib/docker/darkwow-testnet/entrypoint.sh` |
| Entrypoint (monerod) | `contrib/docker/darkwow-testnet/entrypoint-monero.sh` |
| Entrypoint (p2pool) | `contrib/docker/darkwow-testnet/entrypoint-p2pool.sh` |
| Docker Compose | `contrib/docker/darkwow-testnet/docker-compose.yml` |
| Build/push script | `contrib/docker/darkwow-testnet/build-and-push.sh` |
| Base build script | `contrib/docker/darkwow-testnet/build-base.sh` |
| Join testnet script | `contrib/docker/darkwow-testnet/join-testnet.sh` |
| Test pipeline | `contrib/docker/darkwow-testnet/test_pipeline.sh` |
| README | `contrib/docker/darkwow-testnet/README.md` |

## See Also

- [Level 3: Containerized Localnet](level-3-localnet.md) — Docker localnet architecture
- [Bootstrapping Plan](../../testnet/bootstrapping.md) — Multi-phase testnet deployment
- [DarkWow Testnet README](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/contrib/docker/darkwow-testnet/README.md) — Full env var reference and pipeline docs
- [Merge Mining](../../testnet/merge-mining.md) — Monero merge mining guide
