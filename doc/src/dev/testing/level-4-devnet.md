# Level 4: Containerized Devnet Node

A self-contained Docker image that turns any Linux machine into a DarkWow
devnet mining node. Run a shared devnet across idle machines on your LAN,
optionally open it to internet participants.

**Location:** `contrib/docker/dwow-devnet/`

## What It Is

Unlike Level 3 (which runs a fixed 3-container topology on one machine),
Level 4 is a single-container node designed for multi-machine deployment.
Each machine runs one container. One machine acts as the seed; others join
by pointing to it.

Uses **host networking** so P2P peers see the host's real IP address —
essential for LAN discovery and external connectivity.

## Quick Start

### Start a Fresh Devnet (Seed Node)

```shell
docker run --rm --network=host \
  -e IS_SEED=true \
  -e NETWORK_NAME=our-devnet \
  dwow-devnet:latest
```

This starts a single-node devnet with mining enabled. Blocks mine instantly
(`FIXED_DIFFICULTY=1`). RPC on port 31345, stratum mining on 31347.

### Join an Existing Devnet (Miner Node)

```shell
docker run --rm --network=host \
  -e SEED_ADDR=<seed-lan-ip>:31342 \
  -e NETWORK_NAME=our-devnet \
  dwow-devnet:latest
```

Replace `<seed-lan-ip>` with the seed node's LAN IP (e.g., `192.168.1.10`).

### Verify It Works

From any machine with the `dww` CLI wallet:

```shell
dww -n dwow-devnet -c tcp://127.0.0.1:31345 info
```

## Multi-Machine LAN Deployment

Each machine runs one container with host networking. The seed must be
reachable from all other machines.

| Machine | Role | Key Env Var |
|---------|------|-------------|
| Any | Seed | `IS_SEED=true` — no `SEED_ADDR` needed |
| Any | Miner | `SEED_ADDR=<seed-ip>:31342` |
| Any | More miners | Same as above |

All nodes must use the same `NETWORK_NAME` — this controls P2P magic bytes
(derived via blake3 hash) so nodes can find each other. **Pick a unique name
per devnet** to avoid collisions on shared networks.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `NETWORK_NAME` | `dwow-devnet` | Unique devnet name (determines P2P isolation) |
| `IS_SEED` | `false` | First node in a fresh devnet |
| `SEED_ADDR` | (empty) | `host:port` of seed to join an existing devnet |
| `EXTERNAL_ADDR` | (empty) | Public `host:port` for internet-facing nodes |
| `MAGIC_BYTES` | auto | 4 comma-separated bytes for P2P isolation |
| `P2P_PORT` | `31342` | P2P inbound listen port |
| `RPC_PORT` | `31345` | JSON-RPC port |
| `STRATUM_PORT` | `31347` | Stratum mining port |
| `MANAGEMENT_PORT` | `31346` | Management RPC port |
| `FIXED_DIFFICULTY` | `1` | Fixed PoW difficulty (unset for dynamic) |
| `TARGET_BLOCK_TIME` | `120` | Block time target in seconds |
| `MINING_ENABLED` | `true` | Auto-start xmrig mining |
| `MINING_THREADS` | `1` | xmrig thread count |
| `THRESHOLD` | `1` | Confirmation threshold in blocks |
| `SKIP_SYNC` | `true` | Skip blockchain sync on startup |
| `SKIP_FEES` | `true` | Disable fee verification |
| `WALLET_ADDRESS` | auto | Mining payout address (auto-generated if unset) |
| `WALLET_SECRET_FILE` | (empty) | Path to file containing hex-encoded secret key (preferred) |
| `WALLET_SECRET` | auto | Hex-encoded secret key (deprecated — use WALLET_SECRET_FILE) |

## Opening to the Internet

1. **On the seed machine**: set up port forwarding on your router for the P2P
   port (default 31342).
2. **Set `EXTERNAL_ADDR`** on the seed:
   ```shell
   docker run --network=host \
     -e IS_SEED=true \
     -e EXTERNAL_ADDR=<your-public-ip>:31342 \
     dwow-devnet:latest
   ```
3. **External participants** join with:
   ```shell
   docker run --network=host \
     -e SEED_ADDR=<your-public-ip>:31342 \
     dwow-devnet:latest
   ```

## Networking

The container uses **host networking** (`--network=host`):
- Container shares the host's network stack
- P2P peers see the host's real IP (essential for LAN discovery)
- No port mapping needed (ports bind directly on the host)

If you need bridge networking instead, map ports and set `EXTERNAL_ADDR` to
the host's IP with mapped ports.

## Data Persistence

Blockchain data is stored in `~/.local/share/dwow/dwowd/<network-name>/` inside
the container. Mount a volume to persist across restarts:

```shell
docker run --network=host \
  -v /data/dwow-devnet:/root/.local/share/dwow/dwowd \
  -e IS_SEED=true \
  dwow-devnet:latest
```

## Docker Compose Template

A [docker-compose.yml](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/contrib/docker/dwow-devnet/docker-compose.yml)
template is provided with seed + miner services.

**Single-machine test:**
```shell
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml up
```

**Multi-machine (one service per machine, host networking):**
```shell
# Machine 1
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml --profile host up seed

# Machine 2 (after editing SEED_ADDR in compose file)
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml --profile host up miner
```

## Building from Source

The build takes 30-60 minutes. Pre-built images are preferred.

```shell
# From the repo root:
docker build -t dwow-devnet . -f contrib/docker/dwow-devnet/Dockerfile

# Or use the build script:
./contrib/docker/dwow-devnet/build-and-push.sh
```

To push to a registry for other machines to pull:

```shell
REGISTRY=docker.io/youruser/ ./contrib/docker/dwow-devnet/build-and-push.sh
```

## Differences from Level 3

| Aspect | Level 3 (Localnet) | Level 4 (Devnet) |
|--------|-------------------|-----------------|
| Topology | Fixed 3-container | Single container per machine |
| Networking | Bridge (`dwow-local`) | Host (`--network=host`) |
| Config | Hardcoded node0/node1 | Env-var-driven (any topology) |
| Scale | 1 machine, 3 containers | N machines, 1 container each |
| Discovery | Internal bridge DNS | Real LAN IPs |
| Internet access | Not designed for | Built-in via `EXTERNAL_ADDR` |
| Use case | Local testing | Shared devnet across machines |

## File Locations

| Component | Path |
|-----------|------|
| Dockerfile | `contrib/docker/dwow-devnet/Dockerfile` |
| Entrypoint script | `contrib/docker/dwow-devnet/entrypoint.sh` |
| Docker Compose template | `contrib/docker/dwow-devnet/docker-compose.yml` |
| Build/push script | `contrib/docker/dwow-devnet/build-and-push.sh` |
| README | `contrib/docker/dwow-devnet/README.md` |
| test_pipeline.sh | `contrib/docker/dwow-devnet/test_pipeline.sh` |
| contract_test.sh | `contrib/docker/dwow-devnet/contract_test.sh` |
| test-contracts.sh | `contrib/docker/dwow-devnet/test-contracts.sh` |
| Config reference | `bin/darkfid/dwowd_config.toml` (`dwow-devnet` section) |

## Public Testnet Node

A separate image, `darkwow-node/testnet`, provides a one-command entry point
for joining the **public DarkWow testnet** as a mining node. Unlike the LAN
devnet image above, this connects to public seed infrastructure.

Two mining modes:
- `MODE=native` — solo RandomX mining (dwowd + xmrig)
- `MODE=merge` — Monero merge mining via p2pool (monerod + dwowd + p2pool + xmrig)

```bash
docker pull darkwow-node/testnet:latest
docker run --network=host -e MODE=native darkwow-node/testnet:latest
```

→ [Public Testnet Node README](https://github.com/darkrenaissance/darkfi/blob/master/contrib/docker/testnet-node/README.md)
