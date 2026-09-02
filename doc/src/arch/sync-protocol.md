# Sync Protocol — ρ-Calculus Specification

This is the authoritative, **WYSIWYG** specification of DarkWow's linear blockchain
sync. Every constant, command name, message field, and code reference below matches the
code 1:1 — the primary implementation is `src/linear/src/sync_connection.rs`. If the
spec and code disagree, the code is the bug; fix the code, not the spec.

It is founded in the ρ-calculus (see
[Type System §0](type-system.md#0-foundational-calculus) and
[§10 — P2P Network as Replicated Process Nets](type-system.md#10-p2p-network-as-replicated-process-nets)).
It supersedes `sync.md` (pre-migration) and is the output of the HAZOP in
[sync-hazop.md](sync-hazop.md). Uses SHALL / MUST / SHALL NOT / MUST NOT per RFC 2119.

---

## 0. Philosophy

Sync is a **single, minimal, pull-based chain sync** — one code path for the wallet,
the observer, and the mining node. It is shaped like Monero's chain sync (connect →
handshake → pull blocks in batches) and Electrum's simple client pull. It replaced the
divergent session/hostlist/seed/refine/ban slice of the legacy P2P stack that the wallet
and node previously rode on separately (the root cause of four silent wallet-sync
failures — see `sync-hazop.md`).

Production pattern: Monero's `handle_get_objects`/`handle_get_hashes` batch pull and
Electrum's client-driven tip query; the single-rail design is DarkWow-specific.

Four commitments:

1. **Unified** — one `SyncPeer` (client) and one `SyncServer` (server), used identically
   by every role. A wallet dials a fixed peer list; a mining/observer node dials
   discovered peers *and* accepts inbound. The sync itself is the same process.
2. **WYSIWYG** — this document mirrors the code exactly. No aspirational sections.
3. **Fails clearly with inherent safety** — every failure is logged (no silent fails),
   and the design is safe by construction: magic bytes, protocol version, genesis hash,
   request timeouts, and size caps all reject bad peers/inputs up front.
4. **Never blocks the critical path; proven patterns only** — nothing on the sync path
   (especially wallet sync) blocks: a component that cannot make progress warns (if a
   warning is required) and continues the next tick, never holding or deadlocking. Every
   sync pattern is a proven production pattern, chosen from a documented range by fitness
   for purpose (§18).

---

## 1. The Sync Process (ρ-calculus)

```
Sync = SyncPeer | SyncServer | BlockSink
```

| Component | ρ-calculus role | Rust type |
|-----------|-----------------|-----------|
| `SyncPeer` | `SyncClient = !νc. connect(c, peer) . handshake(c) . (GetTip!(c) \| Tip?(c)) . (GetBlocks!(c) \| Blocks?(c))` | `sync_connection::SyncPeer` |
| `SyncServer` | `SyncHandler = !νc. accept(c) . handshake(c) . (GetTip?(c).Tip!(c) \| GetBlocks?(c).Blocks!(c))` | `sync_connection::SyncServer` |
| `BlockSink` | the sole per-role process | wallet `insert_synced_block`, node `accept_block` |

`BlockSink` is where roles differ — the wallet inserts+scans (↓verify), the observer/
mining node validates+executes+accepts (↓verify, ↓commit, ↓mine). The sync wire path is
**identical** for every role.

Production pattern: DarkWow-specific (ρ-calculus process-net framing of a
Monero/Electrum pull loop).

---

## 2. Message Type Authority

The sync message types are the single source of truth, defined in
`dwow_chain::sync_types` (`src/linear/src/sync_types.rs`). No node defines its own copy.

```
GetTip, Tip, GetBlocks, Blocks, BroadcastTx, BroadcastTxAck
```

The handshake pair `SyncHello` / `SyncHelloAck` is defined in `sync_connection.rs`
(§8.3), not in `sync_types`. `BroadcastTx` / `BroadcastTxAck` are **sync-rail-only**:
they are not registered via `impl_p2p_message!`.

Production pattern: Bitcoin Core's single `protocol.h`/`net` message enum and Monero's
single `cryptonote_protocol` — one shared definition, no per-node copies.

## 3. Nominal Types on the Wire

| Wire field | Nominal type | Encoding |
|-----------|--------------|----------|
| `GetBlocks.start_height` | `BlockHeight` | JSON number |
| `Tip.height` | `BlockHeight` | JSON number |
| `Tip.hash` | `BlockHash` | hex string |
| `Tip.genesis_hash` | `Option<BlockHash>` | hex string or absent |

`BlockHash` re-lifts only through `from_hex_str` (empty string = genesis sentinel → `None`;
wrong length → reject). It SHALL NOT be constructed by a bare `[u8; 32]` round-trip.

Production pattern: Bitcoin Core uses `uint256` for hashes and a dedicated
`arith_uint256` for consensus; Monero wraps scalars in nominal crypto types. DarkWow's
`BlockHash`/`BlockHeight` are the same nominal-newtype discipline.

## 4. MAX_BYTES (unified-rail frame bound)

The sync rail enforces **one** upper bound on any sync-frame payload:
`MAX_FRAME_PAYLOAD = 16 MiB` (`sync_connection.rs`). It is applied uniformly in
`read_frame` to **every** command — `GetTip`, `Tip`, `GetBlocks`, `Blocks`,
`BroadcastTx`, and `BroadcastTxAck` alike. A peer-controlled payload length above
`MAX_FRAME_PAYLOAD` SHALL be rejected **before** allocation (`read_frame` returns
`Err(InvalidData, "payload too large")`), preventing an unbounded
`vec![0u8; payload_len]` allocation.

Production pattern: Bitcoin Core bounds inbound P2P payloads with a single
`MAX_SIZE`/`MAX_PROTOCOL_MESSAGE_LENGTH` guard before allocation; geth bounds
`MaxMessageSize`; Monero caps `MAX_BLOCK_SIZE`. DarkWow applies the same single-rail
bound across all sync commands.

## 5. genesis_hash Validation

`Tip` carries `genesis_hash: Option<BlockHash>`. A receiver SHALL compare it against its
local genesis and skip mismatched peers before downloading blocks. `None` ⇒ unverified.

Production pattern: Bitcoin Core's network magic + genesis block hash and Monero's
genesis hash provide the same "same-chain" precondition; geth pairs network id with the
genesis hash. DarkWow carries the genesis hash in-band on `Tip` so a receiver rejects a
wrong-chain peer before fetching a single block.

## 7. Wire Format

Each frame is, in order:

```
magic (4 raw bytes)
command_len (varint)  command (UTF-8 bytes)
payload_len (varint)  payload (JSON bytes)
```

The JSON payload is the serde_json encoding of the message (`GetTip` serialises as
`null`). Command length SHALL NOT exceed 255 bytes. A magic-bytes mismatch SHALL abort
the connection with a logged error.

Production pattern: Bitcoin Core's `magic | command(12) | length | checksum | payload`
frame; DarkWow substitutes a varint command length and JSON payload.

---

## 8. The Unified Sync Connection (WYSIWYG)

Primary implementation: `src/linear/src/sync_connection.rs` (gated `sync-p2p`).

### 8.1 Constants

| Constant | Value | Meaning |
|----------|-------|---------|
| `SYNC_PROTOCOL_VERSION` | `(1, 0)` | exchanged at handshake; mismatch rejected |
| `SYNC_PORT_OFFSET` | `2` | sync listener = node inbound port + offset |
| `TIP_TIMEOUT` | `5s` | every `GetTip` request |
| `BLOCKS_TIMEOUT` | `30s` | every `GetBlocks` request |
| `BROADCAST_TIMEOUT` | `10s` | every `BroadcastTx` request (single frame, shorter than block fetch) |
| `LINEAR_SYNC_BATCH` | `20` | max blocks per response (genesis served alone = 1) |
| `MAX_BATCH_BYTES` | `12 MiB` | cumulative encoded-size budget, under the 16 MiB `Blocks` cap |
| `MAX_FRAME_PAYLOAD` | `16 MiB` | unified upper bound on any sync-frame payload (§4) |

### 8.2 Command names

| Command | Message |
|---------|---------|
| `"lineargettip"` | `GetTip` |
| `"lineartip"` | `Tip` |
| `"lineargetblocks"` | `GetBlocks` |
| `"linearblocks"` | `Blocks` |
| `"synchello"` | `SyncHello` |
| `"synchelloack"` | `SyncHelloAck` |
| `"broadcasttx"` | `BroadcastTx` |
| `"broadcasttxack"` | `BroadcastTxAck` |

### 8.3 Handshake

```
client → server:  synchello   { major: u64, minor: u64, genesis_hash: Option<BlockHash> }
server → client:  synchelloack { ok: bool }
```

The server SHALL set `ok = (major, minor) == SYNC_PROTOCOL_VERSION` **and** the client's
`genesis_hash` (if `Some`) matches the server's local genesis. On `ok = false`, both sides
log and drop the connection. The client SHALL treat `ok = false` as a hard error.

Production pattern: Bitcoin Core's `version`/`verack` and Monero's
`HANDSHAKE`/`HANDSHAKE_OK` pair; DarkWow adds the genesis hash to the handshake.

### 8.4 Client (`SyncPeer`)

```
dial(url, magic, genesis_hash, timeout) -> Result<SyncPeer>
request_tip(&mut self) -> Result<Tip>
request_blocks(&mut self, start_height, count) -> Result<Vec<Block>>
broadcast_tx(&mut self, tx) -> Result<String>
```

`dial` installs the rustls crypto provider (the sync connection bypasses `P2p::new`,
which would otherwise install it), dials TCP+TLS, then performs the handshake.

### 8.5 Server (`SyncServer`)

```
listen(url, magic, chain_state, tx_sink) -> Result<SyncServer>
run(self) -> Result<()>
```

`run` accepts connections forever, handshakes each, then serves `GetTip`/`GetBlocks` from
`CChainState` and forwards `BroadcastTx` to the optional `tx_sink`.
`GetBlocks` at `BlockHeight::GENESIS` serves 1 block; otherwise serves
`min(count, LINEAR_SYNC_BATCH)`.

### 8.6 Port derivation

The sync listener is a **dedicated** port, distinct from the tx/broadcast P2P port (two
listeners cannot share a port):

- Node: serves sync on `inbound + SYNC_PORT_OFFSET` (`bin/dwowd/src/proto/mod.rs`).
- Wallet: dials sync on `peer + SYNC_PORT_OFFSET` (`bin/dww/src/sync_task.rs`).

The wallet reads its configured peers + magic from `p2p_settings`
(`dww.p2p_settings`), not from the legacy P2P session list.

Production pattern: DarkWow-specific (dedicated `+2` sync listener; Bitcoin Core and
Monero serve block sync on the same P2P port).

---

## 9. Inherent Safety

Every property below is enforced by the sync connection itself and **fails with a logged
error** — there is no silent-fail path.

| # | Property | Enforcement | Failure mode |
|---|----------|-------------|--------------|
| S1 | Network identity | 4 magic bytes checked on every frame (`read_frame`) | `warn!` "magic bytes mismatch", connection dropped |
| S2 | Protocol version | `SYNC_PROTOCOL_VERSION` checked at handshake | handshake `ok=false`, both sides `warn!` |
| S3 | Chain identity | `genesis_hash` checked at handshake (and in `Tip`) | handshake `ok=false` / peer skipped |
| S4 | Request liveness | `TIP_TIMEOUT` (5s), `BLOCKS_TIMEOUT` (30s) on every request | timeout → `Err`, retried |
| S5 | Command size | command length ≤ 255 bytes | `Err(InvalidData)` |
| S6 | Payload size | `MAX_FRAME_PAYLOAD` (16 MiB) unified-rail bound in `read_frame` | oversized payload rejected before allocation |
| S7 | Batch size | `LINEAR_SYNC_BATCH` (20) + `MAX_BATCH_BYTES` (12 MiB), genesis alone | response trimmed |
| S8 | Observability | every dial/TLS/framing/handshake failure logs | `warn!`/`error!` always emitted |

S8 is load-bearing: it was the absence of a wallet tracing subscriber (and silent `Err`
returns in the legacy transport) that made four rounds of wallet-sync failures invisible
(`sync-hazop.md` R1/R2). The wallet now installs a subscriber
(`bin/dww/src/main.rs`), and the transport logs its failures
(`src/net/connector.rs`, `transport/{tcp,tls,mod}.rs`, `acceptor.rs`).

Production pattern: Bitcoin Core's `CheckMagicAndCommand`/message-size guards, geth's
`discard`-on-oversize, and Monero's `CORE_SYNC_DATA_MAX_SIZE`; S8's no-silent-fail is
DarkWow-specific.

---

## 10. Testability

Each safety property has a runtime witness.

| Property | Test |
|----------|------|
| S1/S2/S3/S8 | `test_sync_connection_end_to_end` — dial+handshake+tip+blocks; **no-silent-fail** (dial refused → `Err`, logged) and **magic-mismatch** (→ `Err`) |
| full wallet sync | `test_wallet_sync_pulls_blocks_to_balance` — real `SyncServer` + `p2p_settings` → non-zero DRKW |
| wire format | `sync_types::tests::wire_format_golden` |
| re-lift (nominal types) | `consensus_coordination::test_peertip_rejects_invalid`, `test_tip_missing_genesis_hash_rejected`, `test_tip_max_height_rejected` |
| spec conformance | `python3 contrib/model/sync_model.py` (31 checks) |

The no-silent-fail assertion is the regression guard for the exact failure that the
Docker pipeline hit: a dial to an unreachable peer MUST return a logged error, never a
silent `peers=0`.

Production pattern: Bitcoin Core's functional/regression test suite and Monero's
regression tests pin the same runtime invariants; the Python-model executable spec is
DarkWow-specific.

---

## 11. Reuse

The connection reuses the clean primitives and writes fresh only the hodge-podge:

- **Reuse** `dwow_core::net::transport` (TCP + TLS dial/listen) and
  `dwow_chain::sync_types` (message types + serde_json codecs).
- **Write fresh** the framing, the version/genesis handshake, and the dial/accept loops.
  No hostlist, no seed/refine sessions, no ban-policy, no seed-error protocol, no
  metering map.

> Note: this "no ban-policy" applies to the unified `SyncPeer`/`SyncServer` block-sync rail.
> The wallet's P2P `ManualSession` (which drives `Peers`/`is_synced()`) still carries the
> legacy ban/blacklist from `dwow_core::net`; on DarkWow terms the wallet sets
> `BanPolicy::Relaxed` (never bans its configured peers) and the `Black` hostlist now expires
> after `BLACKLIST_EXPIRY_SECS` (`sync-hazop.md` R5).

The wallet's **transaction broadcast** now rides this same `SyncPeer`/`SyncServer` rail
via a `BroadcastTx` command — the wallet no longer keeps a separate `dwow_core::net::P2p`
socket for tx send. The node forwards `BroadcastTx` into its mempool through the same
admission path as the P2P `ProtocolTx` handler (`admit_tx_to_mempool`).

The mining node's *own* tx relay and block broadcast remain on `dwow_core::net`
(unchanged). The mining/observer node's *client-side* pull (`consensus_linear.rs`) now
dials `SyncPeer` via `dial_sync_peers`, so node↔node sync rides the same unified
`SyncPeer`/`SyncServer` rail as the wallet.

The private-fee + `FeeThreshold_V1` threshold-proof model is unworkable in practice (the
wallet cannot know the miner's per-block key ahead of time) and is being replaced by a
public gas/fee model — deferred to a separate plan; see `fee-spec.md` §14.

Production pattern: DarkWow-specific (single transport reused for wallet, observer, and
node; Monero/Bitcoin keep separate wallet/daemon transports).

---

## 12. Conformance

The Rust SHALL conform to this spec. The Python model `contrib/model/sync_model.py` is
the 1:1 executable specification; if the model and Rust disagree, the model is correct
until proven otherwise.

Production pattern: DarkWow-specific (a runnable Python model as the normative spec,
analogous to Bitcoin Core's functional tests and Monero's regression suite).

---

## 13. Async Production Logic

The ρ-calculus process net becomes executable only through the process→task mapping of
[Type System §9](type-system.md#9-concurrent-execution-model). Every ρ-process is exactly one
`smol::Task<T>`; the timing, retry and backoff below are the *production logic* that turns the
calculus into a real, observable process. There is no "background" work that is not a named task.

### 13.1 Process → task mapping

| ρ-process | Rust task (one `smol::Task`) | Location |
|-----------|------------------------------|----------|
| `SyncClient` (wallet) | `run_wallet_sync` | `bin/dww/src/sync_task.rs` |
| `SyncClient` (node) | `consensus_linear_init_task` | `bin/dwowd/src/task/consensus_linear.rs` |
| `SyncHandler` | `SyncServer::run` (accept-forever) | `src/linear/src/sync_connection.rs` |
| `BlockSink` (wallet) | `insert_synced_block` + `scan_blocks` | `bin/dww/src/lib.rs`, `bin/dww/src/scan.rs` |
| `BlockSink` (observer/miner) | `accept_block` | `bin/dwowd/src/block_acceptor.rs` |

### 13.2 Timeouts and re-poll

| Constant | Value | Justification |
|----------|-------|---------------|
| `TIP_TIMEOUT` | 5 s | a `Tip` is small; 5 s exceeds any RTT the transports allow |
| `BLOCKS_TIMEOUT` | 30 s | a `Blocks` batch over a slow link needs headroom |
| node sync re-poll | 30 s | one pull pass per tick; a caught-up node does not spam peers |

### 13.3 The pull loop

The node's sync client (`consensus_linear_init_task`) SHALL be a single pull loop, identical in shape
to the wallet's (`bin/dww/src/sync_task.rs`):

1. Every 30 s tick, dial full-node peers over the sync rail (`dial_sync_peers`).
2. Request each peer's tip and take `max_peer_height = max(...)`.
3. `caught_up = local_height >= max_peer_height`; `mine = caught_up AND (authority OR has_peers)`.
4. While `local_height < max_peer_height`, request `request_blocks(next_height, batch)` and accept each
   block through the full validation path. On a request failure, try the next peer; on a non-canonical
   or invalid block, stop the pass (never reorg).

Peer discipline SHALL be a single persistent score (Bitcoin Core `Misbehaving()`): a peer that serves
an **invalid block** is disconnected; a peer that times out is simply skipped and the next peer tried.
There is no deadness/slowness taxonomy, no round-robin, no bounded backoff, no heartbeat, and no
watchdog — those are DarkWow-idiosyncratic machinery with no production analogue. The wallet (fixed
peer set) uses the same loop: a dial failure skips that peer and re-ticks in 10 s.

Production pattern: Monero's pull sync (connect → handshake → `get_objects` batch pull) and Bitcoin
Core's `IsInitialBlockDownload` + `Misbehaving()`.

---

## 14. Command dispatch (P2P rail)

### 14.1 Command dispatch matrix + unknown-command drain

The P2P message/command dispatch layer is split by role. A command with no registered dispatcher
SHALL NOT desync the receiving channel's frame stream.

| Command | Message | Node (dwowd) | Wallet (dww `ManualSession`) |
|---|---|---|---|
| `"linearlblock"` | `BlockBroadcast` | registers (relay/apply) | **not registered** — drain-and-ignore |
| `"tx"` | `Transaction` | registers (mempool admit) | **not registered** — drain-and-ignore |
| `"lineargettip"`/`"lineartip"`/`"lineargetblocks"`/`"linearblocks"`/`"synchello"`/`"synchelloack"`/`"broadcasttx"`/`"broadcasttxack"` | sync messages | sync rail (§13) | sync rail (§13) |
| `version`/`verack`/`ping`/`pong`/`getaddr`/`addr`/`seederr` | base handshake/keepalive | registers | registers |

`linearlblock` (one-hop block broadcast) and `tx` (transaction relay) are **node-only push
commands**; they ride the legacy `dwow_core::net` rail (§11), not the pull sync rail. A peer that
does not subscribe to them (the wallet) SHALL **drain-and-ignore** them, never desync.

**`linearlblock` block-apply — duplicate vs invalid.** When a node applies a `linearlblock` push, it SHALL
distinguish two cases before any `ban()`:

- **Duplicate** — the block is already in the chain, detected **by block hash** (not by height), as the
  **first step before any PoW/ZK validation**. This is normal P2P relay. The node SHALL skip it (log + drop)
  and SHALL NOT call `ban()`. Bitcoin Core `AcceptBlock` does `LookupBlockIndex(block_hash)` first and
  returns "known" with zero work.
- **Genuine invalid** — the block fails PoW, proof-of-token-balance, structural validation, or the
  merkle/nullifier-root check. This is a protocol violation and SHALL trigger `ban()`.

A duplicate must not be re-executed: re-running `pow_reward_v1` on an already-committed block hits
`Duplicate commitment in output`, which the current code misclassifies as "invalid" and bans every relay
peer — parking join nodes in `Behind` on a fresh L1 chain.

**Unknown-command dispatch-or-drain (frame-aligned by construction).** A command with no registered
dispatcher is *drained* — the receive loop reads the whole frame (header + `msg_len ‖ payload`) and
discards the payload — so the stream stays frame-aligned and the caller can honour its role's ban
policy (wallet `Relaxed` log-and-continue, node `Strict` ban). This is the
**interior** invariant of type-system.md §10.5.2, proved in
`proofs/lean/src/DarkFi/Net/Framing.lean` (`dispatchOrDrain_total`, `recvLoop_frame_aligned`); it is
enforced by the receive loop's type, not by a runtime check. The `MAX_INBOUND_PAYLOAD` bound (4 MiB)
is the declared `drain` budget (type-system.md §10.5 obligation 4): a length over the bound is
`MessageInvalid`.

- `BanPolicy::Relaxed` (wallet): log-and-continue — the whole frame was consumed (dispatched or
  drained), so the NEXT frame parses cleanly. The prior behaviour — return `MissingDispatcher`
  without consuming the payload — left the stream half-read, so the next `read_command` misread the
  payload bytes as the magic header, producing a `Magic bytes mismatch` → `Malformed packet`
  teardown/reconnect loop — a **bug**, not an acceptable "log-and-continue".
- `BanPolicy::Strict` (node): ban + close (unchanged).

Enforcement: `contrib/ci/check_sync_conformance.sh` asserts the node registers `linearlblock`/`tx`
and the wallet does not; `contrib/model/sync_model.py` models the drain invariant.

Production pattern: Bitcoin Core's `getaddr`/`inv` dispatch and Monero's command-map both
frame-align and drop unknown commands; the wallet's drain-and-ignore is DarkWow-specific.

---

## 17. Wallet follows the longest chain

The wallet is **PoW-blind**: it does not import the RandomX VM, so it cannot verify a block's proof
of work. It selects its chain target the way a Monero wallet selects its daemon's chain: **follow the
longest (highest) peer-reported tip**.

- Each tick, the wallet queries every configured peer for its tip `(height, hash)`.
- The wallet adopts the **highest** reported tip and fetches blocks up to it, in order, validating
  each block's tx-merkle root and chain continuity.
- A peer reporting a lower or divergent tip never blocks the wallet — the wallet follows the highest
  tip it observed. A discrepancy is at most a `warn!`, never a hold.

PoW verification is a documented trust gap: the wallet trusts its configured peers to serve the
honest chain, exactly the way a Monero wallet trusts its local daemon. This replaces the prior
behaviour — a monotonic `HighestPeerTip` and a quorum vote — neither of which is a production pattern,
and the latter of which blocked the critical path.

Production pattern: Monero CLI wallet → local daemon (trusted-daemon sync); Electrum SPV's
"follow the longest header chain" is the closest Bitcoin analogue.

---

## 18. Critical-Path Principle and Proven Production Patterns

### 18.1 The critical path never blocks

No sync operation SHALL block the critical path. A component that cannot make progress in a tick —
no peers, a failed dial, a failed tip/block request, a tip discrepancy, a rejected block — SHALL
**warn** (if a warning is required) and **continue to the next tick**. It SHALL NOT hold, retry
forever, or gate the fetch loop on an unresolved condition.

The wallet is the binding case: it never stops fetching because peers disagree. A tip discrepancy is
at most a `warn!`; the wallet proceeds with the longest chain it observed. The node may skip a
misbehaving peer (§13.3) but never blocks on it.

Production pattern: Monero's non-blocking sync and Bitcoin Core's asynchronous
`ActivateBestChain`; the no-hold guarantee for the wallet is DarkWow-specific.

### 18.1.1 Caught-up is a LOCAL property; mining is a separate gate

"Caught up" is a **local** property, not a peer-count property (Bitcoin Core `IsInitialBlockDownload`). A
node is caught up iff `local_height >= max_peer_height` (the highest tip reported by any peer, or 0 with no
peers). This comparison does **not** require a peer to be present.

**Mining** is a separate gate: `mine iff caught_up AND (authority OR has_peers)`. The genesis authority
(`CREATE_GENESIS`) mines solo; a join node that is caught up but peerless does **not** mine (it cannot
propagate blocks) — but it is *caught up*, not `Behind`. This removes the old "CaughtUp requires peer
evidence" rule and its authority exception, which repeatedly parked a synced join node in `Behind` forever
and then required a solo-mining exception that forked the chain.

Production pattern: Bitcoin Core `IsInitialBlockDownload` — mining is gated until the chain is
caught up to a peer.

### 18.2 Proven production patterns

Every sync pattern is a proven production pattern, selected from a range by fitness for purpose:

| Pattern | Production source | Selected for |
|---|---|---|
| Full validation — verify PoW, execute, accept | Bitcoin Core / Monero daemon / geth full sync | mining node, observer |
| Header-chain + merkle proofs (SPV) | Bitcoin SPV wallet / Electrum | — (DarkWow has no header chain) |
| Trusted-daemon sync — follow the longest reported chain, trust the peer for PoW | Monero CLI wallet → local daemon | **wallet** (PoW-blind, fixed configured peers) |

The wallet is PoW-blind (no RandomX VM), so it is fitness-selected to the **trusted-daemon** pattern:
it follows the highest peer-reported tip and trusts its configured peers for PoW. It does **not** run
a quorum, a supermajority vote, or a warn-and-hold reorg gate — those are not production patterns and
would block the critical path.

---

## 19. Fork rule: uncle rewards, not reorg

DarkWow resolves forks by **uncle rewards, not reorg** (see `uncle_merkle.md`). A competing block is
stored as an uncle, never reorged. `detect_reorg` (`chain_state.rs`) SHALL, **before** WASM execution,
recognise a next-height block that builds on a competing (uncle) parent and store it as a competing
block (`store_competing_block`) → `UncleExtended`, never executed against the wrong cumulative state.

### 19.1 Contracts-tree undo (`CBlockUndo`)

`accept_block` SHALL capture a contracts-tree undo record (`CBlockUndo`) at connect time — the old
value of every key the WASM execution overlay touched — and store it in `store.contracts_undo` keyed
by height. `disconnect_block` SHALL replay that undo record to reverse **every** contracts-tree write
(not just the three cumulative singletons), then remove the record.

The undo record SHALL be written **atomically with the block commit** (in the same cross-tree sled
transaction as the block, its contracts batch, the supply entry, and the commitment/nullifier sets).
A crash between the block commit and the undo write would leave a canonical block whose contracts-tree
writes could never be reversed; the undo therefore SHALL NOT be a separate post-commit write.

Production pattern: Bitcoin Core writes `CBlockUndo` atomically with the block on `ConnectBlock` and
applies `ApplyBlockUndo` on `DisconnectBlock`; geth journals state and reverts via its snapshot/dirty
trie. DarkWow persists a per-block inverse-op batch and replays it on disconnect.

### 19.2 Symmetric disconnect

`disconnect_block` SHALL reverse **every** state transition `connect_block` performed, in the reverse
order, so a disconnect followed by a reconnect is a no-op:

1. the block record, the coinbase/fee/uncle note commitments and nullifiers, the cumulative-supply
   entry, and the cached next-block target;
2. the consensus state (accumulated work, timestamp window, target);
3. the per-block uncle records (the sled `uncles` tree entries and the in-memory
   `uncle_commitment_set`), via a per-height uncle-hash index captured at connect time;
4. the cumulative-supply in-memory cache (rolled back to the predecessor height);
5. the WASM contracts-tree writes, via the §19.1 undo record.

Production pattern: Bitcoin `DisconnectBlock` reverses exactly what `ConnectBlock` wrote (undo data,
coin-view cache, `setBlockIndexCandidates`); geth reverts the journal. A partial disconnect leaves the
node permanently divergent, so reversal is exhaustive.

---

## Conformance — code witnesses

Normative clauses above are witnessed by these functions (grep by name; no line numbers):

| Clause | Witness |
|--------|---------|
| §4 unified frame bound | `sync_connection::read_frame` (`MAX_FRAME_PAYLOAD`) |
| §5 genesis re-lift | `sync_types::BlockHash::from_hex_str`, `sync_boundary::PeerTip::from_tip` |
| §7 wire format | `sync_connection::write_json_frame` / `read_frame`, `sync_types::varint_encode` / `varint_decode` |
| §8.1 constants | `sync_connection` consts `SYNC_PROTOCOL_VERSION`, `SYNC_PORT_OFFSET`, `TIP_TIMEOUT`, `BLOCKS_TIMEOUT`, `BROADCAST_TIMEOUT`, `LINEAR_SYNC_BATCH`, `MAX_BATCH_BYTES`, `MAX_FRAME_PAYLOAD` |
| §8.3 handshake | `SyncPeer::dial`, `sync_connection::serve_conn` (`SyncHello`/`SyncHelloAck`) |
| §8.4 client | `SyncPeer::request_tip`, `SyncPeer::request_blocks`, `SyncPeer::broadcast_tx` |
| §8.5 server | `SyncServer::listen`, `SyncServer::run`, `sync_connection::serve_conn` |
| §13.3 pull loop | `consensus_linear::consensus_linear_init_task`, `linear_sync_client::dial_sync_peers` |
| §18.1.1 caught-up + mining gate | `consensus_linear::consensus_linear_init_task` (`caught_up` / `mine`) |
| §19 uncle rewards | `chain_state::detect_reorg` (store-as-uncle), `chain_state::store_competing_block`, `chain_state::disconnect_block` |
