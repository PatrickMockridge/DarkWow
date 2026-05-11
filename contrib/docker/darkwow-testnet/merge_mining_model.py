#!/usr/bin/env python3
"""
DarkWow Merge Mining Toy Model — 1:1 with Rust consensus code.

Models the competition between merge-mined blocks (PowData::Monero from
p2pool/monerod) and native blocks (PowData::DarkFi from xmrig→stratum).
Both compete identically under the block_rank() formula. Hashpower ratio
determines who wins canonical slots probabilistically.

Key source files this maps to:
  src/validator/utils.rs       — block_rank, best_fork_index, MAX_32_BYTES
  src/validator/pow.rs         — calculate_hash, next_mine_target_and_difficulty
  src/validator/consensus.rs   — Fork, append_proposal, confirmation
  src/validator/uncle.rs       — compute_reward_distribution, BASE_REWARD
  src/blockchain/header_store.rs — PowData enum, Header
  src/sdk/src/blockchain.rs    — expected_reward

Run:
  python3 contrib/docker/darkwow-testnet/merge_mining_model.py
"""

from __future__ import annotations

import math
import random
import sys
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


# ============================================================================
# Constants — exact from Rust source
# ============================================================================

# MAX_32_BYTES: BigUint::from_bytes_le(&[0xFF; 32]) — validator/utils.rs:46
MAX_32_BYTES = int.from_bytes(bytes([0xFF] * 32), "little")  # 2^256 - 1

# Reward constants — sdk/src/blockchain.rs:63-82
INITIAL_REWARD: int = 1_383_764_049
HALF_LIFE_BLOCKS: int = 1_051_920
TAIL_REWARD: int = 79_853_981
GENESIS_REWARD: int = 0

# Uncle Merkle constants — validator/uncle.rs:44-47
MAX_UNCLE_DEPTH: int = 6
BASE_REWARD: int = 1_000_000_000  # 10 DFI assuming 8 decimals

# Difficulty window — validator/pow.rs:59-80
DIFFICULTY_WINDOW: int = 720
RETAINED: int = 600
CUT_BEGIN: int = 60
CUT_END: int = 660

# Default confirmation threshold — validator/consensus.rs
DEFAULT_CONFIRMATION_THRESHOLD: int = 3

# Default max forks
DEFAULT_MAX_FORKS: int = 8


# ============================================================================
# PowData enum — header_store.rs:44-49
# ============================================================================

class PowData(Enum):
    """Maps to PowData enum in header_store.rs:44-49."""
    DARK_FI = 1   # Native DarkWow PoW
    MONERO = 2    # Monero merge mining PoW


# ============================================================================
# Block — maps to Header + BlockInfo
# ============================================================================

@dataclass
class Block:
    """A simplified block matching the relevant Header fields."""
    height: int
    previous_hash: bytes  # 32 bytes
    timestamp: float  # seconds since epoch
    nonce: int
    pow_data: PowData
    hash: bytes  # 32 bytes — simulated RandomX output
    miner_type: str  # "native" or "merge"
    reward_recipient: str = ""

    @property
    def hash_int(self) -> int:
        """Hash as little-endian integer, matching BigUint::from_bytes_le."""
        return int.from_bytes(self.hash, "little")


# ============================================================================
# BlockRank — maps to the tuple returned by block_rank()
# ============================================================================

@dataclass
class BlockRanks:
    """Matches the tuple returned by block_rank() in utils.rs:172-196."""
    difficulty: int
    targets_rank: int   # target_distance^2
    hashes_rank: int    # hash_distance^2


# ============================================================================
# Chain / Fork — maps to Fork struct in consensus.rs:697-714
# ============================================================================

@dataclass
class Fork:
    """Maps to Fork struct in consensus.rs:697-714."""
    blocks: list[Block] = field(default_factory=list)
    cumulative_difficulty: int = 0
    targets_rank: int = 0   # sum of all block target_distance_sq
    hashes_rank: int = 0    # sum of all block hash_distance_sq

    @property
    def length(self) -> int:
        return len(self.blocks)

    def append_block(self, block: Block, rank: BlockRanks) -> None:
        """Maps to Fork::append_proposal in consensus.rs:738-769."""
        self.blocks.append(block)
        self.targets_rank += rank.targets_rank
        self.hashes_rank += rank.hashes_rank
        self.cumulative_difficulty += rank.difficulty


# ============================================================================
# Core functions — exact 1:1 with Rust
# ============================================================================

def expected_reward(height: int) -> int:
    """Exact match for expected_reward() in sdk/src/blockchain.rs:108-119.

    R(h) = max(R0 * 2^(-h/H), R_tail)
    Genesis (height 0) returns 0.
    """
    if height == 0:
        return GENESIS_REWARD

    decay = 2.0 ** (-height / HALF_LIFE_BLOCKS)
    reward = int(INITIAL_REWARD * decay)
    return max(reward, TAIL_REWARD)


def block_rank(block: Block, target: int, difficulty: int) -> BlockRanks:
    """Exact match for block_rank() in validator/utils.rs:172-196.

    Returns (difficulty, target_distance_sq, hash_distance_sq).

    Genesis block has rank (difficulty, 0, 0).
    Both PowData::DarkFi and PowData::Monero use the same formula.
    """
    if block.height == 0:
        return BlockRanks(difficulty=difficulty, targets_rank=0, hashes_rank=0)

    out_hash = block.hash_int

    # target_distance = MAX_32_BYTES - target
    target_distance = MAX_32_BYTES - target
    target_distance_sq = target_distance * target_distance

    # hash_distance = MAX_32_BYTES - out_hash
    hash_distance = MAX_32_BYTES - out_hash
    hash_distance_sq = hash_distance * hash_distance

    return BlockRanks(
        difficulty=difficulty,
        targets_rank=target_distance_sq,
        hashes_rank=hash_distance_sq,
    )


def best_fork_index(forks: list[Fork]) -> Optional[int]:
    """Exact match for best_fork_index() in validator/utils.rs:259-301.

    Best fork = highest targets_rank. Tiebreak = highest hashes_rank.
    Returns None if no forks exist.
    """
    if not forks:
        return None

    # Find best ranked forks by targets_rank
    best = 0
    indexes: list[int] = []
    for i, fork in enumerate(forks):
        rank = fork.targets_rank

        if rank < best:
            continue

        if rank == best:
            indexes.append(i)
            continue

        # rank > best
        best = rank
        indexes = [i]

    # Single best — done
    if len(indexes) == 1:
        return indexes[0]

    # Tiebreak by hashes_rank
    best_idx = indexes[0]
    for idx in indexes[1:]:
        if forks[idx].hashes_rank > forks[best_idx].hashes_rank:
            best_idx = idx

    return best_idx


def worst_fork_index(forks: list[Fork]) -> Optional[int]:
    """Exact match for worst_fork_index() in validator/utils.rs:309-342.

    Worst fork = lowest targets_rank. Tiebreak = lowest hashes_rank.
    """
    if not forks:
        return None

    worst = forks[0].targets_rank
    indexes = [0]
    for i, fork in enumerate(forks[1:], start=1):
        rank = fork.targets_rank

        if rank > worst:
            continue

        if rank == worst:
            indexes.append(i)
            continue

        # rank < worst
        worst = rank
        indexes = [i]

    if len(indexes) == 1:
        return indexes[0]

    # Tiebreak by lower hashes_rank
    worst_idx = indexes[0]
    for idx in indexes[1:]:
        if forks[idx].hashes_rank < forks[worst_idx].hashes_rank:
            worst_idx = idx

    return worst_idx


def compute_reward_distribution(block_reward: int, uncle_count: int) -> tuple[int, list[dict]]:
    """Model the reward distribution for a block with `uncle_count` uncles.

    Args:
        block_reward: The base block reward from expected_reward(height).
                      This is what PoWRewardV1 mints.
        uncle_count: Number of uncle blocks referenced by this block.

    Returns:
        (canonical_reward, list of uncle_shares).

    Canonical miner receives block_reward as the base PoWRewardV1 mint.
    Each uncle receives block_reward / 2^depth as additional issuance.

    The pairing logic from uncle.rs:125-142 is preserved: uncles are
    paired, and for each pair the first share goes to canonical (as
    incentive for including uncle references), the second to the uncle.
    """
    canonical_reward = block_reward
    uncle_shares: list[dict] = []

    for i in range(uncle_count):
        depth = min(((i // 2) + 1), MAX_UNCLE_DEPTH)
        reward = block_reward // (2 ** depth)

        if i % 2 == 0:
            # Canonical miner gets this bonus share for including uncle
            canonical_reward += reward
        else:
            # Uncle miner gets this share
            uncle_shares.append({"depth": depth, "reward": reward})

    return canonical_reward, uncle_shares


# ============================================================================
# Mining simulation
# ============================================================================

def simulate_best_hash(hashpower: float, duration: float) -> bytes:
    """Simulate the best RandomX hash found in a time window.

    A miner with H hashes/second makes N = H*duration attempts in the
    window. Each attempt produces an independent uniform random 256-bit
    hash. The "best" hash is the minimum of N samples.

    We use order statistics: the minimum of N uniform [0, 2^256) samples
    follows CDF: F(x) = 1 - (1 - x/2^256)^N.

    Sampling: u ~ Uniform(0,1), best = (1 - u^(1/N)) * 2^256.

    More hashpower → larger N → smaller expected hash → better rank.
    """
    attempts = max(1, int(hashpower * duration))

    # The minimum of N uniform [0,1) samples: 1 - u^(1/N)
    u = random.random()
    # Use 1 - (1-u)^(1/N) for numerical stability
    min_val = 1.0 - (1.0 - u) ** (1.0 / attempts)

    # Scale to 256-bit integer
    max_val = 2**256 - 1
    hash_int = int(min_val * max_val)
    return hash_int.to_bytes(32, "little")


def compute_target(difficulty: int) -> int:
    """Match next_mine_target() in pow.rs:250-252.

    target = MAX_32_BYTES / difficulty
    """
    return MAX_32_BYTES // difficulty


# ============================================================================
# Simulation engine
# ============================================================================

@dataclass
class SimulationConfig:
    """Configuration for a merge mining simulation run."""
    # Miner hashpowers (hashes/second)
    native_hashpower: float = 1_000.0       # 1 KH/s
    merge_hashpower: float = 1_000_000.0    # 1 MH/s (~1000x more)

    # Block time target (seconds)
    target_block_time: float = 120.0

    # Number of slots to simulate
    num_slots: int = 100

    # Initial difficulty
    initial_difficulty: int = 255

    # Consensus params
    confirmation_threshold: int = DEFAULT_CONFIRMATION_THRESHOLD
    max_forks: int = DEFAULT_MAX_FORKS

    # Uncle Merkle phase: "phase1" = no uncles (losers orphaned),
    #                      "phase2" = uncles get partial rewards
    uncle_phase: str = "phase2"

    # Random seed for reproducibility
    seed: Optional[int] = None


@dataclass
class SlotResult:
    """Result of a single slot competition."""
    slot: int
    height: int = 0
    emission_reward: int = 0  # expected_reward(height) — base block reward
    canonical_block: Optional[Block] = None
    uncle_blocks: list[Block] = field(default_factory=list)
    orphaned_blocks: list[Block] = field(default_factory=list)
    canonical_total: int = 0   # what canonical miner actually receives
    uncle_rewards: list[int] = field(default_factory=list)


@dataclass
class SimulationResult:
    """Full simulation results."""
    config: SimulationConfig
    slots: list[SlotResult] = field(default_factory=list)

    # Cumulative stats
    total_merge_canonical: int = 0
    total_native_canonical: int = 0
    total_merge_uncles: int = 0
    total_native_uncles: int = 0
    total_merge_reward: int = 0
    total_native_reward: int = 0


def run_simulation(config: SimulationConfig) -> SimulationResult:
    """Run a full merge mining simulation over N slots."""
    if config.seed is not None:
        random.seed(config.seed)

    result = SimulationResult(config=config)

    # State
    canonical_chain: list[Block] = []
    difficulty = config.initial_difficulty
    current_time = 0.0
    prev_hash = bytes([0x42] * 32)  # dummy genesis prev hash

    # Genesis block (height 0)
    genesis = Block(
        height=0,
        previous_hash=bytes(32),
        timestamp=current_time,
        nonce=0,
        pow_data=PowData.DARK_FI,
        hash=bytes(32),  # zero hash for genesis
        miner_type="genesis",
        reward_recipient="genesis",
    )
    canonical_chain.append(genesis)
    current_time += config.target_block_time

    for slot in range(1, config.num_slots + 1):
        height = len(canonical_chain)

        # Both miners attempt to find a block in this slot window
        target = compute_target(difficulty)

        native_hash = simulate_best_hash(config.native_hashpower, config.target_block_time)
        merge_hash = simulate_best_hash(config.merge_hashpower, config.target_block_time)

        native_block = Block(
            height=height,
            previous_hash=canonical_chain[-1].hash,
            timestamp=current_time,
            nonce=random.randint(0, 2**32 - 1),
            pow_data=PowData.DARK_FI,
            hash=native_hash,
            miner_type="native",
            reward_recipient="native_wallet",
        )

        merge_block = Block(
            height=height,
            previous_hash=canonical_chain[-1].hash,
            timestamp=current_time,
            nonce=random.randint(0, 2**32 - 1),
            pow_data=PowData.MONERO,
            hash=merge_hash,
            miner_type="merge",
            reward_recipient="merge_wallet",  # from p2pool --merge-mine address
        )

        # Compute ranks for both blocks
        native_rank = block_rank(native_block, target, difficulty)
        merge_rank = block_rank(merge_block, target, difficulty)

        # Both blocks form competing forks (simplified: no real fork chains)
        # Winner = whichever has better rank. Since both have same target,
        # the one with larger hash_distance wins.
        # This is equivalent to: smaller hash = better.
        native_wins = native_block.hash_int < merge_block.hash_int

        slot_result = SlotResult(slot=slot)

        if native_wins:
            canonical = native_block
            loser = merge_block
            slot_result.canonical_block = native_block
            result.total_native_canonical += 1
        else:
            canonical = merge_block
            loser = native_block
            slot_result.canonical_block = merge_block
            result.total_merge_canonical += 1

        # Block reward from emission schedule
        emission_reward = expected_reward(height)
        slot_result.height = height
        slot_result.emission_reward = emission_reward

        # Handle loser based on uncle phase
        if config.uncle_phase == "phase2":
            # Loser becomes uncle, gets partial reward at depth 1
            slot_result.uncle_blocks = [loser]
            depth = 1  # always depth 1 for single uncle competing for same slot
            uncle_reward = emission_reward // (2 ** depth)

            if loser.miner_type == "merge":
                result.total_merge_uncles += 1
                result.total_merge_reward += uncle_reward
            else:
                result.total_native_uncles += 1
                result.total_native_reward += uncle_reward

            slot_result.uncle_rewards = [uncle_reward]
        else:
            # Phase 1: loser is orphaned, gets nothing
            slot_result.orphaned_blocks = [loser]

        # Canonical reward: base emission + inclusion bonus from paired uncles
        canonical_total, uncle_shares = compute_reward_distribution(
            emission_reward,
            1 if config.uncle_phase == "phase2" else 0,
        )
        slot_result.canonical_total = canonical_total

        if canonical.miner_type == "merge":
            result.total_merge_reward += canonical_total
        else:
            result.total_native_reward += canonical_total

        # Append to canonical chain, update time
        canonical_chain.append(canonical)
        current_time += config.target_block_time
        prev_hash = canonical.hash

        # Simple difficulty adjustment: if we're hitting blocks near
        # target_block_time, keep difficulty stable
        # (Full EMA adjustment omitted for toy model simplicity)
        # difficulty stays stable unless we want to model adjustment

        result.slots.append(slot_result)

    return result


# ============================================================================
# Reporting
# ============================================================================

def print_results(result: SimulationResult) -> None:
    """Print simulation results in a readable format."""
    c = result.config
    total_slots = len(result.slots)

    print("=" * 72)
    print("  DarkWow Merge Mining Simulation")
    print("=" * 72)
    print(f"  Slots:             {total_slots}")
    print(f"  Native hashpower:  {c.native_hashpower:,.0f} H/s")
    print(f"  Merge hashpower:   {c.merge_hashpower:,.0f} H/s")
    print(f"  Hashpower ratio:   {c.merge_hashpower / c.native_hashpower:,.0f}:1 (merge:native)")
    print(f"  Target block time: {c.target_block_time}s")
    print(f"  Uncle phase:       {c.uncle_phase}")
    print(f"  Seed:              {c.seed}")
    print()

    # Canonical slot wins
    print("  --- Canonical Slot Wins ---")
    merge_pct = result.total_merge_canonical / total_slots * 100
    native_pct = result.total_native_canonical / total_slots * 100
    print(f"  Merge-mined:  {result.total_merge_canonical:>6}  ({merge_pct:5.1f}%)")
    print(f"  Native-mined: {result.total_native_canonical:>6}  ({native_pct:5.1f}%)")

    if result.total_merge_canonical + result.total_native_canonical > 0:
        ratio = result.total_merge_canonical / max(1, result.total_native_canonical)
        print(f"  Win ratio (merge:native): {ratio:.1f}:1")
    print()

    # Uncle stats (Phase 2 only)
    if c.uncle_phase == "phase2":
        print("  --- Uncle Blocks (Phase 2) ---")
        print(f"  Merge uncles:  {result.total_merge_uncles}")
        print(f"  Native uncles: {result.total_native_uncles}")
        total_uncles = result.total_merge_uncles + result.total_native_uncles
        print(f"  Total uncles:  {total_uncles}")
        print()

    # Reward distribution
    print("  --- Reward Distribution ---")
    total_reward = result.total_merge_reward + result.total_native_reward
    if total_reward > 0:
        merge_reward_pct = result.total_merge_reward / total_reward * 100
        native_reward_pct = result.total_native_reward / total_reward * 100
    else:
        merge_reward_pct = 0
        native_reward_pct = 0
    print(f"  Merge miner:  {result.total_merge_reward:>15,}  ({merge_reward_pct:5.1f}%)")
    print(f"  Native miner: {result.total_native_reward:>15,}  ({native_reward_pct:5.1f}%)")
    print(f"  Total:        {total_reward:>15,}")
    print()

    # Per-slot detail (first 20 and last 5)
    print("  --- Per-Slot Detail (first 20) ---")
    print(f"  {'Slot':>5} {'H':>6} {'Winner':>8} {'Loser':>8} {'Emission':>14} {'CanonTotal':>14} {'UncleRew':>14}")
    for sr in result.slots[:20]:
        winner = sr.canonical_block.miner_type if sr.canonical_block else "?"
        loser = (sr.uncle_blocks[0].miner_type if sr.uncle_blocks
                 else sr.orphaned_blocks[0].miner_type if sr.orphaned_blocks
                 else "-")
        uncle_reward = sr.uncle_rewards[0] if sr.uncle_rewards else 0
        print(f"  {sr.slot:>5} {sr.height:>6} {winner:>8} {loser:>8} {sr.emission_reward:>14,} {sr.canonical_total:>14,} {uncle_reward:>14,}")

    if len(result.slots) > 25:
        print(f"  {'...':>5}")
        for sr in result.slots[-5:]:
            winner = sr.canonical_block.miner_type if sr.canonical_block else "?"
            loser = (sr.uncle_blocks[0].miner_type if sr.uncle_blocks
                     else sr.orphaned_blocks[0].miner_type if sr.orphaned_blocks
                     else "-")
            uncle_reward = sr.uncle_rewards[0] if sr.uncle_rewards else 0
            print(f"  {sr.slot:>5} {sr.height:>6} {winner:>8} {loser:>8} {sr.emission_reward:>14,} {sr.canonical_total:>14,} {uncle_reward:>14,}")

    print()
    print("=" * 72)


# ============================================================================
# Verification tests
# ============================================================================

def run_verification() -> bool:
    """Run verification tests against the Rust reference.

    Returns True if all tests pass.
    """
    failures = 0

    # Test 1: expected_reward against hand-calculated values
    print("--- Test: expected_reward ---")
    # Genesis
    assert expected_reward(0) == 0, f"Genesis reward: {expected_reward(0)}"
    # Initial (height 1)
    r1 = expected_reward(1)
    decay = 2.0 ** (-1.0 / HALF_LIFE_BLOCKS)  # ≈ 0.99999934
    expected_r1 = int(INITIAL_REWARD * decay)
    assert r1 == expected_r1, f"Height 1: got {r1}, expected {expected_r1}"
    # Tail emission kicks in
    r_tail = expected_reward(HALF_LIFE_BLOCKS * 10)  # well past half-life
    assert r_tail == TAIL_REWARD, f"Tail: got {r_tail}, expected {TAIL_REWARD}"
    print(f"  PASS: reward(0)={expected_reward(0)}, reward(1)={r1}, tail={r_tail}")

    # Test 2: block_rank on known values
    print("--- Test: block_rank ---")
    block = Block(
        height=1,
        previous_hash=bytes(32),
        timestamp=1000.0,
        nonce=42,
        pow_data=PowData.DARK_FI,
        hash=int(12345).to_bytes(32, "little"),
        miner_type="test",
    )
    target = MAX_32_BYTES // 255  # difficulty=255
    difficulty = 255
    rank = block_rank(block, target, difficulty)

    expected_target_dist = MAX_32_BYTES - target
    expected_target_dist_sq = expected_target_dist * expected_target_dist
    assert rank.targets_rank == expected_target_dist_sq, \
        f"targets_rank: {rank.targets_rank} != {expected_target_dist_sq}"

    expected_hash_dist = MAX_32_BYTES - 12345
    expected_hash_dist_sq = expected_hash_dist * expected_hash_dist
    assert rank.hashes_rank == expected_hash_dist_sq, \
        f"hashes_rank: {rank.hashes_rank} != {expected_hash_dist_sq}"

    assert rank.difficulty == 255, f"difficulty: {rank.difficulty} != 255"
    print(f"  PASS: rank(hash=12345) = ({rank.difficulty}, {rank.targets_rank}, {rank.hashes_rank})")

    # Test 3: Genesis block_rank
    print("--- Test: genesis block_rank ---")
    genesis_block = Block(
        height=0,
        previous_hash=bytes(32),
        timestamp=0.0,
        nonce=0,
        pow_data=PowData.DARK_FI,
        hash=bytes(32),
        miner_type="genesis",
    )
    gen_rank = block_rank(genesis_block, target, difficulty)
    assert gen_rank.targets_rank == 0, f"Genesis targets_rank: {gen_rank.targets_rank}"
    assert gen_rank.hashes_rank == 0, f"Genesis hashes_rank: {gen_rank.hashes_rank}"
    print(f"  PASS: genesis rank = ({gen_rank.difficulty}, 0, 0)")

    # Test 4: best_fork_index
    print("--- Test: best_fork_index ---")
    forks = [
        Fork(targets_rank=100, hashes_rank=50),
        Fork(targets_rank=200, hashes_rank=30),
        Fork(targets_rank=100, hashes_rank=60),
    ]
    best = best_fork_index(forks)
    assert best == 1, f"best_fork_index: {best} != 1"
    print(f"  PASS: best_fork_index = {best}")

    # Test 5: best_fork_index tiebreak
    print("--- Test: best_fork_index tiebreak ---")
    forks_tie = [
        Fork(targets_rank=100, hashes_rank=50),
        Fork(targets_rank=100, hashes_rank=70),
    ]
    best_tie = best_fork_index(forks_tie)
    assert best_tie == 1, f"best_fork_index tiebreak: {best_tie} != 1"
    print(f"  PASS: best_fork_index (tiebreak) = {best_tie}")

    # Test 6: worst_fork_index — lowest targets_rank, tiebreak by LOWEST hashes_rank
    print("--- Test: worst_fork_index ---")
    worst = worst_fork_index(forks)
    # Fork 0 (100, 50) and Fork 2 (100, 60) tie on targets_rank=100.
    # Tiebreak: lowest hashes_rank wins → Fork 0 (50 < 60).
    assert worst == 0, f"worst_fork_index: {worst} != 0"
    print(f"  PASS: worst_fork_index = {worst}")

    # Test 6b: worst_fork_index with clear loser
    forks_clear_worst = [
        Fork(targets_rank=100, hashes_rank=50),
        Fork(targets_rank=200, hashes_rank=30),
        Fork(targets_rank=50, hashes_rank=99),   # clear worst
    ]
    worst2 = worst_fork_index(forks_clear_worst)
    assert worst2 == 2, f"worst_fork_index clear: {worst2} != 2"
    print(f"  PASS: worst_fork_index (clear loser) = {worst2}")

    # Test 7: compute_reward_distribution
    # Uses expected_reward(1) ≈ 1.384B as the block reward for testing.
    # Uncle shares are fractions of the block reward.
    # Pairing: first uncle in each pair (i%2==0) → canonical bonus,
    #          second uncle (i%2==1) → uncle payout.
    print("--- Test: compute_reward_distribution ---")
    test_reward = expected_reward(1)  # ≈ 1,383,763,137

    # No uncles
    canon, uncles = compute_reward_distribution(test_reward, 0)
    assert canon == test_reward, f"Canonical (0 uncles): {canon} != {test_reward}"
    assert len(uncles) == 0, f"Uncles (0): {len(uncles)}"

    # 1 uncle — canonical gets the bonus (i=0 is even)
    canon, uncles = compute_reward_distribution(test_reward, 1)
    expected_canon_1 = test_reward + test_reward // 2
    assert canon == expected_canon_1, f"Canonical (1 uncle): {canon} != {expected_canon_1}"
    assert len(uncles) == 0, "1 uncle: canonical absorbs bonus, no uncle payout"

    # 2 uncles — canonical gets first bonus, second goes to uncle
    canon, uncles = compute_reward_distribution(test_reward, 2)
    expected_canon_2 = test_reward + test_reward // 2
    expected_uncle_reward = test_reward // 2
    assert canon == expected_canon_2, f"Canonical (2 uncles): {canon} != {expected_canon_2}"
    assert len(uncles) == 1, f"Uncles (2): {len(uncles)} != 1"
    assert uncles[0]["reward"] == expected_uncle_reward, \
        f"Uncle reward: {uncles[0]['reward']} != {expected_uncle_reward}"

    # 3 uncles — canonical: base + depth1 + depth2 bonus, 1 uncle payout
    canon, uncles = compute_reward_distribution(test_reward, 3)
    expected_canon_3 = test_reward + test_reward // 2 + test_reward // 4
    assert canon == expected_canon_3, f"Canonical (3 uncles): {canon} != {expected_canon_3}"
    assert len(uncles) == 1, "3 uncles: only 1 uncle payout"
    assert uncles[0]["reward"] == test_reward // 2

    print(f"  PASS: block_reward={test_reward:,}")
    print(f"  PASS: 0 uncles: canon={canon:,}")
    print(f"  PASS: 1 uncle:  canon={canon:,}, uncle_payouts=0")
    print(f"  PASS: 2 uncles: canon={canon:,}, uncle_reward={uncles[0]['reward']:,}")
    print(f"  PASS: 3 uncles: canon={canon:,}, uncle_reward={uncles[0]['reward']:,}")

    # Test 8: Both PowData variants produce valid block_rank
    print("--- Test: Both PowData variants ---")
    dark_block = Block(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(50000).to_bytes(32, "little"), miner_type="native",
    )
    monero_block = Block(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.MONERO,
        hash=int(50000).to_bytes(32, "little"), miner_type="merge",
    )
    dark_rank = block_rank(dark_block, target, difficulty)
    monero_rank = block_rank(monero_block, target, difficulty)
    assert dark_rank == monero_rank, \
        f"Same hash should give same rank: {dark_rank} != {monero_rank}"
    print(f"  PASS: Same hash → same rank regardless of PowData variant")

    # Test 9: Better hash (smaller) → better rank (larger hash_distance)
    print("--- Test: Better hash wins ---")
    better_block = Block(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(100).to_bytes(32, "little"), miner_type="native",
    )
    worse_block = Block(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(999999).to_bytes(32, "little"), miner_type="native",
    )
    better_rank = block_rank(better_block, target, difficulty)
    worse_rank = block_rank(worse_block, target, difficulty)
    assert better_rank.hashes_rank > worse_rank.hashes_rank, \
        f"Better hash: {better_rank.hashes_rank} <= worse: {worse_rank.hashes_rank}"
    print(f"  PASS: hash=100 rank > hash=999999 rank ({better_rank.hashes_rank} > {worse_rank.hashes_rank})")

    print()
    print("=" * 72)
    print("  All verification tests PASSED")
    print("=" * 72)
    return True


# ============================================================================
# Main
# ============================================================================

def main() -> None:
    """Run verification tests and simulations."""
    # Always run verification first
    if not run_verification():
        print("VERIFICATION FAILED", file=sys.stderr)
        sys.exit(1)

    print()
    print()

    # Simulation 1: Realistic hashpower ratio (~1000:1 merge:native)
    config1 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        seed=42,
    )
    result1 = run_simulation(config1)
    print_results(result1)

    print()
    print()

    # Simulation 2: Equal hashpower (sanity check)
    config2 = SimulationConfig(
        native_hashpower=10_000.0,
        merge_hashpower=10_000.0,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        seed=42,
    )
    result2 = run_simulation(config2)
    print_results(result2)

    print()
    print()

    # Simulation 3: Phase 1 (no uncle rewards) with realistic ratio
    config3 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase1",
        seed=42,
    )
    result3 = run_simulation(config3)
    print_results(result3)


if __name__ == "__main__":
    main()
