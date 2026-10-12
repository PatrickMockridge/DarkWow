# Sync Conformance — code ↔ spec-clause mapping

Every sync source file carries a `//! Spec: sync-protocol.md §N` module header (enforced by
`contrib/ci/check_sync_conformance.sh`). This document is the full code↔clause mapping that the
header summarizes — "every line justified" as a reviewable artefact, not a slogan
([`sync-protocol.md` §12](sync-protocol.md#12--conformance)).

## Process net → files

One ρ-calculus process net, three roles, one wire path ([§1](sync-protocol.md#1--the-sync-process--ρ-calculus-)):

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
| `src/linear/src/sync_boundary.rs` | §1, §9 | L2 boundary type `PeerTip` + validating re-lift (`from_tip`) |
| `src/linear/src/sync_connection.rs` | §8, §9, §11, §13 | unified `SyncPeer`/`SyncServer`; S1-S8 safety; reuse; timeouts |
| `bin/dwowd/src/task/consensus_linear.rs` | §1, §13 | node `SyncClient` + `BlockSink`; the pull loop and the caught-up/mining gate (§18.1.1) |
| `bin/dwowd/src/proto/linear_sync_client.rs` | §8, §13.3 | node peer discovery + dial onto the sync rail (`filtered_peers`, `dial_sync_peers`) |
| `bin/dwowd/src/proto/linear_broadcast.rs` | §11 | one-hop block broadcast (remains on the net rail) |
| `bin/dwowd/src/proto/mod.rs` | §8.6, §11 | port derivation (`inbound + offset`), `BroadcastTx` tx sink |
| `bin/dww/src/sync_task.rs` | §1, §13, §17 | wallet `SyncClient` + `BlockSink`; follows the longest chain |
| `bin/dww/src/p2p_wallet.rs` | §14.1 | wallet `net-wallet`-tier config (`P2pWalletConfig`, `WalletStream`) |

## Net-crate ownership (§14.1)

These clauses describe `src/net/` as first-party code (owned, not an upstream dependency); they
are documented in §14.1 rather than per-file `//! Spec:` headers:

| File | Clause | What it implements |
|------|--------|--------------------|
| `src/net/hosts.rs` | §14.1 | `HostColor` quarantine states + `BLACKLIST_EXPIRY_SECS` expiry (`refresh`) |
| `src/net/channel.rs` | §14.1 | `ban()` = move to `Black`; SESSION_OUTBOUND-gated magic/version bans |
| `src/net/settings.rs` | §14.1 | `BanPolicy::{Strict, Relaxed}` runtime gate |
| `src/net/protocol/protocol_version.rs` | §14.1 | version-mismatch ban (SESSION_OUTBOUND-gated) |
| `bin/dww/src/config.rs` | §14.1 | wallet sets `BanPolicy::Relaxed` (never bans its configured peers) |

This table previously cited `§14.2`/`§14.3` for the ban-policy rows and a `§15` ("Net-Crate
Ownership + Feature Gate") for the feature-gate rows. Neither section exists: `c7512b2269`
("collapse node sync to a wallet-shaped pull loop") deleted §15 and folded the former §14.1–§14.3
into a single **§14.1 Command dispatch matrix + unknown-command drain**, which is where `ban()`,
`HostColor` and the wallet/node `BanPolicy` difference are stated today. The feature-gate half of
the old §15 is documented at
[`type-system.md` §10.1 Three-tier feature gate as ρ-process hierarchy](type-system.md#10-1-three-tier-feature-gate-as-process-hierarchy)
and in [`wallet-vs-daemon.md`](wallet-vs-daemon.md).

## Safety properties → witnesses (§9 / §10)

| Property | Witness |
|----------|---------|
| S1/S2/S3/S8 | `test_sync_connection_end_to_end` — dial+handshake+tip+blocks; no-silent-fail, magic-mismatch |
| full wallet sync | `test_wallet_sync_pulls_blocks_to_balance` |
| wire format | `sync_types::tests::wire_format_golden` |
| re-lift (nominal types) | `consensus_coordination::test_peertip_rejects_invalid`, `test_tip_missing_genesis_hash_rejected`, `test_tip_max_height_rejected` |
| spec conformance | `python3 contrib/model/sync_model.py` |

## Enforcement

`contrib/ci/check_sync_conformance.sh` does three things. It greps the nine sync source files
above for a `Spec: sync-protocol.md` header and fails if one is missing. It resolves every `§N`
(and `§A-§B` range endpoint) that a header cites against `sync-protocol.md`'s **own headings** and
fails on a clause the document does not have, so a header that cites a renumbered or deleted
section is caught, not merely one that is absent. And it checks the `sync-protocol.md §14.1`
command-registration matrix (the node registers the push commands, the pull-only wallet registers
neither). The first half enforces a repository convention — a file declaring its clause in a module
header — and is not itself a spec clause; the clause-resolution and dispatch-matrix halves are
spec-backed. Run it directly:

```
contrib/ci/check_sync_conformance.sh
```

This section said the gate "fails the build if a header is missing or drifts" until 2026-10-12
(`F-375`). "Drifts" was aspirational: the check was a substring grep that never resolved a clause
number, so it could only report a *missing* header, and every file's header merely had to exist.
Its failure message now names the offending clause — `cites §N — sync-protocol.md does not have
that clause` — and a second negative control (`--self-test`) plants a dead clause number and asserts
the gate reports it.
