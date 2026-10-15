# Sync Red-Team Audit

Adversarial audit of DarkWow node↔wallet sync against the formal specification
([`sync-protocol.md`](sync-protocol.md)) and recognised production patterns. Findings are ranked
CRITICAL / HIGH / MEDIUM / LOW and cite the spec clause they violate.

## Methodology

Attacker model: a Sybil peer set (or a single malicious peer) on the wallet's fixed-peer list or the
node's discovered-peer set. We ask, for each sync node in the ρ-calculus process net
(`SyncClient | SyncHandler | BlockSink`), what an adversary can force, and whether the spec's
inherent-safety guarantees (§9) hold under that forcing.

Production baselines: Bitcoin Core (header-first + PoW + peer scoring), Monero (chain-sync shape +
white/grey/black hostlists), Ethereum/geth (sync modes + peer scoring), Electrum (SPV client pull).

## Production-pattern comparison

| Pattern | DarkWow borrows | Gap |
|---|---|---|
| Bitcoin Core | chain sync, peer ban | **no PoW verification on the wallet** — see R1 |
| Monero | chain-sync shape, hostlists | ban was permanent-for-process (now expiring, §14) |
| Ethereum/geth | sync loop | **no peer scoring** — one bad peer retried forever — see R3 |
| Electrum | fixed-peer SPV pull | **no header-chain / PoW check** on the wallet |

## Findings

### R1 (CRITICAL) — The wallet accepts blocks it cannot verify (PoW-blindness)

`bin/dww/src/lib.rs` `insert_synced_block` (L352-400) verifies the tx merkle root (L360) and chain
continuity (L371) but **deliberately skips the RandomX PoW hash** (L379-387) because the wallet lacks
the PoW VM. Block *selection* then rests entirely on the sync task's tip-hash majority vote
(`sync_task.rs` L195-243).

An adversary controlling a majority of the wallet's configured peers (or a Sybil set on the node's
peer list) can serve an arbitrary low-work fork, and the wallet will treat its tip as canonical.
There is no work-based tie-break and no header-chain to check against.

Spec reference: violates §9 S3 (chain identity is only genesis-hash, not work). Remediation: the
wallet must either (a) verify PoW (give it the PoW VM / a header chain), or (b) document and bound the
trust assumption — the wallet trusts its configured peers to be honest and non-colluding, and this is
an explicit, documented trust model, not an oversight.

### R2 (HIGH) — Reorg auto-`reset()` is a full wallet wipe, triggerable by a lying minority

`bin/dww/src/lib.rs:674` `reset()` drops the entire wallet DB on a majority-hash mismatch. A single
pair of peers that agree on a fabricated tip hash (or a briefly-forked honest peer) can force repeated
full resets — a wallet-DoS that also destroys scan progress.

Spec reference: no bound on `↓reorg` in `sync-protocol.md`. Remediation: rate-limit resets, require a
supermajority (e.g. 2/3) of *distinct* peers agreeing on the alternate tip, and prefer a
reorg-to-known-good-height over a full wipe.

### R3 (HIGH) — `HighestPeerTip` is monotonic and never decays

`bin/dww/src/sync_task.rs:47-65` `HighestPeerTip` only ever increases. A stale/parting peer that once
reported a high tip keeps `is_synced()` (`lib.rs:330-346`) treating the wallet as behind forever,
even after that peer disconnects.

**This finding is RESOLVED — 2026-10-15, in `bin/dww/src/sync_task.rs`.** `HighestPeerTip::set_max`
(`fetch_update` with `if height.get() > c`, which could only raise) is replaced by
`observe(best_tip, local)`, a plain `store` that can lower the value, and the sync loop publishes
**once per tick** — after the peer loop, not inside it — passing the `best_tip` it already computes as
the per-tick maximum over the peers that answered. When no peer answers, the mark decays to the local
height rather than keeping a departed peer's tip. `Dww::is_synced()` (`bin/dww/src/lib.rs`) needed no
change: it already compared `local >= peer_tip`, so a mark that tracks the live peers corrects it at
the source. The register row is `OBL-C238` — opened by this correction, and now `FIXED`, with the
pinned `test_highest_peer_tip_monotonic` replaced rather than weakened.

Spec reference (corrected): this finding cited §13.3 for "a node-side channel with 3 failures is
deprioritised". §13.3 says the opposite — a peer that times out "is simply skipped and the next peer
tried", and the document states plainly that there is *no* deadness/slowness taxonomy and no bounded
backoff. The wallet's missing decay is a real gap; the clause it was attributed to does not describe
the mechanism. Remediation (done 2026-10-15): decay `HighestPeerTip` toward local height when no live
peer confirms it — which is what `observe` now does.

### R4 (HIGH) — `tip_hash()` serves a zero hash with a real height

`src/linear/src/sync_handler.rs:300-317` and `sync_connection.rs:399-402` return a zero `BlockHash`
when the tip hash lookup fails, but still report the real height. The receiver's reorg vote
(`sync_task.rs` `detect_reorg`) counts that zero hash as a legitimate candidate, so a peer with a
corrupt tip index can poison the vote.

Spec reference (corrected): this finding attributed to §3 the requirement that "a zero hash at
height > 0 SHALL NOT round-trip". §3 is the nominal-types table and says nothing of the kind — its
rule is that `BlockHash` re-lifts only through `from_hex_str`, with the empty string as the genesis
sentinel. The requirement this finding wants is real; it is not in the clause it cites.
Remediation: return `Option<BlockHash>` and drop the peer (or mark it unverified) on `None`.

### R5 (MEDIUM) — Two server implementations with divergent byte budgets

`sync_handler.rs` `handle_get_blocks` trims the response by bytes (12 MiB budget, L216), but
`sync_connection.rs` `serve_conn` `GetBlocks` trims only by *count* (`LINEAR_SYNC_BATCH`, L410-431) —
no byte budget. A node serving 20 large blocks over the unified rail can produce a response that
exceeds `Blocks::MAX_BYTES` (16 MiB) and is dropped at the wire, while the P2P-channel handler protects
itself. This is a §11 "two implementations" divergence.

Spec reference (corrected): §**4** (`MAX_BYTES`) + §11 (single implementation) — §5 is
`genesis_hash` validation. Remediation: one serve path, one byte budget.

### R6 (MEDIUM) — `channels[0]` with no round-robin

`consensus_linear.rs:468` uses `channels[0]` after filtering. A healthy-but-slow first channel is
always preferred; other healthy peers are never used for the current pass. No load-spread, no
failover to a faster peer.

Spec reference: §13.3 (peer discipline). Remediation: round-robin or prefer-by-freshness across
healthy channels.

### R7 (MEDIUM) — Duplicated timeout/batch constants

Two independent sets of `TIP_TIMEOUT`/`BLOCKS_TIMEOUT`/`LINEAR_SYNC_BATCH` exist
(`sync_connection.rs:64-68` vs `linear_sync_client.rs:39-42` / `sync_handler.rs:49`). Drift between
them silently changes behaviour on one rail only.

Spec reference: §8.1 (single canonical constants). Remediation: one source of truth.

### R8 (MEDIUM) — The wallet dials with `genesis_hash = None`

`bin/dww/src/sync_task.rs:152` dials `SyncPeer::dial(…, None, …)`, so the wallet never validates the
peer's chain identity at handshake; it relies on the later `Tip.genesis_hash` check. A peer on the
wrong chain is only detected *after* tip exchange, not at connect.

Spec reference: §4 (genesis validation) — handshake SHOULD carry genesis. Remediation: pass the
wallet's local genesis (when known) to `dial`.

### R9 (LOW) — Node tip refresh only advances

`consensus_linear.rs:604-609` `max_peer_height` never decays; a temporary high-water mark persists.
Lower risk than R3 (the node re-polls and can recover), but same shape.

### R10 (LOW) — Dead `ban-policy` cargo feature

`ban-policy` is declared in `Cargo.toml` but no `#[cfg(feature = "ban-policy")]` reads it — the ban
machinery compiles into every build. Documented in [`sync-protocol.md` §11](sync-protocol.md#11--reuse).

## Remediation owners

| Finding | Owner |
|---|---|
| R1 wallet PoW-blindness | wallet trust-model decision (document/bound) — not silently skipped |
| R2 reorg reset DoS | wallet sync task |
| R3 monotonic HighestPeerTip | wallet sync task |
| R4 zero-hash tip | serve side (`sync_handler`/`sync_connection`) |
| R5/R7 two servers + duplicated constants | sync connection unification |
| R6 channels[0] | node consensus task |
| R8 wallet genesis handshake | wallet sync task |
| R9/R10 | node + net crate |

## Resolution status

| Finding | Status | Fix |
|---|---|---|
| R1 wallet PoW-blindness | **RESOLVED** | [`sync-protocol.md` §17](sync-protocol.md#17--wallet-follows-the-longest-chain) — trusted-daemon pattern: the wallet follows the longest (highest) peer-reported tip; PoW-blindness is a documented trust gap, not papered over with peer voting |
| R2 reorg reset DoS | **RESOLVED** | no reorg detection at all — the wallet follows the longest chain and never resets on disagreement |
| R3 monotonic HighestPeerTip | **RESOLVED** | `set_max` (raise-only `fetch_update`) is replaced by `observe(best_tip, local)`, published once per sync tick over the peers that answered and decaying to the local height when none did (`bin/dww/src/sync_task.rs`, 2026-10-15); `is_synced()` needed no change, having already compared `local >= peer_tip`. The distinction this cell drew still stands — the *earlier* "resolution" described a design choice (follow the longest chain, no quorum gate) rather than a repair to the monotonicity the finding names, and the repair is now the one described here |
| R10 dead ban-policy flag | **RESOLVED** | flag deleted; `ban()` runtime-gated by `BanPolicy` (commit `26f948f6ad`) |
| R4 zero-hash tip | **RESOLVED** | serve side logs the failure; wallet ignores zero-hash tips |
| R6 channels[0] | **RESOLVED, by decision rather than by round-robin** | there is no round-robin, and §13.3 says there should not be — the pull loop "request[s] each peer's tip and take `max_peer_height = max(...)`", tries the next peer on a failure, and the document states "no round-robin, no bounded backoff, no heartbeat, and no watchdog — those are DarkWow-idiosyncratic machinery with no production analogue". The cell recorded a mechanism the specification explicitly rejects |
| R8 wallet genesis handshake | **RESOLVED** | wallet learns `local_genesis_hash` from the first tip and passes it to `SyncPeer::dial` |
| R9 node tip refresh | **RESOLVED** | `max_peer_height` decays to the latest observed max |
| R5/R7 two servers + dup constants | **RESOLVED** | legacy `sync_handler.rs` + `linear_sync_client.rs` deleted; one serve path (`sync_connection`) + one byte budget (`MAX_BATCH_BYTES`) + one constant set |
