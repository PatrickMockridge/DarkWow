# Mining Tokenomics

*Plain chassis. Novel engine.*

## What's Under the Hood

DarkWow's tokenomics are assembled from proven, battle-tested parts:

| Component | Source | Status |
|-----------|--------|--------|
| **21M DRKW supply cap** | Satoshi / Bitcoin | Deployed 2009, untouched since |
| **RandomX PoW** | Monero | CPU-mining since 2019 |
| **Permanent tail emission** | Monero | 1% per annum, secures chain forever |
| **Fair launch** | Satoshi | No premine, no SAFT, no insider allocation |
| **Continuous exponential decay** | Novel (math, not mechanism) | Same 4-year half-life as Bitcoin, just smoothed |
| **Uncle Merkle pin rewards** | Novel | Pareto-efficient fork handling — no wasted work |
| **zkVM** | ★ This is the new piece | Stateless ZK verification, WASM contract proofs |

The chassis is boring on purpose. Satoshi's supply model and Monero's mining model
have worked for a combined 30+ years. They don't need reinvention. The only
fundamentally new component is the zkVM — everything else is proven
infrastructure.

## Design Decisions

### 21M Cap (Satoshi)

Same hard cap as Bitcoin. No inflation beyond what the tail emission adds (see
below). Supply is deterministic from genesis — there are no governance knobs,
no minting authorizations, no token-holder votes that can change issuance.

### RandomX PoW (Monero)

CPU-optimized proof-of-work. ~4 GB dataset forces memory-hard computation —
ASICs and GPUs can't get a meaningful advantage. Anyone with a consumer laptop
can mine. This keeps mining distributed rather than concentrated in industrial
farms.

### Tail Emission (Monero)

1% per annum of the 21M cap, permanently. This works out to 79,853,981 base
units per block (~0.80 DRKW), or 210,000 DRKW/year.

Monero's tail exists for the same reason: when the main emission curve
approaches zero, you need a floor on the security budget. Without it,
miners rely entirely on fees, and fee markets are volatile. A permanent
subsidy guarantees a minimum hash rate forever.

The tail is deliberately fat — 1% is higher than Monero's ~0.87%. This is a
longevity decision. A larger security budget buys more resistance to
hashrate-driven attacks when the chain is young and market cap is low
relative to eventual scale. As total supply grows, the percentage rate
decays toward zero asymptotically.

### Continuous Decay (Smoothed Halving)

Bitcoin halves every 4 years in a single step — miners lose 50% of revenue
overnight. This causes hashrate instability around halving events.

DarkWow uses the same 4-year half-life but applies it continuously:
`R(h) = max(R₀ × 2^(-h/H), R_tail)`. Every block's reward is fractionally
smaller than the last. The emission curve is identical in total area under
the curve — it just doesn't have step-function shocks.

### Fair Launch (No Premine)

No tokens were allocated to founders, investors, or early participants.
Every DRKW in circulation was mined. This is the Bitcoin model: the only
way to acquire the native token is to contribute proof-of-work.

---

## Reward Schedule

### Constants

| Parameter | Value | Notes |
|-----------|-------|-------|
| Supply cap | 21,000,000 DRKW | Same as Bitcoin |
| Initial reward (R₀) | 1,383,764,049 base units | ~13.84 DRKW |
| Half-life (H) | 1,051,920 blocks | ~4 years at 2-min blocks |
| Tail reward (R_tail) | 79,853,981 base units | ~0.80 DRKW |
| Tail emission rate | 1% per annum | 210,000 DRKW/year |
| Block time | 120 seconds | 262,980 blocks/year |
| Genesis reward | 0 | Bootstrap block |

### Reward Function

```
R(h) = max( R₀ × 2^(-h/H), R_tail )
```

Genesis (h=0) returns 0.

Implementation at [`src/sdk/src/blockchain.rs`](../../src/sdk/src/blockchain.rs):

```rust
pub fn expected_reward(height: u32) -> u64 {
    if height == 0 {
        return reward::GENESIS_REWARD; // 0
    }
    let decay = 2.0f64.powf(-(height as f64) / reward::HALF_LIFE_BLOCKS as f64);
    let reward = (reward::INITIAL_REWARD as f64 * decay) as u64;
    reward.max(reward::TAIL_REWARD)
}
```

The computation uses `f64::powf` which is deterministic per IEEE 754 across
all supported architectures (x86_64, ARM64).

### Derivation

Initial reward from the total supply constraint:

```
R₀ = ⌊total_supply × ln(2) / half_life_blocks⌋
   = ⌊2,100,000,000,000,000 × ln(2) / 1,051,920⌋
   = 1,383,764,049 base units
```

Tail emission (1% per annum of 21M cap):

```
R_tail = ⌊21,000,000 × 0.01 × 10^8 / 262,980⌋
       = 79,853,981 base units
```

### Emission Curve

```
Reward
  ^
  |  R₀ ≈ 13.84 DRKW
  |  *
  |   *
  |    *
  |     **
  |       *
  |        **
  |          ***
  |              ****
  |                   *****
  |                         ********
  |                                  **********
  |                                              ************ R_tail ≈ 0.80 DRKW
  |                                                          ~~~~~~~~~~~~~~~~~~~~~
  +-----------------------------------------------------------------------------> Height
  0         4yr         8yr        12yr       16.5yr      20yr        forever
             |           |           |           |           |
          1 half-life  2 half-life  3 half-life  tail start
```

The main emission phase runs ~16.5 years before the exponential reward drops
below the per-block tail threshold. After that, the tail takes over permanently.

### Supply Over Time (Tail Era)

| Years after launch | Approx total supply | Annual inflation rate |
|-------------------|---------------------|----------------------|
| 20 | ~21.0M | 1.0% |
| 50 | ~27.3M | 0.77% |
| 100 | ~37.8M | 0.56% |
| 200 | ~58.8M | 0.36% |
| 500 | ~115.5M | 0.18% |

The inflation rate approaches zero as total supply grows. The tail maintains a
minimum security budget — it doesn't meaningfully inflate the supply.

---

## Uncle Merkle Consensus

### Problem

In standard PoW chains, when two miners find blocks at similar heights, one
becomes canonical and the other is orphaned — the miner wasted electricity for
nothing. This punishes smaller miners with higher latency and encourages pool
centralization.

### Solution

The canonical chain is obligated to offer competing uncle chains a pin reward
— a one-time option to join and share the PoW reward.

| Uncle depth | Pin reward (% of base reward) |
|-------------|-------------------------------|
| 1 | 50% |
| 2 | 25% |
| 3 | 12.5% |
| 4+ | Geometric decay, capped at max depth |

Rules:
- Pin is use-it-or-lose-it — uncle chain accepts or rejects within a short window (minutes)
- Accepting gives >0 reward, rejecting gives 0 — strictly dominated
- Not slashing — no one is punished, uncle miners gain, canonical miner keeps majority

This is Pareto efficient: miners are never punished for producing non-canonical
blocks, smaller miners aren't excluded from rewards, and it's simpler than
DAG-based fork tracking (uncle references live in the canonical block header).

### Mining Network Architecture

DarkWow mining operates in three layers. The separation of concerns is absolute:
every computer on the network handshakes via lilith. Pool mining and merge
mining are overlays on top — they add capabilities without replacing the base
P2P layer.

**Layer 1: DarkWow P2P (mandatory)**

Every computer participating in the DarkWow network — whether solo miner, pool
operator, or merge miner — handshakes via lilith. This is non-negotiable. The
lilith handshake provides node discovery, block propagation, and transaction
gossip. dwowd connects to a lilith seed node on startup and participates as a
full peer.

```
┌──────────────┐    lilith     ┌──────────────┐
│   dwowd A    │◄─────────────►│   dwowd B    │
│ (solo miner) │    handshake  │ (pool op)    │
└──────────────┘               └──────┬───────┘
       │                              │
       │ stratum (local)              │ mm_rpc (local)
       ▼                              ▼
  ┌─────────┐                  ┌───────────┐
  │  xmrig  │                  │  p2pool   │
  └─────────┘                  └───────────┘
```

**Layer 2: Pool Mining Overlay (optional)**

A p2pool operator runs a stratum server alongside their dwowd node. Individual
miners connect their xmrig to p2pool's stratum port instead of dwowd's. p2pool
aggregates their hashrate, distributes mining jobs, and pays out rewards via a
PPLNS (Pay Per Last N Shares) scheme over a 2160-block window.

This is how p2pool works on Monero — it's a decentralized pool mining protocol.
On DarkWow, the pool operator's dwowd is the block submitter: from the chain's
perspective, the pool appears as a single miner. All pool participants share the
DRKW reward according to their contributed shares.


For merge mining architecture, protocol, economics, and finality, see the consolidated
[Merge Mining](merge-mining.md) chapter. For reward schedule details and uncle economics,
see below.
---

## Target Adjustment

The Proof-of-Work consensus maintains a **target** — the maximum valid hash value
(`hash_u32 <= target`). Higher target = easier mining (more hashes pass).
Conventional difficulty (higher = harder) is derived as `difficulty = u32::MAX / target`.

Full specification with edge cases at [Consensus / Target Adjustment](consensus/consensus.md#target-adjustment-algorithm).
Summary:

```
avg_interval = sum(last_10_intervals) / (n - 1)
ratio = clamp(target_block_time / avg_interval, 0.5, 2.0)
delta = clamp(ratio - 1.0, -0.10, +0.10)
new_target = clamp(target / (1.0 + delta), min_target, max_target)
```

- **Target block time**: 120 seconds
- **Window**: Rolling average of last 10 block intervals (up to 20 timestamps stored)
- **Delta cap**: ±10% per adjustment step
- **Ratio bound**: [0.5, 2.0] — prevents divergence under extreme hashrate changes
- **Target bounds**: [min_target, max_target] — default [1, u32::MAX]

When blocks arrive too fast (ratio > 1), the target decreases (harder). When blocks
arrive too slow (ratio < 1), the target increases (easier). Division (not multiplication)
provides stable negative feedback — the system converges toward the target block time.

---

## Block Template Generation

Block templates are generated by `generate_linear_block_template()` at
[`bin/dwowd/src/registry/model.rs`](../../bin/dwowd/src/registry/model.rs).

### LinearBlockTemplate Fields

```rust
pub struct LinearBlockTemplate {
    pub previous: [u8; 32],         // Previous block hash
    pub height: u64,                 // Block height
    pub target: u32,                 // PoW target
    pub timestamp: u64,              // Unix seconds (captured once for consistency)
    pub value: u64,                  // Coinbase reward (expected_reward(height))
    pub zk_proof: Vec<u8>,          // ZK proof for privacy-preserving coinbase
    pub zk_public_inputs: [[u8; 32]; 4], // [coin, value_commit_x, value_commit_y, token_commit]
    pub coin: [u8; 32],             // Coin commitment (poseidon hash of attributes)
    pub value_commit_x: [u8; 32],   // Pedersen value commitment x-coordinate
    pub value_commit_y: [u8; 32],   // Pedersen value commitment y-coordinate
    pub token_commit: [u8; 32],     // Poseidon token commitment
    pub encrypted_note: Vec<u8>,    // AEAD encrypted note (coin blinds for recipient)
    pub coin_merkle_root: [u8; 32], // Coin Merkle root including this coinbase
    pub nullifier_root: [u8; 32],   // Nullifier root (all spent nullifiers)
}
```

### Algorithm

1. Compute `height = current_height + 1`
2. Get `previous_hash` from the latest block (or `[0u8; 32]` for genesis)
3. Read current `target` from `PoWConsensus`
4. Compute `reward = expected_reward(height)`
5. Capture `timestamp = now()` (reused for mining blob + verification — must be consistent)
6. If ZK materials are available:
   - Build privacy-preserving coinbase via `build_linear_coinbase()` using the
     `Mint_V1` ZK circuit — creates a ZK proof that the coin was correctly minted
     with Pedersen value commitment, Poseidon token commitment, coin commitment,
     and AEAD encrypted note
   - Compute `coin_merkle_root` including the new coin
   - Compute `nullifier_root` from tracked nullifiers
7. If ZK materials unavailable (development/testing): transparent coinbase fallback
   (all zeroes except height, target, timestamp, value)

### Privacy-Preserving Coinbase

The `build_linear_coinbase()` function creates a ZK coinbase via
`PoWRewardCallBuilder`:

1. Generates an ephemeral block signing keypair
2. Creates a `Mint_V1` ZK proof using the recipient's public key
3. Outputs: ZK proof bytes, 4 public inputs (coin, value_commit_x, value_commit_y,
   token_commit), AEAD encrypted note

The coinbase is privacy-preserving: the recipient, value, and token ID are
hidden behind Pedersen and Poseidon commitments. Only the recipient with the
correct key material can decrypt the note and claim the reward.

### Lazy ZK Initialization

ZK proving materials (`LinearPowRewardZk`) are initialized on the first
template generation request (stratum login or RPC mine call), not at daemon
startup. This avoids blocking startup on proving key construction (~seconds).

Triggered at [`bin/dwowd/src/rpc/stratum.rs`](../../bin/dwowd/src/rpc/stratum.rs)
and [`bin/dwowd/src/rpc/miner.rs`](../../bin/dwowd/src/rpc/miner.rs).

### Template Cache

The daemon caches the current template in `DwowNode.current_linear_template`
(Mutex). On stratum submit, the cached template provides:
- ZK coinbase data (proof, public inputs, coin, commitments)
- Timestamp (must match the mining blob that xmrig hashed)
- Coin/nullifier roots for the reconstructed block header

---

## Mempool Design

The mempool collects transactions with contract calls before they are included
in blocks. Source: [`bin/dwowd/src/mempool.rs`](../../bin/dwowd/src/mempool.rs).

### Data Structure

```rust
pub struct Mempool {
    txs: Mutex<Vec<Transaction>>,
}
```

A simple `Vec<Transaction>` behind `Arc<Mutex>`. No priority queue, no size
limits, no eviction.

### Current Limitations (Intentional for Testnet)

| Limitation | Status | Rationale |
|-----------|--------|-----------|
| No eviction policy | Transactions never evicted | Testnet: mempool is small |
| No size limit | Unbounded `Vec` | Testnet: low transaction volume |
| No fee prioritization | Transactions ordered by insertion (FIFO) | Testnet: fees are minimal |
| No duplicate detection | Only hash-based dedup on insert | Testnet: simple is sufficient |
| No TTL / expiry | Transactions persist until mined | Testnet: no stale-tx problem |

These are **intentional simplifications** for testnet. A production mempool
would add: size caps, fee-based ordering, TTL expiry, replace-by-fee, and
DoS protection.

### API

| Method | Behavior |
|--------|----------|
| `add(tx)` | Push transaction. Rejects if hash already in mempool. |
| `take_all()` | Drain all transactions (used during block construction). |
| `len()` | Current transaction count. |
| `remove(tx_hash)` | Remove specific transaction by hash. |

### Transaction Lifecycle: Two Paths

**Path A — RPC-driven mining (dev / solo):**

```
User ──submit_transaction──► mempool.add(tx)
                                  │
Miner ──mine_linear────────► generate_linear_block_template()
                                  │
                            take_all() → drain mempool into block
                                  │
                            build coinbase (ZK or transparent)
                                  │
                            create block header, mine RandomX nonce
                                  │
                            insert_validated_block()
                                  │
                            broadcast to P2P peers
```

**Path B — Stratum mining (external xmrig):**

```
xmrig ──login──► generate_linear_block_template()
                      │
                cached in current_linear_template
                      │
                push mining.notify to all stratum clients
                      │
xmrig mines RandomX nonce on external hardware
                      │
xmrig ──submit──► verify PoW, reconstruct block
                      │
                insert_validated_block()
                      │
                generate new template, push to all clients
```

**Important**: Stratum does **not** drain the mempool. Stratum templates are
built with a coinbase-only block. Transactions submitted via
`submit_transaction` are only mined through the RPC `mine_linear` path. In the
current testnet architecture, transactions are manual — they're included in
blocks by the RPC mining path, not by stratum miners.

### Transaction Flow from RPC to Block

1. User calls `contract.submit_transaction` JSON-RPC method
2. `DefaultRpcHandler` validates the transaction (ZK proof verification,
   signature checks)
3. Valid transaction is added to mempool via `mempool.add(tx)`
4. When miner calls `miner.mine_linear`:
   - `take_all()` drains all pending transactions from mempool
   - `generate_linear_block_template()` builds a block template
   - Miner hashes the 227-byte blob with RandomX to find a valid nonce
   - Block is assembled with transactions + coinbase
   - `apply_block()` verifies PoW, merkle root, executes WASM contract calls
   - `insert_validated_block()` stores block, adjusts target, records timestamp
   - Block is broadcast to P2P peers via `linear_broadcast`

---

## Mining Flow

1. `dwowd` generates a RandomX key for the next block template
2. Miner receives 227-byte blob (header with zeroed nonce) + target
3. Miner initializes RandomX VM with the key, hashes the blob with different nonces
4. If hash meets target (`hash_u32 <= target`), miner submits solved header to stratum
5. `dwowd` verifies the proof-of-work and assembles the block
6. Coinbase reward = `expected_reward(height)` paid via NativeToken::PoWRewardV1
7. If uncle, partial reward via pin mechanism

Target configuration:

```toml
[network_config."darkwow-testnet".pow]
target_block_time = 120       # seconds
initial_target = 16777215     # 0x00FFFFFF, easy first block (~1/256 hashes)
min_target = 1                # hardest possible
max_target = 4294967295       # u32::MAX, easiest possible
min_block_interval = 10       # seconds between blocks
```

---

## Comparison: DRKW vs BTC vs XMR

| | Bitcoin | Monero | DarkWow |
|---|---------|--------|---------|
| Supply cap | 21M (fixed) | ~18.4M + tail | 21M cap + tail |
| Halving | 4-year step | 4-year step → tail | Continuous exponential → tail |
| Premine | 0 | 0 | 0 |
| PoW | SHA-256 (ASIC) | RandomX (CPU) | RandomX (CPU) |
| Uncle rewards | No (orphaned) | No | Yes (obligated pin, 50%→) |
| Governance risk | Low | Low | None (no governance) |

---

## Coinbase Reward Forwarding

Miners can redirect coinbase rewards to any address — a wallet, DAO, or contract
treasury — without changing the mining keypair. The coinbase is built with the
destination as the recipient, so rewards arrive directly at the target address.

### Design

Rather than minting to the mining address and then spending to the destination in a
separate transaction, the recipient is changed *inside the coinbase itself*. The
`build_linear_coinbase` function takes a `PublicKey` as recipient — normally the
mining address, but optionally any address. This is the tightest possible
implementation: zero extra transactions, zero Merkle tree churn, zero new consensus
rules, zero additional validation.

The cost of adding a separate forwarding transaction (extra signature verification,
extra nullifier tracking, extra Merkle proof, new consensus rules for count/fee check)
would be entirely borne by the network. The direct redirect eliminates all of that
while delivering the same outcome.

### How It Works

A single function, `parse_forward_destination()`, handles address parsing. It takes a
bs58-encoded address string and returns `Option<PublicKey>`. Empty or invalid strings
return `None`, falling back to the mining address.

The function is called from all three mining paths at the point where the coinbase
recipient is determined:

| Path | File | Behavior |
|------|------|----------|
| Built-in miner | [lib.rs](bin/dwowd/src/lib.rs) — `miner_task` | Checks `forward_destination` each block |
| Stratum | [stratum.rs](bin/dwowd/src/rpc/stratum.rs) — template generation | Overrides the login-time recipient config |
| Merge mining | [mm_rpc.rs](bin/dwowd/src/rpc/mm_rpc.rs) — template generation | Same as stratum |

**Zero consensus impact.** The coinbase transaction is structurally identical
regardless of recipient — same Mint_V1 ZK proof, same Pedersen commitment `C_H`,
same block structure. The recipient is encrypted inside the AeadEncryptedNote.
Other nodes cannot distinguish a forwarded coinbase from a normal one. All existing
consensus defenses (Pedersen mass balance, nullifier checks, WASM entrypoint
validation) apply identically with no modification.

### Key Ownership

The **destination address's keypair** is required to spend the forwarded rewards.
The mining keypair's secret is used to build the coinbase proof but **cannot**
decrypt the note or spend the coins. Ensure you control the destination's keypair
before enabling forwarding — coins sent to an address without a known secret are
permanently locked.

### Configuration

```bash
# docker-compose.yml environment or shell
FORWARD_DESTINATION="dV1abc123destaddr..."
```

Set at node startup via env var. Read once during `init_linear()`, stored in
`MiningState.forward_destination`, immutable after startup. If the env var is empty,
unset, or contains an invalid address, the node falls back to mining to the normal
mining address. No runtime API to change it — restart required.

### Current Limitations

- **No network prefix validation.** The node does not verify that the forwarding
  address belongs to the same network (testnet vs mainnet). A mainnet address passed
  to a testnet node would produce unspendable coins. This will be added in a follow-up.

---

## Open Questions

### Absolute Supply Under Tail

The tail means supply is technically unbounded. After 100 years the total is
~37.8M DRKW (still under 2× cap), and the annual rate is 0.56% and falling.
Whether this matters depends on whether you view the tail as a security
mechanism (intent) or an inflation source (side effect).

### Pool Centralization

CPU-friendly PoW doesn't prevent pool formation — miners still join pools for
steady payouts. Stratum centralizes block template creation. This is true of
all PoW chains and RandomX doesn't solve it.

### ASIC Risk

No PoW algorithm has remained ASIC-free indefinitely. RandomX has held since
2019 but there's no guarantee it stays that way.

### Economic Security at Tail

At 0.80 DRKW/block tail rate, the daily security budget is ~576 DRKW/day.
Security depends on DRKW market price — if the tail value drops below the
cost of attack, the chain becomes vulnerable. This is the fundamental tension
of tail emission: it provides a floor but doesn't guarantee it's high enough.

---

## See Also

- [Consensus](consensus/consensus.md) — PoW consensus and block production
- [NativeToken](../contract/native_token.md) — Consensus-first native token contract
- [Slashing & Economic Security](slashing.md) — Relayer and validator economics
- [Architecture Overview](overview.md) — Full system design
