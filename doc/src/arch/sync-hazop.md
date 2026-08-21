# Sync HAZOP + ρ-Calculus Trace

This is the exhaustive Hazard and Operability study of DarkWow's sync code, produced after the
fourth consecutive wallet-sync failure in the Docker pipeline (`wallet-1 never synced any
blocks after 600s`, `peers=0`, with no error logged). It identifies the root causes, traces the
sync code through the ρ-calculus model, and documents the divergences the rewrite must close.

All findings cite `file:line` on `linear-master`. This document is the specification input to
the unified sync-connection rewrite (see `sync-protocol.md`).

---

## 1. System boundary and scope

- **In scope**: the connection + sync path a node uses to pull or serve blocks:
  dial → transport → TLS → framing → version/verack handshake → peer registration →
  `GetTip`/`Tip` → `GetBlocks`/`Blocks`, for wallet (`bin/dww`), observer, and mining node
  (`bin/dwowd`).
- **Out of scope** (unchanged): tx relay (`protocol_tx.rs`), block broadcast
  (`linear_broadcast.rs`), event-graph (`src/event_graph/`), and the `net-full` transport
  plugins (Tor/I2P/SOCKS5/QUIC/Unix).

## 2. Guidewords and nodes

Guidewords applied per HAZOP: NO / NOT / PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY /
LATE. Nodes traced (in order):

| Node | Files |
|------|-------|
| N1 Dial (outbound) | `src/net/connector.rs` |
| N2 Transport (TCP) | `src/net/transport/tcp.rs` |
| N3 Transport (TLS) | `src/net/transport/tls.rs`, `src/net/transport/mod.rs` |
| N4 Framing (magic/varint/dispatch) | `src/net/channel.rs` |
| N5 Handshake (version/verack) | `src/net/protocol/protocol_version.rs` |
| N6 Peer registration | `src/net/session/mod.rs`, `src/net/hosts.rs`, `src/net/session/manual_session.rs` |
| N7 Sync protocol | `dwow_chain::sync_types`, `linear_sync_client`, `sync_handler` |
| N8 Wallet init/logging | `bin/dww/src/main.rs`, `bin/dww/src/lib.rs`, `bin/dww/src/dispatch.rs`, `bin/dww/src/config.rs` |

## 3. Findings (root causes)

### R1 — The wallet has no logging (systemic silent-fail)

`bin/dww/src/main.rs` installs no `tracing` subscriber (zero matches for `tracing_subscriber` /
`setup_logging` / `EnvFilter` in `bin/dww`). Every `warn!`/`error!` in `src/net/` is a **no-op**
in the wallet; only `eprintln!` is visible. The observed symptom — `[dww] P2P initialized
successfully.` followed by `[sync] Tick: local=0 peers=0` forever — is fully explained by this:
the real dial/TLS/magic/version failure **is** logged in `src/net/`, but the wallet discards it.

- `bin/dww/src/dispatch.rs:652-665` — the only `eprintln!` diagnostics.
- `bin/dww/src/config.rs:219` — `[dww] P2P config parsed: …`.
- `bin/dww/src/sync_task.rs:145` — `[sync] Tick: local={} peers={}`.
- `P2p::start()` only errors on inbound-listener bind failure (`src/net/p2p.rs:154-157`), so
  "P2P initialized successfully" carries no information about outbound dial health.

### R2 — The connection layer returns `Err` with no log on the paths the wallet drives

| Location | Failure | Logged? |
|----------|---------|---------|
| `src/net/connector.rs:97` | `Dialer::new` fails → `Err(ConnectFailed)` | **NO** |
| `src/net/connector.rs:133` | dial error → `Err(ConnectFailed)` | **NO** |
| `src/net/connector.rs:136` | stop-signal wins → `Err(ConnectorStopped)` | **NO** |
| `src/net/transport/tcp.rs:111` | TCP connect OS error | **NO** |
| `src/net/transport/tcp.rs:137,141` | connect timeout / error | **NO** |
| `src/net/transport/tls.rs:375` | `connector.connect().await?` TLS error | **NO** |
| `src/net/transport/mod.rs:136-142` | `enforce_hostport!` missing host/port → `ENETUNREACH` | **NO** |
| `src/net/acceptor.rs:292` | inbound TLS EOF → bare `continue` | **NO** (even on node) |
| `src/net/session/manual_session.rs:175-182` | `try_register(Connect)` blocked → `debug!` | near-silent |

The caller's `warn!` (`manual_session.rs:223-226`) is then swallowed by R1. Net effect: the
wallet's manual dial fails and nothing is observable.

### R3 — Wallet and node ride different connection paths

Wallet (`bin/dww/Cargo.toml:18` → `net-wallet`, defined `Cargo.toml:303-320`) compiles **only**
`ManualSession` (dial `config.peers`) + Inbound/Outbound/Direct; **no** `seed()` symbol, no
SeedSync/Refine. Node (`bin/dwowd/Cargo.toml:18` → `net-node` + `rpc`→`net`→`net-full`) compiles
all six sessions + hostlist + seed discovery. The sync *protocol* is shared (my prior work), but
the connection is a divergent slice of the legacy 6-session/hostlist/seed/refine/ban stack.

- `ManualSession::start` dials `settings.peers` (`manual_session.rs:85`) — correct for the wallet.
- The node's outbound uses hostlist/seed discovery (`outbound_session.rs`, `seedsync_session.rs`).
- `bin/dww/src/sync_task.rs:167` has a stale comment "seed() in init_p2p() handles initial
  connection" — `init_p2p` never calls `seed()` and the wallet has no `seed()`.

### R4 — The sync protocol is correct but wrapped in the hodge-podge

`GetTip→Tip→GetBlocks→Blocks` is simple and Monero-chain-sync-shaped; the hazard is the
surrounding session/hostlist/seed/refine/ban/metering machinery, of which the wallet needs a
tiny divergent slice.

## 4. Ranked silent-fail list (why `peers=0` is silent)

L1 — **No logging in the wallet** (R1) — makes every case below invisible.
L2 — **TCP dial failure** (ECONNREFUSED/EHOSTUNREACH/ENETUNREACH/timeout) — R2, silent; retried
     every 15s (`manual_session.rs:241`).
L3 — **TLS handshake failure** — R2; cert SAN `dark.fi` (`tls.rs:47,75`), ED25519-only
     (`tls.rs:87-90`); `localnet=true` skips DNS check but the node-side EOF drop is silent
     (`acceptor.rs:292`).
L4 — **Magic-bytes mismatch** — `channel.rs:390-411` logs `error!` (swallowed by R1) + ban.
L5 — **Version-exchange timeout** — `protocol_version.rs:120-143` `error!` (swallowed).
L6 — **Version major.minor mismatch** — `protocol_version.rs:257,341` `error!` (swallowed) + ban.
L7 — **Peer URL silently dropped** — `bin/dww/src/config.rs:326-328` `filter_map(Url::parse().ok())`
     drops malformed URLs → empty `Settings.peers` → zero ManualSession slots → never dials.
L8 — **Registry state blocks the manual slot** — `manual_session.rs:175-182` `debug!` stall.

## 5. ρ-calculus trace

The spec (`sync-protocol.md` §0, `type-system.md` §10) says the sync process is a replicated
process net `Sync = SyncClient | SyncHandler | BlockSink`, **identical** across wallet/observer/
mining. The code diverges at three points:

### D1 — `SyncClient` is not one process (R3)

The ρ-calculus output `connect!(c, peer)` has two implementations: the wallet's `ManualSession`
dial and the node's `Outbound/Seed` dial. Strong bisimulation `P ~ Q` fails because the two
processes exhibit different observable connection behavior (barbs) — e.g. the wallet cannot
observe its own connection failure (R1). The rewrite must make `connect` a single process.

### D2 — The channel boundary has no re-lift failure signal (R1/R2)

Per §10.5 a channel boundary is a `quote(x)`/`eval(x)` edge with four runtime obligations; the
first is re-lift validation with an observable failure. Here a connection failure produces no
observation (R1/R2), so a process can fail to exhibit `↓sync-barrier` and the observer cannot
distinguish it from "not yet syncing". The rewrite must make every connection failure a logged,
observable barb.

### D3 — `Message::BARBS` is declared but unenforced

The dispatch-time barb check is commented out (`src/net/message_publisher.rs:246,318`), so the
"barb as type" guarantee is declared in `impl_p2p_message!` but never enforced at the boundary.
This remains a known net-layer gap (out of the sync-connection rewrite scope).

## 6. Outcome

The unified sync-connection rewrite (see `sync-protocol.md`) closes R1 (wallet logging), R2
(logged silent-fail paths), and R3 (one `SyncClient`/`SyncHandler` for every role), resolving
D1 and D2. D3 (barb enforcement) is a separate net-layer hardening item.
