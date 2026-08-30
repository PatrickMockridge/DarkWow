# Node Startup & Role Behavior — WYSIWYG Specification

This is the authoritative, **WYSIWYG** specification of how a DarkWow node behaves on startup: its role, the
genesis ceremony, mining, and chain joining. Every constant, role name, env var, and state below matches the
code 1:1. If the spec and code disagree, the code is the bug — fix the code, not the spec.

Normative anchors: `type-system.md §5.1` (no bare `bool` gating consensus), `sync-protocol.md` (sync state
machine), `l3-readiness-spec.md` AC-2/AC-4 (consensus determinism).

## 1. Roles

There are exactly three roles, selected by `darkwow node --role <role>` (`bin/darkwow/src/main.rs:126-134`),
which sets the `MINING_ENABLED` / `CREATE_GENESIS` env for the `dwowd` child in a single derivation point:

| Role | `MINING_ENABLED` | `CREATE_GENESIS` | Meaning |
|---|---|---|---|
| `genesis` | `true` | `true` | Creates the genesis block (explicit, one-time); then mines. |
| `miner` | `true` | `false` | Starts in observer mode (mining off); becomes a mining node only after `CaughtUp` (§2). |
| `observer` | `false` | `false` | Sync + serve only. **The default.** |

**Default role is `observer`** — a node started with no `--role` SHALL scan for an existing genesis + chain and
join it; it SHALL NOT mine and SHALL NOT create genesis. This is enforced at two layers:

- `contrib/docker/darkwow-testnet/entrypoint.sh:118` — `NODE_ROLE="${NODE_ROLE:-observer}"`.
- `bin/dwowd/src/main.rs:244-246` — `MINING_ENABLED` defaults to `false` (join-first; mining is explicit).

Mining or genesis creation is therefore an **explicit opt-in** via `--role miner` / `--role genesis`, never the
default.

## 2. Startup state machine (deterministic — no coin toss)

On startup a node runs, in order (`bin/dwowd/src/main.rs`, `Dwowd::init_linear` → `start` →
`consensus_linear_init_task`):

1. **Load local state** (`init_linear`, `bin/dwowd/src/lib.rs:767-814`): if `CREATE_GENESIS` is set and the
   node is the genesis authority, build genesis; otherwise skip and plan to sync.
2. **Scan peers for an existing chain** (`consensus_linear_init_task`): request peer tips; the sync decision is
   made by `wait_for_peers_or_proceed` (`proto/linear_sync_client.rs:245-288`) — a non-authority node at height
   0 returns `WaitForGenesis`, never `ProceedSolo`.
3. **Sync to tip** (`consensus_linear.rs:453-587`): pull `local_height+1 ..= peer_tip` and apply through
   `accept_block`.
4. **Only then mine** — the miner task waits for `sync_state == CaughtUp` (`lib.rs:1264,1298`) before producing
   blocks.

**Miner = observer until `CaughtUp`.** A `miner` node SHALL NOT produce a block until its tip is confirmed
synced. Before `CaughtUp` it behaves exactly as an `observer` (sync-only); only after `CaughtUp` does it
become a mining node. Mining is gated on `CaughtUp`, not on the `miner` role alone.

**The mine gate is "CaughtUp on the canonical chain", not "has a tip".** When no genesis exists anywhere
(`current_height == 0 && max_peer_height == 0`), the node MUST set `Behind` (miner paused), not `CaughtUp`
(`consensus_linear.rs:661-668`). A node must never mine while behind or on a divergent fork.

## 3. Genesis ceremony (explicit, last resort)

- Genesis is created only when `CREATE_GENESIS=true`, which only `--role genesis` sets
  (`bin/darkwow/src/main.rs:127`). Genesis creation is **decoupled from mining**: `CREATE_GENESIS` is a
  separate explicit flag; mining is governed by `MINING_ENABLED` + the `CaughtUp` gate, never by the
  genesis role.
- The ceremony is deterministic: `init_genesis` (`bin/dwowd/src/lib.rs:482-651`) builds block 1 with pinned
  `timestamp=0`, `previous=blake3([0u8;32])`, `target=BlockTarget::MAX`, a fixed deployment key (scalar 1), one
  coinbase + 9 genesis contract deployments; the hash is verified against the compile-time `genesis_hash.txt`.
- Creation is gated by the `GenesisAuthority` marker (`bin/dwowd/src/task/consensus_linear.rs:76-118`), which is
  constructible only on the `CREATE_GENESIS` path. A second node MUST NOT independently create genesis; peers
  reject a divergent genesis via the pinned hash.

## 4. Fork policy

A node SHALL NOT mine a divergent fork; it SHALL adopt the canonical (heaviest) chain. Competing blocks at the
same height are stored as uncles for reward only (`src/linear/src/chain_state.rs:787-887`).

**Known gap (tracked):** the reorg path is currently unreachable — `accept_block` executes WASM before fork
classification (`block_acceptor.rs:246` vs `:326`), and the sync loop never fetches the fork pivot
(`consensus_linear.rs:453`). A divergent-coinbase fork is therefore permanently un-resolvable. This is deferred
to the reorg-resolution remediation; the deterministic-default changes above prevent a node from *entering* a
fork in the first place.

## 5. Config — connecting a miner to an existing chain

To mine against an existing chain, a node MUST be configured with:

- `--role miner` (explicit), and
- a peer/hostlist pointing at a genesis-bearing node, so the initial sync finds the existing genesis and tip
  (a `miner` with no peers and no genesis stays `Behind` and never mines).

Without these, a node defaults to `observer` (join-first) and simply syncs.

### Docker topology (devnet)

The devnet compose (`contrib/docker/darkwow-testnet/docker-compose.yml`) instantiates the invariant
**exactly one `genesis`; every other mining node is a `miner`**:

- `node0` = `genesis` (`:98`) — the single authority; runs the genesis ceremony, then mines.
- `node1`..`node4` = `miner` (`:158,227,288,351`) — pure mining; start as observer, sync to node0's tip, then mine.
- `observer` = `observer` (`:40`) — sync + serve only.
- `join-merge` node = `MINING_ENABLED=false` (`:576`) — mining is external via p2pool; the internal miner stays off.
