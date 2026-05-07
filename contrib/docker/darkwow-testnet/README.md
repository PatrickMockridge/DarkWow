# DarkWow Testnet

DarkWow is a fork of DarkFi. Magic bytes `[68, 82, 75, 87]` encode "DRKW" in ASCII, uniquely identifying DarkWow nodes on the P2P network.

## Deployment

```bash
# Build and start the testnet (lilith seed + 2 mining nodes)
docker compose up --build

# Start in detached mode
docker compose up --build -d

# Tear down
docker compose down
```

## Network Parameters

| Parameter | Value |
|-----------|-------|
| Block time | 120 seconds |
| Initial difficulty | 255 (auto-adjusting) |
| PoW algorithm | RandomX (rx/0) |
| P2P port | 31340 |
| RPC port (node0) | 31345 |
| Stratum port (node0) | 31347 |

## Architecture

Three containers on a bridge network:

1. **lilith** — seed node, P2P listen on 31340
2. **node0** — mining node, connects to lilith
3. **node1** — mining node, connects to lilith + node0
