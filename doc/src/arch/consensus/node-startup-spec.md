# Node Startup & Role Behavior — WYSIWYG Specification

This is the authoritative, **WYSIWYG** specification of how a DarkWow node behaves on startup: its role, the
genesis ceremony, mining, and chain joining. Every constant, role name, env var, and state below matches the
code 1:1. If the spec and code disagree, the code is the bug — fix the code, not the spec.

Normative anchors: `type-system.md §5.1` (no bare `bool` gating consensus), `sync-protocol.md` (sync state
machine), `l3-readiness-spec.md` AC-2/AC-4 (consensus determinism).

## 1. Roles

There are exactly three roles, selected by `darkwow node --role <role>` (`run_node` in
`bin/darkwow/src/main.rs`), which sets the `MINING_ENABLED` / `CREATE_GENESIS` env for the
`dwowd` child in a single derivation point:

| Role | `MINING_ENABLED` | `CREATE_GENESIS` | Meaning |
|---|---|---|---|
| `genesis` | `true` | `true` | Creates the genesis block (explicit, one-time); then mines. |
| `miner` | `true` | `false` | Starts in observer mode (mining off); becomes a mining node only after `CaughtUp` (§2). |
| `observer` | `false` | `false` | Sync + serve only. **The default.** |

**Default role is `observer`** — a node started with no `--role` SHALL scan for an existing genesis + chain and
join it; it SHALL NOT mine and SHALL NOT create genesis. This is enforced at two layers:

- `contrib/docker/darkwow-testnet/entrypoint.sh` — `NODE_ROLE="${NODE_ROLE:-observer}"`.
- `bin/dwowd/src/main.rs` — `MINING_ENABLED` defaults to `false` (join-first; mining is explicit).

Mining or genesis creation is therefore an **explicit opt-in** via `--role miner` / `--role genesis`, never the
default.

## 2. Startup state machine (deterministic — no coin toss)

On startup a node runs, in order (`bin/dwowd/src/main.rs`, `Dwowd::init_linear` → `start` →
`consensus_linear_init_task`):

1. **Load local state** (`Dwowd::init_linear`, `bin/dwowd/src/lib.rs`): if `CREATE_GENESIS` is set and the
   node is the genesis authority, build genesis; otherwise skip and plan to sync.
2. **Dial sync peers and collect tips** (`consensus_linear_init_task` → `dial_sync_peers`): dial the
   full-node peers onto the sync rail and take `max_peer_height` from their tips.
3. **Pull missing blocks**: `next_height ..= max_peer_height`, applied one at a time through
   `accept_block`.
4. **Gate, then mine**: caught-up is a LOCAL property —
   `caught_up = blockchain.get_height() >= max_peer_height`; mining is a separate gate —
   `mine = caught_up && (authority || !sync_peers.is_empty())`. `sync_state` is stored
   `CaughtUp` when `mine` holds, `Behind` otherwise. The miner task waits for
   `sync_state == CaughtUp` before producing blocks.

**Miner = observer until `CaughtUp`.** A `miner` node SHALL NOT produce a block until its tip is confirmed
synced. Before `CaughtUp` it behaves exactly as an `observer` (sync-only); only after `CaughtUp` does it
become a mining node. Mining is gated on `CaughtUp`, not on the `miner` role alone.

**The mine gate is "CaughtUp on the canonical chain", not "has a tip".** With no genesis anywhere
(`current_height == 0 && max_peer_height == 0`), a non-authority node has `mine = false` and stays
`Behind` (miner paused). The genesis authority alone mines from height 0 — it creates genesis. A node
must never mine while behind or on a divergent fork.

## 3. Genesis ceremony (explicit, last resort)

- Genesis is created only when `CREATE_GENESIS=true`, which only `--role genesis` sets
  (`run_node` in `bin/darkwow/src/main.rs`). Genesis creation is **decoupled from mining**:
  `CREATE_GENESIS` is a separate explicit flag; mining is governed by `MINING_ENABLED` + the
  `CaughtUp` gate, never by the genesis role.
- The ceremony is deterministic: `init_genesis` (`bin/dwowd/src/lib.rs`) builds block 1 with pinned
  `timestamp=0`, `previous=blake3([0u8;32])`, `target=BlockTarget::MAX`, `miner=[0u8;32]`, the network
  magic bytes embedded in `anchor_tx_id[0..4]`, one coinbase + 9 genesis contract deployments; the hash is
  verified against the compile-time `genesis_hash.txt` (`include_str!("../genesis_hash.txt")`). The hash
  depends on exactly three inputs — the genesis miner key, the magic bytes and the compiled contract WASM.
  `miner=[0u8;32]` is why the coinbase-to-`header.miner` binding enforced for ordinary blocks is EXEMPT at
  genesis (see `block_acceptor.rs`, alongside the size-cap and witness exemptions): genesis carries no
  miner identity, its authenticity is the pin. The pin is now POPULATED, so a mismatch is a hard error.
- **The genesis identity is a single repository-wide value**: the devnet node0 key
  (`contrib/docker/darkwow-testnet/keys.toml`) with the `DRKW` magic. A second node MUST NOT
  independently create genesis; peers reject a divergent genesis via the pinned hash, and a genesis built
  from any other key or magic will not match the pin at all. Tests use the same identity, defined once in
  `bin/dwowd/src/tests/modules/chain_setup.rs`.
- Creation is gated by the `GenesisAuthority` marker (`bin/dwowd/src/task/consensus_linear.rs`), constructed
  via the infallible `GenesisAuthority::new()` only on the `CREATE_GENESIS` path.
- A re-roll requires a FRESH datadir: `init_linear` refuses to overwrite a datadir that already holds a
  height ≥ 1 chain (it verifies the stored genesis against the pin instead).

## 4. Fork policy

A node SHALL NOT mine a divergent fork; it SHALL adopt the canonical (heaviest) chain. Competing blocks at the
same height are stored as uncles for reward only (`store_competing_block`,
`src/linear/src/chain_state.rs`). The normative fork-choice rule is
[consensus.md §Fork Choice Rule](consensus.md#fork-choice-rule).

**Reorg (heaviest-chain, general depth).** When a node receives a block that builds on a chain it does not
hold, it SHALL resolve the fork by accumulated work, following Bitcoin's `DisconnectBlock`/`ConnectBlock`
pattern:

1. **Fetch** the competing chain segment from the peer, walking back — each ancestor fetched via
   `peer.request_blocks(cursor, 1)` and PoW-validated — to the common ancestor (`reorg_to_heavier_chain`,
   `bin/dwowd/src/task/consensus_linear.rs`).
2. **Decide** by accumulated work — reorg only if the competing chain carries **strictly more** accumulated
   work than the local canonical chain ([consensus.md §Fork Choice Rule](consensus.md#fork-choice-rule));
   otherwise store the block as competing/uncle and move on.
3. **Disconnect** local canonical blocks from the tip down to the common ancestor, rolling the cumulative
   supply commitment singletons back to `S_{fork_point}`.
4. **Connect** the fetched competing blocks in order (`accept_block`), then the extension block.

The mine gate (§2) prevents a node from *entering* a fork in the normal flow; the reorg path is the
corrective safety net for divergence that nonetheless arises (network partition, a stalled peer, or a
malicious competing chain).

## 5. Config — connecting a miner to an existing chain

To mine against an existing chain, a node MUST be configured with:

- `--role miner` (explicit), and
- a peer/hostlist pointing at a genesis-bearing node, so the initial sync finds the existing genesis and tip
  (a `miner` with no peers and no genesis stays `Behind` and never mines).

Without these, a node defaults to `observer` (join-first) and simply syncs.

### Docker topology (devnet)

The devnet compose (`contrib/docker/darkwow-testnet/docker-compose.yml`) instantiates the invariant
**exactly one `genesis`; every other mining node is a `miner`**:

- `node0` = `genesis` — the single authority; runs the genesis ceremony, then mines.
- `node1`..`node4` = `miner` — pure mining; start as observer, sync to node0's tip, then mine.
- `observer` = `observer` — sync + serve only.
- `join-merge` node = `MINING_ENABLED=false` — mining is external via p2pool; the internal miner stays off.

> **Devnet wipe required.** The `UncleBlock` sled binary (`uncles` tree) and JSON wire formats are
> struct-layout-locked (`src/linear/src/block.rs`) — a sled DB or peer running a different build fails
> uncle deserialization. A devnet restart SHALL start from wiped sled DBs and run one build across all
> nodes.
