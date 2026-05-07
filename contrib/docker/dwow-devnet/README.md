# DarkWow Devnet — Docker Mining Node

A self-contained Docker image that turns any Linux machine into a DarkWow devnet
mining node. Run a private blockchain across idle machines on your LAN, open it
to external participants, and mine DRKW blocks.

## Quick Start

**Pull the pre-built image:**

```shell
docker pull <registry>/dwow-devnet:latest
```

**Start a fresh devnet (first node — the seed):**

```shell
docker run --rm --network=host \
  -e IS_SEED=true \
  -e NETWORK_NAME=our-devnet \
  <registry>/dwow-devnet:latest
```

This starts a single-node devnet with mining enabled. Blocks mine instantly
(fixed difficulty 1). RPC available on port 31345, stratum mining on 31347.

**Join an existing devnet from another machine:**

```shell
docker run --rm --network=host \
  -e SEED_ADDR=<seed-lan-ip>:31342 \
  -e NETWORK_NAME=our-devnet \
  <registry>/dwow-devnet:latest
```

Replace `<seed-lan-ip>` with the seed node's LAN IP address (e.g., `192.168.1.10`).

## Check That It Works

From any machine with the `dww` CLI wallet:

```shell
# Query the node's blockchain status
dww -n <network-name> -c <rpc-url> info
```

## Multi-Machine LAN Deployment

Each machine runs one container with host networking. The seed machine must be
reachable from all other machines.

| Machine | Role      | Command / Config |
|---------|-----------|-----------------|
| Any     | Seed      | `IS_SEED=true` — no `SEED_ADDR` needed |
| Any     | Miner     | `SEED_ADDR=<seed-ip>:31342` |
| Any     | More...   | Same as miner, additional nodes |

All nodes use the same `NETWORK_NAME` — this controls P2P magic bytes so nodes
can find each other. Pick a unique name per devnet to avoid collisions on shared
networks.

## Environment Variable Reference

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
| `LOCALNET` | `false` | P2P localnet flag |
| `WALLET_ADDRESS` | auto | Mining payout address (auto-generated if unset) |

## Opening to the Internet

To allow external participants (outside your LAN) to join:

1. **On the seed machine**: set up port forwarding on your router for the P2P
   port (default 31342).
2. **Set `EXTERNAL_ADDR`** on the seed:
   ```shell
   docker run --network=host \
     -e IS_SEED=true \
     -e EXTERNAL_ADDR=<your-public-ip>:31342 \
     <registry>/dwow-devnet:latest
   ```
3. **External participants** connect with:
   ```shell
   docker run --network=host \
     -e SEED_ADDR=<your-public-ip>:31342 \
     <registry>/dwow-devnet:latest
   ```

## Networking

The container uses **host networking** (`--network=host`). This means:
- The container shares the host's network stack
- P2P peers see the host's real IP address (essential for LAN discovery)
- No port mapping needed (ports bind directly on the host)

If you need bridge networking instead, map ports and set `EXTERNAL_ADDR` to the
host's IP with the mapped port.

## Building from Source

The build takes 30-60 minutes on a typical machine (8GB RAM, 4 cores). Ensure
you have sufficient disk space (~15GB for build artifacts).

```shell
# From the repo root:
docker build -t dwow-devnet . -f contrib/docker/dwow-devnet/Dockerfile
```

Or use the build script:

```shell
./contrib/docker/dwow-devnet/build-and-push.sh
```

## Docker Compose

A [docker-compose.yml](docker-compose.yml) template is provided with seed +
miner services. For single-machine testing:

```shell
docker-compose -f contrib/docker/dwow-devnet/docker-compose.yml up
```

For multi-machine deployment, run one service per machine:

```shell
# Machine 1
docker-compose -f contrib/docker/dwow-devnet/docker-compose.yml up seed

# Machine 2 (after editing SEED_ADDR in the compose file)
docker-compose -f contrib/docker/dwow-devnet/docker-compose.yml up miner
```

## Data Persistence

Blockchain data is stored in `~/.local/share/dwow/dwowd/<network-name>/` inside
the container. Mount a volume to persist across restarts:

```shell
docker run --network=host \
  -v /data/dwow-devnet:/root/.local/share/dwow/dwowd \
  -e IS_SEED=true \
  <registry>/dwow-devnet:latest
```
