#!/usr/bin/env python3
"""
Exhaustive Block Production Model — Two Mining Nodes.

Models every path through block production with two independent miners.
No simulation. Every function maps 1-to-1 with Rust. This model must
pass ALL scenarios before any Rust code is written.

Verification targets:
  A. Two nodes with the same chain always compute the same expected target.
  B. Target is derived from canonical chain blocks, never from an accumulator.
  C. Mining target = validation target for the same height on the same chain.
  D. Two miners converge on the same chain (or the model explains why not).
  E. Block hashes match between nodes at every shared height.
  F. Continuous production works indefinitely.
"""

import hashlib, struct, time
from dataclasses import dataclass, field
from typing import Optional, List, Dict

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

# ============================================================================
# PoWConsensus — difficulty adjustment (pure functions, no mutable state)
# ============================================================================

def initial_target() -> int:
    """Consensus starts here. Genesis uses U32_MAX."""
    return INITIAL_TARGET

def compute_adjustment(timestamps: List[int], current_target: int,
                       target_block_time: int, min_t: int, max_t: int) -> int:
    """
    Pure function. Same logic as Rust consensus.rs adjust_target().
    Takes a timestamp window and current target, returns adjusted target.
    No mutable state. Deterministic for the same inputs.
    """
    if len(timestamps) < 2:
        return current_target
    n = min(len(timestamps), 10)
    recent = timestamps[-n:]
    total_interval = 0
    for i in range(1, len(recent)):
        total_interval += max(0, recent[i] - recent[i - 1])
    count = len(recent) - 1
    avg_interval = total_interval // count if count > 0 else target_block_time

    if avg_interval == 0:
        ratio_scaled = SCALE * 9 // 10
    else:
        r = (target_block_time * SCALE) // avg_interval
        ratio_scaled = max(SCALE // 2, min(SCALE * 2, r))

    tenth = SCALE // 10
    if ratio_scaled > SCALE:
        adjustment = SCALE + min(ratio_scaled - SCALE, tenth)
    elif ratio_scaled < SCALE:
        adjustment = SCALE - min(SCALE - ratio_scaled, tenth)
    else:
        adjustment = SCALE

    new_target = (current_target * SCALE // adjustment)
    return max(min_t, min(max_t, new_target))

def get_next_work_required(chain_blocks: Dict[int, 'Block'], height: int,
                           target_block_time=TARGET_BLOCK_TIME,
                           min_t=MIN_TARGET, max_t=MAX_TARGET) -> int:
    """
    THE key function. Bitcoin's GetNextWorkRequired.
    Computes target from CANONICAL CHAIN BLOCKS only.
    No accumulator. No mutable state. Fully deterministic.

    For height 1 (genesis): returns u32::MAX.
    For height > 1: walks chain from genesis through height-1,
    recomputing target from each block's timestamp in order.
    """
    if height <= 1:
        return U32_MAX

    target = INITIAL_TARGET
    timestamps: List[int] = []

    for h in range(1, height):
        if h not in chain_blocks:
            return target  # chain incomplete, return current best
        block = chain_blocks[h]
        timestamps.append(block.header.timestamp)
        if len(timestamps) > TIMESTAMP_WINDOW:
            timestamps.pop(0)
        if len(timestamps) >= 2:
            target = compute_adjustment(timestamps, target,
                                        target_block_time, min_t, max_t)

    return target

# ============================================================================
# Block types
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
# Mining (RandomX stand-in: blake3)
# ============================================================================

def derive_key(height: int) -> bytes:
    key = bytearray(32)
    key[0:8] = struct.pack('<Q', height)
    return bytes(key)

def _mining_blob(h: BlockHeader) -> bytes:
    blob = bytearray()
    blob.extend(struct.pack('<B', h.version))
    blob.extend(h.previous)
    blob.extend(h.merkle_root)
    blob.extend(struct.pack('<Q', h.timestamp))
    blob.extend(struct.pack('<I', h.target))
    blob.extend(struct.pack('<Q', h.nonce))
    blob.extend(struct.pack('<Q', h.height))
    blob.extend(h.uncle_merkle_root)
    blob.extend(h.randomx_key)
    return bytes(blob)

def hash_block(header: BlockHeader) -> int:
    h = hashlib.blake2b(_mining_blob(header), digest_size=32).digest()
    return struct.unpack('<I', h[0:4])[0]

def block_hash_bytes(header: BlockHeader) -> bytes:
    return hashlib.blake2b(_mining_blob(header), digest_size=32).digest()

def mine_block(previous_hash: bytes, height: int, target: int,
               txs: List[Transaction], timestamp: int,
               uncle_root: bytes = b'\x00' * 32) -> Optional[Block]:
    """Find a nonce where hash_u32 <= target."""
    key = derive_key(height)
    header = BlockHeader(
        previous=previous_hash, height=height, target=target,
        randomx_key=key, timestamp=timestamp, uncle_merkle_root=uncle_root)
    block = Block(header=header, transactions=txs)
    for nonce in range(10_000_000):
        block.header.nonce = nonce
        if hash_block(block.header) <= target:
            return block
    return None

# ============================================================================
# Block Validation
# ============================================================================

class ValidationError(Exception):
    pass

def validate_block(block: Block, chain: Dict[int, Block]) -> None:
    """
    Full block validation against canonical chain.
    1. PoW: hash_u32 <= block.header.target
    2. Target: block.header.target == get_next_work_required(chain, height)
    3. Height continuity: block.header.height == len(chain) + 1
    4. Previous hash: block.header.previous == hash(chain[height-1])
    """
    h = block.header.height
    hash_u32 = hash_block(block.header)

    # Stage 1: PoW
    if hash_u32 > block.header.target:
        raise ValidationError(f"PoW: hash_u32={hash_u32} > target={block.header.target}")

    # Stage 2: Target matches chain rules
    expected = get_next_work_required(chain, h)
    if block.header.target != expected:
        raise ValidationError(
            f"Target mismatch at h={h}: declared={block.header.target} expected={expected}")

    # Height continuity
    expected_height = len(chain) + 1
    if h != expected_height:
        raise ValidationError(f"Height: {h} != expected {expected_height}")

    # Previous hash
    if h > 1:
        prev_block = chain.get(h - 1)
        if prev_block:
            prev_hash = block_hash_bytes(prev_block.header)
            if block.header.previous != prev_hash:
                raise ValidationError(f"Previous hash mismatch at h={h}")

# ============================================================================
# Chain State (per-node)
# ============================================================================

class NodeChain:
    """Single node's view of the blockchain."""
    def __init__(self):
        self.blocks: Dict[int, Block] = {}  # height -> canonical block
        self.competing: Dict[int, List[Block]] = {}  # height -> uncle candidates
        self.block_count = 0

    @property
    def height(self) -> int:
        return len(self.blocks)

    def latest_block(self) -> Optional[Block]:
        return self.blocks.get(self.height)

    def add_block(self, block: Block) -> bool:
        """Validate and add a canonical block. Returns True on success."""
        try:
            validate_block(block, self.blocks)
        except ValidationError as e:
            return False
        self.blocks[block.header.height] = block
        self.block_count += 1
        return True

    def receive_broadcast(self, block: Block) -> str:
        """
        Handle an incoming block from P2P broadcast.
        Returns: 'applied', 'competing', 'future', 'rejected'
        """
        h = block.header.height
        cur = self.height

        # Already have this height — competing block (potential uncle)
        if h == cur:
            self.competing.setdefault(h, []).append(block)
            return 'competing'

        # Extends our chain — validate and apply
        if h == cur + 1:
            if self.add_block(block):
                return 'applied'
            return 'rejected'

        # Future height — we're behind
        if h > cur + 1:
            return 'future'

        # Past height — already processed
        return 'rejected'

    def take_uncles(self, height: int) -> List[Block]:
        """Retrieve and clear competing blocks as uncle candidates."""
        return self.competing.pop(height, [])

    def recompute_target(self, height: int) -> int:
        """Recompute the target for a given height from chain blocks."""
        return get_next_work_required(self.blocks, height)

    def reorganize_to(self, peer_blocks: Dict[int, Block]) -> int:
        """
        Chain reorganization: adopt the peer's chain if it wins fork choice.
        Bitcoin's ActivateBestChain pattern.

        Fork choice rule (deterministic, same on every node):
        1. Longer chain wins (more blocks).
        2. If same height, lower block hash at the fork point wins (tiebreaker).
        """
        # Find common ancestor
        ancestor = 0
        for h in sorted(self.blocks.keys()):
            if h in peer_blocks:
                our_hash = block_hash_bytes(self.blocks[h].header)
                peer_hash = block_hash_bytes(peer_blocks[h].header)
                if our_hash == peer_hash:
                    ancestor = h
                else:
                    break

        peer_max = max(peer_blocks.keys()) if peer_blocks else 0

        # Same height: use tiebreaker (lower hash at fork point wins)
        if peer_max == self.height and ancestor < self.height:
            fork_h = ancestor + 1
            our_fork_hash = block_hash_bytes(self.blocks[fork_h].header)
            peer_fork_hash = block_hash_bytes(peer_blocks[fork_h].header)
            if peer_fork_hash >= our_fork_hash:
                return 0  # our fork wins or tie — no reorg

        # Peer must be strictly ahead (or win tiebreaker)
        if peer_max <= self.height:
            return 0

        # Disconnect our blocks above the ancestor
        for h in range(ancestor + 1, self.height + 1):
            if h in self.blocks:
                del self.blocks[h]

        # Connect peer blocks from ancestor+1 to peer_max
        reorg_count = 0
        for h in range(ancestor + 1, peer_max + 1):
            if h in peer_blocks:
                block = peer_blocks[h]
                try:
                    validate_block(block, self.blocks)
                    self.blocks[h] = block
                    reorg_count += 1
                except ValidationError:
                    break  # invalid block — stop reorganizing

        return reorg_count

# ============================================================================
# Mining Node
# ============================================================================

class MiningNode:
    """A complete mining node with chain state and mining capability."""

    def __init__(self, node_id: str, genesis: bool = False):
        self.node_id = node_id
        self.chain = NodeChain()
        self.mined = 0
        self.received = 0
        self.forks = 0

        if genesis:
            self._create_genesis()

    def _create_genesis(self):
        key = derive_key(1)
        h = BlockHeader(previous=b'\x00' * 32, height=1, target=U32_MAX,
                        randomx_key=key, timestamp=int(time.time()))
        block = Block(header=h, transactions=[Transaction(reward=13_837_500_000_000)])
        assert self.chain.add_block(block), "Genesis failed!"

    def mine_next_block(self, ts: Optional[int] = None) -> Optional[Block]:
        """Mine the next block on top of our canonical tip."""
        cur = self.chain.latest_block()
        if not cur:
            return None

        height = cur.header.height + 1
        prev_hash = block_hash_bytes(cur.header)

        # KEY FIX: mining target = get_next_work_required(chain, height)
        # This reads from CANONICAL CHAIN BLOCKS, not an accumulator.
        target = get_next_work_required(self.chain.blocks, height)

        # Collect uncles from previous height
        uncle_root = b'\x00' * 32
        uncles = self.chain.take_uncles(cur.header.height)
        if uncles:
            uncle_root = block_hash_bytes(uncles[0].header)

        txs = [Transaction(reward=13_837_500_000_000 // max(1, height))]
        timestamp = ts if ts is not None else int(time.time())

        block = mine_block(prev_hash, height, target, txs, timestamp, uncle_root)
        if block and self.chain.add_block(block):
            self.mined += 1
            return block
        return None

    def receive(self, block: Block, peer_chain: 'NodeChain' = None):
        """
        Handle incoming broadcast block.
        If a future-height block arrives, trigger chain reorganization
        to adopt the peer's longer chain. This is Bitcoin's ActivateBestChain.
        """
        result = self.chain.receive_broadcast(block)

        if result == 'competing':
            self.forks += 1
            # Competing block at same height — check if peer's chain wins tiebreaker
            if peer_chain is not None:
                reorg = self.chain.reorganize_to(peer_chain.blocks)
                if reorg > 0:
                    result = 'reorganized'
                    self.forks -= 1  # was a reorg, not just a fork
        elif result == 'applied':
            self.received += 1
        elif result == 'future' and peer_chain is not None:
            reorg = self.chain.reorganize_to(peer_chain.blocks)
            if reorg > 0:
                result = 'reorganized'

        return result

# ============================================================================
# P2P Network (simulated message passing)
# ============================================================================

class P2P:
    def __init__(self):
        self.pending: Dict[str, List[tuple]] = {}  # (block, sender_node)

    def register(self, node_id: str):
        self.pending[node_id] = []

    def broadcast(self, sender: 'MiningNode', block: Block):
        for nid in self.pending:
            if nid != sender.node_id:
                self.pending[nid].append((block, sender))

    def deliver(self, receiver: 'MiningNode'):
        msgs = self.pending[receiver.node_id]
        self.pending[receiver.node_id] = []
        for block, sender in msgs:
            receiver.receive(block, sender.chain)

# ============================================================================
# TESTS
# ============================================================================

def test_target_determinism():
    """Test A/B: same chain → same target."""
    print("=== Test: Target Determinism ===\n")
    node0 = MiningNode("n0", genesis=True)

    # Mine 5 blocks on node0
    ts_base = int(time.time())
    for i in range(5):
        node0.mine_next_block(ts_base + i * 60)

    # Create a second chain with the SAME blocks
    chain2 = NodeChain()
    for h in range(1, node0.chain.height + 1):
        chain2.add_block(node0.chain.blocks[h])

    # Both chains should compute the same target at every height
    all_match = True
    for h in range(2, node0.chain.height + 2):
        t0 = get_next_work_required(node0.chain.blocks, h)
        t2 = get_next_work_required(chain2.blocks, h)
        if t0 != t2:
            all_match = False
            print(f"  DIVERGENCE at h={h}: t0={t0:#x} t2={t2:#x}")
    print(f"  Same chain → same target: {'PASS' if all_match else 'FAIL'}\n")
    return all_match

def test_miner_validator_agree():
    """Test C: miner target = validator target for the same chain."""
    print("=== Test: Miner/Validator Agreement ===\n")
    node0 = MiningNode("n0", genesis=True)

    ts_base = int(time.time())
    for i in range(3):
        block = node0.mine_next_block(ts_base + i * 60)
        if block:
            # The miner used target from chain blocks
            miner_target = block.header.target
            # The validator would use the same chain to compute expected
            validator_target = get_next_work_required(node0.chain.blocks, block.header.height)
            match = "PASS" if miner_target == validator_target else "FAIL"
            print(f"  Block {block.header.height}: miner={miner_target:#x} "
                  f"validator={validator_target:#x} {match}")
    print()

def test_two_miners_converge():
    """Test D/E: two miners converge on the same chain."""
    print("=== Test: Two Miners Converge ===\n")

    node0 = MiningNode("n0", genesis=True)
    node1 = MiningNode("n1", genesis=False)
    p2p = P2P()
    p2p.register("n0"); p2p.register("n1")

    # Phase 1: Node1 syncs genesis from node0
    print("Phase 1: Node1 syncs genesis")
    p2p.broadcast(node0, node0.chain.blocks[1])
    p2p.deliver(node1)
    print(f"  n0={node0.chain.height} n1={node1.chain.height}\n")

    # Phase 2: Node0 mines 3 blocks, broadcasts each. Node1 receives.
    print("Phase 2: Node0 mines, node1 receives")
    ts = int(time.time())
    for i in range(3):
        block = node0.mine_next_block(ts + i * 60)
        if block:
            p2p.broadcast(node0, block)
            p2p.deliver(node1)

    print(f"  n0={node0.chain.height} n1={node1.chain.height}")

    # Verify hashes match
    match = True
    for h in range(1, min(node0.chain.height, node1.chain.height) + 1):
        h0 = block_hash_bytes(node0.chain.blocks[h].header).hex()[:16]
        h1 = block_hash_bytes(node1.chain.blocks[h].header).hex()[:16]
        if h0 != h1:
            match = False
            print(f"  MISMATCH at h={h}")
    print(f"  Hash match: {'PASS' if match else 'FAIL'}\n")
    return match

def test_two_miners_compete():
    """Test: both nodes mine simultaneously, converge via fork resolution."""
    print("=== Test: Competing Miners ===\n")

    node0 = MiningNode("n0", genesis=True)
    node1 = MiningNode("n1", genesis=False)
    p2p = P2P()
    p2p.register("n0"); p2p.register("n1")

    # Sync genesis
    p2p.broadcast(node0, node0.chain.blocks[1])
    p2p.deliver(node1)
    print(f"  After sync: n0={node0.chain.height} n1={node1.chain.height}\n")

    # Both mine block 2 simultaneously (different timestamps)
    print("Round 1: Both mine block 2")
    b0 = node0.mine_next_block(1000)
    b1 = node1.mine_next_block(2000)
    print(f"  n0 mined h={b0.header.height} target={b0.header.target:#x}")
    print(f"  n1 mined h={b1.header.height} target={b1.header.target:#x}")

    # Exchange blocks
    p2p.broadcast(node0, b0)
    p2p.broadcast(node1, b1)
    p2p.deliver(node0)
    p2p.deliver(node1)
    print(f"  After exchange: n0={node0.chain.height} n1={node1.chain.height}")
    print(f"  n0 competing: {list(node0.chain.competing.keys())}")
    print(f"  n1 competing: {list(node1.chain.competing.keys())}")

    # Both mine block 3 — first to broadcast wins
    print("\nRound 2: Both mine block 3")
    b0 = node0.mine_next_block(1060)
    b1 = node1.mine_next_block(2060)
    p2p.broadcast(node0, b0)
    p2p.broadcast(node1, b1)
    p2p.deliver(node0)
    p2p.deliver(node1)
    print(f"  After round 2: n0={node0.chain.height} n1={node1.chain.height}")

    # Both mine block 4
    print("\nRound 3: Both mine block 4")
    b0 = node0.mine_next_block(1120)
    b1 = node1.mine_next_block(2120)
    p2p.broadcast(node0, b0)
    p2p.broadcast(node1, b1)
    p2p.deliver(node0)
    p2p.deliver(node1)

    print(f"\n  Final: n0={node0.chain.height} n1={node1.chain.height}")
    print(f"  n0 mined={node0.mined} received={node0.received} forks={node0.forks}")
    print(f"  n1 mined={node1.mined} received={node1.received} forks={node1.forks}")

    # Show block hashes
    for h in range(1, max(node0.chain.height, node1.chain.height) + 1):
        b0 = node0.chain.blocks.get(h)
        b1 = node1.chain.blocks.get(h)
        h0 = block_hash_bytes(b0.header).hex()[:16] if b0 else "NONE"
        h1 = block_hash_bytes(b1.header).hex()[:16] if b1 else "NONE"
        print(f"  h={h}: n0={h0} n1={h1} {'MATCH' if h0 == h1 else 'DIVERGE'}")

def test_continuous_production():
    """Test F: continuous production over 20 blocks."""
    print("\n=== Test: Continuous Production (20 blocks) ===\n")

    node0 = MiningNode("n0", genesis=True)
    node1 = MiningNode("n1", genesis=False)
    p2p = P2P()
    p2p.register("n0"); p2p.register("n1")

    # Sync
    p2p.broadcast(node0, node0.chain.blocks[1])
    p2p.deliver(node1)

    ts = int(time.time())
    for i in range(20):
        # Node0 mines
        block = node0.mine_next_block(ts + i * 60)
        if block:
            p2p.broadcast(node0, block)
        # Node1 also mines
        node1.mine_next_block(ts + i * 90 + 30)
        # Deliver messages
        p2p.deliver(node0)
        p2p.deliver(node1)

    print(f"  n0={node0.chain.height} (mined={node0.mined} received={node0.received} forks={node0.forks})")
    print(f"  n1={node1.chain.height} (mined={node1.mined} received={node1.received} forks={node1.forks})")

    # Verify hashes
    match = True
    for h in range(1, min(node0.chain.height, node1.chain.height) + 1):
        h0 = block_hash_bytes(node0.chain.blocks[h].header).hex()[:16]
        h1 = block_hash_bytes(node1.chain.blocks[h].header).hex()[:16]
        if h0 != h1:
            match = False
            print(f"  MISMATCH at h={h}")
    print(f"  Consensus: {'PASS' if match else 'FAIL'}")
    return match

if __name__ == "__main__":
    test_target_determinism()
    test_miner_validator_agree()
    test_two_miners_converge()
    test_two_miners_compete()
    test_continuous_production()
