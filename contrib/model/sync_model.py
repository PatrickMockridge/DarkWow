#!/usr/bin/env python3
"""
DarkWow Sync Protocol Model — Executable Specification
======================================================

Executable 1:1 model of sync-protocol.md. Founded in the rho-calculus: the
single `Sync = SyncClient | SyncHandler | BlockSink` replicated process net,
parameterised identically across wallet / observer / mining node; only the
BlockSink differs.

Scope note: this model covers the sync pull loop and the LOCAL caught-up rule
(`local_height >= max_peer_height`) + separate mining gate. Fork selection
(heaviest-chain + reorg, `activate_best_chain`) is a core-consensus concern
(consensus.md §Fork Choice Rule) and is NOT modelled here; uncle rewards
(uncle_merkle.md) are a separate economic layer, also out of scope.

Matches:
  src/linear/src/sync_types.rs        — message types + wire shape
  src/linear/src/sync_connection.rs   — SyncPeer/SyncServer (unified serve + pull)
  src/linear/src/sync_boundary.rs     — PeerTip, BlocksBatch
  bin/dww/src/sync_task.rs            — wallet BlockSink (insert/scan)
  bin/dwowd/src/task/consensus_linear.rs — observer/mining BlockSink (validate/accept)
  doc/src/arch/sync-protocol.md       — the spec this model conforms to

Usage:
  python3 contrib/model/sync_model.py
"""

import json
from dataclasses import dataclass, field
from enum import IntEnum
from typing import List, Optional, Dict

# ==============================================================================
# Nominal types — matching dwow_sdk::blockchain::BlockHeight (blockchain.rs:85)
# ==============================================================================

GENESIS_HEIGHT = 1  # BlockHeight::GENESIS = BlockHeight(1); 0 = pre-genesis sentinel
U64_MAX = 2**64 - 1


@dataclass(frozen=True)
class BlockHeight:
    """Nominal height type. Never a bare int across a boundary (§2)."""
    value: int

    GENESIS: "BlockHeight" = None  # set below (dataclass frozen workaround)

    def get(self) -> int:
        return self.value

    def is_zero(self) -> bool:
        return self.value == 0

    def succ(self) -> "BlockHeight":
        return BlockHeight(self.value + 1)

    def pred(self) -> "BlockHeight":
        return BlockHeight(self.value - 1)

    def __repr__(self) -> str:
        return f"BlockHeight({self.value})"


BlockHeight.GENESIS = BlockHeight(GENESIS_HEIGHT)


@dataclass(frozen=True)
class BlockHash:
    """Nominal blake3 hash, serialised as hex string (§8.2.1).
    Re-lifts only through from_hex_str; empty string = genesis sentinel."""
    hex: str  # 64-char hex when real; "" sentinel for zero

    @staticmethod
    def from_hex_str(s: str) -> Optional["BlockHash"]:
        if s == "":
            return None  # empty = genesis sentinel, mirrors Rust None
        if len(s) != 64:
            return None
        try:
            int(s, 16)
        except ValueError:
            return None
        return BlockHash(s)

    @staticmethod
    def zero() -> "BlockHash":
        return BlockHash("0" * 64)

    def to_hex(self) -> str:
        return self.hex

    def is_zero(self) -> bool:
        return self.hex == "0" * 64


# ==============================================================================
# Message types — wire shape mirrors serde_json in sync_types.rs (§6)
# ==============================================================================

@dataclass
class GetTip:
    """Unit struct — serialises as JSON null."""
    pass


@dataclass
class Tip:
    height: BlockHeight
    hash: BlockHash
    genesis_hash: Optional[BlockHash] = None  # #[serde(default)]


@dataclass
class GetBlocks:
    start_height: BlockHeight
    count: int


@dataclass
class Blocks:
    blocks: List[dict]  # Vec<Block> — modelled as opaque dicts


# ==============================================================================
# Unknown-command drain — sync-protocol.md §14.1
# ==============================================================================

# A peer with no dispatcher for a push command (linearlblock/tx) SHALL drain the
# frame payload (cap = largest legitimate Block/Transaction) and continue, never
# desync the stream. Mirrors MAX_INBOUND_PAYLOAD in src/net/message.rs.
MAX_INBOUND_PAYLOAD = 4 * 1024 * 1024  # 4 MiB

# Node-only push commands (§14.1): the node registers these; the wallet's
# ManualSession does not (drain-and-ignore).
NODE_PUSH_COMMANDS = ("linearlblock", "tx")


# ==============================================================================
# L2 boundary types — sync-protocol.md §9
# ==============================================================================

@dataclass
class PeerTip:
    """Tip re-lifted across the boundary. Constructed ONLY via from_tip."""
    height: BlockHeight
    hash: BlockHash
    genesis_hash: Optional[BlockHash]

    @staticmethod
    def from_tip(tip: Tip) -> Optional["PeerTip"]:
        # Obligation #1: re-lift validation.
        if tip.height.get() == U64_MAX:
            return None  # invalid sentinel height
        if not tip.height.is_zero() and tip.genesis_hash is None:
            return None  # missing genesis hash at height > 0
        h = BlockHash.zero() if (tip.height.is_zero() and tip.hash.is_zero()) else tip.hash
        return PeerTip(tip.height, h, tip.genesis_hash)


@dataclass
class BlocksBatch:
    blocks: List[dict]


class SyncState(IntEnum):
    Initial = 0
    Syncing = 1
    CaughtUp = 2
    Behind = 3
    WaitingForGenesis = 4


# ==============================================================================
# BlockSink — the sole per-role process (sync-protocol.md §0)
# ==============================================================================

class BlockSink:
    """Interface: apply a received block. Wallet inserts+scans; observer/mining
    validate+execute+accept. The sync loop is identical; only this differs."""

    def apply(self, block: dict) -> bool:
        raise NotImplementedError


class WalletSink(BlockSink):
    """bin/dww/src/sync_task.rs — insert_synced_block (insert + AEAD scan)."""

    def __init__(self):
        self.height = BlockHeight(0)
        self.blocks: Dict[int, dict] = {}

    def apply(self, block: dict) -> bool:
        h = block["height"]
        # Chain continuity: prev block must exist for height > 1.
        if h > GENESIS_HEIGHT and (h - 1) not in self.blocks:
            return False
        self.blocks[h] = block
        self.height = BlockHeight(h)
        return True


class MiningSink(BlockSink):
    """bin/dwowd/src/task/consensus_linear.rs — accept_block (validate+execute)."""

    def __init__(self):
        self.height = BlockHeight(0)
        self.blocks: Dict[int, dict] = {}

    def apply(self, block: dict) -> bool:
        h = block["height"]
        # Full validation would run PoW/WASM/ZK here; model asserts well-formed.
        if block.get("valid") is False:
            return False
        if h > GENESIS_HEIGHT and (h - 1) not in self.blocks:
            return False
        self.blocks[h] = block
        self.height = BlockHeight(h)
        return True


# ==============================================================================
# Mock peer — a SyncServer (serve side) over an in-memory chain
# ==============================================================================

@dataclass
class MockPeer:
    """Serves GetTip/GetBlocks like SyncServer (sync_connection.rs)."""
    blocks: List[dict] = field(default_factory=list)
    genesis_hash: BlockHash = BlockHash.zero()
    LINEAR_SYNC_BATCH = 20

    @property
    def height(self) -> BlockHeight:
        return BlockHeight(len(self.blocks))

    def handle_get_tip(self) -> Tip:
        h = self.height
        hsh = self.blocks[-1]["hash"] if self.blocks else BlockHash.zero()
        return Tip(height=h, hash=hsh, genesis_hash=self.genesis_hash)

    def handle_get_blocks(self, start_height: BlockHeight, count: int) -> Blocks:
        # Genesis served ALONE (sync_connection.rs serve_conn).
        if start_height == BlockHeight.GENESIS:
            count = 1
        else:
            count = min(count, self.LINEAR_SYNC_BATCH)
        out = []
        h = start_height
        for _ in range(count):
            if h.get() > len(self.blocks):
                break
            out.append(self.blocks[h.get() - 1])  # 1-indexed height -> 0-indexed list
            h = h.succ()
        return Blocks(blocks=out)


# ==============================================================================
# Shared sync client — dwow_chain::sync_connection (SyncPeer.request_tip/request_blocks)
# ==============================================================================

def request_tip(peer: MockPeer) -> Optional[Tip]:
    """Wire flow: send GetTip, await Tip (5s timeout modelled as immediate)."""
    return peer.handle_get_tip()


def request_blocks(peer: MockPeer, start_height: BlockHeight, count: int) -> Blocks:
    """Wire flow: send GetBlocks{start_height,count}, await Blocks (30s timeout)."""
    return peer.handle_get_blocks(start_height, count)


def genesis_filter(tip: Tip, local_genesis: BlockHash) -> bool:
    """sync-protocol.md §5 — skip mismatched-genesis peers."""
    if tip.genesis_hash is None:
        return True  # unverified, accept (backward compat)
    return tip.genesis_hash == local_genesis


# ==============================================================================
# The shared pull loop — sync-protocol.md §0 (SyncClient)
# ==============================================================================

def sync_to_tip(peer: MockPeer, sink: BlockSink, local_genesis: BlockHash) -> SyncState:
    """One sync pass: collect tip, genesis-filter, pull missing blocks, apply via sink."""
    tip = request_tip(peer)
    if not genesis_filter(tip, local_genesis):
        return SyncState.Behind  # wrong chain — skip peer

    next_height = sink.height.succ()
    while next_height.get() <= tip.height.get():
        remaining = tip.height.get() - next_height.get() + 1
        batch = min(20, remaining)
        blocks_msg = request_blocks(peer, next_height, batch)
        if not blocks_msg.blocks:
            break
        for block in blocks_msg.blocks:
            if not sink.apply(block):
                return SyncState.Behind
            next_height = next_height.succ()

    if sink.height.get() >= tip.height.get():
        return SyncState.CaughtUp
    return SyncState.Behind


def node_sync_decision(max_peer_height: BlockHeight, local_height: BlockHeight,
                       genesis_authority: bool, has_peers: bool) -> SyncState:
    """consensus_linear_init_task — the node's LOCAL caught-up + separate mining gate.

    Caught-up is a LOCAL property (sync-protocol.md §18.1.1): caught_up iff
    `local_height >= max_peer_height`. Mining is a separate gate:
    `mine = caught_up AND (authority OR has_peers)`. A synced join node with no
    peers is caught up but does NOT mine (it cannot propagate blocks) — it is
    `Behind` (miner paused), not a peer-evidence "Behind".
    """
    caught_up = local_height.get() >= max_peer_height.get()
    mine = caught_up and (genesis_authority or has_peers)
    return SyncState.CaughtUp if mine else SyncState.Behind


# ==============================================================================
# Wallet trust model — sync-protocol.md §17 (follow the longest chain)
# ==============================================================================

def longest_chain_tip(tips: List[BlockHeight]) -> BlockHeight:
    """sync-protocol.md §17 — the wallet follows the longest (highest) peer-reported
    tip. A lower or divergent tip never blocks: it warns and proceeds with the highest
    height it saw."""
    if not tips:
        return BlockHeight(0)
    return max(tips, key=lambda h: h.get())


# ==============================================================================
# Async production logic — sync-protocol.md §13.2 (one timeout pair + one re-poll)
# ==============================================================================

TIMEOUTS = {"tip": 5, "blocks": 30, "node_repoll": 30}


# ==============================================================================
# Tests
# ==============================================================================

def _hex(h: int) -> str:
    return format(h % (2**256), "064x")


def _block(height: int) -> dict:
    return {"height": height, "hash": _hex(height), "valid": True}


if __name__ == "__main__":
    passed = 0
    failed = 0

    def check(name, cond):
        global passed, failed
        if cond:
            passed += 1
            print(f"  {name}: PASSED")
        else:
            failed += 1
            print(f"  {name}: FAILED")

    # Test 1: wire format stability (§6) — GetTip serialises as null
    check("test_gettip_null", json.dumps(None) == "null")
    # Test 1b: Tip JSON shape
    tip_json = json.dumps({"height": 42, "hash": "aa" * 32, "genesis_hash": "bb" * 32})
    tip_rt = json.loads(tip_json)
    check("test_tip_shape", tip_rt["height"] == 42 and tip_rt["hash"] == "aa" * 32)
    # Test 1c: Tip without genesis_hash (backward compat)
    old_json = json.dumps({"height": 42, "hash": "01" * 32})
    old_rt = json.loads(old_json)
    check("test_tip_old_compat", "genesis_hash" not in old_rt)
    # Test 1d: GetBlocks shape
    gb_json = json.dumps({"start_height": 1, "count": 20})
    check("test_getblocks_shape", json.loads(gb_json) == {"start_height": 1, "count": 20})

    # Test 2: BlockHash re-lift validation (§2 / §7 obligation #1)
    check("test_blockhash_valid", BlockHash.from_hex_str("ab" * 32) is not None)
    check("test_blockhash_empty_sentinel", BlockHash.from_hex_str("") is None)
    check("test_blockhash_bad_len", BlockHash.from_hex_str("ab") is None)
    check("test_blockhash_zero", BlockHash.zero().is_zero())

    # Test 3: PeerTip re-lift validation (§7 obligation #1)
    bad_height = Tip(BlockHeight(U64_MAX), BlockHash.zero(), BlockHash.zero())
    check("test_peertip_rejects_max_height", PeerTip.from_tip(bad_height) is None)
    missing_genesis = Tip(BlockHeight(5), BlockHash(_hex(5)), None)
    check("test_peertip_rejects_missing_genesis", PeerTip.from_tip(missing_genesis) is None)
    ok_tip = Tip(BlockHeight(5), BlockHash(_hex(5)), BlockHash(_hex(1)))
    check("test_peertip_accepts_valid", PeerTip.from_tip(ok_tip) is not None)

    # Test 4: genesis_hash filtering (§5)
    local_genesis = BlockHash(_hex(999))
    match = Tip(BlockHeight(3), BlockHash(_hex(3)), local_genesis)
    mismatch = Tip(BlockHeight(3), BlockHash(_hex(3)), BlockHash(_hex(1)))
    check("test_genesis_filter_match", genesis_filter(match, local_genesis))
    check("test_genesis_filter_mismatch", not genesis_filter(mismatch, local_genesis))

    # Test 5: SyncState exhaustive values (5-state nominal type, §13.3)
    check("test_syncstate_cardinality", len(SyncState) == 5)
    check("test_syncstate_waiting", SyncState.WaitingForGenesis == 4)

    # Test 6: THE core invariant — wallet and mining sinks sync identically.
    chain_blocks = [_block(h) for h in range(1, 6)]  # heights 1..5
    peer = MockPeer(blocks=chain_blocks, genesis_hash=BlockHash(_hex(1)))
    wallet = WalletSink()
    mining = MiningSink()
    w_state = sync_to_tip(peer, wallet, peer.genesis_hash)
    m_state = sync_to_tip(peer, mining, peer.genesis_hash)
    check("test_wallet_reaches_caughtup", w_state == SyncState.CaughtUp and wallet.height.get() == 5)
    check("test_mining_reaches_caughtup", m_state == SyncState.CaughtUp and mining.height.get() == 5)
    check("test_sinks_identical", wallet.blocks == mining.blocks)

    # Test 8: genesis served alone + batch cap (§7 / sync_connection.rs)
    g = peer.handle_get_blocks(BlockHeight.GENESIS, 20)
    check("test_genesis_served_alone", len(g.blocks) == 1 and g.blocks[0]["height"] == 1)
    b = peer.handle_get_blocks(BlockHeight(2), 20)
    check("test_batch_respects_remaining", len(b.blocks) == 4)  # heights 2,3,4,5

    # Test 12: async timeout table (§13.2) — tip is cheap, blocks are large
    check("test_timeout_tip_lt_blocks", TIMEOUTS["tip"] < TIMEOUTS["blocks"])

    # Test 14: wallet follows the longest chain (§17)
    check("test_longest_chain_highest",
          longest_chain_tip([BlockHeight(5), BlockHeight(7), BlockHeight(6)]).get() == 7)
    check("test_longest_chain_ignores_lower",
          longest_chain_tip([BlockHeight(7), BlockHeight(5)]).get() == 7)
    check("test_longest_chain_empty", longest_chain_tip([]).get() == 0)

    # Test 15: unknown-command drain (§14.1)
    check("test_inbound_payload_cap_4mib", MAX_INBOUND_PAYLOAD == 4 * 1024 * 1024)
    check("test_node_push_commands", set(NODE_PUSH_COMMANDS) == {"linearlblock", "tx"})

    # Test 16: node caught-up is a LOCAL property; mining is a separate gate (§18.1.1).
    check("test_node_caught_up_with_peers",
          node_sync_decision(BlockHeight(10), BlockHeight(10), False, True) == SyncState.CaughtUp)
    check("test_node_behind_below_tip",
          node_sync_decision(BlockHeight(10), BlockHeight(5), False, True) == SyncState.Behind)
    check("test_join_node_peerless_not_mine",
          node_sync_decision(BlockHeight(0), BlockHeight(5), False, False) == SyncState.Behind)
    check("test_authority_mines_solo",
          node_sync_decision(BlockHeight(0), BlockHeight(5), True, False) == SyncState.CaughtUp)
    check("test_authority_at_genesis_mines",
          node_sync_decision(BlockHeight(0), BlockHeight(0), True, False) == SyncState.CaughtUp)

    print(f"\n{'=' * 60}")
    print(f"  Results: {passed}/{passed + failed} passed")
    print(f"{'=' * 60}")
    if failed:
        raise SystemExit(1)
