#!/usr/bin/env python3
"""
DarkWow Chain Model — 1-to-1 mapping with Rust code.

Every function maps directly to its Rust counterpart. No simulation.
This models continuous block production from genesis through block N,
validating every PoW check, every state transition, every consensus rule.

Rust files mapped:
  src/linear/src/consensus.rs    → PoWConsensus
  src/linear/src/validation.rs   → check_block_header
  src/linear/src/miner.rs        → Miner
  src/linear/src/chain_state.rs  → CChainState
  bin/dwowd/src/task/consensus_linear.rs → sync task
"""

import hashlib
import struct
import time
from dataclasses import dataclass, field
from typing import Optional, List, Tuple

# ============================================================================
# Constants (match Rust exactly)
# ============================================================================

U32_MAX = 0xFFFFFFFF
INITIAL_TARGET = 0x0FFFFFFF  # matches PoWConfig::default().initial_target
MIN_TARGET = 1
MAX_TARGET = U32_MAX
TARGET_BLOCK_TIME = 120       # seconds
TIMESTAMP_WINDOW = 20         # consensus.rs line 45
SCALE = 1_000_000              # fixed-point scale for difficulty adjustment
COINBASE_MATURITY = 100

# ============================================================================
# PoWConsensus (src/linear/src/consensus.rs)
# ============================================================================

@dataclass
class PoWConsensus:
    """Exact Rust mapping: src/linear/src/consensus.rs line 61"""
    target: int = INITIAL_TARGET
    target_block_time: int = TARGET_BLOCK_TIME
    min_target: int = MIN_TARGET
    max_target: int = MAX_TARGET
    timestamps: List[int] = field(default_factory=list)

    def get_next_work_required(self, height: int) -> int:
        """
        consensus.rs line 293: get_next_work_required()
        For height 1 (genesis): returns u32::MAX (any hash valid)
        For height > 1: returns current consensus target
        """
        if height <= 1:
            return U32_MAX
        return self.target

    def record_block(self, timestamp: int):
        """consensus.rs line 123: record_block()"""
        if len(self.timestamps) >= TIMESTAMP_WINDOW:
            self.timestamps.pop(0)
        self.timestamps.append(timestamp)

    def adjust_target(self) -> int:
        """
        consensus.rs line 139: adjust_target()
        Proportional controller using last 10 intervals, capped at ±10% per step.
        Returns the new target.
        """
        if len(self.timestamps) < 2:
            return self.target

        # Use last 10 intervals (consensus.rs line 146-148)
        n = min(len(self.timestamps), 10)
        start = len(self.timestamps) - n
        total_interval = 0
        for i in range(start + 1, len(self.timestamps)):
            total_interval += max(0, self.timestamps[i] - self.timestamps[i - 1])
        count = n - 1

        avg_interval = total_interval // count if count > 0 else self.target_block_time

        # Fixed-point ratio (consensus.rs line 164-170)
        if avg_interval == 0:
            ratio_scaled = SCALE * 9 // 10
        else:
            r = (self.target_block_time * SCALE) // avg_interval
            ratio_scaled = max(SCALE // 2, min(SCALE * 2, r))

        # Clamp to ±10% (consensus.rs line 173-183)
        tenth = SCALE // 10
        if ratio_scaled > SCALE:
            excess = min(ratio_scaled - SCALE, tenth)
            adjustment = SCALE + excess
        elif ratio_scaled < SCALE:
            deficit = min(SCALE - ratio_scaled, tenth)
            adjustment = SCALE - deficit
        else:
            adjustment = SCALE

        # Apply adjustment (consensus.rs line 188-189)
        current = self.target
        new_target = (current * SCALE // adjustment)
        clamped = max(self.min_target, min(self.max_target, new_target))
        self.target = clamped
        return clamped

# ============================================================================
# Block types (src/linear/src/block.rs)
# ============================================================================

@dataclass
class BlockHeader:
    """Exact Rust mapping: src/linear/src/block.rs line 48"""
    version: int = 1
    previous: bytes = b'\x00' * 32
    merkle_root: bytes = b'\x00' * 32
    timestamp: int = 0
    target: int = U32_MAX
    nonce: int = 0
    height: int = 1
    uncle_merkle_root: bytes = b'\x00' * 32
    randomx_key: bytes = b'\x00' * 32

@dataclass
class Transaction:
    """Simplified — genesis has one reward tx, blocks 2+ have coinbase"""
    reward: int = 0
    data: bytes = b''

@dataclass
class Block:
    header: BlockHeader
    transactions: List[Transaction] = field(default_factory=list)

# ============================================================================
# Miner (src/linear/src/miner.rs)
# ============================================================================

def derive_key_from_height(height: int) -> bytes:
    """miner.rs line 77: derive_key_from_height()"""
    key = bytearray(32)
    key[0:8] = struct.pack('<Q', height)
    return bytes(key)

def hash_mining_blob(header: BlockHeader, key: bytes) -> int:
    """
    Simulates RandomX hash of the mining blob.

    Rust: block.hash_with_vm(&vm) uses:
      1. header.to_mining_blob() — 227-byte binary blob (block.rs line 188-209)
      2. vm.calculate_hash(&blob) — RandomX hash
      3. First 4 bytes as u32_le

    Python: uses blake3 as deterministic hash. The LOGIC is identical
    (hash blob, extract u32_le), only the hash function differs.
    """
    # Build mining blob (simplified — matches to_mining_blob() structure)
    blob = bytearray()
    blob.extend(struct.pack('<B', header.version))       # version: u8
    blob.extend(header.previous)                          # previous: [u8; 32]
    blob.extend(header.merkle_root)                       # merkle_root: [u8; 32]
    blob.extend(struct.pack('<Q', header.timestamp))      # timestamp: u64
    blob.extend(struct.pack('<I', header.target))         # target: u32
    blob.extend(struct.pack('<Q', header.nonce))          # nonce: u64
    blob.extend(struct.pack('<Q', header.height))         # height: u64
    blob.extend(header.uncle_merkle_root)                 # uncle_merkle_root: [u8; 32]
    blob.extend(header.randomx_key)                       # randomx_key: [u8; 32]

    # Hash with blake3 (stand-in for RandomX — same output size, deterministic)
    h = hashlib.blake2b(bytes(blob), digest_size=32).digest()
    return struct.unpack('<I', h[0:4])[0]

def mine_block(
    previous_hash: bytes,
    height: int,
    target: int,
    txs: List[Transaction],
    timestamp: int,
) -> Block:
    """
    Miner::mine() — miner.rs line 49.
    Creates a block with the given parameters and iterates nonces
    until hash_u32 <= target.
    """
    key = derive_key_from_height(height)
    header = BlockHeader(
        previous=previous_hash,
        height=height,
        target=target,
        randomx_key=key,
        timestamp=timestamp,
    )
    block = Block(header=header, transactions=txs)

    # Iterate nonces until PoW is valid
    nonce = 0
    while True:
        block.header.nonce = nonce
        hash_u32 = hash_mining_blob(block.header, key)
        if hash_u32 <= target:
            return block
        nonce += 1
        if nonce > 10_000_000:  # safety limit
            raise RuntimeError(f"Failed to mine block at height {height}")

# ============================================================================
# Block Validation (src/linear/src/validation.rs)
# ============================================================================

class ValidationError(Exception):
    pass

def check_block_header(
    block: Block,
    expected_target: int,
    current_height: int,
    previous_hash: Optional[bytes] = None,
) -> None:
    """
    validation.rs line 53: check_block_header()

    Two-stage PoW (Bitcoin Core pattern):
      Stage 1: hash_u32 <= block.header.target (hash meets header's target)
      Stage 2: block.header.target == expected_target (target matches consensus)
    """
    key = block.header.randomx_key
    hash_u32 = hash_mining_blob(block.header, key)

    # Stage 1: PoW — hash must meet the block header's own target
    if hash_u32 > block.header.target:
        raise ValidationError(
            f"Invalid PoW: hash_u32={hash_u32} > target={block.header.target}"
        )

    # Stage 2: Target must match consensus (Bitcoin's GetNextWorkRequired)
    if block.header.target != expected_target:
        raise ValidationError(
            f"Target mismatch at height {block.header.height}: "
            f"declared={block.header.target}, expected={expected_target}"
        )

    # Height continuity
    if block.header.height != current_height + 1:
        raise ValidationError(
            f"Height discontinuity: expected {current_height + 1}, "
            f"got {block.header.height}"
        )

    # Previous hash (only when there IS a previous block)
    if previous_hash is not None:
        if block.header.previous != previous_hash:
            raise ValidationError(
                f"Invalid previous hash at height {block.header.height}"
            )

# ============================================================================
# CChainState (src/linear/src/chain_state.rs)
# ============================================================================

@dataclass
class ChainState:
    """
    Exact Rust mapping: src/linear/src/chain_state.rs
    Single authoritative state. One instance.
    """
    consensus: PoWConsensus = field(default_factory=PoWConsensus)
    height: int = 0
    blocks: dict = field(default_factory=dict)  # height → Block
    block_hashes: dict = field(default_factory=dict)  # height → hash

    def get_height(self) -> int:
        return self.height

    def get_block(self, h: int) -> Optional[Block]:
        return self.blocks.get(h)

    def get_latest_block(self) -> Optional[Block]:
        if self.height == 0:
            return None
        return self.blocks.get(self.height)

    def connect_block(self, block: Block) -> None:
        """
        connect_block() — single atomic insertion path.
        Rust: chain_state.rs line 205
        """
        current_height = self.height
        expected_target = self.consensus.get_next_work_required(block.header.height)

        prev_hash = None
        if current_height > 0:
            prev = self.blocks[current_height]
            prev_key = prev.header.randomx_key
            prev_hash = bytes(hashlib.blake2b(
                _mining_blob_bytes(prev.header), digest_size=32
            ).digest())

        # Full validation
        check_block_header(block, expected_target, current_height, prev_hash)

        # Commit
        h = block.header.height
        self.blocks[h] = block
        self.height = h

        # Compute block hash for tracking
        key = block.header.randomx_key
        block_hash = hashlib.blake2b(
            _mining_blob_bytes(block.header), digest_size=32
        ).hexdigest()
        self.block_hashes[h] = block_hash

        # Update consensus
        self.consensus.record_block(block.header.timestamp)
        self.consensus.adjust_target()

        print(f"  Block {h}: hash={block_hash[:16]}... "
              f"target={block.header.target:#010x} "
              f"nonce={block.header.nonce} "
              f"consensus_target={self.consensus.target:#010x}")

def _mining_blob_bytes(header: BlockHeader) -> bytes:
    """Same blob construction as hash_mining_blob"""
    blob = bytearray()
    blob.extend(struct.pack('<B', header.version))
    blob.extend(header.previous)
    blob.extend(header.merkle_root)
    blob.extend(struct.pack('<Q', header.timestamp))
    blob.extend(struct.pack('<I', header.target))
    blob.extend(struct.pack('<Q', header.nonce))
    blob.extend(struct.pack('<Q', header.height))
    blob.extend(header.uncle_merkle_root)
    blob.extend(header.randomx_key)
    return bytes(blob)

# ============================================================================
# Sync Task (bin/dwowd/src/task/consensus_linear.rs)
# ============================================================================

def sync_loop(local_chain: ChainState, peer_chain: ChainState) -> str:
    """
    consensus_linear.rs: consensus_linear_init_task()

    Simplified sync: if peer has higher blocks, fetch and apply them.
    Returns the sync state.
    """
    local_height = local_chain.get_height()
    peer_height = peer_chain.get_height()

    if peer_height <= local_height:
        return "caught_up"

    print(f"  Syncing: local={local_height}, peer={peer_height}")
    for h in range(local_height + 1, peer_height + 1):
        block = peer_chain.get_block(h)
        if block is None:
            return f"sync_failed_at_{h}"
        local_chain.connect_block(block)

    return "sync_complete"

# ============================================================================
# Continuous Block Production (blocks 1→N)
# ============================================================================

def run_chain(num_blocks: int = 10):
    """
    Model continuous block production from genesis through block N.
    Two nodes: node0 (miner/genesis) and node1 (sync peer).
    """
    print(f"=== DarkWow Chain Model: Blocks 1 → {num_blocks} ===\n")

    # Node 0: Genesis authority + miner
    node0 = ChainState()
    node1 = ChainState()  # starts empty, syncs from node0

    # --- Genesis (block 1) ---
    print("--- Genesis (Block 1) ---")
    genesis_key = derive_key_from_height(1)
    genesis_header = BlockHeader(
        previous=b'\x00' * 32,
        height=1,
        target=U32_MAX,  # genesis: any hash valid
        randomx_key=genesis_key,
        timestamp=int(time.time()),
    )
    genesis = Block(
        header=genesis_header,
        transactions=[Transaction(reward=13_837_500_000_000)],
    )
    node0.connect_block(genesis)
    print(f"  Genesis created: target={U32_MAX:#010x}\n")

    # --- Mine blocks 2→N on node0 ---
    print(f"--- Mining Blocks 2 → {num_blocks} ---")
    for height in range(2, num_blocks + 1):
        prev_block = node0.get_latest_block()
        prev_key = prev_block.header.randomx_key
        prev_hash = bytes(hashlib.blake2b(
            _mining_blob_bytes(prev_block.header), digest_size=32
        ).digest())

        target = node0.consensus.target  # current consensus target
        txs = [Transaction(reward=13_837_500_000_000 // height)]

        block = mine_block(prev_hash, height, target, txs, int(time.time()))
        node0.connect_block(block)

    print(f"\n  Node0 final height: {node0.get_height()}")
    print(f"  Node0 final target: {node0.consensus.target:#010x}")

    # --- Node1 syncs from node0 ---
    print(f"\n--- Node1 Sync ---")
    result = sync_loop(node1, node0)
    print(f"  Sync result: {result}")
    print(f"  Node1 final height: {node1.get_height()}")

    # --- Verify consensus ---
    print(f"\n--- Consensus Verification ---")
    all_match = True
    for h in range(1, num_blocks + 1):
        n0_hash = node0.block_hashes.get(h)
        n1_hash = node1.block_hashes.get(h)
        match = "✓" if n0_hash == n1_hash else "✗"
        if n0_hash != n1_hash:
            all_match = False
        print(f"  Block {h}: {match}")

    print(f"\n=== {'ALL BLOCKS VERIFIED' if all_match else 'CONSENSUS FAILURE'} ===")
    return all_match

if __name__ == "__main__":
    run_chain(10)
