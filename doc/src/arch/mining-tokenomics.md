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
