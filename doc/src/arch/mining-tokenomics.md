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

**Layer 3: Merge Mining Bridge (optional)**

The same p2pool instance can bridge to Monero. It connects to a local monerod
(via JSON-RPC for block data, ZMQ PUB for new block notifications), embeds
DarkWow aux data (`TX_EXTRA_MERGE_MINING_TAG`) into Monero coinbase transactions,
and when a block is found, submits the solution to both chains.

The `mm_rpc` interface on dwowd (raw TCP JSON-RPC, port 31348) provides:
- `merge_mining_get_chain_id` — identify the DarkWow chain
- `merge_mining_get_aux_block` — get the current block template with aux data
- `merge_mining_submit_solution` — submit a found merge-mined block

```
                    ┌──────────────┐
                    │   monerod    │
                    └──┬───────┬──┘
               RPC / ZMQ     RPC / ZMQ
                       │           │
  ┌─────────┐    stratum  ┌───────────┐    mm_rpc    ┌──────────┐
  │  xmrig  │◄───────────►│  p2pool   │◄────────────►│  dwowd   │
  └─────────┘              └───────────┘              └──────────┘
                                   │
                                   │ lilith handshake
                                   ▼
                          DarkWow P2P Network
```

**The mm_rpc interface** is a local communication channel between co-located
p2pool and dwowd processes. It is NOT a replacement for P2P participation —
p2pool must still handshake via lilith to participate in the DarkWow network.
The mm_rpc exists because merge mining requires tight coordination: p2pool
polls for aux block data every 500ms and needs to submit solutions to dwowd
immediately when blocks are found.

### Merge Mining Competition

Merge mining introduces a second block source: Monero miners (via p2pool) can
produce DarkWow blocks using the same RandomX PoW, with `PowData::Monero` instead
of `PowData::DarkFi`. Both block types compete identically under the `block_rank()`
formula — neither has protocol-level preference.

```
Merge-mined block (PowData::Monero)       Native block (PowData::DarkFi)
         │                                         │
         ▼                                         ▼
    p2pool submits                             xmrig submits
    solution to                                solution to
    dwowd mm_rpc                               dwowd stratum
         │                                         │
         └──────────────┬──────────────────────────┘
                        ▼
              Both enter fork competition
              Identical block_rank() formula
              Winner = better RandomX hash
                        │
              ┌─────────┴──────────┐
              ▼                    ▼
         Canonical slot       Uncle (Phase 2)
         Full emission        emission / 2^depth
         reward               partial reward
```

**Hashpower asymmetry.** Monero's total network hashrate exceeds DarkWow native
hashrate by several orders of magnitude. Merge-mined blocks will therefore win
>99% of canonical slots. Native miners cannot compete for canonical blocks —
their only path to economic viability is through uncle rewards in Phase 2.

**Phase 1 is not viable for merge mining.** With `uncle_merkle_root = 0` (no
uncle rewards), the loser of every slot is orphaned and earns nothing. At a
1000:1 hashpower ratio, the native miner earns zero DRKW — the chain is secured
entirely by Monero hashpower with no economic participation from DarkWow-native
miners. Phase 2 fixes this: native miners who lose canonical slots become
uncles and receive `emission_reward / 2^depth`.

**Reward split at 1000:1 hashpower (Phase 2):**

| Miner | Canonical wins | Uncle rewards | % of total issuance |
|-------|:---:|:---:|:---:|
| Merge miner (Monero via p2pool) | ~100% | 0% | ~75% |
| Native miner (DarkWow via stratum) | ~0% | 100% of uncle slots | ~25% |

The 75/25 split emerges from the uncle reward formula: canonical gets
`emission_reward + emission_reward/2` (the pair bonus), uncle gets
`emission_reward/2`. At equal hashpower, the split converges to ~50/50.

**Two reward streams.** Merge miners receive two separate rewards on two
separate chains:
- **Monero XMR coinbase** → Monero wallet (Ed25519/Curve25519) via p2pool `--wallet`
- **DarkWow DRKW reward** → DarkWow wallet (Pallas curve) via p2pool `--merge-mine` address

The wallets are on different elliptic curves — keys cannot be shared. The
DarkWow reward is delivered via `NativeToken::PoWRewardV1`, a ZK transaction
minting `expected_reward(height)` to the recipient address.

**Model.** A Python toy model matching the Rust consensus code 1:1 is available
at `contrib/docker/darkwow-testnet/merge_mining_model.py`. It simulates the
competition between merge-mined and native blocks across configurable hashpower
ratios, uncle phases, and slot counts.

### Anchoring Finality Gadget

Merge mining introduces a security asymmetry: Monero's hashrate dwarfs DarkWow's.
At a 1000:1 ratio, a dominant merge miner could reorg the chain at will, orphaning
native-mined blocks and stealing their rewards. Anchoring solves this.

**What it is.** A modular finality overlay backed by Monero's cumulative difficulty.
DarkWow blocks can include a reference to a recent Monero block (the "anchor").
Once that Monero block has N confirmations, the DarkWow block is **finalized** —
it cannot be reorganized. All ancestors of a finalized block are also finalized.

**What it is NOT.** Anchoring does not replace PoW fork choice. Blocks still compete
on DarkWow difficulty via `block_rank()`. The anchor is a constraint, not a weight —
forks that conflict with finalized blocks are simply invalid. Within the set of
valid forks, normal `targets_rank`/`hashes_rank` applies.

**Three security benefits:**

1. **Secures mining rewards** — finalized blocks can't be orphaned. All rewards
   in finalized blocks (canonical coinbase, uncle payouts, inclusion bonuses)
   are permanent once the anchor confirms.
2. **Protects against double-spend attacks** — an attacker can't reverse a
   transaction in a finalized block without also reversing Monero's chain.
3. **Protects against re-ordering attacks** — transaction order within finalized
   blocks is immutable.

**How it works.** An attacker trying to reorganize finalized DarkWow blocks would
need to reorganize Monero's chain (the anchor lives on Monero). Monero's cumulative
difficulty is orders of magnitude higher than DarkWow's — the anchor borrows
Monero's security without requiring Monero validators to know about DarkWow.

```
Forks ──► [Finality Filter: drop forks conflicting with finalized blocks]
              │
              ▼
         Valid forks ──► best_fork_index() by targets_rank, hashes_rank
              │
              ▼
         Best fork becomes canonical
```

**Configuration.** Anchoring is modular — enabled/disabled independently of PoW
consensus. The minimum Monero confirmations before finality is a network parameter
(default: 3). This means a DarkWow block is finalized when the Monero block it
anchors to has 3+ confirmations (~6 minutes on Monero).

See `contrib/docker/darkwow-testnet/merge_mining_model.py` for the simulation
implementation, including reorg attack scenarios with and without anchoring.

---

## Difficulty Adjustment

Dynamic adjustment with delta clamping:

```
ratio = clamp( target_block_time / avg_interval, 0.5, 2.0 )
delta = clamp( ratio - 1.0, -0.1, 0.1 )
new_difficulty = difficulty × (1.0 + delta)
```

- **Target**: 120-second block time
- **Window**: Rolling average of recent block intervals
- **Delta cap**: ±10% per adjustment window
- **Ratio bound**: [0.5, 2.0] — prevents divergence under extreme hashrate changes
- **Bounds**: [1, u32::MAX]

Each adjustment is limited to a 10% change from current difficulty in either
direction. The broader [0.5, 2.0] ratio bound ensures the system stays within
sane limits even under extreme conditions, but typical operation never hits it.

---

## Mining Flow

1. `dwowd` generates a RandomX key for the next block template
2. Miner receives 225-byte blob (header with zeroed nonce) + difficulty target
3. Miner initializes RandomX VM with the key, hashes the blob with different nonces
4. If hash meets difficulty target, miner submits solved header to stratum
5. `dwowd` verifies the proof-of-work and assembles the block
6. Coinbase reward = `expected_reward(height)` paid via NativeToken::PoWRewardV1
7. If uncle, partial reward via pin mechanism

RandomX configuration:

```toml
[network_config."darkwow-testnet".pow]
target_block_time = 120       # seconds
initial_difficulty = 255      # starting difficulty
min_difficulty = 1
max_difficulty = 4294967295   # u32 max
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
