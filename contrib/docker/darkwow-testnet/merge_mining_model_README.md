# Three-Chain Merge Mining Toy Model

Python simulation of DarkWow's merge mining consensus across three overlapping
chains. Maps 1:1 to the Rust consensus code in `src/validator/` and models the
real p2pool sidechain behavior observed in source at `p2pool/src/`.

Includes the **Caribina finality layer** (Arweave-anchored) as a third consensus
mode — independent of p2pool and Monero.

## Quick Start

```bash
python3 contrib/docker/darkwow-testnet/merge_mining_model.py
```

No dependencies beyond Python 3.10+ stdlib.

## Three-Chain Architecture

The simulation models three chains that operate concurrently:

| Chain | Block time | Consensus | Fidelity |
|-------|-----------|-----------|----------|
| **Monero (L1)** | ~120s | Cumulative difficulty | Abstraction — only what matters for anchoring |
| **p2pool (sidechain)** | ~10s | Cumulative difficulty + uncle-merkle | Medium — PPLNS, uncle window, difficulty EMA |
| **DarkWow (merge-mined)** | ~120s target | Uncle Merkle + `block_rank()` | High — 1:1 with Rust source |

```
Monero L1  ──►  p2pool sidechain  ──►  DarkWow
  (anchor)       (merge mining data)     (finality + fork choice)
```

## Anchoring Finality Gadgets

DarkWow provides **two independent** finality overlays — neither replaces PoW fork
choice. Both add security constraints: blocks with confirmed anchors cannot be
reorganized.

### Monero Anchoring (`ConsensusMode.ANCHOR`)

Blocks reference Monero blocks as anchors. Once the Monero anchor gets enough
confirmations, the DarkWow block is finalized. Requires p2pool + Monero node.

### Caribina — Arweave Anchoring (`ConsensusMode.CARIBINA`)

Blocks are anchored to Arweave via ArDrive Turbo. Free, no AR tokens, no p2pool,
no Monero node. Protects ALL miners (native and merge). Settlement in ~1 DarkWow
block (~2 min) vs ~3 Monero blocks (~6 min).

### What Finality Is

- A **finality constraint**: once a DarkWow block's anchor gets enough
  confirmations, that block is finalized and all ancestors are also finalized
- **Modular**: enabled/disabled independently of PoW consensus via `ConsensusMode`
  (`NATIVE`, `ANCHOR`, `CARIBINA`)
- **Does NOT replace PoW**: `block_rank()` fork choice still applies within
  unfinalized blocks. The anchor is a constraint filter, not a fork weight.
- **Two independent mechanisms**: Monero (cumulative-difficulty) and Caribina
  (proof-of-storage). A block protected by either is final.

### Three Security Benefits

1. **Secures mining rewards** — finalized blocks can't be orphaned. All rewards
   in finalized blocks (canonical coinbase, uncle payouts, inclusion bonuses)
   are permanent once the anchor confirms.
2. **Protects against double-spend attacks** — reversing a transaction in a
   finalized block requires also reversing Monero's chain (ANCHOR) or forging
   an Arweave timestamp (CARIBINA).
3. **Protects against re-ordering attacks** — transaction order within finalized
   blocks is immutable.

### How It Works

```
Forks ──► [Finality Filter: drop forks conflicting with finalized blocks]
              │
              ▼
         Valid forks ──► best_fork_index() by targets_rank, hashes_rank
              │
              ▼
         Best fork becomes canonical
```

An attacker trying to reorganize finalized DarkWow blocks must reorganize Monero's
chain (the anchor lives on Monero) — or, for Caribina, forge an Arweave timestamp
on a completely different proof-of-storage consensus mechanism. Both are
prohibitively expensive.

## What It Models

| Rust source | Python function | Purpose |
|---|---|---|
| `validator/utils.rs:172` | `block_rank()` | Block ranking: target_distance^2, hash_distance^2 |
| `validator/utils.rs:259` | `best_fork_index()` | Fork resolution by accumulated rank |
| `validator/utils.rs:309` | `worst_fork_index()` | Worst fork by accumulated rank |
| `validator/uncle.rs:125` | `compute_reward_distribution()` | Reward split: canonical vs uncle miners |
| `sdk/src/blockchain.rs:108` | `expected_reward()` | Emission schedule: exponential decay + tail |
| `blockchain/header_store.rs:44` | `PowData` enum | Two block types: DarkFi and Monero |
| `side_chain.cpp:1270` | `p2pool_get_difficulty()` | p2pool EMA difficulty over middle 80% of window |
| `side_chain.cpp:1961` | `p2pool_is_longer_chain()` | p2pool cumulative-difficulty fork choice |
| `side_chain.cpp:2300` | `p2pool_get_shares()` | p2pool PPLNS share distribution |
| *(finality gadget)* | `get_monero_finalized_blocks()` | Find blocks finalized by Monero anchor confirmations |
| *(finality gadget)* | `get_caribina_finalized_blocks()` | Find blocks finalized by Caribina Arweave anchors |
| *(finality gadget)* | `get_finalized_blocks()` | Union of both Monero and Caribina finalized sets |
| *(finality gadget)* | `fork_conflicts_with_finalized()` | Check if a fork would orphan finalized blocks |
| *(finality gadget)* | `get_valid_forks()` | Filter forks to those respecting finality |
| *(reorg simulation)* | `simulate_reorg_attack()` | Simulate attacker building secret fork, testing reorg success |
| *(difficulty)* | `DynamicDifficulty` | EMA difficulty adjustment, ±10% delta, 720-block window |

## Key Constants

```
# DarkWow (from Rust source)
INITIAL_REWARD     = 1,383,764,049  (~13.84 DRKW)
HALF_LIFE_BLOCKS   = 1,051,920      (~4 years at 120s blocks)
TAIL_REWARD        = 79,853,981     (~0.80 DRKW)
BASE_REWARD        = 1,000,000,000  (uncle reward reference)
MAX_32_BYTES       = 2^256 - 1      (for rank calculations)
MAX_UNCLE_DEPTH    = 6
DARKWOW_BLOCK_TIME = 120.0          (seconds)

# p2pool (from /tmp/p2pool/src/)
P2POOL_BLOCK_TIME       = 10.0      (seconds)
P2POOL_UNCLE_BLOCK_DEPTH = 3
P2POOL_UNCLE_PENALTY     = 20       (percent)
P2POOL_MIN_DIFFICULTY    = 100,000
P2POOL_CHAIN_WINDOW_SIZE = 2160     (PPLNS window)

# Anchoring (Monero)
ANCHOR_MIN_CONFIRMATIONS = 3        (Monero confirmations before finality)

# Caribina (Arweave)
CARIBINA_SETTLE_BLOCKS   = 1        (DarkWow blocks before Arweave anchor settles)
```

## Simulation Scenarios

The default run includes nine scenarios:

1. **Native consensus, 1000:1 hashpower, Phase 2** — Merge miner dominates
   canonical slots. Native miner earns ~25% of total issuance through uncle rewards.

2. **Native consensus, 1:1 hashpower, Phase 2** — Sanity check. ~50/50 canonical
   split, ~50/50 reward split.

3. **Native consensus, 1000:1 hashpower, Phase 1** — No uncle rewards. Native
   miner gets zero. Demonstrates why Phase 1 is not viable for merge mining.

4. **Anchoring finality, 1000:1 hashpower, Phase 2** — All rewards in
   finalized blocks (canonical coinbases, uncle payouts, inclusion bonuses)
   are permanent — can't be stolen by reorg.

5. **Native consensus, 3 p2pools, 1000:1 hashpower** — Three p2pools with
   different `--merge-mine` addresses compete, distributing canonical slots
   and DRKW rewards across pool operators.

6. **Anchoring finality, 3 p2pools, 1000:1 hashpower** — Multiple p2pool
   operators + finality + native miner. Even dominant merge miners can't
   steal rewards from finalized blocks.

7. **Reorg attack simulation** — Builds a 6-block chain at equal hashpower
   (native wins some canonical slots). Attacker with 10x hashpower builds a
   secret fork from height 2. Without anchoring: attacker replaces all 5
   blocks, steals ~13.8B in rewards. With anchoring: 4 blocks finalized,
   only 1 replaced, attacker can only steal ~2.1B. Demonstrates exactly
   what anchoring protects and quantifies the damage reduction.

8. **Caribina finality, 1000:1 hashpower** — Caribina provides Arweave-based
   finality for all blocks. Unlike Monero anchoring, Caribina works without
   p2pool — native miners get the same protection as merge miners. All 200
   blocks anchored, settlement in 1 block (~2 min).

9. **Reorg attack — three-way finality comparison** — Same 10x hashpower
   reorg across all three consensus modes:
   - NATIVE: 5 blocks replaced, 0 protected, attacker wins
   - ANCHOR (Monero): 1 replaced, 4 protected (only merge miners)
   - CARIBINA (Arweave): **0 replaced, 5 protected, attacker fork rejected**
   
   Caribina is the only mode where the attacker's fork is completely
   rejected — all blocks protected from the first confirmation.

## Running Custom Simulations

```python
from merge_mining_model import (
    SimulationConfig, ConsensusMode,
    run_simulation, print_results, simulate_reorg_attack,
)

# Native consensus — pure block_rank() competition
config = SimulationConfig(
    native_hashpower=500.0,
    merge_hashpower=5_000_000.0,
    num_p2pools=1,
    consensus_mode=ConsensusMode.NATIVE,
    num_slots=500,
    target_block_time=120.0,
    uncle_phase="phase2",
    seed=123,
)
result = run_simulation(config)
print_results(result)

# Anchoring finality — adds Monero-backed finality
config_anchor = SimulationConfig(
    native_hashpower=500.0,
    merge_hashpower=5_000_000.0,
    num_p2pools=1,
    consensus_mode=ConsensusMode.ANCHOR,
    num_slots=500,
    target_block_time=120.0,
    uncle_phase="phase2",
    anchor_min_confirmations=3,
    seed=123,
)
result_anchor = run_simulation(config_anchor)
print_results(result_anchor)

# Caribina finality — Arweave-anchored via ArDrive Turbo
config_caribina = SimulationConfig(
    native_hashpower=500.0,
    merge_hashpower=5_000_000.0,
    num_p2pools=1,
    consensus_mode=ConsensusMode.CARIBINA,
    num_slots=500,
    target_block_time=120.0,
    uncle_phase="phase2",
    seed=123,
)
result_caribina = run_simulation(config_caribina)
print_results(result_caribina)

# Reorg attack — simulate attacker building a secret fork
reorg = simulate_reorg_attack(
    native_hashpower=500_000.0,
    merge_hashpower=500_000.0,
    chain_length=6,
    reorg_from_height=2,
    consensus_mode=ConsensusMode.ANCHOR,
    attacker_hashpower=5_000_000.0,
    anchor_min_confirmations=2,
    seed=42,
)
print(f"Attacker accepted: {reorg.attacker_fork_accepted}")
print(f"Blocks replaced: {reorg.blocks_replaced}")
print(f"Blocks protected: {reorg.blocks_protected}")

# Reorg attack — Caribina three-way comparison
reorg_caribina = simulate_reorg_attack(
    native_hashpower=500_000.0,
    merge_hashpower=500_000.0,
    chain_length=6,
    reorg_from_height=2,
    consensus_mode=ConsensusMode.CARIBINA,
    attacker_hashpower=5_000_000.0,
    seed=42,
)
print(f"Attacker accepted: {reorg_caribina.attacker_fork_accepted}")
print(f"Blocks replaced: {reorg_caribina.blocks_replaced}")
print(f"Blocks protected: {reorg_caribina.blocks_protected}")
```

## Verification

All 23 verification tests run automatically before any simulation. Tests validate:

| # | Test | What it checks |
|---|------|---------------|
| 1 | `expected_reward()` | Hand-calculated values from Rust emission schedule |
| 2 | `block_rank()` | Known hash/target pairs, rank formula correctness |
| 3 | Genesis `block_rank()` | Genesis block special case (all zeros) |
| 4 | `best_fork_index()` | Targets rank primary sorting |
| 5 | `best_fork_index()` tiebreak | Hashes rank secondary sorting |
| 6 | `worst_fork_index()` | Worst fork by accumulated rank |
| 7 | `compute_reward_distribution()` | Uncle pairing logic (0, 1, 2, 3 uncles) |
| 8 | Both `PowData` variants | DarkFi and Monero produce identical ranks for identical hashes |
| 9 | Better hash wins | Smaller hash → larger hash_distance → better rank |
| 10 | p2pool difficulty | EMA over middle 80% of window, clamped to min |
| 11 | p2pool `is_longer_chain` | Cumulative-difficulty fork choice |
| 12 | p2pool uncle penalty | 20% penalty on uncle blocks |
| 13 | Anchoring finality | Finalized blocks cannot be reorganized |
| 14 | Finality confirmation threshold | Not-enough-confirmations blocks remain unfinalized |
| 15 | Single-block reorg protection | Without anchoring: attacker wins. With: attacker filtered. |
| 16 | Multi-block reorg (native) | Attacker replaces 5 canonical blocks at 10x hashpower |
| 17 | Multi-block reorg (anchored) | Finalized blocks block the reorg, only unfinalized replaced |
| 18 | Reward conservation | Reorg redistributes rewards, destroys inclusion bonuses |
| 19 | Caribina finality | Block finalization with Arweave anchors (settle_blocks=1) |
| 20 | Caribina reorg protection | Finalized blocks cannot be reorganized (Caribina-specific) |
| 21 | Caribina settlement speed | Caribina settles in 1 block (~2 min) vs Monero's 3 blocks (~6 min) |
| 22 | Multi-block reorg (Caribina) | Caribina protects all blocks at 10x attacker hashpower |
| 23 | Caribina protects native miners | Native miners get finality without p2pool or Monero node |

## See Also

- [Mining Tokenomics](../../../doc/src/arch/mining-tokenomics.md#merge-mining-competition)
- [Anchoring Finality Gadget](../../../doc/src/arch/mining-tokenomics.md#anchoring-finality-gadget)
- [Caribina — Arweave-Anchored Finality](../../../doc/src/arch/caribina.md)
- [Merge Mining Guide](../../../doc/src/testnet/merge-mining.md)
- [Uncle Merkle Consensus](../../../doc/src/arch/consensus/uncle_merkle.md)
