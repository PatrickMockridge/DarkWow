# net-node Boundary — Type Translation Specification

This document defines the type translation boundary between the P2P transport
layer and the blockchain consensus layer. It is the specification to which all
implementation SHALL conform. It uses SHALL, MUST, SHALL NOT, MUST NOT per
RFC 2119.

Per type-system.md §10.5, every channel boundary is a `quote(x)`/`eval(x)` edge
in the ρ-calculus. The boundary SHALL satisfy four runtime obligations:
re-lift validation, violator exclusion, rate discipline, and budget declaration.

## 1. Three-Language Model

The P2P→consensus boundary speaks three distinct languages. Each has its own
type vocabulary, barb set, and translation rules.

### L1: P2P Wire Language

```
Vocabulary: { SerializedMessage, ChannelPtr, MessageSubscription<M> }
Barbs:      {} — bytes have no behavioral constraints (§2.2)
Domain:     src/net/channel.rs, src/net/message_publisher.rs
```

**Translation rules:**
- **L1.1**: Every `subscribe_msg::<M>()` SHALL be paired with a timeout on the
  resulting subscription. Bare `receive()` SHALL NOT compile in consensus code.
- **L1.2**: `subscribe_msg::<M>()` SHALL verify channel liveness before creating
  a subscription. Dead channel subscriptions SHALL return `Err(ChannelStopped)`.
- **L1.3**: Every message type crossing the blockchain boundary SHALL declare
  `Message::BARBS` with all barbs exhibited by that message's domain. The 5-arg
  form of `impl_p2p_message!` (BARBS defaults to `&[]`) SHALL NOT be used for
  blockchain boundary messages.

### L2: net-node Protocol Language

```
Vocabulary: { PeerTip }
Barbs:      { Verify, SyncBarrier, GossipForward, Commit, Mine }
Domain:     bin/dwowd/src/proto/linear_sync_client.rs
```

**Translation rules:**
- **L2.1**: Every boundary type SHALL have a validating constructor. Bare struct
  literals SHALL NOT construct boundary types from P2P message types.
- **L2.2**: Every boundary type SHALL implement `ExhibitsBarb`.
- **L2.3**: The L2→L3 boundary SHALL be a single typed decision point. Consensus
  code SHALL NOT contain P2P loop constructs (`loop { if has_peers() ... }`).
- **L2.4**: Violator exclusion SHALL happen at L2. After N consecutive failures
  from the same channel, the client SHALL call `ban()`.

### L3: Consensus State Machine Language

```
Vocabulary: { SyncState, BlockHeight, GenesisAuthority, Block }
Barbs:      { Mine, Commit, Verify, SyncBarrier }
Domain:     bin/dwowd/src/task/consensus_linear.rs, bin/dwowd/src/lib.rs
```

**Translation rules:**
- **L3.1**: Consensus code SHALL NOT import P2P types (`ChannelPtr`, `GetTip`,
  `Tip`, `GetBlocks`, `Blocks`, `subscribe_msg`, `send`, `receive`).
- **L3.2**: Every consensus state transition SHALL be triggered by a typed
  signal from L2, not by a bare boolean expression. Per §5.1: "A bare `bool`
  SHALL NOT gate consensus-critical paths."
- **L3.3**: `SyncState` SHALL have a variant for every distinguishable waiting
  condition.

## 2. The L2→L3 Decision Point

The L2→L3 translation is a single decision computed at the end of each pull
pass in `consensus_linear_init_task`
(`bin/dwowd/src/task/consensus_linear.rs:474-478`):

```rust
// LOCAL caught-up + a SEPARATE mining gate (Bitcoin IsInitialBlockDownload).
// Computed AFTER the pull so it reflects the post-pull height.
let caught_up = blockchain.get_height() >= max_peer_height;
let mine = caught_up && (authority || !sync_peers.is_empty());
node.mining_state.sync_state.store(
    if mine { SyncState::CaughtUp as u8 } else { SyncState::Behind as u8 },
    Ordering::SeqCst,
);
```

`mine` gates the miner (L3): a node mines only when it is at or above every
peer tip AND is either the genesis authority or has at least one sync peer
(node-sync-hazop.md F1). The result is written atomically to
`mining_state.sync_state` as `SyncState` (§3) — the miner reads it via
`SyncState::load()` (`lib.rs:1271`).

## 3. SyncState Machine

```
                    ┌──────────────────────────────┐
                    │          Behind (3)           │
                    │  Init / syncing / behind      │
                    │  peers / waiting for genesis  │
                    │  — miner paused in all of     │
                    │  these                        │
                    └──────────────┬───────────────┘
                                   │ sync completes
                                   ▼
                    ┌──────────────────────────────┐
                    │         CaughtUp (2)          │
                    │  Within range of tip — miner  │
                    │  may mine                     │
                    └──────────────┬───────────────┘
                                   │ peers advance
                                   ▼
                         (back to Behind (3))
```

`SyncState` (`bin/dwowd/src/lib.rs:169`) has exactly two variants.
`CaughtUp = 2` is stored only when the §2 decision passes; every other
condition — initializing, actively pulling, behind peers, waiting for
genesis — stores `Behind = 3`, which pauses the miner (`lib.rs:1271`).
`SyncState::load()` (`lib.rs:177`) maps code 2 to `CaughtUp` and every
other code (0/1/3/4) to `Behind`, so a stale or non-standard value in the
atomic can never unblock the miner. The discriminants 2/3 keep
`sync_state_code` wire values stable.

## 4. Barb Declaration Catalog

Every type crossing the net-node boundary SHALL declare its barb set.

### Boundary Types

| Type | Barbs | File |
|------|-------|------|
| `PeerTip` | `{Verify, SyncBarrier}` | `src/linear/src/sync_boundary.rs` (re-exported at `bin/dwowd/src/proto/linear_sync_client.rs`) |

### Protocol Handlers

| Type | Barbs | File |
|------|-------|------|
| `LinearSyncClient` | `{Verify, SyncBarrier}` | `bin/dwowd/src/proto/linear_sync_client.rs` |
| `LinearBroadcastHandler` | `{Commit, Verify, Broadcast, GossipForward}` | `bin/dwowd/src/proto/linear_broadcast.rs` |
| `ProtocolTxHandler` | `{Verify, Broadcast, GossipForward}` | `bin/dwowd/src/proto/protocol_tx.rs` |
| `ProtocolEventGraph` | `{DagParent, Broadcast, RateLimit, QuorumQuery}` | `src/event_graph/proto.rs` |

### Consensus Types

| Type | Barbs | File |
|------|-------|------|
| `GenesisAuthority` | `{Mine}` | `bin/dwowd/src/task/consensus_linear.rs` |
| `ConsensusInitTaskConfig` | `{Verify, SyncBarrier, GossipForward, Mine}` | `bin/dwowd/src/task/consensus_linear.rs` |

### P2P Message Types

| Type | Barbs | File |
|------|-------|------|
| `GetTip`, `Tip`, `GetBlocks`, `Blocks` | `{Verify, SyncBarrier, GossipForward}` | `src/linear/src/sync_types.rs` |
| `BlockBroadcast` | `{Commit, Verify, Broadcast, GossipForward}` | `bin/dwowd/src/proto/linear_broadcast.rs` |
| `Transaction` | `{Spend, Verify}` | `src/tx/mod.rs` |

## 5. Four Runtime Obligations — Satisfaction Status

Per type-system.md §10.5, every boundary SHALL satisfy four runtime obligations.
This section documents how each is satisfied at the net-node boundary.

### Obligation 1: Re-lift Validation

**Status: SATISFIED**

`PeerTip::from_tip(&Tip) -> Result<PeerTip>` validates:
1. Height within valid range (not `u64::MAX`)
2. Hash non-empty when height > 0
3. Genesis hash present when height > 0

Tests E (`test_peertip_rejects_invalid`) witnesses this obligation.

### Obligation 2: Violator Exclusion

**Status: PARTIALLY SATISFIED**

Channel failure tracking at L2 (per-channel consecutive failure counter).
After N failures: deprioritize for current sync pass. `ban()` call is
available at L1 but not yet wired at L2.

The L1 `ban()`/blacklist is now on DarkWow terms: the wallet uses `BanPolicy::Relaxed`
(never bans its fixed configured peers), and `HostColor::Black` entries expire after
`BLACKLIST_EXPIRY_SECS` (recoverable) rather than "for the program duration"
(`sync-hazop.md` R5).

Test F (`test_violator_exclusion_at_boundary`) — deferred until ban()
wiring is complete.

### Obligation 3: Rate Discipline

**Status: SATISFIED**

Every sync receive path enforces a timeout:
- `SyncPeer::request_tip`: 5 seconds (`TIP_TIMEOUT`, `src/linear/src/sync_connection.rs:66`)
- `SyncPeer::request_blocks`: 30 seconds (`BLOCKS_TIMEOUT`, `src/linear/src/sync_connection.rs:76`)
- `LinearSyncClient::dial_sync_peers`: 15 seconds per dead peer (`bin/dwowd/src/proto/linear_sync_client.rs:224`)

Bare `receive()` is impossible — tip/block requests go only through `SyncPeer`
methods, and peer discovery only through `dial_sync_peers`. No dedicated
runtime witness test asserts these timeouts yet (§9).

### Obligation 4: Budget Declaration

**Status: SATISFIED at L1, DECLARED at L2**

P2P message types declare `MAX_BYTES` and `METERING_CONFIGURATION` via
`impl_p2p_message!`. The boundary type `PeerTip` is
memory-managed by Rust's ownership model — no separate budget declaration
required at L2.

## 6. Enforcement Points

The barb declarations are enforced at two points in the P2P dispatch path:

1. **Message dispatch** (`src/net/message_publisher.rs`): `MessageDispatcher`
   implements `barbs()` returning `M::BARBS`. The `MessageSubsystem::notify()`
   method can check barb compatibility against the channel's session type.

2. **Channel subscription** (`src/net/channel.rs`): `subscribe_msg::<M>()`
   checks channel liveness before creating a subscription. Dead channel
   subscriptions return `Err(ChannelStopped)` instead of creating zombie
   subscriptions.

3. **Channel stop** (`src/net/channel.rs`): `handle_stop()` calls
   `trigger_error()` for BOTH graceful and error disconnects, ensuring
   all blocked `receive()` calls wake up with `Err(ChannelStopped)`.

## 7. Channel Lifecycle Phases

The P2P channel lifecycle has five phases. The consensus task SHALL have
visibility into which phase each channel is in:

| Phase | Connected? | Protocols? | In peers()? | Subscribers? |
|-------|-----------|------------|-------------|--------------|
| Connect | TCP open, no protocols | No | No | None |
| Register | Protocols init'd, not started | Yes | No | ProtocolGeneric subs active |
| Active | Handshake done, protocols started | Yes | Yes | All active |
| Stopping | Main loop dead, error broadcast | Yes | No (removed) | May get error, then hang |
| Stopped | Channel closed | No (subsystem alive) | No | Zombie (fixed: liveness check) |

`connected_peer_count()` on `Hosts` returns `(filtered, total)` — the gap
is the `SESSION_SEED | SESSION_REFINE` exclusion that `peers()` applies.

## 8. Gate Discipline

This specification defines the net-node boundary. It does NOT open the
following gates:

- **`net-full` gate**: CLOSED. No `BanPolicy`, `session-seed`, or transport
  plugins appear in consensus code.
- **`event-graph` gate**: CLOSED for dwowd. Event-graph types (`EventPut`,
  `EventReq`, etc.) are confined to `src/event_graph/` and darkirc/tau/evgrd.
  The `ProtocolEventGraph` handler declares `{DagParent, Broadcast, RateLimit,
  QuorumQuery}` — these barbs SHALL NOT appear on blockchain channels.

## 9. Integration Tests — Boundary Witnesses

Per §10.5: "Every declared SHALL at a boundary SHALL have at least one
runtime witness test."

| Test | Obligation | What It Witnesses |
|------|-----------|-------------------|
| `test_peertip_rejects_invalid` | #1 | PeerTip::from_tip rejects invalid data |
| `test_tip_missing_genesis_hash_rejected` | #1 | Tip with non-zero height but no genesis hash rejected |
| `test_tip_max_height_rejected` | #1 | Tip with u64::MAX height rejected |
| `test_getblocks_rejects_zero_start` | — | GetBlocks with start_height=0 is a semantic error (genesis is height 1) — documents the expectation |
| `test_barb_declarations_complete` | — | All boundary types have non-empty barb sets |
| `test_peertip_exhibits_correct_barbs` | — | PeerTip exhibits {Verify, SyncBarrier} |

All tests in `bin/dwowd/tests/consensus_coordination.rs`. Obligations #2
(violator exclusion — `test_violator_exclusion_at_boundary`, deferred until
`ban()` wiring is complete) and #3 (rate discipline) have no runtime witness
test yet.
