# Consensus

DarkWow uses **Uncle Merkle consensus** with RandomX Proof-of-Work for new networks (`dwow-devnet`, `darkwow-testnet`, and the legacy-named `linear-testnet`). The original fork/overlay consensus remains active for the legacy `testnet` network. Both implementations coexist in the codebase, with new development targeting the linear blockchain.

## Why Uncle Merkle Was Chosen Over the Overlay/Diff Architecture

The upstream DarkWow consensus uses a speculative overlay/diff system with sled-overlay for transactional state management. This architecture exists primarily to support the "under one tent" DAO governance model: a complex fork-deciding mechanism is required because the DAO must adjudicate which fork is canonical, preventing natural chain splits.

This fork rejects that entire design stack for several reasons:

**1. Engineering complexity from governance requirements**

The overlay/diff system exists because upstream's DAO governance requires a mechanism to decide between competing forks. This creates a cascade of complexity: speculative state verification, overlay checkpoints, diff logging for rollback, and implicit fork competition. None of this is necessary in a pure PoW system where the canonical chain is simply the one with the most work — and where hard forks are a natural, healthy part of the ecosystem.

**2. Non-deterministic behavior breaks testing**

The upstream consensus uses speculative verification where state can be committed, rolled back, or left in limbo. `diff()` computation depends on sequence history — the same code produces different results depending on timing. This makes deterministic testing impossible. Race conditions and timing-dependent state create flaky tests that erode confidence in the entire contract system.

**3. The overlay adds complexity without benefit**

The sled-overlay system provides transactional state with automatic rollback. On this fork, there is no DAO deciding between forks — so there is nothing to roll back. Plain sled is simpler, faster, and predictable. Every state change is final.

**4. Pure PoW means forks are a feature, not a bug**

Without a governance DAO motivated to keep everything under one tent, chain splits are handled the Bitcoin way: miners follow the chain with the most work. If a contentious hard fork occurs, both sides can coexist (like BTC/BCH). The Uncle Merkle mechanism makes this Pareto efficient — competing miners still get partial reward rather than zero.

The old upstream overlay-DAG consensus specification is preserved for reference at [legacy/consensus_dag.md](../legacy/consensus_dag.md).

## Current Design: Uncle Merkle with Pin Mechanism

### The Pin Mechanism

The linear blockchain uses a **use-it-or-lose-it pin mechanism** for uncle blocks:

**Rules:**
1. Canonical chain is **obligated** to offer a pin to valid uncle chains
2. Pin reward: 50% at depth 1, halving each depth (25%, 12.5%, 6.25%...)
3. Uncle chain has a **one-time** option to accept or reject the pin
4. If accepted: Uncle gets pin reward, canonical absorbs all uncle transactions
5. If rejected: Uncle loses coinbase entirely, canonical absorbs transactions anyway

**Why this is elegant:**
- **No secret mining incentive**: By the time an uncle publishes, canonical already has the transactions
- **No uncle-farming**: Canonical must offer, cannot refuse a valid uncle
- **No complex multi-step distribution**: Simple one-shot accept/reject decision
- **Fork absorption**: Uncle chain gives up reward but gains inclusion

**The equilibrium:**
Rejection is strictly dominated - accepting gives 50%+ reward, rejecting gives 0. Rational miners always accept.

### Uncle Merkle Structure

Uncle blocks are referenced in canonical blocks via a merkle tree:
- Uncle merkle root stored in canonical block header
- Merkle proof provides stateless verification
- No uncle storage required for verification (only for archival)

### Reward Distribution

The canonical block pays pin rewards from its own block reward - **no over-minting**:

| Uncle Depth | Pin Reward | Uncle Gets | Canonical Gets |
|-------------|------------|------------|---------------|
| None        | -          | -          | 100%          |
| 1           | 50%        | 50% of block reward | 50% (100% - 50%) |
| 2           | 25%        | 25% of block reward | 75% (100% - 25%) |
| 3           | 12.5%      | 12.5% of block reward | 87.5% (100% - 12.5%) |

**Invariant:** `canonical_reward + sum(uncle_rewards) = base_reward` (exactly 100%)

### Testing Benefits

The linear blockchain's consensus model is **ideal for testing**:

1. **Deterministic**: Same input → same output every time. No race conditions.
2. **No rollback**: State changes are final. No speculative commits that could vanish.
3. **Stateless verification**: Only block headers + merkle proofs needed. No WASM execution.
4. **Plain storage**: Uses sled directly, not sled-overlay. Simpler, faster, predictable.
5. **Isolated**: Can run 5-node localnet harness with full consensus without the full validator stack.

Example test harness behavior:
```rust
// Run Level 1 lightweight tests (deterministic, no network)
cargo test -p dwowd test_linear

// Run Level 3 multi-node Docker testnet
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native

// Run full mining + contract test suite
./contrib/docker/darkwow-testnet/test-contracts.sh
```

### Comparison to Upstream Consensus

| Aspect | Upstream (Fork/Overlay) | This Fork (Uncle Merkle) |
|--------|----------------------|----------------------|
| State management | Overlay + diffs + rollback | Plain sled |
| Fork resolution | Implicit competition | Explicit uncle reference |
| Mining risk | All-or-nothing | Bounded partial reward |
| Verification | Heavy WASM + sled lookups | Merkle proof only |
| Determinism | Non-deterministic in time | Fully deterministic |
| Testing | Flaky, timing-dependent | Deterministic, isolated |
| Complexity | High | Low |

## Fork Choice Rule

The linear blockchain uses **strict longest-chain by height** — the simplest
possible fork choice. There is no cumulative difficulty comparison, no chain
weight computation, and no reorg handling.

```
Rule: The valid chain with the highest block height wins.
      At equal height, the first block received wins permanently.
```

### Implications

- **Single parent pointer**: Each block references exactly one parent via
  `header.previous` (a `blake3::Hash`). No DAG, no multiple parents.
- **No reorg handling**: Once a block is inserted at height N, no block can
  replace it. The `insert_validated_block()` function rejects blocks at
  already-occupied heights if the existing block carries a finality anchor
  (`anchor_tx_id != [0u8; 32]`). In practice this means the first block
  received at height N wins.
- **First-seen wins**: At equal height, network latency determines which
  block propagates first. There is no tie-breaking by hash or target.
- **Design rationale**: The Uncle Merkle mechanism makes this safe. A miner
  who loses the race at height N can still earn partial reward as an uncle
  at height N+1. There is no wasted work.

### Why No Cumulative Difficulty

Bitcoin and Monero use "chain with most accumulated work" as the fork
criterion to allow chain tips to compete. This requires tracking cumulative
difficulty per chain tip and comparing across forks. The linear blockchain
doesn't need this because:

1. Uncle Merkle eliminates the all-or-nothing incentive for fork competition
2. Linear chain structure means only one valid tip at any height
3. Anchored finality (Caribina) makes reorgs impossible for finalized blocks

Source: [`src/linear/src/consensus.rs`](../../../src/linear/src/consensus.rs),
[`bin/dwowd/src/blockchain.rs`](../../../bin/dwowd/src/blockchain.rs).

## Target Adjustment Algorithm

The Proof-of-Work target adjusts each time a block is inserted using a
**proportional controller** with a sliding window and ±10% single-step clamp.

Source: [`src/linear/src/consensus.rs`](../../../src/linear/src/consensus.rs).

### Parameters

| Parameter | Value | Description |
|-----------|-------|-------------|
| `target_block_time` | 120 seconds | Desired interval between blocks |
| `TIMESTAMP_WINDOW` | 20 | Max timestamps stored (sliding window) |
| Clamp ratio | ±10% | Max single-step target change |
| Ratio bounds | [0.5, 2.0] | Prevents divergence under extreme hashrate changes |
| `min_target` | 1 | Hardest possible target |
| `max_target` | u32::MAX | Easiest possible target |

### Algorithm

1. **Record timestamp**: `record_block(timestamp)` pushes the new block's
   timestamp into the sliding window (max 20 entries, oldest evicted first).

2. **Compute average interval**: Sum intervals between the last 10 timestamps
   in the window, divide by count.

3. **Compute ratio**: `ratio = target_block_time / avg_interval` clamped to
   `[0.5, 2.0]`.
   - `ratio > 1.0`: blocks are arriving **too fast** → target decreases (harder)
   - `ratio < 1.0`: blocks are arriving **too slow** → target increases (easier)

4. **Apply clamp**: `adjustment = 1.0 ± min(|ratio - 1.0|, 0.10)`.
   The adjustment is bounded to `[0.90, 1.10]`.

5. **Apply adjustment**: `new_target = old_target / adjustment`, clamped to
   `[min_target, max_target]`.

### Formula

```
avg_interval = sum(last_10_intervals) / (n - 1)
ratio = clamp(target_block_time / avg_interval, 0.5, 2.0)
delta = clamp(ratio - 1.0, -0.10, +0.10)
new_target = clamp(target / (1.0 + delta), min_target, max_target)
```

### Edge Cases

- **Genesis / first block**: `timestamps.len() < 2` → no adjustment, target
  remains at `initial_target`.
- **Instant blocks** (`avg_interval == 0`): ratio clamped to 0.9 (maximum
  difficulty increase of 10%).
- **Window not full** (`< 20 timestamps`): Uses up to 10 most recent
  intervals from whatever timestamps are available.

### Conventional Difficulty

The target is a 32-bit value where `hash_u32 <= target` is valid. Higher
target = easier mining. Conventional difficulty (higher = harder) is derived:

```
difficulty = u32::MAX / target
```

### Configuration

```toml
[network_config."darkwow-testnet".pow]
target_block_time = 120       # seconds
initial_target = 16777215     # 0x00FFFFFF, easy first block
min_target = 1                # hardest possible
max_target = 4294967295       # u32::MAX, easiest possible
```

Default `initial_target` was recently increased from `0x0000FFFF` to
`0x00FFFFFF` to make the first few blocks trivially mineable (~1/256
hashes pass vs ~1/65536).

## Finality Layers

DarkWow supports modular finality on top of PoW consensus. Three modes
control how nodes handle finality anchors.

Source: [`src/linear/src/finality.rs`](../../../src/linear/src/finality.rs).

### Modes

| Mode | Behavior | Use Case |
|------|----------|----------|
| **Native** | Trust PoW only. Ignore all anchors. | Pure PoW chains, development |
| **Always** | Enforce anchors on all blocks that carry them. | Production (default) |
| **Signaled** | Only enforce when a block's `finality_flags` has `FINALITY_SIGNALED` set. | Gradual rollout |

### Flag Bits

Block header field `finality_flags` (u8) at offset 145:

| Bit | Constant | Meaning |
|-----|----------|---------|
| 0x01 | `FINALITY_CARIBNIA` | Block carries a Caribina (Arweave) anchor |
| 0x02 | `FINALITY_MONERO` | Block carries a Monero (p2pool) anchor |
| 0x04 | `FINALITY_SIGNALED` | Block requires finality enforcement |

### Caribina (Arweave) Anchoring

**Status: Implemented and live.** When Caribina is enabled (`caribina_enabled:
true`) and mode is not Native, each mined block is anchored to Arweave via the
ANS-104 DataItem protocol. The Arweave transaction ID is stored in
`header.anchor_tx_id`. Anchoring is best-effort — if the Arweave network or
turbo service is unavailable, the block is still valid but carries no anchor.

### Monero (p2pool) Anchoring

**Status: Implemented.** Anchors a DarkWow block to a Monero block via p2pool
merge mining. When a Monero block containing DarkWow aux data is found, the
Monero block height and hash are stored in `header.anchor_monero_height` and
`header.anchor_monero_hash`. Verification supports two modes:
- **Lightweight plausibility** (default): accepts any block with non-zero
  anchor fields up to `MAX_PLAUSIBLE_MONERO_HEIGHT` (5M blocks).
- **Full monerod verification**: queries a monerod JSON-RPC endpoint to verify
  the anchor hash matches the actual Monero block and that sufficient
  confirmations have elapsed. Requires `--monerod-rpc-url` to be set.

### Configuration

```toml
[network_config."darkwow-testnet".finality]
mode = "always"               # "always" | "native" | "signaled"
caribina_enabled = true       # Enable Arweave anchoring
monero_enabled = false        # Enable Monero anchoring (requires p2pool)
monero_min_confirmations = 3  # Monero confirmations before finality
monerod_url = "http://127.0.0.1:18081/json_rpc"  # monerod JSON-RPC endpoint (optional)
```

CLI overrides: `--finality-mode native|always|signaled`,
`--finality-disable-caribina`, `--finality-enable-monero`,
`--monero-min-confirmations <N>`, `--monerod-rpc-url <URL>`.

### How Anchoring Provides Finality

1. Miner produces a block with PoW
2. Miner (or daemon) submits the block hash to Arweave as an ANS-104 DataItem
3. The Arweave transaction ID is stored in `block.header.anchor_tx_id`
4. Once the Arweave transaction is confirmed, the DarkWow block is **finalized**
5. Any fork that conflicts with a finalized block is rejected by nodes running
   `mode = Always`

To reorganize a finalized block, an attacker would need to reorganize
Arweave — whose cumulative difficulty dwarfs DarkWow's by orders of
magnitude.

## Current State (May 2026)

Both consensus implementations are active. The network name in `dwowd_config.toml`
determines which one is used at startup (`bin/dwowd/src/main.rs:170-180`):

| Network | Consensus | Location | Status |
|---------|-----------|----------|--------|
| `testnet` | Fork/overlay (DAG) | `src/validator/consensus.rs` | Legacy — maintained, no new features |
| `linear-testnet` | Uncle Merkle (linear) | `src/linear/` | Local devnet — fast iteration, fixed difficulty |
| `darkwow-testnet` | Uncle Merkle (linear) | `src/linear/` | Public testnet — mining, contracts, merge mining |

**In the fork-based validator**, uncle Merkle verification is a placeholder
("Phase 2 TODO" at `src/validator/verification.rs:314`). Only the zero/empty
uncle root is accepted — no actual uncle blocks are validated or rewarded.

**In the linear blockchain**, uncle structures and verification are implemented
in `src/linear/src/block.rs`. The `runtime` integration (WASM contract execution
during block validation) is partially complete — marked TODO at
`src/linear/src/blockchain.rs:211`.

All new feature development (contract testing, merge mining, p2pool adaptor,
anchoring finality) targets the linear blockchain. The fork-based validator is
kept for compatibility with existing `testnet` deployments.

## Glossary

| Name                   | Description                                                                            |
|------------------------|----------------------------------------------------------------------------------------|
| Consensus              | Algorithm for reaching blockchain consensus between participating nodes                |
| Node/Validator         | DarkWow daemon participating in the network                                             |
| Lilith Handshake       | Base P2P networking layer — every computer must handshake to participate               |
| Miner                  | Block producer                                                                         |
| Uncle Block            | Block that was mined but not canonical, but referenced by a canonical block            |
| Pin                    | Use-it-or-lose-it reward offer from canonical to uncle chain                            |
| Uncle Merkle           | Merkle tree of uncle blocks referenced by a canonical block                             |
| Block proposal         | Block that has not yet been appended onto the canonical blockchain                     |
| P2P network           | Peer-to-peer network on which nodes communicate with each other                          |
| Confirmation           | State achieved when a block and its contents are appended to the canonical blockchain  |
| Anchor                 | Monero block reference providing finality for a DarkWow block                          |
| Anchoring Finality     | Modular security overlay — finalized blocks cannot be reorganized                      |

See [Uncle Merkle Consensus](uncle_merkle.md) for detailed specification.
See [Mining Tokenomics](../mining-tokenomics.md#anchoring-finality-gadget) for the anchoring finality gadget specification.

The original fork/overlay DAG consensus specification has been archived to [legacy/consensus_dag.md](legacy/consensus_dag.md).
