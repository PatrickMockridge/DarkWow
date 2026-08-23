# Sync Protocol — ρ-Calculus Specification

This is the authoritative, **WYSIWYG** specification of DarkWow's linear blockchain
sync. Every constant, command name, message field, and file reference below matches the
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

Three commitments:

1. **Unified** — one `SyncPeer` (client) and one `SyncServer` (server), used identically
   by every role. A wallet dials a fixed peer list; a mining/observer node dials
   discovered peers *and* accepts inbound. The sync itself is the same process.
2. **WYSIWYG** — this document mirrors the code exactly. No aspirational sections.
3. **Fails clearly with inherent safety** — every failure is logged (no silent fails),
   and the design is safe by construction: magic bytes, protocol version, genesis hash,
   request timeouts, and size caps all reject bad peers/inputs up front.

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

---

## 2. Message Type Authority

The sync message types are the single source of truth, defined in
`dwow_chain::sync_types` (`src/linear/src/sync_types.rs`). No node defines its own copy.

```
GetTip, Tip, GetBlocks, Blocks
```

## 3. Nominal Types on the Wire

| Wire field | Nominal type | Encoding |
|-----------|--------------|----------|
| `GetBlocks.start_height` | `BlockHeight` | JSON number |
| `Tip.height` | `BlockHeight` | JSON number |
| `Tip.hash` | `BlockHash` | hex string |
| `Tip.genesis_hash` | `Option<BlockHash>` | hex string or absent |

`BlockHash` re-lifts only through `from_hex_str` (empty string = genesis sentinel → `None`;
wrong length → reject). It SHALL NOT be constructed by a bare `[u8; 32]` round-trip.

## 4. genesis_hash Validation

`Tip` carries `genesis_hash: Option<BlockHash>`. A receiver SHALL compare it against its
local genesis and skip mismatched peers before downloading blocks. `None` ⇒ unverified.

## 5. MAX_BYTES

| Message | MAX_BYTES |
|---------|-----------|
| `GetTip` | 256 |
| `Tip` | 512 |
| `GetBlocks` | 256 |
| `Blocks` | 16 MiB |

`Blocks` is 16 MiB to accommodate the genesis block (9 contract WASM deployments) served
alone. `MAX_BYTES = 0` (unlimited) SHALL NOT appear.

## 6. Message → Barb Declaration

| Message | BARBS |
|---------|-------|
| `GetTip`, `Tip`, `GetBlocks`, `Blocks` | `{↓verify, ↓sync-barrier, ↓gossip-forward}` |

`BarbId` is defined in `src/barb.rs`.

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
| `LINEAR_SYNC_BATCH` | `20` | max blocks per response (genesis served alone = 1) |

### 8.2 Command names

| Command | Message |
|---------|---------|
| `"lineargettip"` | `GetTip` |
| `"lineartip"` | `Tip` |
| `"lineargetblocks"` | `GetBlocks` |
| `"linearblocks"` | `Blocks` |
| `"synchello"` | `SyncHello` |
| `"synchelloack"` | `SyncHelloAck` |

### 8.3 Handshake

```
client → server:  synchello   { major: u64, minor: u64, genesis_hash: Option<BlockHash> }
server → client:  synchelloack { ok: bool }
```

The server SHALL set `ok = (major, minor) == SYNC_PROTOCOL_VERSION` **and** the client's
`genesis_hash` (if `Some`) matches the server's local genesis. On `ok = false`, both sides
log and drop the connection. The client SHALL treat `ok = false` as a hard error.

### 8.4 Client (`SyncPeer`)

```
dial(url, magic, genesis_hash, timeout) -> Result<SyncPeer>
request_tip(&mut self) -> Result<Tip>
request_blocks(&mut self, start_height, count) -> Result<Vec<Block>>
```

`dial` installs the rustls crypto provider (the sync connection bypasses `P2p::new`,
which would otherwise install it), dials TCP+TLS, then performs the handshake.

### 8.5 Server (`SyncServer`)

```
listen(url, magic, chain_state) -> Result<SyncServer>
run(self) -> Result<()>
```

`run` accepts connections forever, handshakes each, then serves `GetTip`/`GetBlocks` from
`CChainState`. `GetBlocks` at `BlockHeight::GENESIS` serves 1 block; otherwise serves
`min(count, LINEAR_SYNC_BATCH)`.

### 8.6 Port derivation

The sync listener is a **dedicated** port, distinct from the tx/broadcast P2P port (two
listeners cannot share a port):

- Node: serves sync on `inbound + SYNC_PORT_OFFSET` (`bin/dwowd/src/proto/mod.rs`).
- Wallet: dials sync on `peer + SYNC_PORT_OFFSET` (`bin/dww/src/sync_task.rs`).

The wallet reads its configured peers + magic from `p2p_settings`
(`dww.p2p_settings`), not from the legacy P2P session list.

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
| S6 | Payload size | `MAX_BYTES` per message | oversized payload rejected |
| S7 | Batch size | `LINEAR_SYNC_BATCH` (20), genesis alone | response trimmed |
| S8 | Observability | every dial/TLS/framing/handshake failure logs | `warn!`/`error!` always emitted |

S8 is load-bearing: it was the absence of a wallet tracing subscriber (and silent `Err`
returns in the legacy transport) that made four rounds of wallet-sync failures invisible
(`sync-hazop.md` R1/R2). The wallet now installs a subscriber
(`bin/dww/src/main.rs`), and the transport logs its failures
(`src/net/connector.rs`, `transport/{tcp,tls,mod}.rs`, `acceptor.rs`).

---

## 10. Testability

Each safety property has a runtime witness.

| Property | Test |
|----------|------|
| S1/S2/S3/S8 | `test_sync_connection_end_to_end` — dial+handshake+tip+blocks; **no-silent-fail** (dial refused → `Err`, logged) and **magic-mismatch** (→ `Err`) |
| full wallet sync | `test_wallet_sync_pulls_blocks_to_balance` — real `SyncServer` + `p2p_settings` → non-zero DRKW |
| wire format | `sync_types::tests::wire_format_golden` |
| S6 | `sync_handler::tests::max_bytes_sufficient_for_json_encoding` |
| re-lift (nominal types) | `consensus_coordination::test_peertip_rejects_invalid`, `test_tip_missing_genesis_hash_rejected`, `test_tip_max_height_rejected` |
| spec conformance | `python3 contrib/model/sync_model.py` (25 checks) |

The no-silent-fail assertion is the regression guard for the exact failure that the
Docker pipeline hit: a dial to an unreachable peer MUST return a logged error, never a
silent `peers=0`.

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
(unchanged). The mining/observer node's *client-side* pull (`consensus_linear.rs`) still
uses the legacy `LinearSyncClient` for node↔node sync; unifying it onto `SyncPeer` is a
follow-up.

The private-fee + `FeeThreshold_V1` threshold-proof model is unworkable in practice (the
wallet cannot know the miner's per-block key ahead of time) and is being replaced by a
public gas/fee model — deferred to a separate plan; see `fee-spec.md` §14.

---

## 12. Conformance

The Rust SHALL conform to this spec. The Python model `contrib/model/sync_model.py` is
the 1:1 executable specification; if the model and Rust disagree, the model is correct
until proven otherwise.
