# `daemon_sync` failures — HAZOP + bow-tie + root-cause

Guide-word deviation analysis over the sync handshake + bootstrap, plus a bow-tie. Guide words:
NO / NOT / PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY / LATE, plus MORE / LESS. Each finding cites
`file:line` on `linear-master`.

## Subject and observed failures

After the coinbase key-binding fix (6/10 green), four `daemon_sync_integration` tests fail on the sync layer:

- `test_sync_path_reorg_to_heavier_chain` — `SyncPeer::dial: peer … rejected handshake`
- `test_daemon_pull_sync_converges` — `daemon sync timed out … after 900s`
- `test_daemon_broadcast_propagates` — `B never pulled to height 2`
- `test_sync_state_gates_mining_until_caught_up` — `miner never synced to authority tip (0 vs 2)`

## Central root cause

**The sync handshake is fail-closed on genesis, and it rejects a bootstrapping node that presents `None`.**

`serve_conn` (`src/linear/src/sync_connection.rs:452-458`) computes:

```rust
let genesis_ok = match chain_state.genesis_hash() {
    Some(ours) => hello.genesis_hash.as_ref()
        .map(|theirs| BlockHash::from_hash(ours) == *theirs).unwrap_or(false),  // None → false
    None => true,                                                              // no genesis → accept
};
```

A server that **holds** a genesis therefore rejects a client that sends `hello.genesis_hash = None` (or a
different hash). But a bootstrapping node has **no genesis yet** — it is about to fetch it — so it sends
`None`. The authority (which holds genesis) rejects it, and the bootstrap cannot start. The three timeouts are
the downstream symptom (no sync → `0 vs 2` → 900s timeout). The fourth failure is the test's own direct
`SyncPeer::dial(…, None, …)` (`bin/dwowd/src/tests/daemon_sync_integration.rs:1005`) hitting the same gate.

This gate was introduced in **`55a04076c9`** ("fix(consensus): bind the coinbase note's value and the uncle
note's spend key") — the **same commit** that added the coinbase key-binding check. That commit changed the
handshake from fail-open ("the previous form accepted `None`", per the comment at `:444-446`) to fail-closed,
for chain-identity security, but it did not provide a bootstrap path: the comment's "Path B" (a server with
no genesis accepts) never applies, because the only server that matters for bootstrap is the **authority, which
has genesis**. So a new node can never join.

## Mapped path

| step | where | value |
|---|---|---|
| client sends `hello.genesis_hash` | `SyncPeer::dial` `sync_connection.rs:221-225` | `None` during bootstrap (caller passes `None`) |
| server's `genesis_ok` | `serve_conn` `sync_connection.rs:452-459` | `Some(ours) vs None → false` (reject) |
| rejection surfaces | `SyncPeer::dial` `:245-247` | `"peer … rejected handshake"` |
| downstream | test timeouts | `900s` / `0 vs 2` / `never pulled` |

## HAZOP findings

### V1 — NO (genesis) → the server rejects a client with no genesis

- **Node:** `sync_connection.rs:452-458` `genesis_ok`.
- **Deviation:** a client that presents `None` is rejected whenever the server holds a genesis. During
  bootstrap the client necessarily has `None`, so the authority always rejects the joiner.
- **Mechanism:** the fail-closed gate has no bootstrap carve-out; "Path B" (server without genesis accepts)
  never fires on the authority, which is the only server a joiner dials.
- **Invariant violated:** a node SHALL be able to join by syncing genesis from the authority
  (`node-startup-spec.md` §2 "Other nodes start at height 0 and sync genesis via P2P").
- **Structural fix:** allow a `None`-genesis peer through the handshake when the server holds genesis, and
  enforce chain identity *after* the genesis is served (against the pinned `genesis_hash.txt`), rather than at
  the hello.

### V2 — EARLY / LATE (the gate vs the bootstrap) → identity is checked before genesis is known

- **Node:** `sync_connection.rs:452-458` (check) vs the bootstrap flow (genesis served later).
- **Deviation:** chain identity is enforced at the hello, before the joiner has obtained the genesis to
  compare. The check is EARLY relative to the bootstrap.
- **Invariant violated:** identity enforcement SHALL occur at the point where the peer can possess the value
  being checked.
- **Structural fix:** move the genesis comparison to after `GetBlocks`/genesis receipt (the pinned-hash check
  already exists as `check_genesis_pin`, `bin/dwowd/src/lib.rs:699`).

### V3 — OTHER THAN (genesis hash source) → the test dials with the wrong value

- **Node:** `bin/dwowd/src/tests/daemon_sync_integration.rs:1005-1007` — `SyncPeer::dial(url, chain_magic, None, …)`.
- **Deviation:** the reorg test dials its own `SyncServer` (backed by a chain state that has genesis) but
  passes `None`, so its own server rejects it.
- **Invariant violated:** a client SHALL present the genesis hash it holds, when it holds one.
- **Structural fix:** pass `scratch.genesis_hash().map(BlockHash::from_hash)` (or the pin) instead of `None`.

### V4 — MORE (timeout) → the three timeouts are symptoms, not causes

- **Node:** `test_daemon_pull_sync_converges` (900s) and the `never synced` assertions.
- **Deviation:** the 900s timeout and `0 vs 2` are the observable outcome of V1 (the joiner never syncs),
  not independent failures.
- **Invariant violated:** none — these are downstream of V1.
- **Structural fix:** none separately; they clear when V1/V2 is fixed.

## Bow-tie

```
THREATS                                          TOP EVENT                                   CONSEQUENCES
────────                                          ─────────                                   ────────────
fail-closed genesis gate  ──┐                                                              ┌── joiner cannot sync
  rejects hello.genesis=None │  ┌──────────────────────────────────────────────┐            │   (stuck at height 0)
bootstrapping node sends None │  │  sync handshake rejected on genesis mismatch │────────────┤── miner never caught up
  (no genesis yet)           ──┘  └──────────────────────────────────────────────┘            │   → liveness (no blocks)
                                                                                             └── test timeouts (900s)

        ┌────────────── preventive barriers ──────────────┐               ┌────────────── recovery ──────────────┐
        │ handshake version+genesis check  :440-459      │               │ pinned genesis hash (genesis_hash.txt) │
        │ SYNC_PROTOCOL_VERSION             :440          │               │ daemon_sync suite (caught this)        │
        │ genesis pin  check_genesis_pin    lib.rs:699    │               │ wallet deterministic decrypt           │
        └─────────────────────────────────────────────────┘               └────────────────────────────────────────┘
```

## Root cause (single)

Commit `55a04076c9` made the sync handshake fail-closed on genesis (reject a client presenting `None`) as a
chain-identity hardening, but with no bootstrap path — so a joiner with no genesis is always rejected by the
authority. The same commit also added the coinbase key-binding check that broke the built-in miner (fixed
separately). The four sync failures are one root cause: the fail-closed genesis gate; three are its timeout
symptoms, one is the test's own `None`-dial.

## Verification

- The rejection is attributed to `genesis_ok` (`:452-459`), not `version_ok` — the version check passes, the
  genesis gate rejects `None`; confirmed by `git blame` (`55a04076c9`) and the comment "the previous form
  accepted `None`".
- The three timeouts are downstream of the rejection (no sync → `0 vs 2`).
- The test's direct dial (`daemon_sync_integration.rs:1005`) passes `None`, confirming V3.
