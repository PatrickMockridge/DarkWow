# Sync Protocol — ρ-Calculus Specification

This document is the authoritative specification of DarkWow's linear blockchain
sync protocol. It SHALL be the single source of truth for the sync message type
system, and it is founded in the ρ-calculus (see
[Type System §0](type-system.md#0-foundational-calculus) and
[§10 — P2P Network as Replicated Process Nets](type-system.md#10-p2p-network-as-replicated-process-nets)).
It uses SHALL, MUST, SHALL NOT, MUST NOT per RFC 2119.

It supersedes `sync.md` (which documented the pre-migration
`src/validator/sync/` API and is retained only as a historical pointer) and
generalises `net-node-boundary.md`'s L1/L2/L3 model from the mining node to
**every** sync participant: wallet, observer, and mining node.

---

## 0. The Sync Process

In the ρ-calculus, sync is a single replicated process net, identical across
every node role. Only the block sink differs.

```
Sync = SyncClient | SyncHandler | BlockSink
```

| Component | ρ-calculus role | Meaning |
|-----------|-----------------|---------|
| `SyncClient` | replicated `!GetTip?(…).Tip!(…)\|…` | Pulls tip + blocks from peers |
| `SyncHandler` | replicated `!GetTip?(…).Tip!(…)\|GetBlocks?(…).Blocks!(…)` | Serves tip + blocks to peers |
| `BlockSink` | the sole per-role process | Applies a received block |

`BlockSink` is a typed channel over `Block`, parameterised by role:

```
BlockSink(wallet)   = insert_block  | scan_block       (↓verify)
BlockSink(observer) = validate_block | execute_block | accept_block  (↓verify, ↓commit)
BlockSink(mining)   = validate_block | execute_block | accept_block | mine  (↓verify, ↓commit, ↓mine)
```

Every role runs the **same** `SyncClient` and `SyncHandler`. The wire bytes are
identical; only what happens to a block after it is re-lifted across the
boundary differs. This is the "works the same for wallets, observer nodes, and
mining nodes" invariant.

---

## 1. Message Type Authority

These types are the single source of truth for the linear sync wire protocol.
They live in `dwow_chain::sync_types` (`src/linear/src/sync_types.rs`). All
nodes — wallet (`dww`), observer, mining node (`dwowd`) — import from this one
module. **No node SHALL define its own copy.**

```
GetTip, Tip, GetBlocks, Blocks, GetBlock, BlockResponse
```

## 2. Nominal Types on the Wire

Every consensus scalar uses its nominal newtype, never a bare integer. Serde is
transparent (a `BlockHeight` serialises as a JSON number; a `BlockHash`
serialises as a hex string).

| Wire type | Nominal type | Notes |
|-----------|--------------|-------|
| `GetBlocks.start_height` | `BlockHeight` | not `u64` |
| `GetBlock.height` | `BlockHeight` | not `u64` |
| `Tip.height` | `BlockHeight` | not `u64` |
| `Tip.hash` | `BlockHash` | hex string, not bare `String` (§8.2.1) |
| `Tip.genesis_hash` | `Option<BlockHash>` | `#[serde(default)]` |

`BlockHash` SHALL re-lift only through `from_hex_str` (empty string = genesis
sentinel → `None`; wrong length → reject). It SHALL NOT be constructed by a
bare `[u8; 32]` round-trip across a module boundary (§2.2).

## 3. genesis_hash Validation

Every `Tip` carries `genesis_hash: Option<BlockHash>`. A receiver SHALL compare
it against its local genesis and skip mismatched peers **before** downloading
blocks. This is defense-in-depth against chain-identity confusion (HAZID
F1/F7), independent of the version handshake.

- `genesis_hash == None` ⇒ unverified (forward/backward compatible; old nodes
  omit it, new nodes treat it as unverified).
- `genesis_hash == Some(h)` and `h != local_genesis` ⇒ skip the peer.

## 4. Unified MAX_BYTES

Canonical wire caps, in bytes:

| Message | MAX_BYTES |
|---------|-----------|
| `GetTip` | 256 |
| `Tip` | 512 |
| `GetBlocks` | 256 |
| `GetBlock` | 256 |
| `Blocks` | 16 MiB |
| `BlockResponse` | 16 MiB |

`Blocks`/`BlockResponse` are 16 MiB to accommodate the genesis block (9
contract WASM deployments, measured ~11.35 MiB) served ALONE by
`handle_get_blocks`. `MAX_BYTES = 0` (unlimited) SHALL NOT appear on any sync
message type (§8.6.2). Metering is not a substitute for a declared cap.

## 5. Message → Barb Declaration

Every message type crossing the sync boundary SHALL declare `Message::BARBS`
with all barbs exhibited by its domain (L1.3). The 5-arg form of
`impl_p2p_message!` (BARBS defaults to `&[]`) SHALL NOT be used for sync
messages.

| Message | BARBS |
|---------|-------|
| `GetTip`, `Tip`, `GetBlocks`, `Blocks`, `GetBlock`, `BlockResponse` | `{↓verify, ↓sync-barrier, ↓gossip-forward}` |

Boundary and handler types declare their own sets (see §9). `BarbId` is defined
in `src/barb.rs` (24 variants, a 1:1 mirror of the Lean4 `Barb` inductive).

## 6. Wire Format Stability

The JSON wire shape is frozen. Any change to a sync message type that alters
the JSON shape SHALL break the golden test
`sync_types::tests::wire_format_golden` (`src/linear/src/sync_types.rs`).

- `GetTip` serialises as `null`.
- `Tip` serialises `height` (number), `hash` (hex string), `genesis_hash`
  (hex string or absent).
- Codec is varint-length-prefixed JSON (`varint_encode`/`varint_decode`).

## 7. Re-lift Validation + Runtime Obligations

Per §10.5, every channel boundary is a `quote(x)`/`eval(x)` edge with four
runtime obligations. The sync protocol SHALL satisfy all four on **both**
binaries:

1. **Re-lift validation** — boundary types SHALL be constructed only through a
   validating constructor (`PeerTip::from_tip`), never a bare struct literal.
2. **Violator exclusion** — after N consecutive failures from one channel, the
   client SHALL deprioritise/ban (`channel.ban()`).
3. **Rate discipline** — every receive has a timeout (`request_tip` 5s,
   `request_blocks` 30s). Bare `receive()` SHALL NOT compile in sync code.
4. **Budget declaration** — every message declares `MAX_BYTES` + `METERING_CONFIGURATION`.

---

## 8. Three-Language Model (generalised to both binaries)

`net-node-boundary.md` defined L1/L2/L3 for the mining node. This section
extends it so the **wallet** and **observer** speak the same languages.

### L1 — P2P Wire Language

```
Vocabulary: { SerializedMessage, ChannelPtr, MessageSubscription<M> }
Barbs:      {} — bytes have no behavioral constraints
Domain:     src/net/channel.rs, src/net/message_publisher.rs (shared)
```

### L2 — Sync Protocol Language

```
Vocabulary: { PeerTip, BlocksBatch, SyncDecision, SyncState }
Barbs:      { ↓verify, ↓sync-barrier, ↓commit }
Domain:     src/linear/src/sync_boundary.rs (shared)
```

This is the key change from `net-node-boundary.md`: L2 types now live in
`dwow_chain`, so the wallet reuses the same boundary types as the node. No
binary defines its own L2 vocabulary.

### L3 — Consumer State Machine Language

```
Vocabulary: { BlockHeight, Block, GenesisAuthority }
Barbs:      { ↓verify, ↓commit, ↓mine }
Domain:     bin/dww/src/sync_task.rs (wallet sink)
            bin/dwowd/src/task/consensus_linear.rs (observer/mining sink)
```

---

## 9. Boundary Types + Barb Catalog

Every type crossing the sync boundary SHALL declare its barb set.

| Type | Barbs | Home |
|------|-------|------|
| `PeerTip` | `{↓verify, ↓sync-barrier}` | `src/linear/src/sync_boundary.rs` |
| `BlocksBatch` | `{↓verify, ↓commit}` | `src/linear/src/sync_boundary.rs` |
| `SyncDecision` | N/A (enum, not a process) | `src/linear/src/sync_boundary.rs` |
| `SyncState` | N/A (enum, not a process) | `src/linear/src/sync_boundary.rs` |
| `LinearSyncClient` | `{↓verify, ↓sync-barrier}` | shared sync module |
| `LinearSyncHandler` | `{↓verify, ↓sync-barrier, ↓gossip-forward}` | `src/linear/src/sync_handler.rs` |
| `BlockHash` | `{↓verify}` | `src/linear/src/sync_types.rs` |

### SyncDecision (L2→L3 translation point)

```
enum SyncDecision { PeersAvailable | ProceedSolo | WaitForGenesis | Retry }
```

The consensus/sync task matches exhaustively; adding a variant without updating
the match is a compile error.

### SyncState machine

```
Initial(0) → Syncing(1) ⇄ Behind(3)
Initial(0) → CaughtUp(2)
Initial(0) → WaitingForGenesis(4)
```

---

## 10. Process Hierarchy

The three node roles map to the ρ-calculus process hierarchy, all sharing the
same `Sync` process:

```
ProcessNet(wallet)   = Sync | ProtocolAddress | ProtocolVersion
ProcessNet(observer) = ProcessNet(wallet) | ValidateRelay
ProcessNet(mining)   = ProcessNet(observer) | Mine
```

So `ProcessNet(wallet) ⊂ ProcessNet(observer) ⊂ ProcessNet(mining)`, and any
peer can sync from any other because later tiers add processes without removing
the shared `Sync` process (see
[Wallet vs Daemon](wallet-vs-daemon.md#processnet-mapping)).

---

## 11. Conformance

The Rust SHALL conform to this spec. The Python model
`contrib/model/sync_model.py` is the 1:1 executable specification; if the model
and Rust disagree, the model is correct until proven otherwise.
