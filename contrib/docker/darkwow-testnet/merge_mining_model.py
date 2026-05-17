#!/usr/bin/env python3
"""
DarkWow Merge Mining Toy Model — Three-Chain Architecture.

Models the three overlapping chains in merge mining:
  Chain 1: Monero (L1, ~120s blocks, RandomX PoW) — abstracted
  Chain 2: p2pool (sidechain, ~10s blocks, PPLNS, uncle-merkle)
  Chain 3: DarkWow (merge-mined, ~120s target, uncle-merkle consensus)

DarkWow supports three consensus modes:
  - NATIVE:   Pure block_rank() competition (no finality)
  - ANCHOR:   Monero-anchored finality via p2pool merge mining
  - CARIBINA: Arweave-anchored finality via ArDrive Turbo (no p2pool needed)

Key source files this maps to:
  src/validator/utils.rs       — block_rank, best_fork_index, MAX_32_BYTES
  src/validator/pow.rs         — calculate_hash, next_mine_target_and_difficulty
  src/validator/consensus.rs   — Fork, append_proposal, confirmation
  src/validator/uncle.rs       — compute_reward_distribution, BASE_REWARD
  src/blockchain/header_store.rs — PowData enum, Header
  src/sdk/src/blockchain.rs    — expected_reward
  src/linear/src/blockchain.rs — LinearBlockchain
  /tmp/p2pool/src/side_chain.cpp — SideChain::get_difficulty, is_longer_chain,
                                    fill_sidechain_data, get_shares, UNCLE_BLOCK_DEPTH
  /tmp/p2pool/src/merge_mining_client_json_rpc.cpp — merge mining RPC protocol

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
# Constants — DarkWow (exact from Rust source)
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
BASE_REWARD: int = 1_000_000_000

# DarkWow difficulty window — validator/pow.rs:59-80
DIFFICULTY_WINDOW: int = 720
DEFAULT_CONFIRMATION_THRESHOLD: int = 3
DEFAULT_MAX_FORKS: int = 8

# Target block time for DarkWow
DARKWOW_BLOCK_TIME: float = 120.0


# ============================================================================
# Constants — p2pool (from /tmp/p2pool/src/)
# ============================================================================

# side_chain.cpp:54
MONERO_BLOCK_TIME: float = 120.0

# sidechain_config.json:13 — p2pool sidechain target block time
P2POOL_BLOCK_TIME: float = 10.0

# side_chain.cpp:50 — uncle block window (how many heights back uncles can come from)
P2POOL_UNCLE_BLOCK_DEPTH: int = 3

# sidechain_config.json:16 — uncle penalty percentage
P2POOL_UNCLE_PENALTY: int = 20

# side_chain.cpp:49 — minimum sidechain difficulty
P2POOL_MIN_DIFFICULTY: int = 100_000

# sidechain_config.json:15 — PPLNS window size (blocks)
P2POOL_CHAIN_WINDOW_SIZE: int = 2160

# side_chain.cpp:2401 — default chain window
P2POOL_DEFAULT_WINDOW: int = 2160

# stratum_server.cpp:32 — minimum stratum difficulty for miners
P2POOL_STRATUM_MIN_DIFF: int = 1000


# ============================================================================
# Constants — Anchoring
# ============================================================================

# Default difficulty ratio: how much weight one Monero difficulty unit has
# relative to one DarkWow difficulty unit. This is a consensus parameter that
# nodes can propose and agree on.
DEFAULT_DIFFICULTY_RATIO: float = 1.0

# Minimum number of Monero confirmations before a block can be used as anchor
ANCHOR_MIN_CONFIRMATIONS: int = 3

# Caribina (Arweave) anchoring — independent finality layer
# Arweave block time is ~2 minutes, so an anchor settles after ~1 DarkWow block
CARIBINA_SETTLE_BLOCKS: int = 1


# ============================================================================
# PowData enum — header_store.rs:44-49
# ============================================================================

class PowData(Enum):
    DARK_FI = 1   # Native DarkWow PoW
    MONERO = 2    # Monero merge mining PoW


# ============================================================================
# Consensus mode for DarkWow
# ============================================================================

class ConsensusMode(Enum):
    NATIVE = "native"       # Pure block_rank() competition
    ANCHOR = "anchor"       # Anchoring to Monero blocks
    CARIBINA = "caribina"   # Anchoring to Arweave via Caribina


# ============================================================================
# Block types for each chain
# ============================================================================

@dataclass
class MoneroBlock:
    """A simplified Monero L1 block — only what matters for anchoring."""
    height: int
    hash: bytes  # 32 bytes
    previous_hash: bytes
    timestamp: float
    difficulty: int  # Monero network difficulty
    cumulative_difficulty: int  # sum of all difficulty up to this block


@dataclass
class P2poolBlock:
    """A p2pool sidechain block — medium fidelity model."""
    height: int  # sidechain height
    parent_hash: bytes  # 32 bytes — parent sidechain block
    timestamp: float
    nonce: int
    hash: bytes  # 32 bytes — sidechain block ID
    difficulty: int  # this block's difficulty
    cumulative_difficulty: int  # sum of difficulty
    uncles: list[bytes] = field(default_factory=list)  # uncle block hashes
    monero_block_hash: bytes = field(default_factory=lambda: bytes(32))  # associated Monero block
    merge_mining_data: Optional[bytes] = None  # aux chain data (simplified)

    @property
    def is_valid(self) -> bool:
        return self.hash != bytes(32)


@dataclass
class DarkWowBlock:
    """A DarkWow block — high fidelity, maps to Header + BlockInfo."""
    height: int
    previous_hash: bytes  # 32 bytes
    timestamp: float
    nonce: int
    pow_data: PowData
    hash: bytes  # 32 bytes — simulated RandomX output
    miner_type: str  # "native" or "merge"
    reward_recipient: str = ""
    # Anchor fields (Mode B — Monero)
    anchor_monero_height: Optional[int] = None
    anchor_monero_hash: Optional[bytes] = None
    # Caribina fields (Mode C — Arweave)
    caribina_tx_id: Optional[bytes] = None  # 32-byte Arweave TX ID
    caribina_confirmed: bool = False  # True once Arweave block settles

    @property
    def hash_int(self) -> int:
        return int.from_bytes(self.hash, "little")

    @property
    def has_anchor(self) -> bool:
        return self.anchor_monero_height is not None

    @property
    def has_caribina(self) -> bool:
        return self.caribina_tx_id is not None


@dataclass
class BlockRanks:
    """Matches the tuple returned by block_rank() in utils.rs:172-196."""
    difficulty: int
    targets_rank: int
    hashes_rank: int


# ============================================================================
# Fork — maps to Fork struct in consensus.rs:697-714
# ============================================================================

@dataclass
class Fork:
    blocks: list[DarkWowBlock] = field(default_factory=list)
    cumulative_difficulty: int = 0
    targets_rank: int = 0
    hashes_rank: int = 0

    @property
    def length(self) -> int:
        return len(self.blocks)

    @property
    def tip(self) -> Optional[DarkWowBlock]:
        return self.blocks[-1] if self.blocks else None

    def append_block(self, block: DarkWowBlock, rank: BlockRanks) -> None:
        self.blocks.append(block)
        self.targets_rank += rank.targets_rank
        self.hashes_rank += rank.hashes_rank
        self.cumulative_difficulty += rank.difficulty


# ============================================================================
# P2pool share for PPLNS
# ============================================================================

@dataclass
class P2poolShare:
    """A miner's share in the PPLNS window."""
    wallet: str
    weight: int  # difficulty contributed
    block_hash: bytes  # which block the share came from
    is_uncle: bool = False


# ============================================================================
# Dynamic Difficulty Adjustment — 1:1 with validator/pow.rs:59-80
# ============================================================================

@dataclass
class DynamicDifficulty:
    """EMA-based difficulty adjustment matching Rust validator/pow.rs.

    ratio = clamp(target_block_time / avg_interval, 0.5, 2.0)
    delta = clamp(ratio - 1.0, -0.1, 0.1)
    new_difficulty = difficulty * (1.0 + delta)
    """
    difficulty: int
    target_block_time: float = 120.0
    window_size: int = 720
    intervals: list[float] = field(default_factory=list)
    min_difficulty: int = 1
    max_difficulty: int = 4_294_967_295  # u32::MAX

    def record_block(self, interval: float) -> None:
        self.intervals.append(interval)
        if len(self.intervals) > self.window_size:
            self.intervals.pop(0)

    def next_difficulty(self) -> int:
        if not self.intervals:
            return self.difficulty
        avg = sum(self.intervals) / len(self.intervals)
        ratio = max(0.5, min(2.0, self.target_block_time / avg))
        delta = max(-0.1, min(0.1, ratio - 1.0))
        self.difficulty = int(self.difficulty * (1.0 + delta))
        self.difficulty = max(self.min_difficulty, min(self.max_difficulty, self.difficulty))
        return self.difficulty


# ============================================================================
# Core DarkWow functions — exact 1:1 with Rust
# ============================================================================

def expected_reward(height: int) -> int:
    """Exact match for expected_reward() in sdk/src/blockchain.rs:108-119."""
    if height == 0:
        return GENESIS_REWARD
    decay = 2.0 ** (-height / HALF_LIFE_BLOCKS)
    reward = int(INITIAL_REWARD * decay)
    return max(reward, TAIL_REWARD)


def block_rank(block: DarkWowBlock, target: int, difficulty: int) -> BlockRanks:
    """Exact match for block_rank() in validator/utils.rs:172-196."""
    if block.height == 0:
        return BlockRanks(difficulty=difficulty, targets_rank=0, hashes_rank=0)

    out_hash = block.hash_int

    target_distance = MAX_32_BYTES - target
    target_distance_sq = target_distance * target_distance

    hash_distance = MAX_32_BYTES - out_hash
    hash_distance_sq = hash_distance * hash_distance

    return BlockRanks(
        difficulty=difficulty,
        targets_rank=target_distance_sq,
        hashes_rank=hash_distance_sq,
    )


def best_fork_index(forks: list[Fork]) -> Optional[int]:
    """Exact match for best_fork_index() in validator/utils.rs:259-301."""
    if not forks:
        return None

    best = 0
    indexes: list[int] = []
    for i, fork in enumerate(forks):
        rank = fork.targets_rank
        if rank < best:
            continue
        if rank == best:
            indexes.append(i)
            continue
        best = rank
        indexes = [i]

    if len(indexes) == 1:
        return indexes[0]

    best_idx = indexes[0]
    for idx in indexes[1:]:
        if forks[idx].hashes_rank > forks[best_idx].hashes_rank:
            best_idx = idx
    return best_idx


def worst_fork_index(forks: list[Fork]) -> Optional[int]:
    """Exact match for worst_fork_index() in validator/utils.rs:309-342."""
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
        worst = rank
        indexes = [i]

    if len(indexes) == 1:
        return indexes[0]

    worst_idx = indexes[0]
    for idx in indexes[1:]:
        if forks[idx].hashes_rank < forks[worst_idx].hashes_rank:
            worst_idx = idx
    return worst_idx


def compute_reward_distribution(block_reward: int, uncle_count: int) -> tuple[int, list[dict]]:
    """Model the reward distribution for a block with `uncle_count` uncles.

    Uses `block_reward` (= expected_reward(height)) as the base, fixing the
    discrepancy where the Rust code hardcodes BASE_REWARD instead.
    """
    canonical_reward = block_reward
    uncle_shares: list[dict] = []

    for i in range(uncle_count):
        depth = min(((i // 2) + 1), MAX_UNCLE_DEPTH)
        reward = block_reward // (2 ** depth)

        if i % 2 == 0:
            canonical_reward += reward
        else:
            uncle_shares.append({"depth": depth, "reward": reward})

    return canonical_reward, uncle_shares


def compute_target(difficulty: int) -> int:
    """Match next_mine_target() in pow.rs:250-252."""
    return MAX_32_BYTES // difficulty


# ============================================================================
# p2pool Sidechain Functions — model of /tmp/p2pool/src/side_chain.cpp
# ============================================================================

def p2pool_get_difficulty(
    difficulty_data: list[tuple[float, int]],
    target_block_time: float = P2POOL_BLOCK_TIME,
    min_difficulty: int = P2POOL_MIN_DIFFICULTY,
) -> int:
    """Model of SideChain::get_difficulty() — side_chain.cpp:1270-1364.

    Uses the middle 80% of timestamps in the window, discarding 10% oldest
    and 10% newest, then computes difficulty = (diff_range * target_time) / time_delta.
    """
    if len(difficulty_data) < 10:
        return min_difficulty

    size = len(difficulty_data)
    cut_size = max(1, size // 10)

    # Sort by timestamp
    sorted_data = sorted(difficulty_data, key=lambda x: x[0])

    # Discard 10% oldest and 10% newest
    middle = sorted_data[cut_size:size - cut_size]

    if len(middle) < 2:
        return min_difficulty

    t1 = middle[0][0]
    t2 = middle[-1][0]
    d1 = middle[0][1]
    d2 = middle[-1][1]

    delta_t = max(t2 - t1, 1.0)
    diff_range = d2 - d1

    if diff_range <= 0:
        return min_difficulty

    # Clamp delta_t to prevent timestamp manipulation
    # side_chain.cpp:1341: delta_t = max(delta_t, delta_index)
    delta_index = len(middle) * target_block_time
    delta_t = max(delta_t, delta_index)

    new_diff = int(diff_range * target_block_time / delta_t)
    return max(new_diff, min_difficulty)


def p2pool_is_longer_chain(
    current_cumulative_diff: int,
    candidate_cumulative_diff: int,
    current_height: int = 0,
    candidate_height: int = 0,
) -> bool:
    """Model of SideChain::is_longer_chain() — side_chain.cpp:1961-2102.

    For same-chain comparisons (common ancestor): simply compare cumulative difficulty.
    For alternative chains: compare over the window.
    """
    del current_height, candidate_height  # reserved for alternative-chain logic
    return candidate_cumulative_diff > current_cumulative_diff


def p2pool_get_shares(
    blocks: list[P2poolBlock],
    window_size: int = P2POOL_CHAIN_WINDOW_SIZE,
    uncle_penalty: int = P2POOL_UNCLE_PENALTY,
) -> list[P2poolShare]:
    """Model of SideChain::get_shares() — side_chain.cpp:356-486.

    Walk backward from tip collecting shares, applying uncle penalty (20%).
    Uncle weight = difficulty * (100 - penalty) / 100.
    Penalty goes to the block that included the uncle.
    """
    shares: list[P2poolShare] = []
    total_weight = 0
    max_weight = 2 * 100_000  # simplified: 2x min difficulty cap

    # Walk newest to oldest
    for block in reversed(blocks[-window_size:]):
        if not block.is_valid:
            continue

        # Main block share
        block_weight = block.difficulty
        shares.append(P2poolShare(
            wallet=f"miner_{block.nonce % 1000}",
            weight=block_weight,
            block_hash=block.hash,
            is_uncle=False,
        ))
        total_weight += block_weight

        # Uncle shares (penalized)
        for _uncle_hash in block.uncles:
            uncle_weight = block.difficulty * (100 - uncle_penalty) // 100
            penalty = block.difficulty - uncle_weight
            shares.append(P2poolShare(
                wallet=f"uncle_miner_{random.randint(0, 999)}",
                weight=uncle_weight,
                block_hash=block.hash,
                is_uncle=True,
            ))
            total_weight += uncle_weight
            # Penalty added to including block's weight
            total_weight += penalty

        if total_weight >= max_weight:
            break

    return shares


# ============================================================================
# Anchoring Finality Gadget — modular security overlay
# ============================================================================
# Finality does NOT modify fork choice. It adds a finality constraint:
# once a DarkWow block's anchor gets enough confirmations, that block is
# finalized and cannot be reorganized. This protects against 51% attacks.
#
# Two independent finality mechanisms:
#   ANCHOR  — Monero cumulative-difficulty via p2pool merge mining
#   CARIBINA — Arweave proof-of-storage via ArDrive Turbo (no p2pool needed)
#
# Normal fork choice (best_fork_index by targets_rank/hashes_rank) still
# applies — but only among forks that respect finalized blocks.


def get_monero_finalized_blocks(
    canonical_chain: list[DarkWowBlock],
    monero_chain: dict[int, MoneroBlock],
    current_monero_height: int,
    min_confirmations: int = ANCHOR_MIN_CONFIRMATIONS,
) -> set[bytes]:
    """Find blocks finalized by Monero anchors.

    A block is finalized if its Monero anchor has `min_confirmations`
    confirmations. All ancestors of a finalized block are also finalized.
    """
    finalized: set[bytes] = set()
    finalized_ancestors: set[bytes] = set()

    for block in canonical_chain:
        if block.has_anchor and block.anchor_monero_hash is not None:
            for mblock in monero_chain.values():
                if mblock.hash == block.anchor_monero_hash:
                    confirmations = current_monero_height - mblock.height
                    if confirmations >= min_confirmations:
                        finalized.add(block.hash)
                        cursor = block.previous_hash
                        finalized_ancestors.add(cursor)
                    break

    return finalized | finalized_ancestors


def get_caribina_finalized_blocks(
    canonical_chain: list[DarkWowBlock],
    current_height: int,
    settle_blocks: int = CARIBINA_SETTLE_BLOCKS,
) -> set[bytes]:
    """Find blocks finalized by Caribina (Arweave) anchors.

    A block is finalized if it has a Caribina anchor and the Arweave
    block containing it has settled (current_height - block_height >=
    settle_blocks). Caribina settlement is much faster than Monero —
    typically 1 DarkWow block (~2 min) vs 3 Monero blocks (~6 min).

    All ancestors of a finalized block are also finalized.
    """
    finalized: set[bytes] = set()
    finalized_ancestors: set[bytes] = set()

    for block in canonical_chain:
        if block.has_caribina and block.caribina_tx_id is not None:
            confirmations = current_height - block.height
            if confirmations >= settle_blocks:
                finalized.add(block.hash)
                cursor = block.previous_hash
                finalized_ancestors.add(cursor)

    return finalized | finalized_ancestors


def get_finalized_blocks(
    canonical_chain: list[DarkWowBlock],
    monero_chain: dict[int, MoneroBlock],
    current_monero_height: int,
    min_confirmations: int = ANCHOR_MIN_CONFIRMATIONS,
    # Caribina params
    caribina_enabled: bool = False,
    current_darkwow_height: int = 0,
    caribina_settle_blocks: int = CARIBINA_SETTLE_BLOCKS,
) -> set[bytes]:
    """Find all finalized blocks from both finality mechanisms.

    Returns the union of Monero-anchored and Caribina-anchored finalized sets.
    """
    finalized = get_monero_finalized_blocks(
        canonical_chain, monero_chain, current_monero_height, min_confirmations,
    )
    if caribina_enabled:
        finalized |= get_caribina_finalized_blocks(
            canonical_chain, current_darkwow_height, caribina_settle_blocks,
        )
    return finalized


def fork_conflicts_with_finalized(
    fork: Fork,
    finalized_set: set[bytes],
    canonical_chain: list[DarkWowBlock],
) -> bool:
    """Check if a fork would orphan any finalized blocks.

    A fork conflicts if it replaces a finalized block at the same height
    with a different block, or if it replaces an ancestor of a finalized
    block (which would transitively orphan the finalized block).

    For single-block forks (our model), this means:
    - If the fork block is at height H, and canonical[H] is finalized
      and the fork block has a different hash → conflict.
    - If the fork block chains from a different parent than canonical[H-1]
      and canonical[H-1] is finalized → conflict (replaces ancestor).
    """
    if not fork.blocks or not finalized_set:
        return False

    fork_block = fork.blocks[0]
    fork_height = fork_block.height

    # Build a height-to-hash map from the canonical chain
    canonical_by_height: dict[int, bytes] = {}
    for b in canonical_chain:
        canonical_by_height[b.height] = b.hash

    # Check: is the fork replacing a finalized block at its height?
    if fork_height in canonical_by_height:
        canonical_hash_at_height = canonical_by_height[fork_height]
        if canonical_hash_at_height in finalized_set:
            if fork_block.hash != canonical_hash_at_height:
                return True  # directly replacing a finalized block

    # Check: is the fork's parent a finalized block?
    # If the fork chains from a different parent than the canonical block
    # at fork_height, and that canonical block is finalized, it's a conflict.
    parent_height = fork_height - 1
    if parent_height >= 0 and parent_height in canonical_by_height:
        canonical_parent_hash = canonical_by_height[parent_height]
        if canonical_parent_hash in finalized_set:
            if fork_block.previous_hash != canonical_parent_hash:
                return True  # would orphan the finalized parent

    return False


def get_valid_forks(
    forks: list[Fork],
    finalized_set: set[bytes],
    canonical_chain: list[DarkWowBlock],
) -> list[Fork]:
    """Filter forks to only those that respect finality.

    Returns only forks that do not conflict with finalized blocks.
    If no forks are valid AND there are finalized blocks, returns empty list
    (chain stalls rather than accepting an invalid fork).
    If there are no finalized blocks yet (early chain), returns all forks.
    """
    valid = [
        f for f in forks
        if not fork_conflicts_with_finalized(f, finalized_set, canonical_chain)
    ]
    if valid:
        return valid
    # Only fall back to all forks if nothing has been finalized yet
    if not finalized_set:
        return forks
    return []  # chain stalled: all forks conflict with finality


# ============================================================================
# Mining simulation helpers
# ============================================================================

def simulate_best_hash(hashpower: float, duration: float) -> bytes:
    """Simulate the best RandomX hash found in a time window.

    Uses order statistics: the minimum of N uniform [0, 2^256) samples.
    More hashpower -> larger N -> smaller expected hash -> better rank.
    """
    attempts = max(1, int(hashpower * duration))
    u = random.random()
    min_val = 1.0 - (1.0 - u) ** (1.0 / attempts)
    max_val = 2**256 - 1
    hash_int = int(min_val * max_val)
    return hash_int.to_bytes(32, "little")


# ============================================================================
# Chain State Containers
# ============================================================================

@dataclass
class MoneroChainState:
    """Tracks the Monero L1 chain for anchoring purposes."""
    blocks: dict[int, MoneroBlock] = field(default_factory=dict)
    current_height: int = 0
    current_cumulative_difficulty: int = 0
    base_difficulty: int = 20000  # offline mode fixed difficulty

    def produce_block(self, timestamp: float) -> MoneroBlock:
        """Produce a new Monero block (simplified — always succeeds)."""
        self.current_height += 1
        prev_hash = self.blocks[self.current_height - 1].hash if self.current_height > 1 else bytes(32)
        self.current_cumulative_difficulty += self.base_difficulty

        block = MoneroBlock(
            height=self.current_height,
            hash=simulate_best_hash(1e12, MONERO_BLOCK_TIME),  # huge hashpower = always find
            previous_hash=prev_hash,
            timestamp=timestamp,
            difficulty=self.base_difficulty,
            cumulative_difficulty=self.current_cumulative_difficulty,
        )
        self.blocks[self.current_height] = block
        return block

    def get_anchor_candidates(self) -> list[MoneroBlock]:
        """Return Monero blocks that can be used as anchors (with min confirmations)."""
        if self.current_height <= ANCHOR_MIN_CONFIRMATIONS:
            return []
        max_height = self.current_height - ANCHOR_MIN_CONFIRMATIONS
        return [b for h, b in self.blocks.items() if h <= max_height]

    def get_best_anchor(self) -> Optional[MoneroBlock]:
        """Return the highest-difficulty anchor candidate."""
        candidates = self.get_anchor_candidates()
        if not candidates:
            return None
        return max(candidates, key=lambda b: b.cumulative_difficulty)


@dataclass
class P2poolChainState:
    """Tracks the p2pool sidechain."""
    blocks: list[P2poolBlock] = field(default_factory=list)
    difficulty: int = P2POOL_MIN_DIFFICULTY
    cumulative_difficulty: int = 0
    height: int = 0
    difficulty_data: list[tuple[float, int]] = field(default_factory=list)

    def produce_block(
        self,
        timestamp: float,
        hashpower: float,
        monero_block: MoneroBlock,
        merge_mining_data: Optional[bytes] = None,
    ) -> Optional[P2poolBlock]:
        """Try to produce a p2pool sidechain block.

        Returns a block if hashpower suffices, or None if no block found.
        In practice with sufficient hashpower, p2pool always finds blocks.
        """
        # p2pool miners always find blocks (simplified: huge hashpower pool)
        self.height += 1
        prev_hash = self.blocks[-1].hash if self.blocks else bytes(32)

        # Collect uncles from up to 3 heights back
        uncles: list[bytes] = []
        uncle_start = max(0, len(self.blocks) - P2POOL_UNCLE_BLOCK_DEPTH)
        for old_block in self.blocks[uncle_start:]:
            if old_block.hash != prev_hash and old_block.is_valid:
                uncles.append(old_block.hash)

        block_hash = simulate_best_hash(hashpower, P2POOL_BLOCK_TIME)

        self.cumulative_difficulty += self.difficulty

        block = P2poolBlock(
            height=self.height,
            parent_hash=prev_hash,
            timestamp=timestamp,
            nonce=random.randint(0, 2**32 - 1),
            hash=block_hash,
            difficulty=self.difficulty,
            cumulative_difficulty=self.cumulative_difficulty,
            uncles=uncles[:P2POOL_UNCLE_BLOCK_DEPTH],
            monero_block_hash=monero_block.hash,
            merge_mining_data=merge_mining_data,
        )
        self.blocks.append(block)

        # Track difficulty data
        self.difficulty_data.append((timestamp, self.cumulative_difficulty))
        if len(self.difficulty_data) > P2POOL_CHAIN_WINDOW_SIZE:
            self.difficulty_data = self.difficulty_data[-P2POOL_CHAIN_WINDOW_SIZE:]

        # Recalculate difficulty
        self.difficulty = p2pool_get_difficulty(self.difficulty_data)

        return block

    def get_tip(self) -> Optional[P2poolBlock]:
        return self.blocks[-1] if self.blocks else None


# ============================================================================
# Simulation Configuration
# ============================================================================

@dataclass
class SimulationConfig:
    """Configuration for a merge mining simulation run."""
    # Miner hashpowers (hashes/second)
    native_hashpower: float = 1_000.0
    merge_hashpower: float = 1_000_000.0  # aggregated Monero hashpower via p2pool

    # Number of p2pools doing merge mining (each with different --merge-mine address)
    num_p2pools: int = 1

    # DarkWow consensus mode
    consensus_mode: ConsensusMode = ConsensusMode.NATIVE

    # Block time targets
    target_block_time: float = 120.0
    p2pool_block_time: float = 10.0

    # Number of DarkWow slots to simulate
    num_slots: int = 100

    # Initial difficulties
    initial_difficulty: int = 255
    monero_base_difficulty: int = 20000
    p2pool_initial_difficulty: int = P2POOL_MIN_DIFFICULTY

    # Consensus params
    confirmation_threshold: int = DEFAULT_CONFIRMATION_THRESHOLD
    max_forks: int = DEFAULT_MAX_FORKS

    # Uncle Merkle phase
    uncle_phase: str = "phase2"

    # Anchoring params (Monero)
    difficulty_ratio: float = DEFAULT_DIFFICULTY_RATIO
    anchor_min_confirmations: int = ANCHOR_MIN_CONFIRMATIONS

    # Caribina params (Arweave)
    caribina_enabled: bool = False
    caribina_settle_blocks: int = CARIBINA_SETTLE_BLOCKS

    # Dynamic difficulty adjustment
    adjust_difficulty: bool = False

    # Random seed
    seed: Optional[int] = None


# ============================================================================
# Simulation Results
# ============================================================================

@dataclass
class SlotResult:
    """Result of a single DarkWow slot competition."""
    slot: int
    height: int = 0
    monero_height: int = 0
    p2pool_height: int = 0
    emission_reward: int = 0
    canonical_block: Optional[DarkWowBlock] = None
    uncle_blocks: list[DarkWowBlock] = field(default_factory=list)
    orphaned_blocks: list[DarkWowBlock] = field(default_factory=list)
    canonical_total: int = 0
    uncle_rewards: list[int] = field(default_factory=list)
    anchor_used: bool = False
    caribina_used: bool = False
    # Reward tracking by recipient type
    merge_reward_this_slot: int = 0
    native_reward_this_slot: int = 0


@dataclass
class SimulationResult:
    """Full simulation results."""
    config: SimulationConfig
    slots: list[SlotResult] = field(default_factory=list)

    # DarkWow cumulative stats
    total_merge_canonical: int = 0
    total_native_canonical: int = 0
    total_merge_uncles: int = 0
    total_native_uncles: int = 0
    total_merge_reward: int = 0
    total_native_reward: int = 0
    total_anchored_blocks: int = 0
    total_caribina_blocks: int = 0

    # Monero stats
    monero_blocks_produced: int = 0
    monero_total_difficulty: int = 0

    # p2pool stats
    p2pool_blocks_produced: int = 0
    p2pool_merge_blocks: int = 0  # blocks with DarkWow merge-mining data

    # Monero chain state (for post-simulation finality checks)
    monero_blocks: dict[int, MoneroBlock] = field(default_factory=dict)
    monero_current_height: int = 0


@dataclass
class ReorgResult:
    """Result of a reorg attack simulation."""
    # Configuration
    chain_length: int = 0
    reorg_from_height: int = 0
    consensus_mode: str = ""

    # Original chain before reorg
    original_chain: list[DarkWowBlock] = field(default_factory=list)
    original_rewards: dict[str, int] = field(default_factory=dict)  # miner_type -> total

    # Attacker fork
    attacker_fork_blocks: list[DarkWowBlock] = field(default_factory=list)
    attacker_fork_accepted: bool = False

    # Blocks protected by finality
    finalized_blocks: set[bytes] = field(default_factory=set)
    caribina_finalized: set[bytes] = field(default_factory=set)
    blocks_protected: list[int] = field(default_factory=list)  # heights protected
    blocks_replaced: list[int] = field(default_factory=list)   # heights replaced

    # Reward redistribution
    rewards_lost: dict[str, int] = field(default_factory=dict)   # miner_type -> amount
    rewards_gained: dict[str, int] = field(default_factory=dict)  # miner_type -> amount
    net_redistribution: dict[str, int] = field(default_factory=dict)  # net change per miner

    # Issuance check
    total_issuance_before: int = 0
    total_issuance_after: int = 0


# ============================================================================
# Simulation Engine
# ============================================================================

def run_simulation(config: SimulationConfig) -> SimulationResult:
    """Run a full three-chain merge mining simulation."""
    if config.seed is not None:
        random.seed(config.seed)

    result = SimulationResult(config=config)

    # ---- Initialize chains ----
    monero = MoneroChainState(base_difficulty=config.monero_base_difficulty)
    # Create genesis Monero block
    genesis_monero = MoneroBlock(
        height=0, hash=bytes(32), previous_hash=bytes(32),
        timestamp=0.0, difficulty=0, cumulative_difficulty=0,
    )
    monero.blocks[0] = genesis_monero

    # Each p2pool gets its own sidechain (different --merge-mine addresses)
    p2pools: list[P2poolChainState] = []
    for i in range(config.num_p2pools):
        p2pool = P2poolChainState(difficulty=config.p2pool_initial_difficulty)
        p2pools.append(p2pool)

    # DarkWow canonical chain
    canonical_chain: list[DarkWowBlock] = []
    difficulty = config.initial_difficulty
    diff_adjuster = DynamicDifficulty(
        difficulty=difficulty,
        target_block_time=config.target_block_time,
    ) if config.adjust_difficulty else None
    current_time = 0.0

    # DarkWow reward wallets
    p2pool_wallets = [f"merge_pool_{i}_wallet" for i in range(config.num_p2pools)]

    # Genesis block (height 0)
    genesis = DarkWowBlock(
        height=0, previous_hash=bytes(32), timestamp=current_time,
        nonce=0, pow_data=PowData.DARK_FI, hash=bytes(32),
        miner_type="genesis", reward_recipient="genesis",
    )
    canonical_chain.append(genesis)
    current_time += config.target_block_time

    # ---- Main simulation loop ----
    for slot in range(1, config.num_slots + 1):
        height = len(canonical_chain)
        slot_result = SlotResult(slot=slot, height=height)
        slot_start_time = current_time

        # ---- Step 1: Advance Monero chain ----
        # Monero produces blocks at ~120s intervals
        monero_blocks_this_slot: list[MoneroBlock] = []
        while current_time - slot_start_time < config.target_block_time:
            # Monero finds a block every ~120s
            if current_time - (monero.blocks[monero.current_height].timestamp if monero.current_height > 0 else 0) >= MONERO_BLOCK_TIME:
                mblock = monero.produce_block(current_time)
                monero_blocks_this_slot.append(mblock)
                result.monero_blocks_produced += 1
                result.monero_total_difficulty += mblock.difficulty
            current_time += config.p2pool_block_time

        slot_result.monero_height = monero.current_height
        current_time = slot_start_time + config.target_block_time  # align to slot boundary

        # ---- Step 2: Advance p2pool sidechains ----
        for pi, p2pool in enumerate(p2pools):
            # p2pool produces blocks at ~10s intervals
            p2pool_time = slot_start_time
            for _ in range(int(config.target_block_time / config.p2pool_block_time)):
                # Determine if this p2pool should embed DarkWow merge-mining data
                # When a Monero block is found, p2pool updates its template
                merge_data = None
                if monero_blocks_this_slot:
                    # Embed DarkWow merge-mining data in p2pool block
                    merge_data = f"darkwow_aux_{slot}_{pi}".encode()

                pblock = p2pool.produce_block(
                    timestamp=p2pool_time,
                    hashpower=config.merge_hashpower / config.num_p2pools,
                    monero_block=monero_blocks_this_slot[-1] if monero_blocks_this_slot else monero.blocks[monero.current_height],
                    merge_mining_data=merge_data,
                )
                if pblock:
                    result.p2pool_blocks_produced += 1
                    if merge_data:
                        result.p2pool_merge_blocks += 1
                p2pool_time += config.p2pool_block_time

        slot_result.p2pool_height = max((p.height for p in p2pools), default=0)

        # ---- Step 3: DarkWow block production ----
        current_difficulty = diff_adjuster.difficulty if diff_adjuster else difficulty
        target = compute_target(current_difficulty)

        # -- Path A: Merge-mined blocks (one per p2pool) --
        merge_blocks: list[DarkWowBlock] = []
        for pi, p2pool in enumerate(p2pools):
            merge_hash = simulate_best_hash(
                config.merge_hashpower / config.num_p2pools,
                config.target_block_time,
            )
            merge_block = DarkWowBlock(
                height=height,
                previous_hash=canonical_chain[-1].hash,
                timestamp=current_time,
                nonce=random.randint(0, 2**32 - 1),
                pow_data=PowData.MONERO,
                hash=merge_hash,
                miner_type="merge",
                reward_recipient=p2pool_wallets[pi],
            )
            # If in anchor mode, attach best Monero anchor
            if config.consensus_mode == ConsensusMode.ANCHOR:
                anchor = monero.get_best_anchor()
                if anchor:
                    merge_block.anchor_monero_height = anchor.height
                    merge_block.anchor_monero_hash = anchor.hash

            # If Caribina is enabled, attach simulated Arweave anchor
            if config.caribina_enabled or config.consensus_mode == ConsensusMode.CARIBINA:
                # Simulated Arweave TX ID (random 32 bytes — in production
                # this comes from ArDrive Turbo after POSTing the block data)
                merge_block.caribina_tx_id = bytes(random.randint(0, 255) for _ in range(32))
                # Mark Caribina anchor as "pending" — it settles after
                # caribina_settle_blocks confirmations

            merge_blocks.append(merge_block)

        # -- Path B: Native blocks (independent DarkWow miners) --
        native_block = DarkWowBlock(
            height=height,
            previous_hash=canonical_chain[-1].hash,
            timestamp=current_time,
            nonce=random.randint(0, 2**32 - 1),
            pow_data=PowData.DARK_FI,
            hash=simulate_best_hash(config.native_hashpower, config.target_block_time),
            miner_type="native",
            reward_recipient="native_wallet",
        )
        if config.consensus_mode == ConsensusMode.ANCHOR:
            anchor = monero.get_best_anchor()
            if anchor:
                native_block.anchor_monero_height = anchor.height
                native_block.anchor_monero_hash = anchor.hash

        if config.caribina_enabled or config.consensus_mode == ConsensusMode.CARIBINA:
            native_block.caribina_tx_id = bytes(random.randint(0, 255) for _ in range(32))

        # ---- Step 4: Fork choice ----
        all_candidates = merge_blocks + [native_block]
        forks: list[Fork] = []
        for candidate in all_candidates:
            rank = block_rank(candidate, target, current_difficulty)
            fork = Fork()
            fork.append_block(candidate, rank)
            forks.append(fork)

        # ---- Check Caribina confirmation for blocks in the canonical chain ----
        # A Caribina anchor settles after caribina_settle_blocks DarkWow blocks.
        # Walk back and mark anchors as confirmed once enough height has passed.
        if config.caribina_enabled or config.consensus_mode == ConsensusMode.CARIBINA:
            current_chain_height = len(canonical_chain)
            for cb in canonical_chain:
                if cb.has_caribina and not cb.caribina_confirmed:
                    if current_chain_height - cb.height >= config.caribina_settle_blocks:
                        cb.caribina_confirmed = True

        # If finality is enabled (Monero or Caribina), filter out forks that
        # conflict with finalized blocks.
        if config.consensus_mode in (ConsensusMode.ANCHOR, ConsensusMode.CARIBINA):
            finalized = get_finalized_blocks(
                canonical_chain, monero.blocks, monero.current_height,
                config.anchor_min_confirmations,
                caribina_enabled=(config.caribina_enabled or config.consensus_mode == ConsensusMode.CARIBINA),
                current_darkwow_height=len(canonical_chain),
                caribina_settle_blocks=config.caribina_settle_blocks,
            )
            valid_forks = get_valid_forks(forks, finalized, canonical_chain)
        else:
            valid_forks = forks

        best_idx = best_fork_index(valid_forks)

        # ---- Step 5: Determine winner and loser ----
        if best_idx is None:
            # No valid fork (shouldn't happen)
            continue

        winner = forks[best_idx].tip
        losers = [forks[i].tip for i in range(len(forks)) if i != best_idx]
        losers = [b for b in losers if b is not None]

        slot_result.canonical_block = winner
        if winner.miner_type == "merge":
            result.total_merge_canonical += 1
        else:
            result.total_native_canonical += 1

        if winner.has_anchor:
            result.total_anchored_blocks += 1
            slot_result.anchor_used = True

        if winner.has_caribina:
            result.total_caribina_blocks += 1
            slot_result.caribina_used = True

        # ---- Step 6: Reward distribution ----
        emission_reward = expected_reward(height)
        slot_result.emission_reward = emission_reward

        if config.uncle_phase == "phase2" and losers:
            depth = 1  # single depth for same-height competition
            uncle_reward = emission_reward // (2 ** depth)

            for loser in losers:
                if loser.miner_type == "merge":
                    result.total_merge_uncles += 1
                    result.total_merge_reward += uncle_reward
                else:
                    result.total_native_uncles += 1
                    result.total_native_reward += uncle_reward

                slot_result.uncle_blocks.append(loser)
                slot_result.uncle_rewards.append(uncle_reward)

            # Canonical reward with inclusion bonuses
            canonical_total, uncle_shares = compute_reward_distribution(
                emission_reward,
                len(losers),
            )
        else:
            # Phase 1: losers orphaned
            slot_result.orphaned_blocks = losers
            canonical_total = emission_reward

        slot_result.canonical_total = canonical_total

        if winner.miner_type == "merge":
            result.total_merge_reward += canonical_total
            slot_result.merge_reward_this_slot = canonical_total
        else:
            result.total_native_reward += canonical_total
            slot_result.native_reward_this_slot = canonical_total

        # Append to canonical chain
        canonical_chain.append(winner)
        block_interval = config.target_block_time  # target interval (actual = target in model)
        if diff_adjuster:
            diff_adjuster.record_block(block_interval)
            diff_adjuster.next_difficulty()
        current_time += config.target_block_time

        result.slots.append(slot_result)

    # Save Monero chain state for post-simulation finality checks
    result.monero_blocks = monero.blocks
    result.monero_current_height = monero.current_height

    return result


def simulate_reorg_attack(
    native_hashpower: float,
    merge_hashpower: float,
    chain_length: int = 6,
    reorg_from_height: int = 2,
    consensus_mode: ConsensusMode = ConsensusMode.NATIVE,
    anchor_min_confirmations: int = ANCHOR_MIN_CONFIRMATIONS,
    attacker_hashpower: Optional[float] = None,
    seed: Optional[int] = None,
) -> ReorgResult:
    """Simulate a reorg attack: attacker mines a secret fork and tries to replace
    part of the canonical chain.

    1. Builds a chain of `chain_length` blocks via normal slot competition.
    2. Attacker builds a competing fork from `reorg_from_height`.
    3. Tests whether the attacker fork would be accepted under fork choice rules.
    4. If anchoring is enabled, tests whether finality blocks the reorg.

    The attacker hashpower defaults to merge_hashpower (simulating a dominant
    merge miner performing the reorg). To guarantee reorg success against a
    stronger canonical chain, pass a higher value.

    Returns a ReorgResult detailing which blocks were replaced, which were
    protected by finality, and the reward redistribution.
    """
    if attacker_hashpower is None:
        attacker_hashpower = merge_hashpower
    if seed is not None:
        random.seed(seed)

    result = ReorgResult(
        chain_length=chain_length,
        reorg_from_height=reorg_from_height,
        consensus_mode=consensus_mode.value,
    )

    # ---- Step 1: Build the canonical chain ----
    caribina_enabled = (consensus_mode == ConsensusMode.CARIBINA)
    config = SimulationConfig(
        native_hashpower=native_hashpower,
        merge_hashpower=merge_hashpower,
        num_p2pools=1,
        consensus_mode=consensus_mode,
        num_slots=chain_length,
        target_block_time=120.0,
        uncle_phase="phase2",
        anchor_min_confirmations=anchor_min_confirmations,
        caribina_enabled=caribina_enabled,
        seed=seed,
    )
    sim_result = run_simulation(config)
    canonical_chain = []
    for sr in sim_result.slots:
        if sr.canonical_block:
            canonical_chain.append(sr.canonical_block)
    # Prepend genesis
    genesis = DarkWowBlock(
        height=0, previous_hash=bytes(32), timestamp=0.0,
        nonce=0, pow_data=PowData.DARK_FI, hash=bytes(32),
        miner_type="genesis", reward_recipient="genesis",
    )
    canonical_chain = [genesis] + canonical_chain

    result.original_chain = canonical_chain

    # ---- Step 2: Compute original rewards ----
    original_rewards: dict[str, int] = {}
    for sr in sim_result.slots:
        if sr.canonical_block:
            mt = sr.canonical_block.miner_type
            original_rewards[mt] = original_rewards.get(mt, 0) + sr.canonical_total
        for loser in sr.uncle_blocks:
            mt = loser.miner_type
            # uncle reward = emission_reward / 2 (depth 1)
            uncle_r = sr.emission_reward // 2
            original_rewards[mt] = original_rewards.get(mt, 0) + uncle_r
    result.original_rewards = original_rewards
    result.total_issuance_before = sum(original_rewards.values())

    # ---- Step 3: Compute finalized blocks ----
    if consensus_mode in (ConsensusMode.ANCHOR, ConsensusMode.CARIBINA):
        finalized = get_finalized_blocks(
            canonical_chain, sim_result.monero_blocks, sim_result.monero_current_height,
            anchor_min_confirmations,
            caribina_enabled=caribina_enabled,
            current_darkwow_height=len(canonical_chain),
        )
        result.finalized_blocks = finalized
    else:
        finalized = set()

    # ---- Step 4: Build attacker fork ----
    attacker_blocks: list[DarkWowBlock] = []
    # Attacker chains from the block just before the reorg point
    parent = canonical_chain[reorg_from_height - 1]
    current_height = reorg_from_height

    for i in range(reorg_from_height, chain_length + 1):
        attacker_hash = simulate_best_hash(attacker_hashpower, 120.0)
        attacker_block = DarkWowBlock(
            height=current_height,
            previous_hash=attacker_blocks[-1].hash if attacker_blocks else parent.hash,
            timestamp=120.0 * current_height,
            nonce=random.randint(0, 2**32 - 1),
            pow_data=PowData.MONERO,
            hash=attacker_hash,
            miner_type="merge",
            reward_recipient="attacker_wallet",
        )
        attacker_blocks.append(attacker_block)
        current_height += 1

    result.attacker_fork_blocks = attacker_blocks

    # ---- Step 5: Fork choice ----
    # Build the canonical fork starting from reorg_from_height
    canonical_suffix = [b for b in canonical_chain if b.height >= reorg_from_height]
    canonical_from_reorg = canonical_chain[reorg_from_height - 1]  # parent

    # Build fork objects
    # Canonical continuation fork
    canon_fork = Fork()
    for b in canonical_suffix:
        target = compute_target(255)  # fixed difficulty for simplicity
        rank = block_rank(b, target, 255)
        canon_fork.append_block(b, rank)

    # Attacker fork
    attacker_fork = Fork()
    for b in attacker_blocks:
        target = compute_target(255)
        rank = block_rank(b, target, 255)
        attacker_fork.append_block(b, rank)

    # Without finality: best fork by rank wins
    native_forks = [canon_fork, attacker_fork]
    if consensus_mode in (ConsensusMode.ANCHOR, ConsensusMode.CARIBINA):
        valid_forks = get_valid_forks(native_forks, finalized, canonical_chain)
    else:
        valid_forks = native_forks

    best_idx = best_fork_index(valid_forks)
    attacker_wins = best_idx is not None and valid_forks[best_idx] is attacker_fork
    result.attacker_fork_accepted = attacker_wins

    # ---- Step 6: Determine what was replaced / protected ----
    # Build height-to-canonical-hash map
    canonical_by_height: dict[int, bytes] = {}
    for b in canonical_chain:
        canonical_by_height[b.height] = b.hash

    for b in attacker_blocks:
        h = b.height
        if h in canonical_by_height:
            if canonical_by_height[h] in finalized:
                result.blocks_protected.append(h)
            else:
                result.blocks_replaced.append(h)

    # ---- Step 7: Compute reward redistribution ----
    rewards_lost: dict[str, int] = {}
    rewards_gained: dict[str, int] = {}

    for h in result.blocks_replaced:
        slot_idx = h - 1  # height 1 = slot 0
        if slot_idx < len(sim_result.slots):
            sr = sim_result.slots[slot_idx]
            emission = sr.emission_reward

            # Original canonical miner loses their reward
            if sr.canonical_block:
                mt = sr.canonical_block.miner_type
                rewards_lost[mt] = rewards_lost.get(mt, 0) + sr.canonical_total

            # Uncle miners lose their payouts (paid by the now-replaced canonical block)
            for loser in sr.uncle_blocks:
                mt = loser.miner_type
                uncle_r = emission // 2
                rewards_lost[mt] = rewards_lost.get(mt, 0) + uncle_r

            # Attacker gains the emission reward for each replaced height
            # (attacker's replacement blocks don't include uncles, so no split)
            rewards_gained["merge"] = rewards_gained.get("merge", 0) + emission

    result.rewards_lost = rewards_lost
    result.rewards_gained = rewards_gained

    # Net redistribution: money moved, not created
    all_types = set(list(rewards_lost.keys()) + list(rewards_gained.keys()))
    result.net_redistribution = {
        t: rewards_gained.get(t, 0) - rewards_lost.get(t, 0)
        for t in all_types
    }

    # Total issuance unchanged: each height still produces one emission_reward
    result.total_issuance_after = result.total_issuance_before

    return result


# ============================================================================
# Reporting
# ============================================================================

def print_results(result: SimulationResult) -> None:
    """Print simulation results in a readable format."""
    c = result.config
    total_slots = len(result.slots)

    print("=" * 72)
    print("  DarkWow Merge Mining Simulation — Three-Chain Model")
    print("=" * 72)
    print(f"  Slots:               {total_slots}")
    print(f"  Consensus mode:      {c.consensus_mode.value}")
    print(f"  Num p2pools:         {c.num_p2pools}")
    print(f"  Native hashpower:    {c.native_hashpower:,.0f} H/s")
    print(f"  Merge hashpower:     {c.merge_hashpower:,.0f} H/s (total, split across p2pools)")
    print(f"  Hashpower ratio:     {c.merge_hashpower / c.native_hashpower:,.0f}:1")
    print(f"  Target block time:   {c.target_block_time}s")
    print(f"  Uncle phase:         {c.uncle_phase}")
    if c.consensus_mode == ConsensusMode.ANCHOR:
        print(f"  Difficulty ratio:    {c.difficulty_ratio}")
        print(f"  Anchor confirmations:{c.anchor_min_confirmations}")
    if c.caribina_enabled or c.consensus_mode == ConsensusMode.CARIBINA:
        print(f"  Caribina enabled:    True (settle blocks: {c.caribina_settle_blocks})")
    print(f"  Seed:                {c.seed}")
    print()

    # Chain stats
    print("  --- Chain Production ---")
    print(f"  Monero blocks:      {result.monero_blocks_produced}")
    print(f"  Monero total diff:  {result.monero_total_difficulty:,}")
    print(f"  p2pool blocks:      {result.p2pool_blocks_produced}")
    print(f"  p2pool merge blocks:{result.p2pool_merge_blocks}")
    print()

    # Canonical slot wins
    print("  --- Canonical Slot Wins ---")
    merge_pct = result.total_merge_canonical / total_slots * 100
    native_pct = result.total_native_canonical / total_slots * 100
    print(f"  Merge-mined:  {result.total_merge_canonical:>6}  ({merge_pct:5.1f}%)")
    print(f"  Native-mined: {result.total_native_canonical:>6}  ({native_pct:5.1f}%)")
    if result.total_native_canonical > 0:
        ratio = result.total_merge_canonical / result.total_native_canonical
        print(f"  Win ratio (merge:native): {ratio:.1f}:1")
    if c.consensus_mode == ConsensusMode.ANCHOR:
        anchored_pct = result.total_anchored_blocks / total_slots * 100
        print(f"  Anchored blocks: {result.total_anchored_blocks:>6}  ({anchored_pct:5.1f}%)")
    if c.caribina_enabled or c.consensus_mode == ConsensusMode.CARIBINA:
        caribina_pct = result.total_caribina_blocks / total_slots * 100
        print(f"  Caribina blocks: {result.total_caribina_blocks:>6}  ({caribina_pct:5.1f}%)")
    print()

    # Uncle stats
    if c.uncle_phase == "phase2":
        print("  --- Uncle Blocks (Phase 2) ---")
        print(f"  Merge uncles:  {result.total_merge_uncles}")
        print(f"  Native uncles: {result.total_native_uncles}")
        total_uncles = result.total_merge_uncles + result.total_native_uncles
        print(f"  Total uncles:  {total_uncles}")
        print()

    # Reward distribution
    print("  --- DarkWow DRKW Reward Distribution ---")
    total_reward = result.total_merge_reward + result.total_native_reward
    if total_reward > 0:
        merge_reward_pct = result.total_merge_reward / total_reward * 100
        native_reward_pct = result.total_native_reward / total_reward * 100
    else:
        merge_reward_pct = native_reward_pct = 0
    print(f"  Merge miners:  {result.total_merge_reward:>15,}  ({merge_reward_pct:5.1f}%)")
    print(f"  Native miners: {result.total_native_reward:>15,}  ({native_reward_pct:5.1f}%)")
    print(f"  Total DRKW:    {total_reward:>15,}")
    print()

    # Per-slot detail (first 20 and last 5)
    print("  --- Per-Slot Detail (first 20) ---")
    header = f"  {'Slot':>5} {'H':>6} {'Winner':>8} {'Loser':>8} {'Emission':>14} {'CanonTotal':>14} {'UncleRew':>14}"
    if c.consensus_mode == ConsensusMode.ANCHOR:
        header += f" {'Anchor':>8}"
    if c.caribina_enabled or c.consensus_mode == ConsensusMode.CARIBINA:
        header += f" {'Caribina':>9}"
    print(header)
    for sr in result.slots[:20]:
        _print_slot_row(sr, c)

    if len(result.slots) > 25:
        print(f"  {'...':>5}")
        for sr in result.slots[-5:]:
            _print_slot_row(sr, c)

    print()
    print("=" * 72)


def _print_slot_row(sr: SlotResult, c: SimulationConfig) -> None:
    """Print a single slot result row."""
    winner = sr.canonical_block.miner_type if sr.canonical_block else "?"
    loser = (sr.uncle_blocks[0].miner_type if sr.uncle_blocks
             else sr.orphaned_blocks[0].miner_type if sr.orphaned_blocks
             else "-")
    uncle_reward = sr.uncle_rewards[0] if sr.uncle_rewards else 0
    row = f"  {sr.slot:>5} {sr.height:>6} {winner:>8} {loser:>8} {sr.emission_reward:>14,} {sr.canonical_total:>14,} {uncle_reward:>14,}"
    if c.consensus_mode == ConsensusMode.ANCHOR:
        anchor_str = "Y" if sr.anchor_used else "-"
        row += f" {anchor_str:>8}"
    if c.caribina_enabled or c.consensus_mode == ConsensusMode.CARIBINA:
        caribina_str = "Y" if sr.caribina_used else "-"
        row += f" {caribina_str:>9}"
    print(row)


# ============================================================================
# Verification Tests
# ============================================================================

def run_verification() -> bool:
    """Run verification tests against the Rust reference and p2pool model.

    Returns True if all tests pass.
    """
    failures = 0

    # ---- Test 1: expected_reward ----
    print("--- Test 1: expected_reward ---")
    assert expected_reward(0) == 0, f"Genesis: {expected_reward(0)}"
    r1 = expected_reward(1)
    decay = 2.0 ** (-1.0 / HALF_LIFE_BLOCKS)
    expected_r1 = int(INITIAL_REWARD * decay)
    assert r1 == expected_r1, f"Height 1: {r1} != {expected_r1}"
    r_tail = expected_reward(HALF_LIFE_BLOCKS * 10)
    assert r_tail == TAIL_REWARD, f"Tail: {r_tail} != {TAIL_REWARD}"
    print(f"  PASS: reward(0)={expected_reward(0)}, reward(1)={r1}, tail={r_tail}")

    # ---- Test 2: block_rank on known values ----
    print("--- Test 2: block_rank ---")
    block = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=42, pow_data=PowData.DARK_FI,
        hash=int(12345).to_bytes(32, "little"), miner_type="test",
    )
    target = MAX_32_BYTES // 255
    difficulty = 255
    rank = block_rank(block, target, difficulty)
    expected_target_dist = MAX_32_BYTES - target
    expected_target_dist_sq = expected_target_dist * expected_target_dist
    assert rank.targets_rank == expected_target_dist_sq
    expected_hash_dist = MAX_32_BYTES - 12345
    expected_hash_dist_sq = expected_hash_dist * expected_hash_dist
    assert rank.hashes_rank == expected_hash_dist_sq
    assert rank.difficulty == 255
    print(f"  PASS: rank(hash=12345) = ({rank.difficulty}, {rank.targets_rank}, {rank.hashes_rank})")

    # ---- Test 3: Genesis block_rank ----
    print("--- Test 3: Genesis block_rank ---")
    gen_block = DarkWowBlock(
        height=0, previous_hash=bytes(32), timestamp=0.0,
        nonce=0, pow_data=PowData.DARK_FI, hash=bytes(32), miner_type="genesis",
    )
    gen_rank = block_rank(gen_block, target, difficulty)
    assert gen_rank.targets_rank == 0
    assert gen_rank.hashes_rank == 0
    print(f"  PASS: genesis rank = ({gen_rank.difficulty}, 0, 0)")

    # ---- Test 4: best_fork_index ----
    print("--- Test 4: best_fork_index ---")
    forks = [
        Fork(targets_rank=100, hashes_rank=50),
        Fork(targets_rank=200, hashes_rank=30),
        Fork(targets_rank=100, hashes_rank=60),
    ]
    best = best_fork_index(forks)
    assert best == 1, f"best_fork_index: {best} != 1"
    print(f"  PASS: best_fork_index = {best}")

    # ---- Test 5: best_fork_index tiebreak ----
    print("--- Test 5: best_fork_index tiebreak ---")
    forks_tie = [
        Fork(targets_rank=100, hashes_rank=50),
        Fork(targets_rank=100, hashes_rank=70),
    ]
    best_tie = best_fork_index(forks_tie)
    assert best_tie == 1, f"best_fork_index tiebreak: {best_tie} != 1"
    print(f"  PASS: best_fork_index (tiebreak) = {best_tie}")

    # ---- Test 6: worst_fork_index ----
    print("--- Test 6: worst_fork_index ---")
    worst = worst_fork_index(forks)
    assert worst == 0, f"worst_fork_index: {worst} != 0"
    forks_clear = [
        Fork(targets_rank=100, hashes_rank=50),
        Fork(targets_rank=200, hashes_rank=30),
        Fork(targets_rank=50, hashes_rank=99),
    ]
    worst2 = worst_fork_index(forks_clear)
    assert worst2 == 2, f"worst_fork_index clear: {worst2} != 2"
    print(f"  PASS: worst_fork_index = {worst} (tie), = {worst2} (clear)")

    # ---- Test 7: compute_reward_distribution ----
    print("--- Test 7: compute_reward_distribution ---")
    test_reward = expected_reward(1)
    canon, uncles = compute_reward_distribution(test_reward, 0)
    assert canon == test_reward, f"0 uncles: {canon} != {test_reward}"
    assert len(uncles) == 0

    canon1, u1 = compute_reward_distribution(test_reward, 1)
    assert canon1 == test_reward + test_reward // 2
    assert len(u1) == 0  # even index goes to canonical

    canon2, u2 = compute_reward_distribution(test_reward, 2)
    assert canon2 == test_reward + test_reward // 2
    assert len(u2) == 1
    assert u2[0]["reward"] == test_reward // 2

    canon3, u3 = compute_reward_distribution(test_reward, 3)
    assert canon3 == test_reward + test_reward // 2 + test_reward // 4
    assert len(u3) == 1
    print(f"  PASS: block_reward={test_reward:,}")
    print(f"  PASS: 0 uncles: canon={canon:,}")
    print(f"  PASS: 1 uncle:  canon={canon1:,}, 0 uncle_payouts")
    print(f"  PASS: 2 uncles: canon={canon2:,}, uncle_reward={u2[0]['reward']:,}")
    print(f"  PASS: 3 uncles: canon={canon3:,}, uncle_reward={u3[0]['reward']:,}")

    # ---- Test 8: Both PowData variants ----
    print("--- Test 8: Both PowData variants ---")
    dblock = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(50000).to_bytes(32, "little"), miner_type="native",
    )
    mblock = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.MONERO,
        hash=int(50000).to_bytes(32, "little"), miner_type="merge",
    )
    assert block_rank(dblock, target, difficulty) == block_rank(mblock, target, difficulty)
    print(f"  PASS: Same hash -> same rank regardless of PowData variant")

    # ---- Test 9: Better hash wins ----
    print("--- Test 9: Better hash wins ---")
    better = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(100).to_bytes(32, "little"), miner_type="native",
    )
    worse = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=1000.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(999999).to_bytes(32, "little"), miner_type="native",
    )
    better_rank = block_rank(better, target, difficulty)
    worse_rank = block_rank(worse, target, difficulty)
    assert better_rank.hashes_rank > worse_rank.hashes_rank
    print(f"  PASS: hash=100 rank > hash=999999 rank ({better_rank.hashes_rank} > {worse_rank.hashes_rank})")

    # ---- Test 10: p2pool difficulty adjustment ----
    print("--- Test 10: p2pool difficulty adjustment ---")
    p2pool_data: list[tuple[float, int]] = []
    cum_diff = 0
    for i in range(100):
        t = i * P2POOL_BLOCK_TIME
        cum_diff += 100000
        p2pool_data.append((t, cum_diff))
    diff = p2pool_get_difficulty(p2pool_data)
    assert diff >= P2POOL_MIN_DIFFICULTY, f"p2pool diff {diff} < min {P2POOL_MIN_DIFFICULTY}"
    # With uniform difficulty increases matching the target time, diff should be stable
    assert 90000 <= diff <= 110000, f"p2pool diff {diff} out of expected range [90000, 110000]"
    print(f"  PASS: p2pool difficulty = {diff} (stable at ~100k with uniform increments)")

    # ---- Test 11: p2pool is_longer_chain ----
    print("--- Test 11: p2pool is_longer_chain ---")
    assert p2pool_is_longer_chain(1000, 2000) == True
    assert p2pool_is_longer_chain(2000, 1000) == False
    assert p2pool_is_longer_chain(1000, 1000) == False
    print(f"  PASS: is_longer_chain works by cumulative difficulty")

    # ---- Test 12: p2pool uncle penalty ----
    print("--- Test 12: p2pool uncle penalty ---")
    blocks = []
    for i in range(10):
        block = P2poolBlock(
            height=i + 1,
            parent_hash=bytes(32),
            timestamp=i * 10.0,
            nonce=i,
            hash=simulate_best_hash(1e6, 10.0),
            difficulty=100000,
            cumulative_difficulty=(i + 1) * 100000,
            uncles=[bytes([i + 1]) * 32] if i > 0 else [],
        )
        blocks.append(block)
    shares = p2pool_get_shares(blocks, window_size=10)
    # Should have shares from blocks and uncles
    uncle_shares = [s for s in shares if s.is_uncle]
    non_uncle_shares = [s for s in shares if not s.is_uncle]
    assert len(uncle_shares) > 0, "Should have uncle shares"
    # Uncle weight should be 80% of difficulty (20% penalty)
    expected_uncle_weight = 100000 * (100 - P2POOL_UNCLE_PENALTY) // 100
    for s in uncle_shares:
        assert s.weight == expected_uncle_weight, f"Uncle weight {s.weight} != {expected_uncle_weight}"
    print(f"  PASS: {len(shares)} shares, {len(uncle_shares)} uncle (penalty={P2POOL_UNCLE_PENALTY}%), uncle_weight={expected_uncle_weight}")

    # ---- Test 13: Anchoring finality — finalized blocks cannot be reorged ----
    print("--- Test 13: Anchoring finality ---")
    monero_chain: dict[int, MoneroBlock] = {}
    for h in range(10):
        monero_chain[h] = MoneroBlock(
            height=h,
            hash=bytes([h]) * 32,
            previous_hash=bytes([h - 1]) * 32 if h > 0 else bytes(32),
            timestamp=h * 120.0,
            difficulty=20000,
            cumulative_difficulty=h * 20000,
        )

    # Build a canonical chain with anchored blocks
    canonical: list[DarkWowBlock] = []
    genesis = DarkWowBlock(
        height=0, previous_hash=bytes(32), timestamp=0.0,
        nonce=0, pow_data=PowData.DARK_FI, hash=bytes(32), miner_type="genesis",
    )
    canonical.append(genesis)

    # Block 1 anchors to Monero height 1
    b1 = DarkWowBlock(
        height=1, previous_hash=canonical[0].hash, timestamp=120.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(1000).to_bytes(32, "little"), miner_type="native",
        anchor_monero_height=1, anchor_monero_hash=bytes([1]) * 32,
    )
    canonical.append(b1)

    # Block 2 anchors to Monero height 2
    b2 = DarkWowBlock(
        height=2, previous_hash=canonical[1].hash, timestamp=240.0,
        nonce=2, pow_data=PowData.DARK_FI,
        hash=int(2000).to_bytes(32, "little"), miner_type="native",
        anchor_monero_height=2, anchor_monero_hash=bytes([2]) * 32,
    )
    canonical.append(b2)

    # Block 3 anchors to Monero height 3
    b3 = DarkWowBlock(
        height=3, previous_hash=canonical[2].hash, timestamp=360.0,
        nonce=3, pow_data=PowData.DARK_FI,
        hash=int(3000).to_bytes(32, "little"), miner_type="native",
        anchor_monero_height=3, anchor_monero_hash=bytes([3]) * 32,
    )
    canonical.append(b3)

    # Monero at height 9, anchor_min_confirmations=3
    # Block 1 anchored at Monero height 1: confirmations = 9-1 = 8 >= 3 -> finalized
    # Block 2 anchored at Monero height 2: confirmations = 9-2 = 7 >= 3 -> finalized
    # Block 3 anchored at Monero height 3: confirmations = 9-3 = 6 >= 3 -> finalized
    finalized = get_finalized_blocks(canonical, monero_chain, 9, min_confirmations=3)
    assert b1.hash in finalized, f"Block 1 should be finalized"
    assert b2.hash in finalized, f"Block 2 should be finalized"
    assert b3.hash in finalized, f"Block 3 should be finalized"
    print(f"  PASS: Blocks 1-3 finalized (Monero height=9, confirmations>=3)")

    # An attacker creates a fork at height 3 — tries to replace finalized block
    attacker_block = DarkWowBlock(
        height=3, previous_hash=canonical[1].hash,  # diverges at height 3
        timestamp=360.0, nonce=99, pow_data=PowData.DARK_FI,
        hash=int(1).to_bytes(32, "little"), miner_type="merge",  # very good hash
    )
    attacker_fork = Fork(
        blocks=[attacker_block],
        targets_rank=9999999,
        hashes_rank=9999999,  # better rank than canonical!
    )

    # Fork choice without finality would prefer the attacker (better rank)
    # But with the finality gadget, this fork conflicts with finalized b3
    assert fork_conflicts_with_finalized(attacker_fork, finalized, canonical) == True

    valid_forks = get_valid_forks([attacker_fork], finalized, canonical)
    assert len(valid_forks) == 0, "Attacker fork should be filtered out"
    print(f"  PASS: Attacker fork (better rank) rejected — conflicts with finalized block")

    # A fork that extends from a finalized block is valid
    valid_extension = DarkWowBlock(
        height=4, previous_hash=canonical[3].hash,  # extends canonical
        timestamp=480.0, nonce=100, pow_data=PowData.DARK_FI,
        hash=int(4000).to_bytes(32, "little"), miner_type="native",
    )
    valid_fork = Fork(
        blocks=[valid_extension],
        targets_rank=500,
        hashes_rank=500,
    )
    assert fork_conflicts_with_finalized(valid_fork, finalized, canonical) == False
    valid_forks2 = get_valid_forks([valid_fork], finalized, canonical)
    assert len(valid_forks2) == 1
    print(f"  PASS: Fork extending from finalized block is valid")

    # ---- Test 14: Finality requires sufficient confirmations ----
    print("--- Test 14: Finality confirmation threshold ---")
    # Monero at height 2, anchor_min_confirmations=3
    # Block 1 anchored at height 1: confirmations = 2-1 = 1 < 3 -> NOT finalized
    not_finalized = get_finalized_blocks(canonical, monero_chain, 2, min_confirmations=3)
    assert b1.hash not in not_finalized, "Block 1 should NOT be finalized yet"
    assert b2.hash not in not_finalized
    print(f"  PASS: Blocks not finalized when Monero confirmations < threshold")

    # Monero at height 5 — Block 1 has 4 confirmations (>=3), Block 2 has 3 (>=3), Block 3 has 2 (<3)
    partial_finalized = get_finalized_blocks(canonical, monero_chain, 5, min_confirmations=3)
    assert b1.hash in partial_finalized, "Block 1 (4 confirmations) should be finalized"
    assert b2.hash in partial_finalized, "Block 2 (3 confirmations) should be finalized"
    assert b3.hash not in partial_finalized, "Block 3 (2 confirmations) should NOT be finalized"
    print(f"  PASS: Finality rolls forward with Monero chain — blocks 1-2 done, block 3 pending")

    # ---- Test 15: Reorg attack without anchoring vs with anchoring ----
    print("--- Test 15: Reorg attack protection ---")
    # Scenario: merge miner tries to replace a native-mined block that has rewards
    # Without anchoring: attacker's better hash wins (normal fork choice)
    native_block = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=120.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(50000).to_bytes(32, "little"), miner_type="native",
        reward_recipient="native_wallet",
    )
    native_fork = Fork(blocks=[native_block], targets_rank=1000, hashes_rank=1000)

    # Attacker mines a replacement with better hash
    attacker_replacement = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=120.0,
        nonce=99, pow_data=PowData.MONERO,
        hash=int(100).to_bytes(32, "little"), miner_type="merge",  # better hash
        reward_recipient="attacker_wallet",
    )
    attacker_fork = Fork(blocks=[attacker_replacement], targets_rank=1000, hashes_rank=9999999)

    # Without anchoring: attacker wins (better hashes_rank)
    best_native = best_fork_index([native_fork, attacker_fork])
    assert best_native == 1, f"Without anchoring, attacker wins: {best_native}"
    print(f"  PASS: Without anchoring — attacker with better hash can reorg native block")

    # With anchoring: if native block is anchored and finalized, attacker is filtered
    native_anchored = DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=120.0,
        nonce=1, pow_data=PowData.DARK_FI,
        hash=int(50000).to_bytes(32, "little"), miner_type="native",
        reward_recipient="native_wallet",
        anchor_monero_height=1, anchor_monero_hash=bytes([1]) * 32,
    )
    canonical_with_native = [genesis, native_anchored]
    anchored_finalized = get_finalized_blocks(
        canonical_with_native, monero_chain, 9, min_confirmations=3,
    )
    assert native_anchored.hash in anchored_finalized, "Native block should be finalized"

    # Attacker tries same replacement
    attacker_fork2 = Fork(blocks=[attacker_replacement], targets_rank=1000, hashes_rank=9999999)
    assert fork_conflicts_with_finalized(attacker_fork2, anchored_finalized, canonical_with_native) == True
    valid_finality = get_valid_forks([attacker_fork2], anchored_finalized, canonical_with_native)
    assert len(valid_finality) == 0, "Attacker fork should be rejected by finality"
    print(f"  PASS: With anchoring — attacker cannot reorg finalized native block")
    print(f"  PASS: All rewards in finalized block are protected by Monero finality")

    # ---- Test 16: Multi-block reorg without anchoring ----
    print("--- Test 16: Multi-block reorg without anchoring ---")
    # Boost native hashpower so native wins some canonical slots,
    # then attacker (with full merge hashpower) reorgs them
    reorg_native = simulate_reorg_attack(
        native_hashpower=500_000.0,  # equal hashpower → native wins some slots
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.NATIVE,
        attacker_hashpower=5_000_000.0,  # 10x attacker advantage
        seed=99,
    )
    assert reorg_native.attacker_fork_accepted, \
        "Without anchoring, attacker with better hashpower should win"
    assert len(reorg_native.blocks_replaced) > 0, \
        "Attacker should replace canonical blocks"
    print(f"  PASS: Attacker replaced {len(reorg_native.blocks_replaced)} blocks (heights {reorg_native.blocks_replaced})")
    print(f"  PASS: Blocks protected: {len(reorg_native.blocks_protected)} (no anchoring = 0)")

    # ---- Test 17: Multi-block reorg with anchoring ----
    print("--- Test 17: Multi-block reorg with anchoring ---")
    reorg_anchor = simulate_reorg_attack(
        native_hashpower=500_000.0,
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.ANCHOR,
        anchor_min_confirmations=2,
        attacker_hashpower=5_000_000.0,
        seed=99,
    )
    assert len(reorg_anchor.finalized_blocks) > 0, \
        "Anchoring mode should produce finalized blocks"
    assert len(reorg_anchor.blocks_protected) > 0, \
        "Anchoring should protect finalized blocks from reorg"
    print(f"  PASS: Finalized blocks: {len(reorg_anchor.finalized_blocks)}")
    print(f"  PASS: Blocks protected: {len(reorg_anchor.blocks_protected)} (heights {reorg_anchor.blocks_protected})")
    print(f"  PASS: Blocks replaced: {len(reorg_anchor.blocks_replaced)} (heights {reorg_anchor.blocks_replaced})")
    if reorg_anchor.attacker_fork_accepted:
        print(f"  INFO: Attacker fork accepted but only replaced unfinalized blocks")
    else:
        print(f"  PASS: Attacker fork rejected entirely (all targets finalized)")

    # ---- Test 18: Caribina finality — basic block finalization ----
    print("--- Test 18: Caribina finality ---")
    # Build a chain where every block has a Caribina anchor
    gc_blocks = [DarkWowBlock(height=0, previous_hash=bytes(32), timestamp=0.0,
                  nonce=0, pow_data=PowData.DARK_FI, hash=bytes(32), miner_type="genesis")]
    for h in range(1, 6):
        gc_blocks.append(DarkWowBlock(
            height=h, previous_hash=gc_blocks[-1].hash, timestamp=h * 120.0,
            nonce=h, pow_data=PowData.DARK_FI, hash=bytes([h]) * 32, miner_type="native",
            caribina_tx_id=bytes([h + 100]) * 32,
        ))
    # At chain height 5, block 1 (height diff=4) should be finalized (settle_blocks=1)
    # Block 4 (height diff=1) also finalized. Block 5 (height diff=0) not yet.
    caribina_final = get_caribina_finalized_blocks(gc_blocks, 5, settle_blocks=1)
    # Block 5 (height=5) has current_height - height = 5-5 = 0 < 1 → not finalized
    # But its ancestor hash (parent of block 5 = block 4's hash) is in finalized_ancestors
    assert gc_blocks[1].hash in caribina_final, "Block 1 should be Caribina-finalized (diff=4 >= 1)"
    assert gc_blocks[4].hash in caribina_final, "Block 4 should be Caribina-finalized (diff=1 >= 1)"
    assert gc_blocks[5].hash not in caribina_final, "Block 5 should NOT be finalized (diff=0 < 1)"
    print(f"  PASS: Caribina finality — blocks 1,4 finalized; block 5 pending")

    # ---- Test 19: Caribina finality blocks reorg ----
    print("--- Test 19: Caribina reorg protection ---")
    # Same chain, attacker tries to replace block 4 which is Caribina-finalized
    attacker_caribina_fork = Fork()
    attacker_block = DarkWowBlock(
        height=4, previous_hash=gc_blocks[3].hash,
        timestamp=480.0, nonce=999, pow_data=PowData.DARK_FI,
        hash=bytes([99]) * 32, miner_type="native",
    )
    attacker_caribina_fork.append_block(attacker_block, BlockRanks(
        difficulty=255, targets_rank=1000, hashes_rank=1000,
    ))
    # Attacker fork has better rank, but conflicts with finalized block 4
    assert fork_conflicts_with_finalized(attacker_caribina_fork, caribina_final, gc_blocks) == True
    valid_c = get_valid_forks([attacker_caribina_fork], caribina_final, gc_blocks)
    assert len(valid_c) == 0, "Attacker fork should be rejected by Caribina finality"
    print(f"  PASS: Caribina finality blocks reorg of finalized block")

    # ---- Test 20: Caribina faster than Monero anchoring ----
    print("--- Test 20: Caribina settlement speed ---")
    # Monero needs ~3 blocks × 120s = 360s (min_confirmations=3)
    # Caribina needs ~1 block  × 120s = 120s (settle_blocks=1)
    # Build a minimal chain to compare settlement
    settle_chain = [DarkWowBlock(height=0, previous_hash=bytes(32), timestamp=0.0,
                    nonce=0, pow_data=PowData.DARK_FI, hash=bytes(32), miner_type="genesis")]
    settle_chain.append(DarkWowBlock(
        height=1, previous_hash=settle_chain[-1].hash, timestamp=120.0,
        nonce=1, pow_data=PowData.DARK_FI, hash=bytes([1]) * 32, miner_type="native",
        caribina_tx_id=bytes([200]) * 32,
        # Monero anchor would need 3 more Monero blocks (~360s)
        anchor_monero_height=1, anchor_monero_hash=bytes([200]) * 32,
    ))
    # At chain height 2, Caribina settles (diff=1 >= 1) but Monero may not
    caribina_settled = get_caribina_finalized_blocks(settle_chain, 2, settle_blocks=1)
    assert settle_chain[1].hash in caribina_settled, "Caribina: block 1 settled at height 2"
    # Monero needs min_confirmations=3 Monero blocks at ~120s each
    # If only 1-2 Monero blocks exist, Monero anchor won't be finalized yet
    monero_blocks = {1: MoneroBlock(height=1, hash=bytes([200]) * 32, previous_hash=bytes(32),
                                     timestamp=120.0, difficulty=1000, cumulative_difficulty=1000)}
    monero_settled = get_monero_finalized_blocks(settle_chain, monero_blocks, 2, min_confirmations=3)
    assert settle_chain[1].hash not in monero_settled, "Monero: block 1 NOT settled yet (only 1 confirmation)"
    print(f"  PASS: Caribina settles at height 2 (1 block); Monero needs ~3 Monero blocks (~360s)")

    # ---- Test 21: Reward conservation under reorg ----
    print("--- Test 21: Reward conservation under reorg ---")
    assert reorg_native.total_issuance_before > 0, "Should have issuance before reorg"
    # The reorg redistributes rewards: original miners lose, attacker gains.
    # emission_reward per height is conserved (one per slot).
    # Uncle inclusion bonuses and uncle payouts from replaced blocks are lost
    # (attacker's replacement blocks don't recreate them).
    # Net redistribution is negative — the reorg destroys the inclusion bonuses.
    assert len(reorg_native.rewards_lost) > 0, "Should have lost rewards"
    assert len(reorg_native.rewards_gained) > 0, "Attacker should gain rewards"
    emission_lost = sum(reorg_native.rewards_lost.values())
    emission_gained = sum(reorg_native.rewards_gained.values())
    net_change = sum(reorg_native.net_redistribution.values())
    print(f"  PASS: Rewards lost total: {emission_lost:>15,}")
    print(f"  PASS: Rewards gained total: {emission_gained:>15,}")
    print(f"  PASS: Net redistribution: {net_change:>15,}")
    print(f"  PASS: (negative = inclusion bonuses + uncle payouts destroyed)")
    print(f"  PASS: Rewards lost: native={reorg_native.rewards_lost.get('native', 0):,}, merge={reorg_native.rewards_lost.get('merge', 0):,}")
    print(f"  PASS: Rewards gained: merge={reorg_native.rewards_gained.get('merge', 0):,}")

    # ---- Test 22: Multi-block reorg with Caribina ----
    print("--- Test 22: Multi-block reorg with Caribina ---")
    reorg_caribina = simulate_reorg_attack(
        native_hashpower=500_000.0,
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.CARIBINA,
        attacker_hashpower=5_000_000.0,
        seed=99,
    )
    assert len(reorg_caribina.finalized_blocks) > 0, \
        "Caribina should have finalized blocks"
    # Caribina settles after 1 block, so blocks at height <= chain_length-1
    # should be finalized (can't be replaced). Only the tip block may not be
    # finalized if it's the very last block.
    assert len(reorg_caribina.blocks_protected) > 0, \
        "Caribina should protect finalized blocks from reorg"
    print(f"  PASS: Caribina finalized blocks: {len(reorg_caribina.finalized_blocks)}")
    print(f"  PASS: Caribina blocks protected: {len(reorg_caribina.blocks_protected)} (heights {reorg_caribina.blocks_protected})")
    print(f"  PASS: Caribina blocks replaced: {len(reorg_caribina.blocks_replaced)} (heights {reorg_caribina.blocks_replaced})")
    if reorg_caribina.attacker_fork_accepted:
        print(f"  INFO: Attacker replaced only unfinalized blocks")
    else:
        print(f"  PASS: Attacker fork rejected entirely by Caribina finality")

    # ---- Test 23: Caribina protects native miners (no p2pool needed) ----
    print("--- Test 23: Caribina protects native miners ---")
    # Key advantage: native miners don't need p2pool to get finality.
    # Build a purely native-miner chain with Caribina anchors.
    native_caribina_chain = [
        DarkWowBlock(height=0, previous_hash=bytes(32), timestamp=0.0,
                     nonce=0, pow_data=PowData.DARK_FI, hash=bytes(32), miner_type="genesis"),
        DarkWowBlock(height=1, previous_hash=bytes(32), timestamp=120.0,
                     nonce=1, pow_data=PowData.DARK_FI, hash=bytes([1]) * 32,
                     miner_type="native", caribina_tx_id=bytes([200]) * 32, caribina_confirmed=True),
        DarkWowBlock(height=2, previous_hash=bytes([1]) * 32, timestamp=240.0,
                     nonce=2, pow_data=PowData.DARK_FI, hash=bytes([2]) * 32,
                     miner_type="native", caribina_tx_id=bytes([201]) * 32, caribina_confirmed=True),
    ]
    # Block 1 is Caribina-finalized
    nc_finalized = get_caribina_finalized_blocks(native_caribina_chain, 3, settle_blocks=1)
    assert native_caribina_chain[1].hash in nc_finalized, "Native block 1 should be Caribina-finalized"
    # Attacker tries to replace block 1
    nc_attacker = Fork()
    nc_attacker.append_block(DarkWowBlock(
        height=1, previous_hash=bytes(32), timestamp=120.0,
        nonce=999, pow_data=PowData.DARK_FI, hash=bytes([99]) * 32, miner_type="native",
    ), BlockRanks(difficulty=255, targets_rank=1000, hashes_rank=1000))
    assert fork_conflicts_with_finalized(nc_attacker, nc_finalized, native_caribina_chain) == True
    print(f"  PASS: Native miner block protected by Caribina (no p2pool/Monero needed)")
    print(f"  PASS: Caribina provides finality independent of merge mining")

    print()
    print("=" * 72)
    print("  All verification tests PASSED (23/23)")
    print("=" * 72)
    return True


# ============================================================================
# Main
# ============================================================================

def main() -> None:
    """Run verification tests and simulations."""
    if not run_verification():
        print("VERIFICATION FAILED", file=sys.stderr)
        sys.exit(1)

    print()
    print()

    # ---- Scenario 1: Native mode, 1000:1 hashpower, Phase 2 ----
    print("=== Scenario 1: Native Consensus, 1000:1 Hashpower, Phase 2 ===")
    print("  Single p2pool dominates — merge miner gets most canonical slots.")
    print("  Native miner gets uncle rewards only.")
    print()
    config1 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_p2pools=1,
        consensus_mode=ConsensusMode.NATIVE,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        seed=42,
    )
    result1 = run_simulation(config1)
    print_results(result1)

    print()
    print()

    # ---- Scenario 2: Native mode, 1:1 hashpower, Phase 2 ----
    print("=== Scenario 2: Native Consensus, 1:1 Hashpower, Phase 2 ===")
    print("  Equal hashpower — should be ~50/50 split.")
    print()
    config2 = SimulationConfig(
        native_hashpower=10_000.0,
        merge_hashpower=10_000.0,
        num_p2pools=1,
        consensus_mode=ConsensusMode.NATIVE,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        seed=42,
    )
    result2 = run_simulation(config2)
    print_results(result2)

    print()
    print()

    # ---- Scenario 3: Native mode, 1000:1, Phase 1 (no uncles) ----
    print("=== Scenario 3: Native Consensus, 1000:1 Hashpower, Phase 1 ===")
    print("  No uncle rewards — native miner gets NOTHING.")
    print("  Demonstrates the Phase 1 -> Phase 2 necessity.")
    print()
    config3 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_p2pools=1,
        consensus_mode=ConsensusMode.NATIVE,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase1",
        seed=42,
    )
    result3 = run_simulation(config3)
    print_results(result3)

    print()
    print()

    # ---- Scenario 4: Anchor finality, 1000:1 hashpower, Phase 2 ----
    print("=== Scenario 4: Anchoring Finality, 1000:1 Hashpower, Phase 2 ===")
    print("  Anchoring adds finality via Monero — blocks can't be reorged once")
    print("  their Monero anchor has sufficient confirmations.")
    print("  Fork choice is still block_rank(). Anchoring is a security overlay.")
    print("  All rewards in finalized blocks are protected from reorg —")
    print("  canonical coinbase, uncle payouts, and inclusion bonuses.")
    print()
    config4 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_p2pools=1,
        consensus_mode=ConsensusMode.ANCHOR,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        difficulty_ratio=1.0,
        seed=42,
    )
    result4 = run_simulation(config4)
    print_results(result4)

    print()
    print()

    # ---- Scenario 5: Multiple p2pools competing ----
    print("=== Scenario 5: Native Consensus, 3 p2pools, 1000:1 Hashpower ===")
    print("  Three p2pools with different --merge-mine addresses compete.")
    print("  Each has 1/3 of the merge hashpower.")
    print("  Competition between p2pools distributes canonical slots.")
    print()
    config5 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_p2pools=3,
        consensus_mode=ConsensusMode.NATIVE,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        seed=42,
    )
    result5 = run_simulation(config5)
    print_results(result5)

    print()
    print()

    # ---- Scenario 6: Anchor finality, multiple p2pools ----
    print("=== Scenario 6: Anchoring Finality, 3 p2pools, 1000:1 Hashpower ===")
    print("  Multiple p2pools + finality + native miner.")
    print("  All rewards in finalized blocks are permanent —")
    print("  canonical coinbase, uncle payouts, inclusion bonuses.")
    print("  Even dominant merge miners can't steal finalized rewards.")
    print()
    config6 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_p2pools=3,
        consensus_mode=ConsensusMode.ANCHOR,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        difficulty_ratio=1.0,
        seed=42,
    )
    result6 = run_simulation(config6)
    print_results(result6)

    print()
    print()

    # ---- Scenario 7: Reorg attack — anchoring vs no anchoring ----
    print("=== Scenario 7: Reorg Attack — Anchoring Finality Protection ===")
    print("  Simulates an actual reorg: attacker builds a secret fork from")
    print("  height 2 and tries to replace the canonical chain.")
    print("  Without anchoring: attacker's better hashpower wins the reorg.")
    print("  With anchoring: finalized blocks are immovable — the attacker")
    print("  can only replace unfinalized blocks.")
    print()

    seed = 12345

    # ---- Run without anchoring ----
    print("  --- Native Consensus (no anchoring) ---")
    reorg_native = simulate_reorg_attack(
        native_hashpower=500_000.0,
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.NATIVE,
        attacker_hashpower=5_000_000.0,
        seed=seed,
    )
    print(f"    Attacker fork accepted: {reorg_native.attacker_fork_accepted}")
    print(f"    Blocks replaced: {len(reorg_native.blocks_replaced)} (heights {reorg_native.blocks_replaced})")
    print(f"    Blocks protected: {len(reorg_native.blocks_protected)}")
    print(f"    Original rewards: {reorg_native.original_rewards}")
    print(f"    Rewards lost:     {reorg_native.rewards_lost}")
    print(f"    Rewards gained:   {reorg_native.rewards_gained}")
    print(f"    Net redistribution: {reorg_native.net_redistribution}")
    print()

    # ---- Run WITH anchoring ----
    print("  --- Anchoring Finality ---")
    reorg_anchor = simulate_reorg_attack(
        native_hashpower=500_000.0,
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.ANCHOR,
        anchor_min_confirmations=2,
        attacker_hashpower=5_000_000.0,
        seed=seed,
    )
    print(f"    Attacker fork accepted: {reorg_anchor.attacker_fork_accepted}")
    print(f"    Finalized blocks: {len(reorg_anchor.finalized_blocks)}")
    print(f"    Blocks replaced: {len(reorg_anchor.blocks_replaced)} (heights {reorg_anchor.blocks_replaced})")
    print(f"    Blocks protected: {len(reorg_anchor.blocks_protected)} (heights {reorg_anchor.blocks_protected})")
    print(f"    Original rewards: {reorg_anchor.original_rewards}")
    print(f"    Rewards lost:     {reorg_anchor.rewards_lost}")
    print(f"    Rewards gained:   {reorg_anchor.rewards_gained}")
    print(f"    Net redistribution: {reorg_anchor.net_redistribution}")
    print()

    print(f"  Key insight: Without anchoring, the attacker can reorg and replace")
    print(f"  blocks, redistributing rewards from the original miners to the")
    print(f"  attacker. With anchoring, finalized blocks are immovable — the")
    print(f"  attacker cannot steal rewards from finalized blocks. The anchor")
    print(f"  borrows Monero's cumulative difficulty to secure DarkWow rewards.")
    print()
    print("=" * 72)
    print()
    print()

    # ---- Scenario 8: Caribina finality, 1000:1 hashpower ----
    print("=== Scenario 8: Caribina Finality (Arweave), 1000:1 Hashpower ===")
    print("  Caribina provides finality via Arweave proof-of-storage.")
    print("  Unlike Monero anchoring, Caribina works WITHOUT p2pool —")
    print("  native miners get the same finality guarantees as merge miners.")
    print("  Anchors settle after just 1 DarkWow block (~2 min vs ~6 min).")
    print()
    config8 = SimulationConfig(
        native_hashpower=1_000.0,
        merge_hashpower=1_000_000.0,
        num_p2pools=1,
        consensus_mode=ConsensusMode.CARIBINA,
        num_slots=200,
        target_block_time=120.0,
        uncle_phase="phase2",
        caribina_settle_blocks=1,
        seed=42,
    )
    result8 = run_simulation(config8)
    print_results(result8)

    print()
    print()

    # ---- Scenario 9: Reorg attack — NATIVE vs ANCHOR vs CARIBINA ----
    print("=== Scenario 9: Reorg Attack — Three-Way Finality Comparison ===")
    print("  Same reorg scenario across all three consensus modes:")
    print("  NATIVE:   Attacker can replace ALL blocks (no finality)")
    print("  ANCHOR:   Monero-finalized blocks are protected (needs p2pool)")
    print("  CARIBINA: Arweave-finalized blocks are protected (no p2pool needed)")
    print("  Key difference: Caribina protects native miners who don't merge mine.")
    print()

    seed = 12345

    # --- Native (no finality) ---
    print("  --- NATIVE Consensus (no finality) ---")
    reorg_native2 = simulate_reorg_attack(
        native_hashpower=500_000.0,
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.NATIVE,
        attacker_hashpower=5_000_000.0,
        seed=seed,
    )
    print(f"    Attacker fork accepted: {reorg_native2.attacker_fork_accepted}")
    print(f"    Blocks replaced: {len(reorg_native2.blocks_replaced)} (heights {reorg_native2.blocks_replaced})")
    print(f"    Blocks protected: {len(reorg_native2.blocks_protected)}")
    print(f"    Rewards lost:     {reorg_native2.rewards_lost}")
    print(f"    Rewards gained:   {reorg_native2.rewards_gained}")
    print()

    # --- ANCHOR (Monero finality) ---
    print("  --- ANCHOR Consensus (Monero finality) ---")
    reorg_anchor2 = simulate_reorg_attack(
        native_hashpower=500_000.0,
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.ANCHOR,
        anchor_min_confirmations=2,
        attacker_hashpower=5_000_000.0,
        seed=seed,
    )
    print(f"    Attacker fork accepted: {reorg_anchor2.attacker_fork_accepted}")
    print(f"    Finalized blocks: {len(reorg_anchor2.finalized_blocks)}")
    print(f"    Blocks replaced: {len(reorg_anchor2.blocks_replaced)} (heights {reorg_anchor2.blocks_replaced})")
    print(f"    Blocks protected: {len(reorg_anchor2.blocks_protected)} (heights {reorg_anchor2.blocks_protected})")
    print(f"    Rewards lost:     {reorg_anchor2.rewards_lost}")
    print(f"    Rewards gained:   {reorg_anchor2.rewards_gained}")
    print()

    # --- CARIBINA (Arweave finality) ---
    print("  --- CARIBINA Consensus (Arweave finality) ---")
    reorg_caribina2 = simulate_reorg_attack(
        native_hashpower=500_000.0,
        merge_hashpower=500_000.0,
        chain_length=6,
        reorg_from_height=2,
        consensus_mode=ConsensusMode.CARIBINA,
        attacker_hashpower=5_000_000.0,
        seed=seed,
    )
    print(f"    Attacker fork accepted: {reorg_caribina2.attacker_fork_accepted}")
    print(f"    Finalized blocks: {len(reorg_caribina2.finalized_blocks)}")
    print(f"    Blocks replaced: {len(reorg_caribina2.blocks_replaced)} (heights {reorg_caribina2.blocks_replaced})")
    print(f"    Blocks protected: {len(reorg_caribina2.blocks_protected)} (heights {reorg_caribina2.blocks_protected})")
    print(f"    Rewards lost:     {reorg_caribina2.rewards_lost}")
    print(f"    Rewards gained:   {reorg_caribina2.rewards_gained}")
    print()

    print(f"  Key insight: NATIVE mode gives zero protection — attacker replaces")
    print(f"  everything. ANCHOR mode protects blocks backed by Monero cumulative")
    print(f"  difficulty — but only merge-miners get this. CARIBINA mode protects")
    print(f"  ALL blocks (native and merge) via Arweave, a completely independent")
    print(f"  proof-of-storage consensus mechanism. No p2pool or Monero required.")
    print(f"  This makes Caribina the universal finality layer for DarkWow.")
    print()
    print("=" * 72)


if __name__ == "__main__":
    main()
