# Observer (Relay Node)

*Bitcoin-style community relay full node. No mining. Full validation. Active relay using structured fan-out gossip.*

## Design Intent

The observer is a full node that validates every block and transaction but does
not produce blocks. It functions as a network amplifier, broadcasting blocks and
transactions to all connected peers. This is the same role as a Bitcoin relay
full node — the kind enthusiasts and community members run on modest hardware to
boost network robustness.

An observer:

- Validates every block (PoW, proof-of-token-balance, WASM execution)
- Maintains a complete coin set (UTXO set equivalent)
- Relays blocks to all peers (receive AND forward)
- Relays transactions between P2P peers and JSON-RPC clients
- Serves historical blocks to syncing peers (GetBlocks/GetBlock/GetTip)
- Serves as a P2P seed/bootstrapping node for new peers
- Does NOT mine — `MINING_ENABLED=false`

The weight is in networking bandwidth, not memory or compute. Fibre optic into
a modest machine; the observer does the business.

## Comparison to Bitcoin Relay Node

| Property | Bitcoin Relay | DarkWow Observer |
|----------|--------------|-----------------|
| Full block validation | Yes | Yes |
| Complete UTXO/coin set | Yes | Yes |
| Mempool + tx relay | Yes | Yes |
| Block relay (receive + forward) | Yes | Yes |
| Serve historical blocks | Yes | Yes |
| P2P seed/bootstrapping | Yes | Yes |
| Block production | No | No |
| Genesis creation | No | No |

## Configuration

The observer is a profile of `dwowd` with specific environment variables:

| Variable | Value | Purpose |
|----------|-------|---------|
| `MINING_ENABLED` | `false` | Disables the built-in miner task |
| `CREATE_GENESIS` | `false` | Does not create genesis — syncs from peers |
| `IS_SEED` | `true` | No upstream seeds configured; bootstraps from peer list |
| `LOCALNET` | `true` | Auto-generates a keypair (no keys.toml needed) |
| `MINING_EASY` | `true` | Low-difficulty target for testnet validation |

The observer uses the same `dwowd` binary and Docker image as mining nodes.
The `MINING_ENABLED=false` flag in the Rust code (`Dwowd::start()`) gates the
miner task spawn. If mining is disabled, the daemon logs "Mining disabled —
relay-only mode" and devotes all compute to validation and relay.

## Runtime Behavior

### Startup

1. `init_genesis_contracts()` — deploys 9 genesis contract WASM binaries and
   runs their `__initialize` exports to seed ZK circuits, Merkle trees, and
   nullifier roots. This runs exactly once per build (build-fingerprinted
   sled marker prevents re-execution on restart).

2. `consensus_linear_init_task()` — connects to P2P peers, queries `GetTip`,
   validates genesis hash compatibility, pulls missing blocks via `GetBlocks`/
   `Blocks`, and applies them through full validation.

3. `Dwowd::start()` — begins serving RPC, listening for P2P connections, and
   running the continuous sync loop. The miner task is NOT spawned.

### Steady State

- **Block relay**: Incoming blocks are validated (PoW, proof-of-token-balance,
  WASM). Valid blocks are accepted and relayed via structured fan-out gossip:
  `k = ⌈log₂(N)⌉` randomly selected peers receive the block (see
  [P2P Network](net/p2p-network.md#structured-gossip)). The `broadcast_block()`
  function at `linear_broadcast.rs:206-256` implements this. The receive-side
  relay at `:385` currently uses `p2p.broadcast()` (flood) — an acknowledged
  limitation. Height-gap rejection handles duplicates gracefully.

- **Transaction relay**: P2P-received transactions are added to the mempool and
  broadcast to all peers. JSON-RPC-submitted transactions (`tx.submit_linear`)
  are added to the mempool and broadcast identically.

- **Mempool management**: The mempool maintains fee-ordered transaction
  selection, nullifier deduplication, size-limit eviction, and sled persistence.
  Since the observer does not mine, it never drains the mempool — it serves as
  a pure transaction pool for mining nodes.

- **Continuous sync**: Every 30 seconds, re-polls peers for their tip height.
  If the observer falls behind (e.g., after a network partition), it pulls
  missing blocks and catches up.

- **Hostlist propagation**: The base P2P layer shares peer addresses with
  connected peers, helping new nodes discover the network.

### Shutdown

The observer stops the sync task and P2P handler. The sled database is flushed
to disk. No special teardown is required.

## Interaction with Other Nodes

```
                    ┌──────────────┐
                    │   Observer    │
                    │  (relay node) │
                    └──┬───────┬───┘
           P2P blocks │       │ P2P blocks
           + txs      │       │ + txs
        ┌─────────────┘       └─────────────┐
        ▼                                   ▼
┌──────────────┐                     ┌──────────────┐
│    node0      │◄──── P2P blocks ───►│    node1      │
│ (mining node) │     + txs           │ (mining node) │
└──────────────┘                     └──────────────┘
        ▲                                   ▲
        │                                   │
        │     RPC submit_transaction        │
        │                                   │
   ┌────┴────┐                         ┌────┴────┐
   │ wallet  │                         │ wallet  │
   └─────────┘                         └─────────┘
```

- **To mining nodes**: Relays blocks and transactions. Mining nodes use the
  observer as a seed/bootstrap peer and as a source of mempool transactions.

- **To wallets**: Wallets connect to the observer as a P2P peer, sync blocks
  via GetTip/GetBlocks, and submit transactions via JSON-RPC
  `tx.submit_linear`. The observer broadcasts submitted transactions to the
  network.

- **To other observers**: Multiple observers form a redundant relay mesh.
  Each observer independently validates and relays.

## Resource Profile

The observer is designed to run on modest hardware. A Raspberry Pi with 2 GB
RAM and an SSD can serve as a community relay node.

| Resource | Minimum | Comfortable |
|----------|---------|-------------|
| RAM | 2 GB | 4 GB |
| CPU | 1 core | 2 cores |
| Disk | 10 GB (pruned) | 100 GB (full history) |
| Network | 10 Mbps | 100 Mbps |

The build-fingerprinted genesis marker means the one-time halo2 keygen cost
(1-2 GB RAM) is paid only once per build. On restart, genesis initialization
is skipped entirely.

## Docker Configuration

```yaml
observer:
  image: darkwow-testnet:latest
  container_name: dwow-observer
  environment:
    - MINING_ENABLED=false
    - CREATE_GENESIS=false
    - IS_SEED=true
    - MINING_EASY=true
    - LOCALNET=true
    - RANDOMX_MAX_THREADS=1
    - DWOW_RAYON_THREADS=2
  ports:
    - "127.0.0.1:31340:31340"
  volumes:
    - observer_data:/root/.local/share/dwow/dwowd
```

## References

- [Consensus & Coinbase](consensus-coinbase.md) — reward schedule, uncle-merkle consensus
- [Consensus](consensus/consensus.md) — supply audit, target adjustment, finality
- [Uncle Merkle Consensus](consensus/uncle_merkle.md) — uncle pin mechanism, coinbase split
- [P2P Network](net/p2p-network.md) — lilith handshake, hostlist, magic bytes
- [Wallet Architecture](wallet.md) — capability kernel, manifest resolution
