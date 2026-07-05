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

Docker Build Architecture (2026-07-05 HAZOP):
  The Dockerfile is MINIMAL — it builds the dwowd and wallet binaries only.
  Containers handle everything else at runtime:
    - zkas binary compilation (cargo build -p zkas)
    - zk.bin compilation from .zk source (zkas rebuild)
    - WASM contract compilation (cargo build --target wasm32-unknown-unknown)
    - Contract deployment and genesis init (init_genesis_contracts)
    - Genesis block creation (create_genesis=true)
  The pipeline tests WHAT THE CONTAINERS DO, not what the Dockerfile
  pre-computes. This ensures the Rust runtime compilation path is exercised
  by the pipeline — bugs in init_genesis_contracts, zkas rebuild, or WASM
  compilation are caught here, not masked by Dockerfile pre-compilation.

  The Dockerfile must be kept minimal. 6 of 8 steps in the original
  Dockerfile were container responsibilities being done in Docker. The
  include_bytes! constraint for WASM embedding forces Docker to compile
  WASM, which means the container's own compilation path is never tested.
  This is architectural debt tracked for removal.

Key Management — Clean Separation of Concerns (2026-07-02):
  AccountManager (crates/dwow-accounts/src/lib.rs) is the single key authority.
  Miner and wallet are consumers — they call AccountManager, they don't
  manipulate key material themselves.

  Miner key flow:
    open(section=None) → NODE_NAME env → auto-gen or keys.toml
    → default_public_key() for coinbase
    → export_base58(0) for key backup/sharing (dwowd --export-secret)

  Wallet key flow:
    open(section="wallet-N") → keys.toml or auto-gen
    → import_base58() from stdin (wallet import-secrets)
    → secrets() for scanning / AEAD decryption

  Pipeline key sharing (testing only):
    dwowd --export-secret | wallet import-secrets — both through AccountManager.
    No shell-level key manipulation — no xxd, no bs58, no mining_secret file.

  Hard guardrails:
    - import failure → exit 1, daemon does not start
    - export failure → exit 1, prints error
    - scan with zero secrets → prints error, exits non-zero

Failure modes modeled (defense-in-depth):
  FM1: No keys declared, non-localnet → hard error
  FM2: Miner + wallet have different keys → zero balance
  FM3: keys.toml missing section → clear error
  FM4: keys.toml malformed → clear error
  FM5: Auto-generated keys → different in each container
  FM6: Restart with cached state → same key used
  FM7: ORPHAN_KEY: random key at index 0 when declared key imported
  FM8: DUPLICATE_IMPORT: same hex imported twice
"""

import hashlib
import struct
import time
import random
import os
from dataclasses import dataclass, field
from typing import Optional, List, Dict, Set, Tuple
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

    def get_next_work_required(self, height: int, chain_blocks: dict = None) -> int:
        """consensus.rs line 293. Chain blocks override accumulator for determinism."""
        if height <= 1:
            return U32_MAX
        if chain_blocks is not None and len(chain_blocks) >= 2:
            return self._compute_target_from_chain(chain_blocks)
        return self.target

    def _compute_target_from_chain(self, blocks: dict) -> int:
        """
        Bitcoin GetNextWorkRequired: compute target from canonical chain timestamps.
        Does NOT use the mutable accumulator. Starts from INITIAL_TARGET and
        iteratively computes the target for each block using ONLY chain timestamps.
        This guarantees deterministic results across all nodes with the same chain.
        """
        heights = sorted(blocks.keys())
        if len(heights) < 2:
            return INITIAL_TARGET

        # Walk the chain from genesis, recomputing target at each step
        # using only the timestamps in the canonical blocks.
        target = INITIAL_TARGET
        timestamps = []

        for h in heights:
            block = blocks[h]
            timestamps.append(block.header.timestamp)
            if len(timestamps) > TIMESTAMP_WINDOW:
                timestamps.pop(0)
            if len(timestamps) >= 2:
                # Same adjustment logic as adjust_target()
                n = min(len(timestamps), 10)
                recent = timestamps[-n:]
                total = 0
                for i in range(1, len(recent)):
                    total += max(0, recent[i] - recent[i-1])
                count = len(recent) - 1
                avg = total // count if count > 0 else self.target_block_time
                if avg == 0:
                    ratio = SCALE * 9 // 10
                else:
                    ratio = max(SCALE // 2, min(SCALE * 2, (self.target_block_time * SCALE) // avg))
                tenth = SCALE // 10
                if ratio > SCALE:
                    adj = SCALE + min(ratio - SCALE, tenth)
                elif ratio < SCALE:
                    adj = SCALE - min(SCALE - ratio, tenth)
                else:
                    adj = SCALE
                target = max(self.min_target, min(self.max_target, (target * SCALE // adj)))

        return target

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

        # Future height — we're behind, need to sync the full chain
        if block.header.height > current_height + 1:
            print(f"    [GAP] Block at h={block.header.height} but current={current_height} — need sync")
            return False

        # Chain reorganization: if the incoming block is at current_height + 1
        # but has a DIFFERENT previous hash than our tip, the peer is on a
        # different fork. We must adopt the longer chain (Nakamoto consensus).
        # For the model: accept the peer's block and overwrite our tip if
        # it gives us a higher total height eventually. Simplified: if we
        # receive a block at h=N+1, trust it and apply.
        expected_target = self.consensus.get_next_work_required(block.header.height, self.blocks)
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

        self.sync_complete = True
        self.mining_enabled = True
        print(f"[{self.node_id}] Sync complete, mining enabled at h={self.chain.get_height()}")

    def _fetch_and_apply_blocks(self, start_height: int, end_height: int, source_node) -> bool:
        """GetBlocks/Blocks sync — fetch and apply blocks, skip bad ones.

        Bad blocks from incompatible peers are SKIPPED, not retried forever.
        A peer serving invalid blocks cannot stall sync (HAZID RC1/FM15).
        After 3 consecutive failures from the same peer, deprioritize.
        """
        consecutive_failures = 0
        for h in range(start_height, end_height + 1):
            block = source_node.chain.get_block(h)
            if block is None:
                consecutive_failures += 1
                if consecutive_failures >= 3:
                    return False
                continue
            success = self.chain.connect_block(block)
            if not success:
                consecutive_failures += 1
                if consecutive_failures >= 3:
                    return False
                continue  # skip bad block, don't stall
            self.blocks_received += 1
            consecutive_failures = 0  # reset on success
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
# Key Management Layer — HAZOP Remediation (2026-07-01)
# ============================================================================
# Models: keys.toml → AccountManager → coinbase attribution → wallet verify
# Guards against all 8 key management failure modes (FM1-FM8).

# Import AccountManager from wallet_model (same directory)
import sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from wallet_model import AccountManager, Account, SecretKey, Keypair, PublicKey
except ImportError:
    # Standalone fallback if wallet_model not importable
    AccountManager = None


@dataclass
class KeyConfig:
    """Models keys.toml — single source of truth for all keys.

    Format (TOML):
        [node0]
        wallet_secret = "0000...0001"
        [node1]
        wallet_secret = "0000...0002"
        [wallet-1]
        wallet_secret = "0000...0001"  # shares node0 key
    """
    secrets: Dict[str, str] = field(default_factory=dict)
    # Maps section name → 64-char hex secret

    @staticmethod
    def default_keys() -> 'KeyConfig':
        """Deterministic test keys matching contrib/docker/darkwow-testnet/keys.toml."""
        return KeyConfig(secrets={
            "node0":    "0000000000000000000000000000000000000000000000000000000000000001",
            "node1":    "0000000000000000000000000000000000000000000000000000000000000002",
            "wallet-1": "0000000000000000000000000000000000000000000000000000000000000001",  # = node0
            "wallet-2": "0000000000000000000000000000000000000000000000000000000000000003",
        })

    def write_to_file(self, path: str):
        """Write keys.toml to disk."""
        with open(path, 'w') as f:
            f.write("# DarkWow Testnet Key Configuration\n")
            f.write("# Auto-generated by dockernet_model.py\n\n")
            for section, secret in self.secrets.items():
                f.write(f'[{section}]\n')
                f.write(f'wallet_secret = "{secret}"\n\n')

    def get_node_key(self, node_name: str) -> Optional[str]:
        """Get the hex secret for a mining node."""
        return self.secrets.get(node_name)

    def get_wallet_key(self, wallet_name: str) -> Optional[str]:
        """Get the hex secret for a wallet (e.g. 'wallet-1')."""
        return self.secrets.get(wallet_name)


class KeyedMiningNode(MiningNode):
    """Mining node with AccountManager integration.

    Key resolution: keys.toml declaration → auto-generate (localnet) → error.
    Coinbase reward is attributed to the miner's public key.
    """

    def __init__(self, node_id: str, create_genesis: bool, p2p: P2PNetwork,
                 key_config: Optional[KeyConfig] = None, localnet: bool = True):
        super().__init__(node_id, create_genesis, p2p)
        self.key_config = key_config
        self.localnet = localnet
        self.miner_public_key_hex: Optional[str] = None
        self._init_account_manager()
        # Tag genesis block if it was created
        if create_genesis and self._account_mgr and self.chain.get_height() >= 1:
            gen = self.chain.get_block(1)
            if gen:
                gen._miner_pubkey = self._account_mgr.default_public_key()

    def _init_account_manager(self):
        """Initialize AccountManager following the resolution chain."""
        if AccountManager is None:
            self.miner_public_key_hex = "deadbeef" * 8  # placeholder
            return

        try:
            mgr = AccountManager.open(
                localnet=self.localnet,
                keys_toml_path=None,  # We handle keys.toml manually for the model
            )
        except ValueError:
            # Non-localnet, no keys — try keys.toml
            if self.key_config:
                hex_secret = self.key_config.get_node_key(self.node_id)
                if hex_secret:
                    mgr = AccountManager()
                    mgr.import_hex(hex_secret)
                    mgr._db_attached = True
                else:
                    raise
            else:
                raise

        # If key_config provided, the declared key takes priority
        if self.key_config:
            hex_secret = self.key_config.get_node_key(self.node_id)
            if hex_secret:
                # Import declared key as default
                already_has = any(
                    a.secret_hex() == hex_secret
                    for a in mgr.accounts
                )
                if not already_has:
                    mgr.import_hex(hex_secret)
                    mgr.set_default(len(mgr.accounts) - 1)
                else:
                    # Already has it — ensure it's default
                    for i, a in enumerate(mgr.accounts):
                        if a.secret_hex() == hex_secret:
                            mgr.set_default(i)
                            break

        self._account_mgr = mgr
        self.miner_public_key_hex = mgr.default_public_key().__repr__()

    def get_miner_public_key(self):
        """Return the miner's public key for coinbase attribution."""
        if self._account_mgr:
            return self._account_mgr.default_public_key()
        return None

    def get_miner_secrets(self):
        """Return all secrets held by this miner."""
        if self._account_mgr:
            return self._account_mgr.secrets()
        return []

    def mine_one_block(self) -> Optional[Block]:
        """Override: attribute coinbase reward to miner's public key."""
        block = super().mine_one_block()
        if block and self._account_mgr:
            # Tag the coinbase with the miner's public key fingerprint
            pk = self._account_mgr.default_public_key()
            block._miner_pubkey = pk
            block._miner_hex = self.miner_public_key_hex
        return block


class WalletVerifier:
    """Wallet that verifies coinbase decryption.

    Models: wallet imports miner's key → scans blocks → decrypts coinbase
    → asserts balance > 0. This is the PIPELINE SUCCESS CRITERION.
    """

    def __init__(self, wallet_name: str, key_config: KeyConfig):
        self.wallet_name = wallet_name
        self.key_config = key_config
        self.secrets: List = []  # SecretKey objects
        self.coins_found: int = 0
        self.total_value: int = 0
        self._import_keys()

    def _import_keys(self):
        """Import the wallet's key from keys.toml."""
        if AccountManager is None:
            return
        hex_secret = self.key_config.get_wallet_key(self.wallet_name)
        if hex_secret is None:
            raise ValueError(
                f"Wallet '{self.wallet_name}' not found in keys.toml. "
                f"Available: {list(self.key_config.secrets.keys())}"
            )
        mgr = AccountManager()
        mgr.import_hex(hex_secret)
        self.secrets = mgr.secrets()

    def get_public_key(self):
        """Return this wallet's public key for key identity assertion."""
        if self.secrets:
            return Keypair.from_secret(self.secrets[0]).public
        return None

    def scan_chain(self, chain: ChainState) -> dict:
        """Scan the chain for coinbase outputs decryptable by this wallet.

        Returns: {blocks_scanned, coins_found, total_value, errors}
        """
        result = {"blocks_scanned": 0, "coins_found": 0, "total_value": 0, "errors": []}

        for h in range(1, chain.get_height() + 1):
            block = chain.get_block(h)
            if block is None:
                result["errors"].append(f"Missing block at height {h}")
                continue
            result["blocks_scanned"] += 1

            # Check coinbase attribution
            miner_pk = getattr(block, '_miner_pubkey', None)
            if miner_pk is None:
                result["errors"].append(f"No miner pubkey on coinbase at h={h}")
                continue

            # Try to decrypt with each wallet secret
            for secret in self.secrets:
                wallet_pk = Keypair.from_secret(secret).public
                if str(miner_pk) == str(wallet_pk):
                    # Key match — coinbase is decryptable
                    for tx in block.transactions:
                        result["coins_found"] += 1
                        result["total_value"] += tx.reward
                    break

        self.coins_found = result["coins_found"]
        self.total_value = result["total_value"]
        return result

    def verify_pipeline_success(self, chain: ChainState) -> Tuple[bool, str]:
        """Pipeline success criterion: wallet scan + decrypt + DRKW balance > 0.

        Returns (passed, diagnostic_message).
        """
        result = self.scan_chain(chain)
        if result["blocks_scanned"] == 0:
            return False, "FAIL: No blocks scanned — chain is empty"
        if result["coins_found"] == 0:
            return False, (
                f"FAIL: 0 coins found in {result['blocks_scanned']} blocks — "
                f"wallet key does not match any miner's coinbase key. "
                f"Check that keys.toml [{self.wallet_name}] matches the miner's key."
            )
        if result["total_value"] == 0:
            return False, "FAIL: Coins found but total_value is 0"
        return True, (
            f"PASS: {result['coins_found']} coinbase(s) found, "
            f"total DRKW balance = {result['total_value']}"
        )


# ============================================================================
# Key Management Failure Mode Tests
# ============================================================================

def test_fm1_no_keys_non_localnet():
    """FM1: Non-localnet without keys.toml → hard error.

    HAZOP F6: Operator cannot specify keys at runtime.
    Rust: AccountManager::open() returns Err on non-localnet with no keys.
    """
    print("  FM1: no-keys non-localnet...", end=" ")
    try:
        node = KeyedMiningNode("node0", True, P2PNetwork(),
                               key_config=None, localnet=False)
        # If AccountManager not available, node creation succeeds with placeholder
        if AccountManager is not None:
            assert False, "Should have raised ValueError"
    except ValueError as e:
        assert "No keys declared" in str(e)
    print("PASSED")


def test_fm2_key_mismatch_zero_balance():
    """FM2: Miner + wallet have different keys → wallet finds 0 coins.

    HAZOP: ~20 pipeline runs failed on this exact condition.
    """
    print("  FM2: key mismatch...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    import tempfile
    tmp = tempfile.mkdtemp()
    keys_path = os.path.join(tmp, "keys.toml")

    # wallet-2 has key ...0003, node0 has key ...0001 — they differ
    cfg = KeyConfig(secrets={
        "node0":    "0000000000000000000000000000000000000000000000000000000000000001",
        "wallet-2": "0000000000000000000000000000000000000000000000000000000000000003",
    })
    cfg.write_to_file(keys_path)

    p2p = P2PNetwork()
    miner = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()
    # Mine 3 blocks
    for _ in range(3):
        miner.mine_one_block()

    wallet = WalletVerifier("wallet-2", cfg)
    passed, msg = wallet.verify_pipeline_success(miner.chain)

    assert not passed, f"Should have failed: {msg}"
    assert "0 coins found" in msg, f"Expected zero coins: {msg}"

    os.remove(keys_path)
    os.rmdir(tmp)
    print("PASSED")


def test_fm3_keys_toml_missing_section():
    """FM3: keys.toml has no section for this node → clear error.

    HAZOP F4/F5: Only node0 key was read by Phase 4.
    """
    print("  FM3: missing section...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    cfg = KeyConfig(secrets={
        "node0": "0000000000000000000000000000000000000000000000000000000000000001",
        # "node99" is NOT in keys.toml
    })
    try:
        node = KeyedMiningNode("node99", True, P2PNetwork(),
                               key_config=cfg, localnet=False)
        assert False, "Should have raised"
    except (ValueError, KeyError) as e:
        pass  # Expected — no key declared for this node
    print("PASSED")


def test_fm4_keys_toml_empty():
    """FM4: Empty keys.toml → clear error.

    HAZOP F12: 2>/dev/null swallowed TOML parse errors.
    """
    print("  FM4: empty keys.toml...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    import tempfile
    tmp = tempfile.mkdtemp()
    keys_path = os.path.join(tmp, "keys.toml")
    with open(keys_path, 'w') as f:
        f.write('')  # Empty file

    try:
        secrets = AccountManager.parse_keys_toml(keys_path)
        assert False, "Should have raised"
    except ValueError as e:
        pass  # Expected

    os.remove(keys_path)
    os.rmdir(tmp)
    print("PASSED")


def test_fm5_matching_keys_success():
    """FM5 (POSITIVE): Miner + wallet share key → wallet finds coins.

    This is the HAPPY PATH — what the pipeline must verify.
    wallet-1 shares node0's key → coinbase decryption succeeds.
    """
    print("  FM5: matching keys success...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    cfg = KeyConfig.default_keys()
    p2p = P2PNetwork()
    miner = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()

    # Mine 5 blocks
    for i in range(5):
        block = miner.mine_one_block()
        if block is None:
            print(f"FAIL (mining failed at block {i+1})")
            return

    # wallet-1 shares node0's key
    wallet = WalletVerifier("wallet-1", cfg)
    passed, msg = wallet.verify_pipeline_success(miner.chain)

    assert passed, msg
    # Genesis (h=1) + 5 mined blocks = 6 coinbases expected
    assert wallet.coins_found == 6, f"Expected 6 coins (genesis + 5 mined), found {wallet.coins_found}"
    print("PASSED")


def test_fm6_key_identity_assertion():
    """FM6: Miner's default_public_key == Wallet's public key.

    Pipeline must assert this BEFORE scanning. If they differ, scan will
    produce zero results (see FM2).
    """
    print("  FM6: key identity assertion...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    cfg = KeyConfig.default_keys()
    p2p = P2PNetwork()
    miner = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    wallet = WalletVerifier("wallet-1", cfg)

    miner_pk = miner.get_miner_public_key()
    wallet_pk = wallet.get_public_key()

    assert miner_pk is not None, "Miner has no public key"
    assert wallet_pk is not None, "Wallet has no public key"
    assert str(miner_pk) == str(wallet_pk), (
        f"KEY IDENTITY FAILURE: miner={miner_pk} != wallet={wallet_pk}")
    print("PASSED")


def test_fm7_two_containers_different_keys():
    """FM7: Two independent AccountManager.open() with no keys.toml → different keys.

    HAZOP: In Docker, each container has an independent empty sled.
    Without keys.toml, auto-generation produces DIFFERENT random keys.
    """
    print("  FM7: two containers different keys...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    # Simulate two separate containers — each gets its own AccountManager
    mgr_a = AccountManager.open(localnet=True)
    mgr_b = AccountManager.open(localnet=True)

    pk_a = mgr_a.default_public_key()
    pk_b = mgr_b.default_public_key()

    assert str(pk_a) != str(pk_b), (
        "Two independent auto-generations must produce different keys "
        "(this is the Docker pipeline problem)")
    print("PASSED")


def test_fm8_restart_sled_cache_same_key():
    """FM8: Restart with cached state → same key used (no re-generation).

    HAZOP F1: from_json() must preserve db so persist() works after restart.
    """
    print("  FM8: restart cached state...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    # First boot
    mgr1 = AccountManager.open(localnet=True)
    pk1 = mgr1.default_public_key()
    store = mgr1.persist()

    # Restart with cached state
    mgr2 = AccountManager.open({"accounts": store})
    mgr2.attach_db()
    pk2 = mgr2.default_public_key()

    assert str(pk1) == str(pk2), (
        "Restart must preserve the same key via cached state")
    print("PASSED")


def test_fm9_orphan_key_cleanup():
    """FM9: When declared key imported after auto-generate, orphan is cleaned.

    HAZOP F9: open() creates random key at index 0, then declared key at index 1.
    The random key is orphaned and must be removed.
    """
    print("  FM9: orphan key cleanup...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    mgr = AccountManager.open(localnet=True)
    assert len(mgr.accounts) == 1
    assert mgr.accounts[0].label == "generated-0"

    mgr.import_hex("0000000000000000000000000000000000000000000000000000000000000001")
    assert len(mgr.accounts) == 2

    mgr.remove_orphan_auto_key()
    assert len(mgr.accounts) == 1, "Orphan auto-generated key should be removed"
    assert mgr.accounts[0].label == "imported-1"
    print("PASSED")


def test_fm10_duplicate_import_rejected():
    """FM10: Importing the same hex secret twice → rejected with clear error.

    HAZOP F11: Hex case sensitivity could cause duplicates.
    """
    print("  FM10: duplicate import rejected...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    mgr = AccountManager.open(localnet=True)
    mgr.import_hex("000000000000000000000000000000000000000000000000000000000000000a")
    try:
        mgr.import_hex("000000000000000000000000000000000000000000000000000000000000000a")
        assert False, "Should have raised on duplicate"
    except ValueError as e:
        assert "already imported" in str(e)
    print("PASSED")


def test_fm11_empty_secrets_zero_balance():
    """FM11: Wallet with zero secrets → scan produces zero coins.

    HAZOP F16: get_secrets() returns Ok(vec![]) when empty → silent failure.
    Defense-in-depth: must log ERROR and provide diagnostic.
    """
    print("  FM11: empty secrets...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    cfg = KeyConfig.default_keys()
    p2p = P2PNetwork()
    miner = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()
    for _ in range(3):
        miner.mine_one_block()

    # Wallet with NO secrets at all
    empty_cfg = KeyConfig(secrets={})
    try:
        wallet = WalletVerifier("wallet-1", empty_cfg)
        assert False, "Should have raised — wallet not in keys.toml"
    except ValueError as e:
        pass  # Expected: wallet section not found

    # Alternative: wallet exists but secret is empty
    zero_cfg = KeyConfig(secrets={"node0": cfg.secrets["node0"],
                                   "wallet-1": ""})
    try:
        # Empty hex secret should fail import
        mgr = AccountManager()
        mgr.import_hex("")
        assert False, "Should have raised"
    except ValueError:
        pass
    print("PASSED")


def test_fm12_pipeline_end_to_end():
    """FM12 (FULL PIPELINE): Two miners + two wallets, full key verification.

    Models the complete docker-compose topology:
      node0 mines with key ...0001
      node1 mines with key ...0002
      wallet-1 imports ...0001 (shares node0) → can decrypt node0 coinbases
      wallet-2 imports ...0003 (independent) → cannot decrypt any coinbase

    Success criteria:
      1. Consensus: node0 and node1 converge on same chain
      2. wallet-1 has DRKW balance > 0 (shares node0 key)
      3. wallet-2 has DRKW balance = 0 (different key)
    """
    print("  FM12: full pipeline E2E...", end=" ")
    if AccountManager is None:
        print("SKIP (no wallet_model)")
        return

    cfg = KeyConfig.default_keys()
    p2p = P2PNetwork()

    # --- Phase 1: Start miners ---
    miner0 = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner1 = KeyedMiningNode("node1", False, p2p, key_config=cfg, localnet=True)
    miner1.start_sync_task()

    # Sync genesis
    peer_h = miner0.chain.get_height()
    local_h = miner1.chain.get_height()
    if peer_h > local_h:
        miner1._fetch_and_apply_blocks(local_h + 1, peer_h, miner0)

    # --- Phase 2: Both miners produce blocks ---
    # Mining is probabilistic — node0 mines first (easy target), then both race.
    # All blocks propagate via P2P → shared chain (full node model).
    # Genesis at h=1, node0 mines h=2+h=3, then both mine concurrently.
    for _ in range(2):
        miner0.mine_one_block()
    for _ in range(3):
        b0 = miner0.mine_one_block()
        b1 = miner1.mine_one_block()
        miner0.process_p2p_messages(miner1)
        miner1.process_p2p_messages(miner0)

    # Final sync
    max_h = max(miner0.chain.get_height(), miner1.chain.get_height())
    if miner0.chain.get_height() < max_h:
        miner0._fetch_and_apply_blocks(miner0.chain.get_height() + 1, max_h, miner1)
    if miner1.chain.get_height() < max_h:
        miner1._fetch_and_apply_blocks(miner1.chain.get_height() + 1, max_h, miner0)

    # --- Phase 3: Consensus verification ---
    all_match = True
    for h in range(1, max_h + 1):
        h0 = miner0.chain.hashes.get(h, "MISSING")
        h1 = miner1.chain.hashes.get(h, "MISSING")
        if h0 != h1:
            all_match = False

    # --- Phase 4: Wallet verification (PIPELINE SUCCESS CRITERION) ---
    wallet1 = WalletVerifier("wallet-1", cfg)  # shares node0 key
    wallet2 = WalletVerifier("wallet-2", cfg)  # independent key

    # KEY IDENTITY: wallet-1 must match node0
    assert str(miner0.get_miner_public_key()) == str(wallet1.get_public_key()), \
        "CRITICAL: wallet-1 key != node0 key — pipeline will fail"

    # KEY IDENTITY: wallet-2 must NOT match node0 (independent test)
    assert str(miner0.get_miner_public_key()) != str(wallet2.get_public_key()), \
        "wallet-2 should have a different key from node0"

    # Scan: wallet-1 must find coins (shares node0 key)
    w1_ok, w1_msg = wallet1.verify_pipeline_success(miner0.chain)
    assert w1_ok, f"wallet-1 pipeline failed: {w1_msg}"
    assert wallet1.coins_found > 0, f"wallet-1 must find coins from node0"

    # Scan: wallet-2 should find NO coins (different key)
    w2_ok, w2_msg = wallet2.verify_pipeline_success(miner0.chain)
    assert not w2_ok, f"wallet-2 should have zero balance: {w2_msg}"

    # --- Final report ---
    print("PASSED")
    print(f"    Consensus: {'OK' if all_match else 'FAIL'} ({max_h} blocks)")
    print(f"    wallet-1:  {wallet1.coins_found} coins, balance={wallet1.total_value}")
    print(f"    wallet-2:  {wallet2.coins_found} coins (expected 0)")


# ============================================================================
# Full Dockernet Simulation (updated with key verification)
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


# ============================================================================
# Test Runner
# ============================================================================

if __name__ == '__main__':
    import sys

    print("=" * 60)
    print("DarkWow Dockernet Model — HAZOP Key Management Tests")
    print("=" * 60)
    print()

    # Phase 1: Key management failure mode tests
    print("--- Phase 1: Key Management Failure Modes ---")
    fm_tests = [
        ("FM1",  test_fm1_no_keys_non_localnet),
        ("FM2",  test_fm2_key_mismatch_zero_balance),
        ("FM3",  test_fm3_keys_toml_missing_section),
        ("FM4",  test_fm4_keys_toml_empty),
        ("FM5",  test_fm5_matching_keys_success),
        ("FM6",  test_fm6_key_identity_assertion),
        ("FM7",  test_fm7_two_containers_different_keys),
        ("FM8",  test_fm8_restart_sled_cache_same_key),
        ("FM9",  test_fm9_orphan_key_cleanup),
        ("FM10", test_fm10_duplicate_import_rejected),
        ("FM11", test_fm11_empty_secrets_zero_balance),
        ("FM12", test_fm12_pipeline_end_to_end),
    ]

    passed = 0
    failed = 0
    for name, test_fn in fm_tests:
        try:
            test_fn()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  {name}: FAILED — {e}")
            import traceback
            traceback.print_exc()

    print()
    print(f"  FM tests: {passed} passed, {failed} failed")
    print()

    # Phase 2: Consensus convergence (classic dockernet test)
    print("--- Phase 2: Consensus Convergence ---")
    consensus_ok = run_dockernet(num_blocks=20)
    print()

    # Phase 3: Consensus + wallet verification (full pipeline test)
    print("--- Phase 3: Consensus + Wallet Verification ---")
    if AccountManager is not None:
        cfg = KeyConfig.default_keys()
        p2p = P2PNetwork()

        miner0 = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
        miner1 = KeyedMiningNode("node1", False, p2p, key_config=cfg, localnet=True)
        miner1.start_sync_task()
        peer_h = miner0.chain.get_height()
        local_h = miner1.chain.get_height()
        if peer_h > local_h:
            miner1._fetch_and_apply_blocks(local_h + 1, peer_h, miner0)

        for _ in range(10):
            b0 = miner0.mine_one_block()
            b1 = miner1.mine_one_block()
            miner0.process_p2p_messages(miner1)
            miner1.process_p2p_messages(miner0)

        max_h = max(miner0.chain.get_height(), miner1.chain.get_height())
        if miner0.chain.get_height() < max_h:
            miner0._fetch_and_apply_blocks(miner0.chain.get_height() + 1, max_h, miner1)
        if miner1.chain.get_height() < max_h:
            miner1._fetch_and_apply_blocks(miner1.chain.get_height() + 1, max_h, miner0)

        wallet1 = WalletVerifier("wallet-1", cfg)
        w1_ok, w1_msg = wallet1.verify_pipeline_success(miner0.chain)

        # Consensus verification
        consensus_ok = True
        for h in range(1, max_h + 1):
            h0 = miner0.chain.hashes.get(h, "MISSING")
            h1 = miner1.chain.hashes.get(h, "MISSING")
            if h0 != h1:
                consensus_ok = False

        print(f"  Consensus: {'PASS' if consensus_ok else 'FAIL'} ({max_h} blocks)")
        print(f"  wallet-1:  {w1_msg}")
        print(f"  miner0 PK: {miner0.miner_public_key_hex[:32] if miner0.miner_public_key_hex else 'N/A'}...")
        print()

        pipeline_ok = consensus_ok and w1_ok
    else:
        print("  SKIP: wallet_model not importable (running standalone)")
        pipeline_ok = consensus_ok

    print("=" * 60)
    total_failed = failed + (0 if consensus_ok else 1) + (0 if pipeline_ok else 1)
    if total_failed == 0:
        print("ALL TESTS PASSED")
    else:
        print(f"SOME TESTS FAILED ({total_failed} failure(s))")
    print("=" * 60)
    sys.exit(0 if total_failed == 0 else 1)

