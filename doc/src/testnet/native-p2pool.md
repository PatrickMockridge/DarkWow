# Merge Mining Adaptor

The `dwow-p2pool-adaptor` bridges p2pool to dwowd's stratum protocol, enabling
merge mining with Monero and the [anchoring/finality gadget](#anchoring-finality-gadget).
DarkWow is a minority-mined RandomX L1 — in the beginning, it borrows security
from Monero's hashpower through this pathway.

**Scope boundary:** The adaptor is merge mining and finality gadget
infrastructure. DarkWow-native pooled mining (DRKW reward distribution without
Monero merge mining) is an ecosystem concern — this repo provides the node
software and the adaptor; pool protocols and reward distribution schemes use the
same stratum interface but are not bundled or maintained here.

## Architecture

```text
xmrig --stratum--> p2pool --[monerod RPC]--> adaptor --[stratum]--> dwowd
                                                              --> lilith P2P
```

The adaptor is a protocol bridge: it translates dwowd's native stratum
interface into monerod-compatible JSON-RPC (`get_block_template`, `submit_block`,
`get_info`). p2pool thinks it's talking to monerod, and the adaptor translates
every request into DarkWow's stratum protocol.

This design reuses p2pool (SChernykh's Monero p2pool) without modification — the
adaptor handles the protocol translation, and p2pool provides its full feature
set: PPLNS payout distribution, a sidechain with uncle blocks every 10 seconds,
a stratum server for xmrig, and share tracking. No Monero chain is required when
mining through the adaptor.

### Why a Protocol Bridge?

The adaptor exists because DarkWow does not implement its own pool protocol.
Instead, it reuses Monero's p2pool ecosystem through a compatibility layer. This
is deliberate:

- **p2pool is battle-tested.** It has years of production use in the Monero
  ecosystem. Reimplementing its PPLNS logic, sidechain accounting, and share
  tracking in Rust would be thousands of lines of consensus-critical code.
- **The adaptor is thin.** ~600 lines of Rust across 4 source files. It only
  translates protocols — it does not implement pool logic itself.
- **p2pool can be upgraded independently.** The adaptor isolates protocol
  translation from pool mechanics. When p2pool releases improvements, DarkWow
  benefits without code changes.

### Relationship to Merge Mining

The adaptor and merge mining are complementary:

| Pathway | Data flow | p2pool flag | Rewards |
|---------|-----------|-------------|---------|
| **Merge mining** | xmrig → p2pool → monerod + dwowd `mm_rpc` | `--merge-mine` | XMR + DRKW |
| **Adaptor** | xmrig → p2pool → adaptor → dwowd stratum | (none) | DRKW only |

The adaptor pathway is a subset: it uses the same p2pool stratum and the same
dwowd chain, but without Monero as the parent chain. Merge mining provides the
dual-reward + finality pathway; the adaptor provides the pool infrastructure
that both pathways share.

## Anchoring Finality Gadget

DarkWow is a minority-mined RandomX L1. Without merge mining, an attacker with
superior RandomX hashpower could reorganize the chain. The anchoring finality
gadget prevents this by borrowing Monero's cumulative difficulty.

Each merge-mined DarkWow block references a specific Monero block as an anchor.
Once the Monero anchor has enough confirmations (default 3), the DarkWow block
is **finalized** — it cannot be reorganized, even by an attacker with superior
hashpower. Among candidate forks, any fork that would orphan a finalized block
is rejected.

The adaptor is critical infrastructure for this model: it enables the stratum
pathway that merge miners use to connect p2pool to dwowd. Without the adaptor,
the finality gadget has no mining infrastructure to run on.

See [Mining Tokenomics](../arch/mining-tokenomics.md#anchoring-finality-gadget) for
the full design, and `contrib/docker/darkwow-testnet/merge_mining_model.py` for
the Python simulation.

## Prerequisites

- Built binaries: `dwowd`, `dww`, `dwow-p2pool-adaptor`
- xmrig installed
- p2pool binary (from [p2pool releases](https://github.com/SChernykh/p2pool/releases))

Build the adaptor:

```bash
cargo build -p dwow-p2pool-adaptor --release
```

## Docker Quick Start

The quickest way to run with the adaptor is the containerized testnet:

```bash
cd contrib/docker/darkwow-testnet

# Full pipeline (build + start + verify):
./test_pipeline.sh --mode native-p2pool

# Or directly with docker compose:
docker compose --profile native-p2pool up -d
```

This starts three additional containers alongside the base 3-node testnet:

| Container | Role |
|-----------|------|
| `dwow-adaptor` | Protocol bridge (monerod RPC ↔ dwowd stratum) |
| `dwow-p2pool-darkwow` | p2pool mining pool |
| `dwow-xmrig-p2pool` | xmrig miner (connects to p2pool stratum) |

## Bare-Metal Setup

### Step 1: Start dwowd

Start dwowd with stratum enabled:

```bash
dwowd -c dwowd_config.toml
```

Ensure the config has:

```toml
[network_config."darkwow-testnet".stratum_rpc]
rpc_listen = "tcp://127.0.0.1:31347"
```

### Step 2: Start the Adaptor

```bash
dwow-p2pool-adaptor \
    --dwowd-rpc 127.0.0.1:31345 \
    --dwowd-stratum 127.0.0.1:31347 \
    --listen 0.0.0.0:28081 \
    --wallet-address <YOUR_WALLET_ADDRESS>
```

Expected output:
```
=== dwow-p2pool-adaptor ===
dwowd RPC:       127.0.0.1:31345
dwowd stratum:   127.0.0.1:31347
Listen:          0.0.0.0:28081
[INFO] Connected to dwowd stratum
[INFO] Listening for p2pool connections on 0.0.0.0:28081
```

### Step 3: Start p2pool

```bash
p2pool \
    --host 127.0.0.1 \
    --rpc-port 28081 \
    --wallet <DUMMY_MONERO_ADDRESS> \
    --stratum 0.0.0.0:3333 \
    --data-dir /root/.p2pool \
    --no-igd \
    --mini \
    --no-upnp
```

The `--wallet` parameter takes a Monero-format address, but the adaptor ignores
it for block rewards. Use any valid Monero address (it won't receive funds).

### Step 4: Start xmrig

```bash
xmrig -o stratum+tcp://127.0.0.1:3333 \
      -u <YOUR_WALLET_ADDRESS> \
      -a rx/0 \
      -t 1
```

## Environment Variables

### Adaptor (`entrypoint-adaptor.sh`)

| Variable | Default | Description |
|----------|---------|-------------|
| `DWOWD_RPC` | `node0:31345` | dwowd JSON-RPC for chain queries |
| `DWOWD_STRATUM` | `node0:31347` | dwowd stratum for block templates |
| `ADAPTOR_LISTEN` | `0.0.0.0:28081` | Where the adaptor listens for p2pool |
| `WALLET_ADDRESS` | *(required)* | DarkWow wallet address for stratum login |
| `CONNECT_RETRIES` | `30` | Max retry attempts for dwowd stratum connection |

### p2pool (`entrypoint-p2pool-darkwow.sh`)

| Variable | Default | Description |
|----------|---------|-------------|
| `MONERO_HOST` | `adaptor` | p2pool's monerod endpoint (the adaptor) |
| `MONERO_RPC_PORT` | `28081` | Adaptor's listening port |
| `STRATUM_PORT` | `3333` | p2pool stratum port for xmrig |
| `WALLET_ADDRESS` | *(optional)* | DarkWow wallet for mining rewards |

## Adaptor RPC Reference

The adaptor exposes a monerod-compatible JSON-RPC interface over HTTP on its
listen port. p2pool calls these methods:

### `get_block_template`

Returns a DarkWow block header translated into Monero's block template format.

```json
{
  "jsonrpc": "2.0",
  "method": "get_block_template",
  "params": {
    "wallet_address": "<darkwow_bs58_address>",
    "reserve_size": 8
  },
  "id": 1
}
```

**Response:**
```json
{
  "result": {
    "blocktemplate_blob": "...",
    "blockhashing_blob": "...",
    "difficulty": 255,
    "height": 1234,
    "prev_hash": "..."
  }
}
```

### `submit_block`

Accepts a solved block from p2pool and submits it to dwowd's stratum.

```json
{
  "jsonrpc": "2.0",
  "method": "submit_block",
  "params": ["hex_block_blob"],
  "id": 1
}
```

### `get_info`

Returns current DarkWow chain state in Monero-compatible format.

```json
{
  "jsonrpc": "2.0",
  "method": "get_info",
  "params": [],
  "id": 1
}
```

**Response:**
```json
{
  "result": {
    "height": 1234,
    "top_block_hash": "...",
    "difficulty": 255
  }
}
```

## Block Header Translation

The adaptor translates between the DarkWow header format (225 bytes) and
Monero's block template format. Key offsets:

| Field | Offset | Size |
|-------|--------|------|
| Version | 0 | 1 |
| Previous Hash | 1 | 32 |
| Merkle Root | 33 | 32 |
| Timestamp | 65 | 8 |
| Difficulty Target | 73 | 4 |
| **Nonce** | **77** | **4** |
| Height | 81 | 4 |
| Uncle Merkle Root | 85 | 32 |
| Total Reward | 117 | 8 |
| RandomX Key | 125 | 60 |
| Coin Merkle Root | 185 | 32 |
| Nullifier Root | 217 | 8 |

Miners modify bytes 77-80 (the nonce) to find a valid RandomX hash.

## Known Limitations

- **No ZMQ**: The adaptor does not support ZMQ PUB for `chain-main`
  notifications. p2pool polls `get_block_template` on its own interval instead.
  Block template propagation is slightly slower than it would be with ZMQ push.
- **Thread-per-connection**: The adaptor spawns a new OS thread per incoming
  p2pool connection. For a single p2pool instance, this is fine; multi-pool
  deployments would need refactoring.
- **Blake3 placeholder hash**: The `submit_block` path uses blake3 for block
  hashing during submission rather than full RandomX verification. The dwowd
  stratum performs the actual RandomX PoW check.
- **Monero wallet address**: p2pool requires a Monero-format wallet address even
  in native mode. The adaptor ignores this address for block rewards — pass any
  valid Monero testnet address.

## See Also

- [Merge Mining](merge-mining.md) — Monero merge mining with the finality gadget
- [Mining on Testnet](testnet-mining.md) — Solo mining with xmrig
- [DarkWow Testnet README](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/contrib/docker/darkwow-testnet/README.md) — Containerized devnet
- [Mining Tokenomics](../arch/mining-tokenomics.md) — Architecture overview
