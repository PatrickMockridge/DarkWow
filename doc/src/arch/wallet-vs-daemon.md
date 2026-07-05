# Wallet vs Daemon Architecture

DarkWow has two primary binaries: `dwowd` (mining node daemon) and `dwow_wallet` (wallet
CLI tool). Both are **full nodes** — they sync the full chain, store it locally,
and speak the same P2P wire protocol. But their **runtime architectures** are
fundamentally different. The daemon is a permanent async server; the wallet is a
static CLI tool that runs async code only when it needs the network.

This page documents the split, explains what each binary owns exclusively and
what they share, and gives the resource and dependency implications of each model.

## Architectural Split

```
┌─────────────────────────────────┐  ┌─────────────────────────────────┐
│         dwowd (daemon)          │  │          dwow_wallet (wallet)           │
│                                 │  │                                 │
│  Permanent async executor       │  │  smol::block_on() only for      │
│  7+ always-running tasks:       │  │  network commands (5 of ~30)    │
│                                 │  │                                 │
│  ▸ P2P: inbound + outbound      │  │  ▸ P2P: outbound client only   │
│  ▸ RPC: 4 servers (main, mgmt,  │  │  ▸ No RPC servers              │
│    stratum, merge-mining)       │  │  ▸ No mining                   │
│  ▸ Mining: built-in + stratum   │  │  ▸ No mempool                  │
│  ▸ Mempool: 10k tx, 1hr TTL    │  │  ▸ No WASM execution           │
│  ▸ WASM execution: full runtime │  │  ▸ No block validation         │
│  ▸ Block validation: full       │  │                                 │
│    (PoW, consensus, ZK, finality│  │  What the wallet does:          │
│  ▸ Signal handling: graceful    │  │  ▸ Key management (SQLite)     │
│    shutdown                     │  │  ▸ Chain sync (P2P fetch)      │
│                                 │  │  ▸ Local AEAD block scan       │
│  Runs 24/7. Serves the network. │  │  ▸ Capability discovery         │
│  Produces blocks.               │  │  ▸ Transaction building (ZK)   │
│                                 │  │  ▸ Contract interaction         │
│                                 │  │                                 │
│                                 │  │  One command → one process →    │
│                                 │  │  exits. Stateless between runs. │
└─────────────────────────────────┘  └─────────────────────────────────┘
```

The fundamental difference: **the daemon is a server that runs forever; the
wallet is a tool that does one thing and exits.** They share chain storage and
the P2P wire protocol, but almost nothing above that layer.

## Runtime Model

| | Daemon (`dwowd`) | Wallet (`dwow_wallet`) |
|---|---|---|
| **Runtime** | Permanent `smol` async executor, daemonized | Sync CLI; `smol::block_on()` only for 5 network commands |
| **Process lifecycle** | Starts → runs forever → signal → graceful shutdown | Starts → does one thing → exits immediately |
| **Background tasks** | 7+ always-running | Zero (sync loop is per-command, not persistent) |
| **Signal handling** | `signal-hook-async-std` graceful shutdown | None (exit on completion) |
| **Config** | `structopt-toml` merged dual-source (CLI + TOML) | Manual argv parse + TOML merge |
| **Entry point** | `async_daemonize!(realmain)` → `Dwowd::init_linear()` + `Dwowd::start()` | `main()` → `parse_args()` → `open_wallet()` → `dispatch()` |

The wallet classifies its ~30+ commands into four categories:

- **`Local`** — pure read operations (balance, address, capabilities). No async.
- **`LocalStdin`** — read from stdin, write to stdout. No async.
- **`LocalBuild`** — build a transaction, optionally broadcast. Brief `smol::block_on()`.
- **`Network`** — requires P2P: `sync init`, `sync status`, `scan`, `broadcast`, `mine`. Wrapped in `smol::block_on()`.

Only the `Network` category spins up an async executor. Everything else is
synchronous. A `wallet balance` command opens the SQLite database, reads it,
prints a number, and exits — no async runtime, no network, no P2P.

## P2P Networking

Both binaries use `dwow_core::net::P2p` — the **same P2P stack**. The difference
is configuration (client vs full node) and feature flags (wallet excludes transport
plugins).

| | Daemon | Wallet |
|---|---|---|
| **dwow_core feature** | `net` (= `net-wallet` + `net-full`) | `net-wallet` |
| **net-wallet** | ✓ (via `net`) | ✓ — P2p + sessions + transport (TCP+TLS) |
| **net-full** | ✓ — transport plugins (Tor, QUIC, I2P, etc.) | — not compiled |
| **P2P module** | `dwow_core::net::P2p` | `dwow_core::net::P2p` (same) |
| **Connection model** | Full node: inbound + outbound | Pure client: outbound only |
| **Session management** | 6 session types, all active | 6 session types, inbound is no-op |
| **Host management** | HostContainer: grey/white/gold/black/dark lists | HostContainer (same code, less data) |
| **Protocols** | Full protocol registry | Full protocol registry (same code) |
| **Metering** | Per-message rate limiting queues | Per-message rate limiting queues (same code) |
| **Transport layer** | TCP+TLS + Tor + SOCKS5 + QUIC + Unix + I2P | TCP+TLS only |

### The Transport Split

The daemon's transport layer was extracted into a standalone **`dwow_transport`**
crate (`src/transport/`) — a pluggable `Dialer` with URL-scheme-based dispatch
that has zero dependency on `dwow_core`. Both binaries can use it, but they
consume it differently:

- **Daemon**: `dwow_core::net` has its own copy of the transport layer
  (`src/net/transport/`). The extracted crate will eventually replace it, but
  for now they coexist. The daemon accesses transports through sessions,
  connectors, and host mixing.

- **Wallet**: `dwow_transport` is an **optional** dependency. When the
  `transport` feature is off (the default), the transport crate is not
  compiled, and the wallet uses only its built-in TCP+TLS path (Layer 0).
  When enabled, external transports (Tor, SOCKS5, QUIC) become available as
  purely additive Layer 1 code paths — they cannot affect the critical TCP
  path because the two layers share no code, no state, and no error handling.

```
          Wallet P2P (p2p_wallet.rs)
          ──────────────────────────
          │                         │
     Layer 0 (always)         Layer 1 (optional)
     Built-in TCP+TLS         dwow_transport::Dialer
          │                         │
     tcp://, tcp+tls://       tor://, socks5://, etc.
          │                         │
     Critical path             Feature-gated
     Never touches             Additive only
     dwow_transport            Each transport independent
```

**Wire protocol**: Both binaries speak the same protocol at the byte level —
magic bytes prefix, binary `VersionMessage`/`VerackMessage` handshake, varint
framing. They share `dwow_core::net::message` types (compiled via `net-wire`).
The wallet can sync from any `dwowd` node because they share this protocol.
But the daemon's full protocol suite (ping/pong, address gossip, seed sync,
hole punching) is absent from the wallet — it only implements the messages it
needs (`GetTip`/`Tip`, `GetBlocks`/`Blocks`, `TxMessage`).

## Blockchain Processing

| | Daemon | Wallet |
|---|---|---|
| **Block validation** | Full: PoW, PoW consensus, Uncle Merkle, WASM execution, ZK verification, Pedersen mass balance, finality | None — inserts blocks as-is from P2P sync |
| **Contract execution** | Full WASM runtime (`wasmer`, singlepass/cranelift) | No execution |
| **Mempool** | Yes (10k tx, 1hr TTL) | No |
| **Mining** | Built-in miner + Stratum + Merge-mining | No |
| **Genesis** | Creates genesis block, stores 9 genesis contracts with WASM + manifests | Reads genesis blocks from P2P sync |

The daemon **enforces rules**. It validates every block, executes every contract
call, verifies every ZK proof, tracks every coin and nullifier, and rejects
anything that doesn't comply. It is the enforcement layer of the network.

The wallet **observes outcomes**. It downloads blocks that the daemon already
validated, stores them locally, and scans them with AEAD decryption to discover
which outputs belong to its keys. It trusts the daemon's validation — the wallet
does not re-execute contracts or re-verify proofs. It is the observation layer.

This division of labor is intentional: validation is expensive (WASM runtime,
ZK verification, RandomX mining) and belongs on always-on nodes. Observation
is cheap (decryption, Merkle proof verification) and belongs in the wallet.

## Subsystems

### Daemon-Only

| Subsystem | File | Purpose |
|---|---|---|
| Mempool | `bin/dwowd/src/mempool.rs` | Pending transaction buffer |
| BlockAcceptor | `bin/dwowd/src/block_acceptor.rs` | Single block acceptance pipeline |
| WASM Execution | `bin/dwowd/src/execution.rs` | Contract call execution with gas limits |
| ProofOfTokenBalance | `bin/dwowd/src/proof_of_token_balance.rs` | Per-block Pedersen mass balance |
| DwowMinersRegistry | `bin/dwowd/src/registry/mod.rs` | Stratum + merge-mining RPC |
| StratumRpcHandler | `bin/dwowd/src/rpc/stratum.rs` | xmrig-compatible stratum |
| MergeMiningRpcHandler | `bin/dwowd/src/rpc/mm_rpc.rs` | Monero p2pool coordination |
| MinerRpc | `bin/dwowd/src/rpc/miner.rs` | Local miner control |
| ProtocolHandlers | `bin/dwowd/src/proto/` | P2P serve-side handlers |
| ZK Verifier | `bin/dwowd/src/zk.rs` | Linear blockchain ZK verification |
| Finality | `dwow_chain::FinalityConfig` | Arweave/Monero anchoring |
| JSON-RPC Servers | `bin/dwowd/src/rpc/` | Main, Management, Stratum, MM |

### Wallet-Only

| Subsystem | File | Purpose |
|---|---|---|
| WalletDb | `bin/dww/src/walletdb.rs` | SQLite (sqlcipher): keys, caps, contracts |
| Cache | `bin/dww/src/cache.rs` | Sled: SMT merkle trees, scan progress |
| Scan | `bin/dww/src/scan.rs` | Local AEAD decryption, capability discovery |
| CapabilityResolver | `bin/dww/src/capability.rs` | 18+ contract resolvers |
| Transfer | `bin/dww/src/transfer.rs` | Payment transaction building |
| FeeBuilder | `bin/dww/src/fee_builder.rs` | NativeToken fee call construction |
| Deploy | `bin/dww/src/deploy.rs` | Contract deployment |
| ManifestResolver | `bin/dww/src/manifest_resolver.rs` | On-chain ABI queries |
| ManifestVerify | `bin/dww/src/manifest_verify.rs` | WASM manifest verification |
| ContractMetadata | `bin/dww/src/contract_metadata.rs` | Universal contract registry |
| SyncTask | `bin/dww/src/sync_task.rs` | P2P block sync loop |
| P2pWallet | `bin/dww/src/p2p_wallet.rs` | Wallet-owned P2P client |

### Shared

| Component | Used by both | Location |
|---|---|---|
| LinearStore (sled) | Block storage | `src/linear/` |
| dwow-sdk | Cryptography, manifests, contract clients | `src/sdk/` |
| dwow-serial | Binary serialization | `src/serial/` |
| Wire protocol | GetTip/Tip, GetBlocks/Blocks, varint framing | — |
| Genesis contracts | Same 9 contracts, same ContractIds | `src/contract/` |
| dwow_transport | Optional transport layer (Tor, SOCKS5, etc.) | `src/transport/` |

## Feature Flags and Dependencies

Both binaries depend on `dwow_core` (the workspace root crate), but enable
different feature sets for their different roles:

| Feature | Daemon (`dwowd`) | Wallet (`dwow_wallet`) | What It Pulls In |
|---|---|---|---|
| `blockchain` | ✓ | ✓ | `bs58`, `dwow-serial`, `tx`, `util` — chain types, tx types |
| `net-wallet` | ✓ (via `net`) | ✓ | P2p + 6 sessions + channel + connector + settings + transport (TCP+TLS). All P2P infrastructure. |
| `net-full` | ✓ (via `net`) | — | Transport plugins: Tor, I2P, SOCKS5, Unix, QUIC |
| `rpc` | ✓ | — | `net` + `httparse` — all 4 JSON-RPC servers |
| `wasm-runtime` | ✓ | — | `wasmer`, `wasmer-compiler-singlepass`, `wasmer-middlewares` |
| `async-daemonize` | ✓ | — | `system` feature: `StoppableTask`, `ExecutorPtr`, signal handling |

The wallet enables **`blockchain` + `net-wallet` + `async-serial`** from `dwow_core`.
The `net-wallet` feature includes P2p + sessions + channel + connector + settings +
transport (TCP+TLS only). It excludes transport plugins (Tor, QUIC, I2P, SOCKS5,
Unix) and `structopt` (wallet uses direct TOML deserialization). The wallet and
daemon share the same P2P code — the only difference is the feature flags.

When the wallet does need exotic transports (Tor, SOCKS5), it enables the
optional `dwow_transport` crate — not `dwow_core::net`. The transport crate
carries none of the daemon's P2P infrastructure.

## Resource Profile

### Memory

| | Daemon | Wallet |
|---|---|---|
| **Base** | ~50 MB (smol executor, sled metadata) | ~20 MB (SQLite, sled metadata) |
| **Sled cache** | 256 MB (configured) | ~64 MB (default) |
| **WASM runtime** | ~200 MB (wasmer JIT, compiled contracts) | — |
| **Mempool** | ~10 MB (10k tx ceiling) | — |
| **Stratum** | ~5 MB (miner connections, job buffers) | — |
| **Peak** | ~500+ MB | ~100 MB |

### CPU

| | Daemon | Wallet |
|---|---|---|
| **Idle** | Low (P2P heartbeat, stratum polling) | Zero (process exits between commands) |
| **Mining** | High (RandomX VM, non-stop) | — |
| **Block validation** | Burst (WASM execution, ZK verification per block) | — |
| **Chain sync** | Burst (download + validate) | Burst (download only, then scan) |
| **Transaction building** | — | Burst (ZK proof generation, then exits) |

### Disk

| | Daemon | Wallet |
|---|---|---|
| **Number of DBs** | 1 (sled) | 3 (sled chain, sled cache, SQLite wallet) |
| **Chain data** | ~2 GB (full chain) | ~2 GB (full chain, separate copy) |
| **Contract state** | Stored in sled trees | Not stored locally |
| **Key material** | Mining keypair on filesystem | Encrypted in SQLite (sqlcipher) |
| **Capability data** | — | AEAD-discovered caps + Merkle proofs |

Both store the full chain independently. If you run both on the same machine,
each has its own copy — they do not share the chain database. This is by design:
the daemon's sled DB is tightly coupled to its validation state (coin sets,
nullifier sets, contract trees) that the wallet does not need.

### Network

| | Daemon | Wallet |
|---|---|---|
| **Inbound** | Yes (listen on configured port + transport ports) | No |
| **Outbound** | Yes (seed connections, peer mesh) | Yes (seed connections only, fetch blocks) |
| **Ports** | Configurable main port + stratum port + mm port + transport ports | No open ports |
| **Bandwidth** | Serve blocks to peers, relay transactions | Download blocks, broadcast own transactions |
| **Protocols** | Full P2P (version, ping, address, seed, holepunch) + sync (GetTip/GetBlocks) | Sync only (GetTip/GetBlocks) + tx broadcast |

## When to Use Which

**Run a daemon (`dwowd`) when you want to:**
- Mine blocks and earn block rewards
- Serve the P2P network (be a public peer)
- Run stratum for external miners
- Operate a relayer (XMR, ZEC, AZT, LTC, universal)
- Run merge-mining infrastructure
- Host JSON-RPC for block explorer or other services

**Use the wallet (`dwow_wallet`) when you want to:**
- Manage keys and addresses
- Check balances
- Send and receive payments
- Interact with contracts (DeFi, DAO, gaming, identity, etc.)
- Deploy new contracts
- Do anything that involves *your* money or *your* data

**Both can coexist.** Run a daemon 24/7 on a server to mine and serve the
network. Use the wallet on your laptop to manage your coins. They use separate
datastores and do not interfere with each other. They sync from the same P2P
network independently.

**The wallet is not a light client.** It downloads and stores every block. It
can verify Merkle proofs locally. It does not trust the daemon for capability
discovery — it decrypts coin outputs with its own keys. The architectural split
is about *what you run when*, not about *how much you verify*.

## See Also

- [DarkWow Daemon](../dwowd.md) — full daemon architecture
- [Wallet Architecture](wallet.md) — wallet internals and capability model
- [Consensus](consensus/consensus.md) — Uncle Merkle consensus design
- [What's Different from Upstream](../about/differences_from_upstream.md) — fork comparison
- [P2P Network Connectivity](wallet.md#p2p-network-connectivity) — wallet P2P sync details
- [O-Cap & Composable Privacy](ocap.md) — capability security model
