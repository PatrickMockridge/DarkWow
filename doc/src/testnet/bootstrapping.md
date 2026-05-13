# DarkWow Public Testnet Bootstrapping Plan

This page documents how to bootstrap the DarkWow public testnet — from a single
machine to a multi-machine LAN network to an internet-facing testnet with
external participants.

## How It Works

The P2P bootstrap is simple:

1. **Nodes connect to a seed** — any lilith seed works. DarkWow uses the public
   dark.fi lilith seeds (`lilith0.dark.fi:31340`, `lilith1.dark.fi:31340`) that
   are already configured in `dwowd_config.toml`.
2. **Nodes discover each other** — the seed maintains a hostlist keyed by network
   magic bytes (`[68, 82, 75, 87]` = "DRKW"). Nodes with matching magic bytes
   discover each other through the seed.
3. **Nodes mesh directly** — after discovery, nodes form direct P2P connections.
   The seed is only the rendezvous point; block and transaction propagation is
   peer-to-peer.
4. **Nodes mine and broadcast** — xmrig mines RandomX via local stratum. Found
   blocks propagate to all connected peers.

## Network Parameters

| Parameter | Value |
|-----------|-------|
| Network name | `darkwow-testnet` |
| Magic bytes | `[68, 82, 75, 87]` ("DRKW") |
| Block time | 120 seconds |
| Initial difficulty | 255 (auto-adjusting) |
| PoW algorithm | RandomX (rx/0) |
| Consensus threshold | 3 |
| P2P seed port | 31340 |
| RPC port | 31345 |
| Stratum port | 31347 |
| Seed nodes | `lilith0.dark.fi:31340`, `lilith1.dark.fi:31340` |

## Phase 1: Single-Machine Validation

Start with the 3-container Docker setup to validate the full pipeline on one
machine before going multi-machine.

```bash
# From the repo root
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml up -d

# Wait for containers to start and mining to begin (~30s), then check block height
curl -s http://127.0.0.1:31345 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"blockchain.info","params":[],"id":1}'

# Check P2P connections on node0
curl -s http://127.0.0.1:31345 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"p2p.info","params":[],"id":1}'

# Tear down when done
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml down
```

**Success criteria:** All 3 containers running, blocks produced every ~120s, both
mining nodes at the same block height.

## Phase 2: Multi-Machine LAN

Deploy miners on spare machines across the LAN. No dedicated seed needed — the
miners use the public dark.fi lilith seeds for bootstrapping.

On each machine, first build or pull the image:

```bash
# Build from source
docker build -t darkwow-testnet:latest \
    -f contrib/docker/darkwow-testnet/Dockerfile .
```

Then start the miner:

```bash
docker run -d --name dwow-node --network=host \
    -e ROLE=dwowd \
    -e NETWORK=darkwow-testnet \
    -e P2P_PORT=31342 \
    -e RPC_PORT=31345 \
    -e STRATUM_PORT=31347 \
    -e MAGIC_BYTES=68,82,75,87 \
    -e MINING_THREADS=<cores> \
    -e THRESHOLD=3 \
    -e TARGET_BLOCK_TIME=120 \
    -e EXTERNAL_ADDR=<this-machine-ip>:31342 \
    -v /data/dwowd:/root/.local/share/dwow/dwowd \
    darkwow-testnet:latest
```

Replace `<cores>` with the machine's CPU thread count and `<this-machine-ip>`
with the machine's LAN IP address.

### Verify the Mesh

```bash
# On any miner — check P2P connections
curl -s http://127.0.0.1:31345 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"p2p.info","params":[],"id":1}'

# Check block height — should be identical across all nodes
curl -s http://127.0.0.1:31345 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"blockchain.info","params":[],"id":1}'

# Check mining activity
docker logs dwow-node 2>&1 | grep -i "accepted\|new job"
```

If nodes show each other as peers and block height increases in lockstep, the
P2P mesh is working.

### Wallet Setup

**Recommended — Pre-configured wallet (one-step):**

Generate a keypair on the host before starting the miner. Pass the secret via a
bind-mounted file (not an env var) to avoid exposing it in `docker inspect`.
Coinbase rewards flow directly to your wallet — no manual extraction needed.

```bash
# Generate a keypair for mining rewards
./target/release/dww -n darkwow-testnet wallet keygen
# Output: address (bs58) and secret (hex)

# Write the secret to a secure temp file
echo -n "<hex-secret>" > /tmp/dwow_mining_secret
chmod 600 /tmp/dwow_mining_secret

# Pass address as env var, secret via file mount
docker run -d --name dwow-node --network=host \
    -e ROLE=dwowd \
    -e NETWORK=darkwow-testnet \
    -e WALLET_ADDRESS="<bs58-address>" \
    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
    -v /tmp/dwow_mining_secret:/run/secrets/mining_secret:ro \
    ... \
    darkwow-testnet:latest

# Clean up the temp file
rm -f /tmp/dwow_mining_secret

# Wallet already has the key — just scan
./target/release/dww -n darkwow-testnet scan
./target/release/dww -n darkwow-testnet wallet balance
```

**Alternative — Auto-generated keypair:**

If no `WALLET_SECRET` or `WALLET_SECRET_FILE` is provided, the daemon
auto-generates a random keypair on first startup. The secret exists only inside
the container's datadir. Mining rewards are unspendable until the secret is
imported into a wallet.

The `docker exec cat mining_secret` pattern is **not recommended** for
production — it exposes the secret in shell history and treats the container
filesystem as a secrets store. Pre-generate a keypair and provision it via
SSH or config management instead.

## Phase 3: Internet Expansion

Once the LAN mesh is confirmed, open the testnet to external participants.

### Option A: Port-Forward a Miner

Forward TCP port 31342 on one miner's router to its LAN IP. Set `EXTERNAL_ADDR`
to the public IP. External participants use that address as their seed.

### Option B: Run a Dedicated Public Seed

Deploy a lilith container on a machine with port forwarding:

```bash
docker run -d --name dwow-lilith --network=host \
    -e ROLE=lilith \
    -e NETWORK=darkwow-testnet \
    -e P2P_PORT=31340 \
    -e EXTERNAL_ADDR=<public-ip>:31340 \
    -e MAGIC_BYTES=68,82,75,87 \
    -v /data/lilith:/root/.local/share/dwow/lilith \
    darkwow-testnet:latest
```

External participants then set `SEED_ADDR=<public-ip>:31340`.

### External Participant Quick Start

**Recommended — use join-testnet.sh:**

```bash
# Native mining (solo)
./contrib/docker/darkwow-testnet/join-testnet.sh --mode native

# Merge mining with Monero testnet
./contrib/docker/darkwow-testnet/join-testnet.sh --mode merge
```

The script auto-detects your public IP and sets sensible defaults. See
`join-testnet.sh --help` for all options.

**Test the join before going live:**

```bash
# Verify the join works end-to-end before deploying for real
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-native
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-merge
```

Run `./test_pipeline.sh --help` for full documentation of all modes, phases,
and environment variables. The pipeline builds the image, validates config,
tests the seed fallback, and verifies mining — all sequentially, one phase
at a time, for reproducible results.

**Manual docker run:**

```bash
docker run -d --name dwow-node --network=host \
    -e ROLE=dwowd \
    -e NETWORK=darkwow-testnet \
    -e P2P_PORT=31342 \
    -e RPC_PORT=31345 \
    -e STRATUM_PORT=31347 \
    -e SEED_ADDR=lilith0.dark.fi:31340,lilith1.dark.fi:31340 \
    -e EXTERNAL_ADDR=<my-public-ip>:31342 \
    -e MAGIC_BYTES=68,82,75,87 \
    -e MINING_THREADS=<cores> \
    -e THRESHOLD=3 \
    -e TARGET_BLOCK_TIME=120 \
    -v /data/dwowd:/root/.local/share/dwow/dwowd \
    darkwow-testnet:latest
```

The hostlist file at `/data/dwowd/hostlist.tsv` persists peer addresses across
restarts, so the node remembers peers it has connected to.

### Monitoring

- Health check: `curl http://<node>:31345 -X POST -H 'Content-Type: application/json' -d '{"method":"blockchain.info","params":[],"id":1}'`
- Docker log rotation: configure `max-size` and `max-file` in the Docker daemon or use `--log-opt` on `docker run`
- Data backup: periodic snapshots of `/data/dwowd` and `/data/lilith`

## Phase 4: Merge Mining (Optional)

Once the native mining testnet is stable, add Monero merge mining:

```bash
# Join with merge mining (single node, connects to Monero testnet)
./contrib/docker/darkwow-testnet/join-testnet.sh --mode merge

# Or test the join end-to-end first
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-merge

# Or for local testing: full 3-node devnet with merge mining
MERGE_MINING=true docker compose --profile merge up -d
```

This starts monerod syncing the public Monero testnet, p2pool bridging to dwowd's
mm_rpc, and xmrig mining through p2pool. See [Merge Mining](merge-mining.md) for
the full guide.

## Phase 5: Ongoing Operations

- **Contract deployments**: Deploy the extended contract suite (money_v3, DEX,
  dao_escrow, etc.) to the live testnet
- **Block explorer**: Index the testnet with `bin/explorer/`
- **Faucet**: Distribute testnet DRKW to external participants
- **Documentation**: Publish live seed addresses and network status

## See Also

- [Running a Node](node.md) — Bare-metal node setup
- [Mining on Testnet](testnet-mining.md) — Solo mining with dww
- [Merge Mining](merge-mining.md) — Monero merge mining with the finality gadget
- [Merge Mining Adaptor](native-p2pool.md) — p2pool-to-dwowd protocol bridge
- [Level 3: Containerized Localnet](../dev/testing/level-3-localnet.md) — Docker localnet architecture
- [Level 4: Containerized Devnet Node](../dev/testing/level-4-devnet.md) — Single-container devnet deployment
- [DarkWow Testnet Docker README](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/contrib/docker/darkwow-testnet/README.md) — Full env var reference
