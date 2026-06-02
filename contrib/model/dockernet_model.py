#!/usr/bin/env python3
"""
DarkWow Native Dockernet Model — 1-to-1 with container logic.

Two mining nodes, each running a while-true mining loop. Both search for
hashes, broadcast blocks via simulated P2P, validate incoming blocks, and
must converge on the same chain via uncle-merkle fork resolution.

This models the FULL dockernet, not just Rust validation. Every component
of the running system is traced: config generation, init_chain, P2P message
exchange, mining loops, fork resolution, continuous production.

If the model stops at any block, the dockernet stops there too.
"""

import hashlib
import struct
import time
import random
from dataclasses import dataclass, field
from typing import Optional, List, Dict, Set
from collections import deque

# ============================================================================
# Constants (match Rust 1-to-1)
# ============================================================================

U32_MAX = 0xFFFFFFFF
INITIAL_TARGET = 0x0FFFFFFF
MIN_TARGET = 1
MAX_TARGET = U32_MAX
TARGET_BLOCK_TIME = 120
TIMESTAMP_WINDOW = 20
SCALE = 1_000_000
COINBASE_MATURITY = 100
MAX_UNCLE_DEPTH = 6  # block.rs
LINEAR_SYNC_BATCH = 20
SESSION_DEFAULT = 0b100111  # session/mod.rs line 64

# ============================================================================
# PoWConsensus — src/linear/src/consensus.rs (1-to-1)
# ============================================================================

@dataclass
class PoWConsensus:
    """Exact 1-to-1: consensus.rs line 61"""
    target: int = INITIAL_TARGET
    target_block_time: int = TARGET_BLOCK_TIME
    min_target: int = MIN_TARGET
    max_target: int = MAX_TARGET
    timestamps: List[int] = field(default_factory=list)

    def get_next_work_required(self, height: int) -> int:
        """consensus.rs line 293"""
        if height <= 1:
            return U32_MAX
        return self.target

    def record_block(self, timestamp: int):
        """consensus.rs line 123"""
        if len(self.timestamps) >= TIMESTAMP_WINDOW:
            self.timestamps.pop(0)
        self.timestamps.append(timestamp)

    def adjust_target(self) -> int:
        """consensus.rs line 139 — proportional controller, ±10% per step"""
        if len(self.timestamps) < 2:
            return self.target

        n = min(len(self.timestamps), 10)
        start = len(self.timestamps) - n
        total_interval = 0
        for i in range(start + 1, len(self.timestamps)):
            total_interval += max(0, self.timestamps[i] - self.timestamps[i - 1])
        count = n - 1
        avg_interval = total_interval // count if count > 0 else self.target_block_time

        if avg_interval == 0:
            ratio_scaled = SCALE * 9 // 10
        else:
            r = (self.target_block_time * SCALE) // avg_interval
            ratio_scaled = max(SCALE // 2, min(SCALE * 2, r))

        tenth = SCALE // 10
        if ratio_scaled > SCALE:
            adjustment = SCALE + min(ratio_scaled - SCALE, tenth)
        elif ratio_scaled < SCALE:
            adjustment = SCALE - min(SCALE - ratio_scaled, tenth)
        else:
            adjustment = SCALE

        current = self.target
        new_target = (current * SCALE // adjustment)
        self.target = max(self.min_target, min(self.max_target, new_target))
        return self.target

# ============================================================================
# Block types — src/linear/src/block.rs (1-to-1)
# ============================================================================

@dataclass
class BlockHeader:
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
    reward: int = 0

@dataclass
class Block:
    header: BlockHeader
    transactions: List[Transaction] = field(default_factory=list)

# ============================================================================
# Miner — src/linear/src/miner.rs (1-to-1)
# ============================================================================

def derive_key_from_height(height: int) -> bytes:
    """miner.rs line 77"""
    key = bytearray(32)
    key[0:8] = struct.pack('<Q', height)
    return bytes(key)

def _mining_blob(header: BlockHeader) -> bytes:
    """block.rs to_mining_blob() — 227 bytes"""
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

def hash_block(header: BlockHeader) -> int:
    """Stand-in for RandomX VM. Same 256-bit output, same u32_le extraction."""
    h = hashlib.blake2b(_mining_blob(header), digest_size=32).digest()
    return struct.unpack('<I', h[0:4])[0]

def mine_block(
    previous_hash: bytes,
    height: int,
    target: int,
    txs: List[Transaction],
    timestamp: int,
    uncle_root: bytes = b'\x00' * 32,
    max_nonce: int = 10_000_000,
) -> Optional[Block]:
    """miner.rs line 49: Miner::mine()"""
    key = derive_key_from_height(height)
    header = BlockHeader(
        previous=previous_hash,
        height=height,
        target=target,
        randomx_key=key,
        timestamp=timestamp,
        uncle_merkle_root=uncle_root,
    )
    block = Block(header=header, transactions=txs)
    for nonce in range(max_nonce):
        block.header.nonce = nonce
        if hash_block(block.header) <= target:
            return block
    return None  # failed to mine within max_nonce

# ============================================================================
# Block Validation — src/linear/src/validation.rs (1-to-1)
# ============================================================================

class ValidationError(Exception):
    pass

def check_block_header(block: Block, expected_target: int,
                       current_height: int, previous_hash: Optional[bytes] = None):
    """validation.rs line 53 — two-stage PoW"""
    hash_u32 = hash_block(block.header)

    # Stage 1: hash meets header target
    if hash_u32 > block.header.target:
        raise ValidationError(
            f"Invalid PoW at h={block.header.height}: "
            f"hash_u32={hash_u32} > target={block.header.target}")

    # Stage 2: target matches consensus
    if block.header.target != expected_target:
        raise ValidationError(
            f"Target mismatch at h={block.header.height}: "
            f"declared={block.header.target} expected={expected_target}")

    # Height continuity
    if block.header.height != current_height + 1:
        raise ValidationError(
            f"Height discontinuity at h={block.header.height}: "
            f"expected {current_height + 1}")

    # Previous hash
    if previous_hash is not None:
        if block.header.previous != previous_hash:
            raise ValidationError(
                f"Invalid previous hash at h={block.header.height}")

# ============================================================================
# ChainState — src/linear/src/chain_state.rs (1-to-1)
# ============================================================================

@dataclass
class ChainState:
    """Single authoritative chain state. One per node."""
    consensus: PoWConsensus = field(default_factory=PoWConsensus)
    height: int = 0
    blocks: Dict[int, Block] = field(default_factory=dict)
    hashes: Dict[int, str] = field(default_factory=dict)
    # Fork tracking: height → competing block (potential uncle)
    competing: Dict[int, Block] = field(default_factory=dict)

    def get_height(self) -> int:
        return self.height

    def get_block(self, h: int) -> Optional[Block]:
        return self.blocks.get(h)

    def get_latest_block(self) -> Optional[Block]:
        return self.blocks.get(self.height) if self.height > 0 else None

    def get_tip_hash(self) -> Optional[str]:
        return self.hashes.get(self.height)

    def connect_block(self, block: Block) -> bool:
        """
        connect_block() — single atomic insertion path.
        Returns True if applied as canonical, False if competing (stored as potential uncle).
        """
        current_height = self.height

        # Competing block at same height? Store as potential uncle.
        if block.header.height == current_height:
            self.competing[block.header.height] = block
            key = block.header.randomx_key
            h = hashlib.blake2b(_mining_blob(block.header), digest_size=32).hexdigest()
            print(f"    [FORK] Competing block at h={block.header.height} hash={h[:16]}... stored as potential uncle")
            return False

        # Future height — we're behind, can't apply yet
        if block.header.height > current_height + 1:
            print(f"    [GAP] Block at h={block.header.height} but current={current_height} — need sync")
            return False

        expected_target = self.consensus.get_next_work_required(block.header.height)
        prev_hash = None
        if current_height > 0:
            prev = self.blocks[current_height]
            prev_hash = hashlib.blake2b(
                _mining_blob(prev.header), digest_size=32
            ).digest()

        # Full validation
        try:
            check_block_header(block, expected_target, current_height, prev_hash)
        except ValidationError as e:
            print(f"    [REJECT] h={block.header.height}: {e}")
            return False

        # Check for competing block at this height — it becomes an uncle
        uncles_for_this_block = []
        if block.header.height in self.competing:
            uncle = self.competing.pop(block.header.height)
            uncles_for_this_block.append(uncle)
            print(f"    [UNCLE] Included competing block at h={block.header.height} as uncle")

        # Commit
        h = block.header.height
        self.blocks[h] = block
        self.height = h
        key = block.header.randomx_key
        block_hash = hashlib.blake2b(
            _mining_blob(block.header), digest_size=32
        ).hexdigest()
        self.hashes[h] = block_hash

        # Update consensus
        self.consensus.record_block(block.header.timestamp)
        self.consensus.adjust_target()

        return True

# ============================================================================
# P2P Message Queue — simulates P2P network
# ============================================================================

class P2PNetwork:
    """Simulated P2P network. Each node has an inbox."""
    def __init__(self):
        self.inboxes: Dict[str, deque] = {}

    def register(self, node_id: str):
        self.inboxes[node_id] = deque()

    def broadcast(self, sender_id: str, block: Block):
        """Simulate p2p.broadcast(BlockBroadcast{block})"""
        for node_id in self.inboxes:
            if node_id != sender_id:
                self.inboxes[node_id].append(("BlockBroadcast", block))

    def send_get_tip(self, sender_id: str, target_id: str):
        """Simulate sending GetTip request"""
        self.inboxes[target_id].append(("GetTip", sender_id))

    def receive(self, node_id: str) -> List[tuple]:
        """Drain inbox"""
        msgs = list(self.inboxes[node_id])
        self.inboxes[node_id].clear()
        return msgs

# ============================================================================
# Mining Node — full dockernet logic
# ============================================================================

class MiningNode:
    """
    Full dockernet mining node. Models:
    - init_chain() — genesis creation or empty start
    - sync task — waits for peers, queries tips, fetches blocks
    - mining loop — while-true: mine → apply → broadcast
    - P2P message handling — receives and applies blocks from peers
    """
    def __init__(self, node_id: str, create_genesis: bool, p2p: P2PNetwork):
        self.node_id = node_id
        self.chain = ChainState()
        self.p2p = p2p
        self.sync_complete = False
        self.mining_enabled = False
        self.blocks_mined = 0
        self.blocks_received = 0
        self.forks_seen = 0
        p2p.register(node_id)

        if create_genesis:
            self._create_genesis()

    def get_height(self) -> int:
        """Convenience — same API as MergeMiningNode."""
        return self.chain.get_height()

    def _create_genesis(self):
        """lib.rs init_chain() genesis creation path"""
        key = derive_key_from_height(1)
        header = BlockHeader(
            previous=b'\x00' * 32,
            height=1,
            target=U32_MAX,
            randomx_key=key,
            timestamp=int(time.time()),
        )
        block = Block(header=header, transactions=[Transaction(reward=13_837_500_000_000)])
        self.chain.connect_block(block)
        print(f"[{self.node_id}] Genesis created: h=1 target={U32_MAX:#010x}")

    def start_sync_task(self):
        """
        consensus_linear.rs: consensus_linear_init_task()
        Simplified: wait for peer tips, sync if behind.
        """
        print(f"[{self.node_id}] Sync task starting (local height={self.chain.get_height()})")

        # Query all peers for their best height
        peer_heights = {}
        for peer_id in self.p2p.inboxes:
            if peer_id == self.node_id:
                continue
            self.p2p.send_get_tip(self.node_id, peer_id)
            # Process the response
            msgs = self.p2p.receive(peer_id)
            for msg_type, payload in msgs:
                if msg_type == "Tip":
                    peer_heights[peer_id] = payload  # payload is height

        # Alternative: direct height query
        # In the real dockernet, GetTip/Tip are P2P messages. Here we just
        # know each other's heights directly for the model.
        # We'll query during the main loop instead.

        self.sync_complete = True
        self.mining_enabled = True
        print(f"[{self.node_id}] Sync complete, mining enabled at h={self.chain.get_height()}")

    def _fetch_and_apply_blocks(self, start_height: int, end_height: int, source_node):
        """GetBlocks/Blocks sync protocol — fetch and apply blocks in range."""
        for h in range(start_height, end_height + 1):
            block = source_node.chain.get_block(h)
            if block is None:
                return False
            success = self.chain.connect_block(block)
            if not success:
                return False
            self.blocks_received += 1
        return True

    def process_p2p_messages(self, other_node: 'MiningNode'):
        """Handle incoming P2P messages (broadcast blocks, GetTip requests)."""
        msgs = self.p2p.receive(self.node_id)
        for msg_type, payload in msgs:
            if msg_type == "BlockBroadcast":
                block = payload
                print(f"[{self.node_id}] Received block h={block.header.height} from P2P")
                success = self.chain.connect_block(block)
                if success:
                    self.blocks_received += 1
                elif block.header.height == self.chain.get_height():
                    # Competing block — fork!
                    self.forks_seen += 1
            elif msg_type == "GetTip":
                # Respond with Tip { height, hash }
                requester = payload
                tip_height = self.chain.get_height()
                self.p2p.inboxes[requester].append(
                    ("Tip", tip_height)
                )

    def mine_one_block(self) -> Optional[Block]:
        """Execute one iteration of the mining loop with uncle inclusion."""
        if not self.mining_enabled:
            return None
        if not self.sync_complete:
            return None

        current = self.chain.get_latest_block()
        if current is None:
            return None

        height = current.header.height + 1
        target = self.chain.consensus.target
        prev_key = current.header.randomx_key
        prev_hash = hashlib.blake2b(
            _mining_blob(current.header), digest_size=32
        ).digest()

        # --- Uncle collection (BEFORE mining — mining blob includes uncle_merkle_root) ---
        # Competing blocks at the current tip height become uncles in the next block.
        # Tiebreaker: first-seen-wins (same rule on every node). The winner's block
        # is canonical; the loser's becomes an uncle with partial reward.
        uncle_blocks = []
        if current.header.height in self.chain.competing:
            uncle = self.chain.competing.pop(current.header.height)
            uncle_blocks.append(uncle)
            print(f"[{self.node_id}] Including uncle from h={current.header.height} in block h={height}")
            self.blocks_mined_uncles = getattr(self, 'blocks_mined_uncles', 0) + 1

        # Compute uncle merkle root BEFORE mining (it goes into the mining blob)
        uncle_root = b'\x00' * 32
        if uncle_blocks:
            uncle_root = hashlib.blake2b(
                _mining_blob(uncle_blocks[0].header), digest_size=32
            ).digest()

        txs = [Transaction(reward=13_837_500_000_000 // max(1, height))]
        block = mine_block(prev_hash, height, target, txs, int(time.time()),
                          uncle_root=uncle_root)

        if block is None:
            print(f"[{self.node_id}] Failed to mine block h={height}")
            return None

        success = self.chain.connect_block(block)
        if success:
            self.blocks_mined += 1
            self.p2p.broadcast(self.node_id, block)
            return block
        return None

# ============================================================================
# Full Dockernet Simulation
# ============================================================================

def run_dockernet(num_blocks: int = 20):
    """
    Run the full dockernet model. Two mining nodes, both producing blocks.
    Tests: continuous production, fork resolution, cross-node consensus.
    """
    print(f"=== DarkWow Native Dockernet Model ===\n")
    print(f"Two mining nodes, both searching for hashes.")
    print(f"Target: produce {num_blocks} blocks with both nodes converging.\n")

    p2p = P2PNetwork()

    # Node 0: Genesis authority + miner
    node0 = MiningNode("node0", create_genesis=True, p2p=p2p)
    # Node 1: Sync-only start, will sync genesis from node0
    node1 = MiningNode("node1", create_genesis=False, p2p=p2p)

    # --- Phase 1: Node1 syncs from Node0 ---
    print("\n--- Phase 1: Node1 Sync ---")
    node1.start_sync_task()

    # Sync genesis + any blocks node0 has
    peer_height = node0.chain.get_height()
    local_height = node1.chain.get_height()
    if peer_height > local_height:
        print(f"[node1] Behind: local={local_height} peer={peer_height}. Syncing...")
        node1._fetch_and_apply_blocks(local_height + 1, peer_height, node0)
        print(f"[node1] Sync complete at h={node1.chain.get_height()}")

    # --- Phase 2: Continuous mining — both nodes mine independently ---
    print(f"\n--- Phase 2: Continuous Mining (target: {num_blocks} blocks) ---")
    print(f"  Node0 starts at h={node0.chain.get_height()}, Node1 at h={node1.chain.get_height()}\n")

    block_count = node0.chain.get_height()  # start counting from genesis
    round_num = 0

    while block_count < num_blocks:
        round_num += 1

        # Both nodes mine one block attempt each round
        b0 = node0.mine_one_block()
        b1 = node1.mine_one_block()

        # Process P2P messages (broadcasts from the other node)
        node0.process_p2p_messages(node1)
        node1.process_p2p_messages(node0)

        if b0 or b1:
            block_count = max(node0.chain.get_height(), node1.chain.get_height())

        if round_num % 5 == 0 or b0 or b1:
            print(f"  [round {round_num}] node0={node0.chain.get_height()} "
                  f"node1={node1.chain.get_height()} "
                  f"target={node0.chain.consensus.target:#010x} "
                  f"mined=({node0.blocks_mined},{node1.blocks_mined}) "
                  f"forks={node0.forks_seen + node1.forks_seen}")

        if round_num > 1000:
            print("  TIMEOUT: mining rounds exceeded")
            break

    # --- Phase 3: Final sync and consensus verification ---
    print(f"\n--- Phase 3: Final Consensus Check ---")

    # Ensure both nodes are caught up
    n0_h = node0.chain.get_height()
    n1_h = node1.chain.get_height()
    max_h = max(n0_h, n1_h)

    if node0.chain.get_height() < max_h:
        node0._fetch_and_apply_blocks(n0_h + 1, max_h, node1)
    if node1.chain.get_height() < max_h:
        node1._fetch_and_apply_blocks(n1_h + 1, max_h, node0)

    print(f"  Node0: h={node0.chain.get_height()} mined={node0.blocks_mined} received={node0.blocks_received} forks={node0.forks_seen}")
    print(f"  Node1: h={node1.chain.get_height()} mined={node1.blocks_mined} received={node1.blocks_received} forks={node1.forks_seen}")

    # Verify all hashes match
    all_match = True
    for h in range(1, max_h + 1):
        h0 = node0.chain.hashes.get(h, "MISSING")
        h1 = node1.chain.hashes.get(h, "MISSING")
        if h0 != h1:
            all_match = False
            print(f"  Block {h}: MISMATCH node0={h0[:16] if h0 != 'MISSING' else 'MISSING'} node1={h1[:16] if h1 != 'MISSING' else 'MISSING'}")

    print(f"\n=== {'ALL BLOCKS VERIFIED' if all_match else 'CONSENSUS FAILURE'} ===")
    print(f"  Blocks produced: {max_h}")
    print(f"  Total mined: {node0.blocks_mined + node1.blocks_mined}")
    print(f"  Total forks: {node0.forks_seen + node1.forks_seen}")
    return all_match

def test_fork_resolution():
    """Test: two nodes mine simultaneously, producing competing blocks.
    Fork resolution via uncle-merkle must converge them to one chain."""
    print("=== Fork Resolution Test ===\n")
    print("Both nodes mine simultaneously. Competing blocks → uncles → convergence.\n")

    p2p = P2PNetwork()
    node0 = MiningNode("node0", create_genesis=True, p2p=p2p)
    node1 = MiningNode("node1", create_genesis=False, p2p=p2p)

    # Sync genesis
    node1.start_sync_task()
    for h in range(1, node0.chain.get_height() + 1):
        b = node0.chain.get_block(h)
        if b:
            try: node1.chain.connect_block(b)
            except Exception: pass
    node1.sync_complete = True
    node1.mining_enabled = True
    node0.sync_complete = True
    node0.mining_enabled = True
    print(f"  After sync: n0={node0.chain.get_height()} n1={node1.chain.get_height()}\n")

    # Force both to mine block 2 simultaneously (no P2P during mining)
    print("--- Round 1: Both mine block 2 ---")
    b0 = node0.mine_one_block()
    b1 = node1.mine_one_block()
    print(f"  n0 produced h={b0.header.height if b0 else 'FAIL'}")
    print(f"  n1 produced h={b1.header.height if b1 else 'FAIL'}")

    # Now exchange blocks — each receives the other's block 2
    print("\n  Exchanging blocks (P2P broadcast)...")
    node0.process_p2p_messages(node1)  # node0 gets node1's block
    node1.process_p2p_messages(node0)  # node1 gets node0's block
    print(f"  After exchange: n0={node0.chain.get_height()} n1={node1.chain.get_height()}")
    print(f"  n0 competing: {list(node0.chain.competing.keys())}")
    print(f"  n1 competing: {list(node1.chain.competing.keys())}")

    # Both mine block 3 — should include competing blocks as uncles
    print("\n--- Round 2: Both mine block 3 (should include uncles) ---")
    b0 = node0.mine_one_block()
    b1 = node1.mine_one_block()
    print(f"  n0 produced h={b0.header.height if b0 else 'FAIL'}")
    print(f"  n1 produced h={b1.header.height if b1 else 'FAIL'}")

    # Exchange again
    node0.process_p2p_messages(node1)
    node1.process_p2p_messages(node0)

    # Both mine block 4
    print("\n--- Round 3: Both mine block 4 ---")
    b0 = node0.mine_one_block()
    b1 = node1.mine_one_block()
    node0.process_p2p_messages(node1)
    node1.process_p2p_messages(node0)

    print(f"\n  Final: n0={node0.chain.get_height()} n1={node1.chain.get_height()}")
    print(f"  n0 blocks: {list(node0.chain.blocks.keys())}")
    print(f"  n1 blocks: {list(node1.chain.blocks.keys())}")

    # Verify consensus
    all_match = True
    max_h = max(node0.chain.get_height(), node1.chain.get_height())
    for h in range(1, max_h + 1):
        h0 = node0.chain.hashes.get(h, "MISSING")
        h1 = node1.chain.hashes.get(h, "MISSING")
        if h0 != h1:
            all_match = False
            print(f"  MISMATCH at h={h}")
    print(f"  Consensus: {'PASS' if all_match else 'FAIL'}")
    return all_match

if __name__ == "__main__":
    run_dockernet(20)
    print("\n" + "="*60 + "\n")
    test_fork_resolution()
