# Merge Mining Toy Model

Python simulation of DarkWow's merge mining consensus — maps 1:1 to the Rust
code in `src/validator/`. Used to explore the economics of merge-mined vs
native block competition under different hashpower ratios and uncle phases.

## Quick Start

```bash
python3 contrib/docker/darkwow-testnet/merge_mining_model.py
```

No dependencies beyond Python 3.10+ stdlib.

## What It Models

| Rust source | Python function | Purpose |
|---|---|---|
| `validator/utils.rs:172` | `block_rank()` | Block ranking: target_distance^2, hash_distance^2 |
| `validator/utils.rs:259` | `best_fork_index()` | Fork resolution by accumulated rank |
| `validator/uncle.rs:125` | `compute_reward_distribution()` | Reward split: canonical vs uncle miners |
| `sdk/src/blockchain.rs:108` | `expected_reward()` | Emission schedule: exponential decay + tail |
| `blockchain/header_store.rs:44` | `PowData` enum | Two block types: DarkFi and Monero |

## Simulation Scenarios

The default run includes three simulations:

1. **Realistic hashpower ratio (1000:1 merge:native)** — Phase 2 uncle rewards.
   Merge-mined blocks win ~100% canonical slots; native miners earn ~25% of
   total issuance through uncle rewards.

2. **Equal hashpower (1:1)** — Sanity check. ~50/50 canonical split,
   ~50/50 reward split.

3. **Phase 1 (no uncle rewards)** — 1000:1 hashpower. Merge miner gets 100%
   of rewards; native miner gets zero. Demonstrates why Phase 1 is not viable
   for merge mining economics.

## Key Constants

```
INITIAL_REWARD     = 1,383,764,049  (~13.84 DRKW)
HALF_LIFE_BLOCKS   = 1,051,920      (~4 years at 120s blocks)
TAIL_REWARD        = 79,853,981     (~0.80 DRKW)
MAX_32_BYTES       = 2^256 - 1      (for rank calculations)
MAX_UNCLE_DEPTH    = 6
```

## Running Custom Simulations

```python
from merge_mining_model import SimulationConfig, run_simulation, print_results

config = SimulationConfig(
    native_hashpower=500.0,      # 500 H/s
    merge_hashpower=5_000_000.0, # 5 MH/s (10,000:1 ratio)
    num_slots=500,
    target_block_time=120.0,
    uncle_phase="phase2",
    seed=123,
)
result = run_simulation(config)
print_results(result)
```

## Verification

All verification tests run automatically before any simulation. They validate:

- `expected_reward()` against hand-calculated values from the Rust emission schedule
- `block_rank()` on known hash/target pairs (including genesis special case)
- `best_fork_index()` and `worst_fork_index()` fork resolution
- `compute_reward_distribution()` uncle pairing logic (0, 1, 2, 3 uncles)
- Both PowData variants produce identical ranks for identical hashes
- Better hash (smaller) → better rank (larger hash_distance)

## See Also

- [Mining Tokenomics](../../../doc/src/arch/mining-tokenomics.md#merge-mining-competition)
- [Merge Mining Guide](../../../doc/src/testnet/merge-mining.md)
- [Uncle Merkle Consensus](../../../doc/src/arch/consensus/uncle_merkle.md)
