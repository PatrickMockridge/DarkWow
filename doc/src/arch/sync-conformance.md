# Sync Conformance — code ↔ spec-clause mapping

Every sync source file carries a `//! Spec: sync-protocol.md §N` module header (enforced by
`contrib/ci/check_sync_conformance.sh`). This document is the full code↔clause mapping that the
header summarizes — "every line justified" as a reviewable artefact, not a slogan
([`sync-protocol.md` §16](sync-protocol.md#16-conformance-line-by-line-justification)).

## Process net → files

One ρ-calculus process net, three roles, one wire path ([§1](sync-protocol.md#1-the-sync-process-calculus)):

| ρ-process | Spec | Location |
|-----------|------|----------|
| `SyncClient` (wallet) | §1, §13.1 | `bin/dww/src/sync_task.rs` — `run_wallet_sync` |
| `SyncClient` (node) | §1, §13.1 | `bin/dwowd/src/task/consensus_linear.rs` — `consensus_linear_init_task` |
| `SyncHandler` | §1, §8.5, §13.1 | `src/linear/src/sync_connection.rs` — `SyncServer::run` |
| `BlockSink` (wallet) | §1, §13.1 | `bin/dww/src/lib.rs` — `insert_synced_block`; `bin/dww/src/scan.rs` |
| `BlockSink` (observer/miner) | §1, §13.1 | `bin/dwowd/src/block_acceptor.rs` — `accept_block` |

## File → clause

| File | Clause(s) | What it implements |
|------|-----------|--------------------|
| `src/linear/src/sync_types.rs` | §2-§7 | message authority, nominal types, `genesis_hash`, `MAX_BYTES`, barb declaration, wire format |
| `src/linear/src/sync_boundary.rs` | §1, §9 | L2 boundary types (`PeerTip`/`BlocksBatch`/`SyncDecision`/`SyncState`) + validating re-lift |
| `src/linear/src/sync_connection.rs` | §8, §9, §11, §13 | unified `SyncPeer`/`SyncServer`; S1-S8 safety; reuse; timeouts |
| `bin/dwowd/src/task/consensus_linear.rs` | §1, §13 | node `SyncClient` + `BlockSink`; task mapping; retry/backoff (`channel_failures`) |
| `bin/dwowd/src/proto/linear_sync_client.rs` | §1, §13.3 | node peer discovery + sync gate (`wait_for_peers_or_proceed`, `dial_sync_peers`) |
| `bin/dwowd/src/proto/linear_broadcast.rs` | §11 | one-hop block broadcast (remains on the net rail) |
| `bin/dwowd/src/proto/mod.rs` | §8.6, §11 | port derivation (`inbound + offset`), `BroadcastTx` tx sink |
| `bin/dww/src/sync_task.rs` | §1, §13, §17 | wallet `SyncClient` + `BlockSink`; follows the longest chain |
| `bin/dww/src/p2p_wallet.rs` | §15 | wallet `net-wallet`-tier config (`P2pWalletConfig`, `WalletStream`) |

## Net-crate ownership (§14 / §15)

These clauses describe `src/net/` as first-party code (owned, not an upstream dependency); they
are documented in §14/§15 rather than per-file `//! Spec:` headers:

| File | Clause | What it implements |
|------|--------|--------------------|
| `src/net/hosts.rs` | §14.1 | `HostColor` quarantine states + `BLACKLIST_EXPIRY_SECS` expiry (`refresh`) |
| `src/net/channel.rs` | §14.2 | `ban()` = move to `Black`; SESSION_OUTBOUND-gated magic/version bans |
| `src/net/settings.rs` | §14.2, §15 | `BanPolicy::{Strict, Relaxed}` runtime gate |
| `src/net/protocol/protocol_version.rs` | §14.2 | version-mismatch ban (SESSION_OUTBOUND-gated) |
| `bin/dww/src/config.rs` | §14.2, §15 | wallet sets `BanPolicy::Relaxed` (never bans its configured peers) |

## Safety properties → witnesses (§9 / §10)

| Property | Witness |
|----------|---------|
| S1/S2/S3/S8 | `test_sync_connection_end_to_end` — dial+handshake+tip+blocks; no-silent-fail, magic-mismatch |
| full wallet sync | `test_wallet_sync_pulls_blocks_to_balance` |
| wire format | `sync_types::tests::wire_format_golden` |
| re-lift (nominal types) | `consensus_coordination::test_peertip_rejects_invalid`, `test_tip_missing_genesis_hash_rejected`, `test_tip_max_height_rejected` |
| spec conformance | `python3 contrib/model/sync_model.py` |

## Enforcement

`contrib/ci/check_sync_conformance.sh` greps the nine sync source files above for
`Spec: sync-protocol.md` and fails the build if a header is missing or drifts. Run it directly:

```
contrib/ci/check_sync_conformance.sh
```
