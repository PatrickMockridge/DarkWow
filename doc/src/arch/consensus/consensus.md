# Consensus [IMPLEMENTED]

DarkWow uses **Uncle Merkle consensus** with RandomX Proof-of-Work. This is the only
active consensus mechanism. All supported networks (`darkwow-devnet`, `darkwow-testnet`)
use the same linear blockchain architecture in `src/linear/`. The legacy fork/overlay
DAG consensus has been fully removed — `src/validator/` no longer exists. See
[What's Different from Upstream](../about/differences_from_upstream.md) for the fork
rationale and architectural divergence.

## Implementation Status

| Feature | Status | Code Location |
|---------|--------|---------------|
| Uncle Merkle consensus (RandomX PoW) | [IMPLEMENTED] | `src/linear/` |
| Uncle block creation + merkle tree | [IMPLEMENTED] | `src/linear/src/block.rs` |
| Uncle proof verification (stateless) | [IMPLEMENTED] | `src/linear/src/validation.rs` |
| Pin reward (value-level) | [IMPLEMENTED] | `src/linear/src/chain_state.rs` |
| Pin reward (Pedersen commitment split) | [IMPLEMENTED] | `chain_state.rs:736-760` |
| Exponential reward schedule | [IMPLEMENTED] | `src/sdk/src/blockchain.rs:106-137` |
| nullifier_root block header verification | [IMPLEMENTED] | `chain_state.rs:906-915` |
| Supply audit (Pedersen mass balance) | [IMPLEMENTED] | `src/linear/src/proof_of_token_balance.rs` |
| Target adjustment algorithm | [IMPLEMENTED] | `src/linear/src/consensus.rs` |
| mark_mined — 5 block acceptance paths | [IMPLEMENTED] | `bin/dwowd/` (5 call sites) |
| Caribina (Arweave) anchoring | [IMPLEMENTED] | `src/linear/src/caribina/` |
| Monero merge-mining anchoring | [IMPLEMENTED] | `src/linear/src/monero/` |
| Fork/overlay DAG consensus | [REMOVED] | `src/validator/` deleted |
| Sharding (uncle merkle topology) | [VISION] | Design exploration — see [scaling.md](scaling.md) |
| Parallel contract execution | [VISION] | Gated on wasmer thread safety |

## Why Uncle Merkle Was Chosen Over the Overlay/Diff Architecture

This fork rejects the upstream overlay/DAG architecture in favor of deterministic
Uncle Merkle consensus. The overlay-DAG code (`src/validator/`) has been fully removed.
See [What's Different from Upstream](../about/differences_from_upstream.md) for the
full comparison and rationale.

## Current Design: Uncle Merkle with Pin Mechanism

### The Pin Mechanism

The linear blockchain uses a **use-it-or-lose-it pin mechanism** for uncle blocks:

**Rules:**
1. Canonical chain is **obligated** to offer a pin to valid uncle chains
2. Pin reward: 50% at depth 1, halving each depth (25%, 12.5%, 6.25%...)
3. Uncle chain has a **one-time** option to accept or reject the pin
4. If accepted: Uncle gets pin reward, canonical absorbs all uncle transactions

### HAZOP Consensus Hardening (2026-07-31)

Several validation gaps were closed as part of the HAZOP remediation:

- **Monero coinbase Merkle proof (H-14, M-3):** Competing blocks and uncle chain
  extensions now verify `is_coinbase_valid_merkle_root()` for `PowSource::Monero`
  blocks. Previously only the canonical path checked this — competing/uncle Monero
  blocks were validated via a meaningless RandomX hash.

- **Uncle chain extension difficulty (H-15):** Blocks extending an uncle chain must
  meet the proper `get_next_work_required` target for their height. Previously they
  were only checked against absolute min/max bounds (1 to `u32::MAX`), allowing
  trivially-easy uncle chain extensions.

- **Block reward upper bound (H-1):** A 2x sanity cap on `total_reward` at the host
  level prevents runaway inflation if the WASM `pow_reward_v1` contract is compromised.
  The WASM contract independently enforces the exact emission schedule.
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

## Supply Audit Capability

NativeToken provides a **proof of token balance** — an active consensus rule
enforced at every block acceptance path in `dwowd`. It combines a per-block
Pedersen mass balance check with the cumulative supply commitment chain
(`S_H = S_{H-1} + C_H`) to prove that no hidden darkw minting occurs beyond
the coinbase reward.

### MassBalance Naming Convention

Per [fee-spec.md §0](consensus/fee-spec.md) and [type-system.md §8.2](../type-system.md),
types participating in the Pedersen mass balance proof carry the `MassBalance` prefix:

| Type | Selector | Role |
|------|----------|------|
| `MassBalanceCoinbaseV1CallData` | `0x05` | Block-opening coinbase nullifier claim |
| `MassBalanceFeeCollectV1CallData` | `0x06` | Fee accumulator verification + miner mint |
| `MassBalanceFeeV2CallData` | `0x08` | Dual-domain: `↓pay-fee` [mass_balance] + `↓threshold-prove` [fee_signalling] |

The `MassBalance` prefix is a strong signal: code referencing these types
participates in the consensus-critical block proof. The supply audit verifies
every `MassBalanceCoinbaseV1CallData` (coinbase mint) and `MassBalanceFeeCollectV1CallData`
(fee redistribution) against the Pedersen cumulative commitment chain. A
`MassBalanceFeeV2CallData` carries both a mass_balance barb (`↓pay-fee`, value
conservation) and a fee_signalling barb (`↓threshold-prove`, mempool admission) —
it is the only dual-domain type.

### Process Engineering Context — The Flow Meter

The supply audit is best understood through a process engineering analogy.
In a chemical plant, you cannot see inside a pipeline, reactor, or distillation
column. You instrument the process: flow meters measure throughput, pressure
gauges measure driving force, control valves regulate flow rate. The readings
don't tell you what individual molecules are doing — but they prove that mass
entering the system equals mass leaving it, and that the flow rate matches the
valve setting.

The DarkWow fee architecture maps directly to these concepts. The supply audit
is the **flow meter** — it proves what comes in equals what comes out per
consensus rules. The fee signalling system (see `fee-spec.md §0.1`) is the
**control valve** — it regulates transaction flow into the mempool.

**The Meter Chain (per block):**

```
Coinbase (0x05)         FeeV2 × N (0x08)           FeeCollectV1 (0x06)
───────────────         ────────────────            ──────────────────
Opens the meter          Pulses the totalizer        Closes + reads meter
                         
Creates coinbase         Each fee_value_commit       Verifies accumulator
UTXO at position 0       adds to fee_commit_         matches claimed fees
                         accumulator                 
                                                    Transfers fee pot
                         Commitment₁                  to miner
                         + Commitment₂               
                         + ... + Commitment_N        Resets accumulator
                         = totalizer reading         to Identity (zero)
```

**Why Pedersen commitments for the meter:** In a privacy-preserving system, you
cannot see individual fee amounts inside the pipe. Pedersen commitments are
computationally hiding — no information about the fee value leaks. But their
*homomorphic* property allows the verifier to sum them blind:

```
Commit(f₁, b₁) + Commit(f₂, b₂) = Commit(f₁+f₂, b₁+b₂)
```

The meter works without seeing inside the pipe. It verifies the sum of all fee
commitments equals the claimed total, without knowing any individual fee. This
is the cryptographic equivalent of a flow totalizer that integrates all pulses
into a single reading.

**Meter fraud = hidden inflation:** If the mass balance check could be bypassed,
a miner could mint arbitrary amounts of darkw by forging the coinbase reward
beyond the emission schedule. This is exactly the ZCash Orchard exploit class
(see Motivation below). The supply audit is the defense-in-depth: even if the
ZK circuit has a soundness bug, the Pedersen external audit catches the
inflation because the forged commitment won't match the expected value.

**Separation of concerns:**

| Function | Domain | Analogy Component | Specification |
|----------|--------|-------------------|---------------|
| Fee threshold proof | fee_signalling | Pressure gauge | `fee-spec.md §5.5` |
| Mempool admission | fee_signalling | Control valve | `fee-spec.md §7` |
| Fee window adaptation | fee_signalling | PID controller | `fee-spec.md §12` |
| Per-block mass balance | mass_balance | Flow totalizer | This section |
| Fee commitment accumulation | mass_balance | Totalizer register | `chain_state.rs` (`fee_commit_accumulator`) |
| Cumulative supply chain | mass_balance | Meter log (historical record) | `proof_of_token_balance.rs` |

See: `fee-spec.md §0.1` for the process engineering analogy and the fee_signalling
control valve. See: `consensus-coinbase.md §2-3` for the meter endpoint events
(coinbase open, FeeCollectV1 close).

### Motivation: The Orchard Exploit (May 2026)

In May 2026, a missing circuit constraint was discovered in the Orchard shielded
pool. The circuit had an under-constrained elliptic-curve check that allowed false
inputs to produce valid ZK proofs. The bug existed undetected for four years.

**Why Orchard had no defense:** The Orchard pool had no supply audit capability.
Supply verification relied entirely on per-transaction balance checks. When a
single circuit constraint was missing, the entire edifice collapsed. There was
no independent, cross-transaction audit mechanism. The network still cannot
cryptographically prove the bug wasn't exploited.

### How It Works: Two Properties of the Same Capability

The Pedersen cumulative commitment chain is a single capability verified through
two independent cryptographic properties:

#### Property 1 — ZK Circuit Constraint

Each coinbase ZK proof constrains `S_H = S_{H-1} + C_H` via `ec_add` in the
Mint_V1 circuit (6 public inputs including `new_cumulative_x` and
`new_cumulative_y`). Depends on **Halo2 proof system soundness**.

#### Property 2 — Pedersen Binding (External Audit)

Any node can run `verify_cumulative_supply()` which walks the canonical chain,
recomputes every blind and commitment from the emission schedule, and compares
against stored `S_H`. This function **does not verify a single ZK proof**. It
is pure Pedersen arithmetic. Depends on **Pedersen commitment binding** (the
discrete log between `G_v` and `G_r`).

```
blind_H = blake3("native_token_coinbase_blind" || prev_coin || height)
S_H = S_{H-1} + pedersen_commit(expected_reward(H), blind_H)
```

#### Why Two Properties Matter

A ZK soundness bug alone cannot hide inflation from the external audit — the
forged `S_H` won't match `pedersen_commit(expected_supply, expected_blind)`.
Conversely, a Pedersen binding break alone cannot fool the ZK circuit — `ec_add`
still rejects. Both properties verify the same fact (supply integrity) through
different cryptographic assumptions.

### Active Consensus Enforcement

The proof of token balance is an **active consensus rule** — it is enforced at
every block acceptance path in `dwowd` (P2P broadcast, built-in miner, RPC miner,
stratum, merge mining, and consensus sync). A block that fails the mass balance
check is rejected before it can be applied to the chain.

The check has two components:

1. **Per-block Pedersen mass balance**: For every native token call in the block,
   all `Input.value_commit` and `Output.value_commit` Pedersen points are summed.
   The equation `Σ outputs + Σ burns + Σ fees == Σ inputs` must hold for darkw
   token. This proves that non-coinbase transactions do not secretly mint new supply.

2. **Cumulative supply chain**: The coinbase ZK proof constrains `S_H = S_{H-1} + C_H`
   via `ec_add` in the Mint_V1 circuit. The contract entrypoint verifies that the
   new cumulative commitment matches the expected value from the emission schedule.

Together, these prove that the only new darkw entering circulation is the coinbase
reward specified by the emission schedule.

The implementation is at `src/linear/src/proof_of_token_balance.rs` (always active —
the feature gate was removed in Phase 6). The Python model at
`contrib/model/proof_of_token_balance.py` demonstrates the mass balance
equation with test vectors.

### Uncle Coinbase Split and Supply Audit

When uncles with accepted pins are included in a canonical block, the coinbase
is split at the consensus level using Pedersen commitment subtraction. The full
mass balance proof is specified in [Uncle Merkle Consensus](uncle_merkle.md#formal-specification).

The key equation:

```
C_base = C_effective + Σ C_uncle_i
```

where `C_base` is the ZK-proven coinbase commitment, `C_effective` is the
canonical miner's share, and `C_uncle_i` are deterministic uncle reward
commitments. The mass balance holds by Pedersen additive homomorphism — the
split neither creates nor destroys value. No new ZK proofs are needed.

The two-property supply audit system covers uncle splits:

- **Property 1 (ZK circuit)**: The Mint_V1 circuit constrains `S_H = S_{H-1} + C_base`
  where `C_base` is the full base reward commitment. The circuit doesn't know
  about the split — it proves the total was minted correctly.
- **Property 2 (Pedersen binding)**: Any node can recompute every `r_i`
  deterministically and verify `C_effective + Σ C_uncle_i = C_base` for
  every block. This requires only public data (uncle hashes, pin rewards, heights).

Together: the ZK proof guarantees correct total emission, and the Pedersen
mass balance guarantees correct distribution. A ZK bug could hide which
commitment is which, but not create value. A Pedersen binding break could
falsify a split, but not increase total supply.

### Economic Implications

Nodes reject blocks that fail the mass balance check. The proof of token balance
ensures that total supply cannot exceed the emission schedule. Exchanges, holders,
and auditors can independently verify circulating supply without trusting any
single party — the check is reproducible from public block data.

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

The linear blockchain uses **heaviest-chain fork selection** — the chain with
the most accumulated work wins. Cumulative work is tracked in
`PoWConsensus::accumulated_work` (u128, persisted to sled) and computed per
block as `u32::MAX / target` (the standard Bitcoin formula).

```
Rule: The valid chain with the highest accumulated work wins.
      At equal height and equal work, the first block received wins.
      At equal height, a competing chain with one additional block
      (more accumulated work) triggers a 1-deep reorg.
```

### Reorg Depth

Reorg depth is bounded to **1 block** by the height-gap check in
`connect_block` (`block_height > current_height + 1` is rejected). An
uncle chain can extend at most 1 block past the canonical tip. When a
heavier uncle chain is detected:

1. The canonical block at the fork height is **disconnected** (all state
   reversed in a single cross-tree sled transaction)
2. The competing block is **connected** via the standard `accept_block`
   pipeline (WASM executed, cumulative supply chain updated)
3. The extension block is **connected** similarly

This follows Bitcoin's `DisconnectBlock`/`ConnectBlock` pattern. The
1-deep bound prevents reorg oscillation and limits state reversal
complexity. General-depth reorg support (matching the Python model's
`reorganize_to`) is deferred to a future consensus upgrade.

### Implications

- **Single parent pointer**: Each block references exactly one parent via
  `header.previous` (a `blake3::Hash`). No DAG, no multiple parents.
- **1-deep reorg**: A competing chain that grows longer than canonical and
  carries more accumulated work replaces the canonical block at the fork
  height. Depth is bounded to 1 by the height-gap check.
- **First-seen wins at equal work**: At the same height, both competing and
  canonical blocks target the same `get_next_work_required(H)` value, so
  their `chain_work()` is identical. First-seen wins in this case.
- **Finality guard**: Blocks carrying Caribina (Arweave) or Monero finality
  anchors are never displaced by reorg. The finality check runs before
  disconnect.

### Relationship to Uncle Merkle

Uncle Merkle provides **economic mitigation**: a miner who loses the fork race
at height N can still earn partial reward as an uncle at height N+1. This
eliminates the all-or-nothing incentive for fork hiding. However, it does not
substitute for correct fork selection — a miner with less hashpower who
propagates blocks faster can permanently control the canonical chain if fork
selection ignores accumulated work. Cumulative-work fork selection closes this
gap while preserving the uncle reward mechanism for the common case of
simultaneous block production.

### Why 1-Deep Cumulative-Work Fork Choice

The 1-deep bound is a conservative engineering choice:

1. **Matches the Python model**: `contrib/model/chain_model.py` implements
   cumulative-work fork selection (`reorganize_to`, line 532). The Rust
   implementation now conforms to the specification.
2. **Bounded by height-gap check**: The existing `HeightDiscontinuity` guard
   limits uncle chains to at most 1 block ahead, making the 1-deep bound a
   property of the current architecture.
3. **Anchored finality prevents deep reorgs**: Caribina (Arweave) finality
   makes reorgs past anchored blocks cryptographically infeasible. The 1-deep
   window covers unanchored blocks awaiting finality confirmation.
4. **Uncle Merkle handles the common case**: Simultaneous blocks at the same
   height earn uncle rewards without reorg. Cumulative-work fork selection
   only activates when one chain has objectively more work.

Source: [`src/linear/src/consensus.rs`](../../../src/linear/src/consensus.rs),
[`src/linear/src/chain_state.rs`](../../../src/linear/src/chain_state.rs),
[`bin/dwowd/src/block_acceptor.rs`](../../../bin/dwowd/src/block_acceptor.rs).

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

## Current State (July 2026)

Uncle Merkle consensus is the only active consensus mechanism. The network name
in `dwowd_config.toml` determines configuration at startup
(`bin/dwowd/src/main.rs:160`):

| Network | Consensus | Location | Status |
|---------|-----------|----------|--------|
| `darkwow-devnet` | Uncle Merkle (linear) | `src/linear/` | Local devnet — fast iteration |
| `darkwow-testnet` | Uncle Merkle (linear) | `src/linear/` | Public testnet — mining, contracts, merge mining |

The legacy `testnet` (fork/overlay DAG) and `linear-testnet` networks are no longer
supported. `src/validator/` has been fully removed. WASM contract execution during
block validation is fully implemented — canonical and uncle transactions are executed
via `bin/dwowd/src/execution.rs` with deterministic diff merging. Pure validation
functions live in `src/linear/src/validation.rs`.

## Type-Level Enforcement

The consensus implementation SHALL use Rust's type system to make invalid states
unrepresentable. This section specifies the four mechanisms and their application
to consensus safety. The formal specification is `type-system.md` (§2.3, §4.1,
§5.1, §7, §9.3).

### Nominal Outcome Enums

Functions that can produce semantically different consensus outcomes SHALL
return a nominal enum. `Result<()>` SHALL NOT be used where the `Ok` variant
collapses multiple states.

**Applied:** `BlockConnectOutcome` at `src/linear/src/chain_state.rs` replaces
`Result<()>` with three variants: `CanonicalExtension{new_height}`,
`CompetingStored`, `UncleExtended`. Every caller MUST match all three —
the compiler rejects any code path that calls `mark_mined` on a non-canonical
block. This prevents HAZID H-H7 (mempool transaction loss on competing blocks)
and H-H8 (competing block permanent loss) at the type level — no runtime
check, no convention, no code review can override the compiler's exhaustive
match enforcement.

### Typed State Machines

Consensus state machines crossing module boundaries SHALL be typed enums
with explicit variants. Raw integer constants (`pub const X: u8 = N`) with
manual `AtomicU8::load`/`store` SHALL NOT implement a distributed state
machine.

**Applied:** `SyncState` enum at `bin/dwowd/src/lib.rs` replaces four `u8`
constants (`SYNC_INITIAL` through `SYNC_BEHIND`) with a `#[repr(u8)]` enum
and a single `SyncState::load(&AtomicU8)` accessor. States are `Initial`,
`Syncing`, `CaughtUp`, `Behind`. The miner SHALL check `SyncState::CaughtUp`
before producing blocks; the sync task SHALL set `SyncState::Syncing` during
active download. This prevents premature mining on stale tips (HAZOP F1,
HAZID H-M12).

### Authority Marker Types

Consensus authority SHALL be represented by nominal marker types, not bare
booleans. A `bool` carries no proof of key possession, no type-level
distinction from any other `bool`, and no compiler enforcement.

**Applied:** `GenesisAuthority` at `bin/dwowd/src/task/consensus_linear.rs`
(Change 3 planned) replaces `genesis_authority: bool` with a zero-sized
marker type constructible only via `from_key(secret)`. The authority gate in
the sync state machine requires `Some(GenesisAuthority)` — a node that lost
its genesis key cannot accidentally claim authority. Implements
`ExhibitsBarb { &[Mine] }` — the `↓mine` barb is witnessed at compile time.

### Nominal Consensus Scalars

Every consensus quantity (height, reward, target, gas, supply) SHALL be a
distinct nominal type. The compiler SHALL reject `expected_reward(height)`
where a `BlockHeight` is passed to a `BlockReward` parameter. A bare `as`
cast on any consensus quantity SHALL NOT pass review.

**Applied:** `BlockHeight(u64)` at `src/sdk/src/blockchain.rs` is the
canonical nominal consensus scalar. Planned: `BlockReward(u64)`,
`BlockTarget(u32)`, `GasAmount(u64)` following the same `#[repr(transparent)]`
pattern — named constructors, no `From<u64>`, manual serde, dwow-serial
transparent encoding (Change 4).

## PoWRewardV1 Nullifier Claim — Single-Path Coinbase

### Rationale

Every other native token operation (FeeV1, BurnV1, SpendV1, TransferV1) follows
the o-cap pattern: commit to a coin, prove knowledge of the secret, publish a
nullifier to exercise the capability. The block reward (coinbase) is no different.
The miner who finds a valid PoW gains the capability to claim the reward by
publishing a nullifier against the PoWRewardV1 commitment.

This replaces the dual-path architecture where the coinbase reward flowed through
two mechanisms simultaneously. The single path is:

```
PoW valid → miner derives sk_H → miner computes C + nf → miner proves ZK →
miner publishes block with PoWRewardV1 at transactions[0].contract_calls[0] →
validators verify nf against nullifier SMT → reward claimed
```

### Consensus Rule (7-Phase Validation)

Every validator MUST verify the following before accepting a block. Phases run
in order — cheapest check first, fail fast. Phase failures SHALL produce typed
error barbs per [type-system.md §4](type-system.md): `↓bad-proof` (ZK/signature/
structural), `↓bad-nullifier` (duplicate nullifier), `↓db-fail` (state corruption).

```
Phase 0 — Structural (validate_block_structure):
  0.1 Block has >= 1 transaction
  0.2 transactions[0].contract_calls[0] is PoWRewardV1 (contract_id == NATIVE_TOKEN, data[0] == 0x05)
  0.3 Exactly one PoWRewardV1 call in block
  0.4 Coinbase nullifier is non-zero
  0.5 FeeCollectV1 rules (consensus-coinbase.md §3.15):
      at most one FeeCollectV1 call (data[0] == 0x06);
      present iff sum of FeeV1 fees in block > 0 (checked add — overflow rejects);
      must be the final transaction

Phase 1 — PoW:
  RandomX(to_mining_blob())[0..4] as u32 LE <= block.header.target

Phase 2 — Chain Continuity:
  block.header.height == chain_tip.height + 1
  block.header.previous == hash(chain_tip)

Phase 3 — Nullifier + ZK Proof:
  3.1 Extract nf from PoWRewardV1 call public inputs
  3.2 nf NOT IN nullifier SMT (duplicate claim = reject)
  3.3 verify_ZK(proof, public_inputs) — Mint_V1 circuit constrains nf == poseidon_hash(sk_H, C)
  3.4 reward == expected_reward(H) — emission schedule enforcement
  3.5 FeeCollectV1 witness verification — FeeCollect_V1 circuit, 7 public inputs
      (L2: decode_and_reconcile + verify_core_tx_with_tables)

Phase 4 — WASM Execution:
  execute pow_reward_v1 (0x05) — verifies nullifier, coin uniqueness, cumulative supply chain

Phase 5 — Transactions:
  Execute remaining transactions (fees, transfers, burns, spends) sequentially
  in block order — see Execution Ordering & Atomicity Layers below.
  fee_collect_v1 (0x06) executes LAST: verifies total against fees_db[H]
  accumulated by this block's FeeV1 calls, closes the coin merkle tree.

Phase 6 — Nullifier SMT Update:
  Insert nf into nullifier SMT as first entry for this block
  Insert remaining nullifiers from spends (incl. the fee-collect nullifier)
  Verify nullifier_root matches block header

Phase 7 — Atomic Commit:
  Commit block, contracts overlay, supply chain, coins, nullifiers in single sled transaction
```

### Execution Ordering & Atomicity Layers

*Normative. Added 2026-07-16 with FeeCollectV1 (consensus-coinbase.md §3) —
supersedes the per-call isolated-overlay model.*

Canonical contract calls SHALL execute **sequentially in block order**
(transaction order, then call order within each transaction) against a
**single shared sled overlay**. Call N SHALL observe all state written by
calls 1..N-1 of the same block. Any contract logic that reads state written
by a sibling call in the same block (e.g. `fee_collect_v1` reading
`fees_db[H]` accumulated by this block's FeeV1 calls) depends on exactly
this guarantee and MUST cite this section.

Block state integrity is enforced at three nested atomicity layers:

| Layer | Scope | Mechanism | Integrity check |
|-------|-------|-----------|-----------------|
| 1. Transaction atomicity | one contract call | per-call `checkpoint()` / `revert_to_checkpoint()` on the shared overlay | a failing call leaves zero writes |
| 2. Merkle-tree atomicity | all canonical calls, block order | one shared `SledTreeOverlay` — call N sees calls 1..N-1 | **the fee release check**: PoWRewardV1 opens the coin merkle tree at transactions[0]; FeeCollectV1 closes it at transactions[last] — its entrypoint check `total_fees == fees_db[H]` passes iff every FeeV1 in the block executed and is visible |
| 3. Block-commit atomicity | whole block | single sled cross-tree transaction in `connect_block` (blocks, uncles, contracts, consensus, coins, nullifiers, supply chain) | all-or-nothing block application |

**Failure semantics (strict):** any failed canonical call SHALL reject the
block. A valid miner never includes failing transactions — mempool admission
verifies every transaction's witness before acceptance, and the miner
re-executes at block assembly. Tolerating failed canonical calls would let
a malicious miner include garbage that diverges validator state.

**Uncle calls** remain isolated: they were mined against pre-block state and
execute against independent overlay clones, merged after canonical execution
with canonical-wins semantics (`remove_diff`). Uncle call failures are
tolerated (best-effort). Uncle-vs-uncle duplicate key writes reject the block
(non-deterministic merge order otherwise).

**Double-spend detection:** with sequential visibility, the second spend of
a coin within a block fails directly at the entrypoint's nullifier SMT check
(it sees the first call's nullifier write). The former same-block
double-write conflict rejection is superseded for canonical calls; it is
retained for uncle-vs-uncle merges and Deployooor deployments.

**Consensus impact:** this changes execution semantics relative to the
per-call isolated-overlay model (which rejected any block containing two
calls writing the same key — making a coinbase plus any coin-creating user
transaction unminable). Deployed networks MUST restart from a fresh genesis
(`--fresh`); no mainnet exists.

### Miner Obligation

At height H, with declared identity secret `sk_owner`:

```
sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H.to_le_bytes())
pk_H = PublicKey::from_secret(sk_H)
C    = poseidon_hash(pk_H.x, pk_H.y, reward, DRKW_TOKEN_ID, 0, 0, blind)
nf   = poseidon_hash(sk_H.inner(), C)
π    = prove(Mint_V1, witness={sk_H, pk_H, reward, blind, ...}, public={C, vc, tc, nf, S_H})
```

The miner MUST:
- Use the deterministic per-block key `sk_H` (no random keys — wallet must derive same key)
- Include PoWRewardV1 as `transactions[0].contract_calls[0]`
- Publish the nullifier in the ZK proof public inputs
- Place the coinbase transaction FIRST (index 0) in the block's transaction list

### Cheat Detection — Sybil/Spoof Rejection

Every deviation from the protocol is detectable at a specific phase.
Error terminology follows [type-system.md §4](type-system.md):

| Attack | Detection | Phase | Rejection Error Barb |
|--------|-----------|-------|-----------------|
| Wrong nullifier (random bytes) | ZK proof fails — nf != poseidon_hash(sk_H, C) | 3.3 | `↓bad-proof` |
| Duplicate nullifier (replay) | nf already in nullifier SMT | 3.2 | `↓bad-nullifier` |
| Missing nullifier (zero bytes) | Structural check — nf == 0 | 0.4 | `↓bad-proof` |
| Wrong reward amount | Validator compares with expected_reward(H) | 3.4 | `↓bad-proof` |
| Coinbase not at transactions[0] | Structural check | 0.2 | `↓bad-proof` |
| Multiple coinbases | Count check | 0.3 | `↓bad-proof` |
| Random key (not deterministic) | Wallet can't decrypt — nullifier won't match derived key | 3.3 | `↓bad-proof` (wallet-side detection) |

### Wallet Pure Function Integration

The wallet scan reads this deterministically:

```
scan_block : Secrets × Block → BlockScanResult

For transactions[0].contract_calls:
  If cid == NATIVE_TOKEN_CONTRACT_ID and data[0] == 0x05:
    1. Derive sk_H from secrets (same as miner)
    2. AEAD-decrypt the output note
    3. Compute C = poseidon_hash(pk_H.x, pk_H.y, value, ...)
    4. Verify nf == poseidon_hash(sk_H.inner(), C)
    5. Build CapRecord on match
```

Same keys, same chain → identical wallet state. WalletState = f(AccountManager, ChainBlocks).

### ZK Transparency

The ZK proof hides witness data (coin_secret, value, blinds) but exposes public
inputs that all validators can verify:

| Public Input | What It Proves |
|-------------|----------------|
| C (coin commitment) | Coin attributes are correctly hashed |
| nf (nullifier) | Miner knows sk_H corresponding to pk_H |
| value_commit.x, value_commit.y | Pedersen commitment to reward value |
| token_commit | Only DRKW_TOKEN_ID can be minted |
| S_H.x, S_H.y | Cumulative supply chain is maintained |

All validators see the same public inputs. Foul play is detectable even though
witness data stays private.

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
See [Consensus & Coinbase](../consensus-coinbase.md#anchoring-finality-gadget) for the anchoring finality gadget specification.

The original fork/overlay DAG consensus specification has been superseded by the linear blockchain architecture described in this document.
