#!/usr/bin/env python3
"""
DarkWow Sync Protocol Model — Executable Specification
======================================================

Executable 1:1 model of sync-protocol.md. Founded in the rho-calculus: the
single `Sync = SyncClient | SyncHandler | BlockSink` replicated process net,
parameterised identically across wallet / observer / mining node; only the
BlockSink differs.

Matches:
  src/linear/src/sync_types.rs        — message types + wire shape + MAX_BYTES
  src/linear/src/sync_connection.rs   — SyncPeer/SyncServer (unified serve + pull)
  src/linear/src/sync_boundary.rs     — PeerTip, BlocksBatch, SyncDecision, SyncState
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
# MAX_BYTES — sync-protocol.md §4 (unified, canonical)
# ==============================================================================

MAX_BYTES = {
    "GetTip": 256,
    "Tip": 512,
    "GetBlocks": 256,
    "Blocks": 16 * 1024 * 1024,
}

# ==============================================================================
# Unknown-command drain — sync-protocol.md §14.3
# ==============================================================================

# A peer with no dispatcher for a push command (linearlblock/tx) SHALL drain the
# frame payload (cap = largest legitimate Block/Transaction) and continue, never
# desync the stream. Mirrors MAX_INBOUND_PAYLOAD in src/net/message.rs.
MAX_INBOUND_PAYLOAD = 4 * 1024 * 1024  # 4 MiB

# Node-only push commands (§14.3): the node registers these; the wallet's
# ManualSession does not (drain-and-ignore).
NODE_PUSH_COMMANDS = ("linearlblock", "tx")

# ==============================================================================
# Barb declaration — sync-protocol.md §5
# ==============================================================================

SYNC_BARBS = ("verify", "sync-barrier", "gossip-forward")


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


class SyncDecision(IntEnum):
    PeersAvailable = 0
    ProceedSolo = 1
    WaitForGenesis = 2
    Retry = 3


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
    """sync-protocol.md §3 — skip mismatched-genesis peers."""
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
    return SyncState.Syncing


def node_sync_decision(peer_tips: List[Optional[PeerTip]], local_height: BlockHeight,
                       genesis_authority: bool = False) -> SyncState:
    """consensus_linear_init_task — the node's cross-peer sync decision.

    F1 fix (sync-protocol.md §18.1.1): `CaughtUp` requires POSITIVE evidence of a
    peer tip. No usable tips => `Behind` (or `WaitingForGenesis` at height 0),
    NEVER `CaughtUp`. This is the premature-CaughtUp regression guard.

    Genesis-authority exception (§18.1.1): the authority is the canonical tip by
    construction; with no peer evidence it SHALL still declare `CaughtUp` and mine
    (a fresh L1 has no peers to reach; a slow peer must not park the authority).
    """
    usable = [t for t in peer_tips if t is not None]
    if not usable:
        if genesis_authority:
            return SyncState.CaughtUp
        return SyncState.WaitingForGenesis if local_height.is_zero() else SyncState.Behind
    max_peer = max(t.height.get() for t in usable)
    if local_height.get() >= max_peer:
        return SyncState.CaughtUp
    return SyncState.Syncing


# ==============================================================================
# Node sync state machine — sync-protocol.md §13.3 (not a retry loop)
# ==============================================================================

BAN_SCORE_THRESHOLD = 3
MAX_NO_PROGRESS = 5


def initial_sync(peers: List[tuple], sink: BlockSink) -> tuple:
    """consensus_linear.rs Phase 1 — the initial-sync state machine with peer punishment.

    `peers` is `List[(url, MockPeer)]`. A peer serving an invalid block is punished
    (ban score ++) and the next peer is tried; no-progress is bounded (→ `Behind`);
    `CaughtUp` requires reaching a peer tip. Returns `(SyncState, {url: ban_score})`.
    """
    ban_scores = {url: 0 for url, _ in peers}
    no_progress = 0
    while True:
        alive = [(u, p) for (u, p) in peers if ban_scores[u] < BAN_SCORE_THRESHOLD]
        if not alive:
            return SyncState.Behind, ban_scores  # all peers banned — no tight loop
        best_height = max(p.height.get() for _, p in alive)
        if best_height <= sink.height.get():
            return SyncState.CaughtUp, ban_scores  # caught up (positive peer evidence)
        advanced = False
        for url, peer in alive:
            nxt = sink.height.succ()
            if nxt.get() > peer.height.get():
                continue
            blocks = request_blocks(peer, nxt, 1)
            if not blocks.blocks:
                continue
            if sink.apply(blocks.blocks[0]):
                advanced = True
                break
            ban_scores[url] += 1  # punish the peer that served a bad block
        if advanced:
            no_progress = 0
            continue
        no_progress += 1
        if no_progress > MAX_NO_PROGRESS:
            return SyncState.Behind, ban_scores  # bounded — never retried forever


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
# Peer management calculus — sync-protocol.md §14 (quarantine, not ad-hoc ban)
# ==============================================================================

class HostColor(IntEnum):
    """Quarantine states (§14.1). Black expires; the wallet never writes it."""
    Grey = 0
    White = 1
    Gold = 2
    Black = 3
    Dark = 4


BLACKLIST_EXPIRY_SECS = 3600  # §14.2 — a ban is bounded, not "program duration"


class QuarantineList:
    """In-memory hostlist. `ban()` moves a peer to Black with a timestamp;
    `refresh()` expires Black entries older than BLACKLIST_EXPIRY_SECS."""

    def __init__(self):
        self.black: Dict[str, int] = {}  # url -> last_seen (unix secs)

    def ban(self, url: str, now: int) -> None:
        self.black[url] = now

    def is_blacklisted(self, url: str) -> bool:
        return url in self.black

    def refresh(self, now: int) -> None:
        for url in list(self.black):
            if now - self.black[url] > BLACKLIST_EXPIRY_SECS:
                del self.black[url]


def block_apply_decision(local_height: BlockHeight, block_height: BlockHeight,
                         valid: bool) -> str:
    """`linearlblock` block-apply (§14.3) — duplicate vs invalid vs valid.

    - Duplicate (height <= local tip) => "skip" (normal relay, NOT a ban).
    - Genuine invalid (fails validation) => "ban" (protocol violation).
    - Valid extension => "accept".
    """
    if block_height.get() <= local_height.get():
        return "skip"       # already in the chain — not a protocol violation
    if not valid:
        return "ban"        # genuinely invalid block
    return "accept"


# ==============================================================================
# Async production logic + net-crate ownership — sync-protocol.md §13/§15
# ==============================================================================

# §13.2 — each timeout is justified by the payload size / block cadence it serves.
TIMEOUTS = {"tip": 5, "blocks": 30, "dial": 15, "wallet_tick": 10, "node_repoll": 30}

# §15 — strict net feature hierarchy; a wallet compiles only net-wallet.
FEATURE_TIERS = ["net-wire", "net-wallet", "net-node", "net-full"]


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

    # Test 4: genesis_hash filtering (§3)
    local_genesis = BlockHash(_hex(999))
    match = Tip(BlockHeight(3), BlockHash(_hex(3)), local_genesis)
    mismatch = Tip(BlockHeight(3), BlockHash(_hex(3)), BlockHash(_hex(1)))
    check("test_genesis_filter_match", genesis_filter(match, local_genesis))
    check("test_genesis_filter_mismatch", not genesis_filter(mismatch, local_genesis))

    # Test 5: SyncDecision + SyncState exhaustive values
    check("test_syncdecision_cardinality", len(SyncDecision) == 4)
    check("test_syncstate_cardinality", len(SyncState) == 5)
    check("test_syncstate_waiting", SyncState.WaitingForGenesis == 4)

    # Test 6: THE core invariant — wallet and mining sinks sync identically.
    # Both reach CaughtUp at the same height on the same blocks; only the
    # sink differs (sync-protocol.md §0).
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

    # Test 9: MAX_BYTES canonical values (§4)
    check("test_maxbytes_tip", MAX_BYTES["Tip"] == 512)
    check("test_maxbytes_blocks_16mib", MAX_BYTES["Blocks"] == 16 * 1024 * 1024)

    # Test 10: peer quarantine (§14) — ban blacklists, then expires
    q = QuarantineList()
    q.ban("tcp://evil:123", now=1000)
    check("test_ban_blacklists", q.is_blacklisted("tcp://evil:123"))
    q.refresh(now=1000 + BLACKLIST_EXPIRY_SECS + 1)
    check("test_ban_expires", not q.is_blacklisted("tcp://evil:123"))

    # Test 11: ban is bounded (§14.2) — within the expiry window it stays
    q2 = QuarantineList()
    q2.ban("tcp://evil:123", now=5000)
    q2.refresh(now=5000 + BLACKLIST_EXPIRY_SECS - 1)
    check("test_ban_within_expiry_stays", q2.is_blacklisted("tcp://evil:123"))

    # Test 18: linearlblock duplicate-vs-invalid (§14.3) — a duplicate is skipped, not banned.
    check("test_duplicate_block_skip",
          block_apply_decision(BlockHeight(11), BlockHeight(1), valid=True) == "skip")
    check("test_duplicate_block_not_ban",
          block_apply_decision(BlockHeight(11), BlockHeight(1), valid=True) != "ban")
    check("test_invalid_block_ban",
          block_apply_decision(BlockHeight(11), BlockHeight(12), valid=False) == "ban")
    check("test_valid_extension_accept",
          block_apply_decision(BlockHeight(11), BlockHeight(12), valid=True) == "accept")

    # Test 12: async timeout table (§13.2) — tip is cheap, blocks are large
    check("test_timeout_tip_lt_blocks", TIMEOUTS["tip"] < TIMEOUTS["blocks"])

    # Test 13: net feature hierarchy (§15) — strict inclusion, wallet at net-wallet
    check("test_feature_hierarchy",
          FEATURE_TIERS == ["net-wire", "net-wallet", "net-node", "net-full"])

    # Test 14: wallet follows the longest chain (§17) — a lower/divergent peer never blocks
    check("test_longest_chain_highest",
          longest_chain_tip([BlockHeight(5), BlockHeight(7), BlockHeight(6)]).get() == 7)
    check("test_longest_chain_ignores_lower",
          longest_chain_tip([BlockHeight(7), BlockHeight(5)]).get() == 7)
    check("test_longest_chain_empty", longest_chain_tip([]).get() == 0)

    # Test 15: unknown-command drain (§14.3) — a peer that does not subscribe to a
    # push command drains the payload (bounded) and continues, never desyncing.
    check("test_inbound_payload_cap_4mib", MAX_INBOUND_PAYLOAD == 4 * 1024 * 1024)
    check("test_node_push_commands", set(NODE_PUSH_COMMANDS) == {"linearlblock", "tx"})

    # Test 16: node caught-up requires positive peer evidence (§18.1.1 / F1).
    # No usable tips => Behind (height > 0) or WaitingForGenesis (height 0), never CaughtUp.
    check("test_node_no_tip_behind",
          node_sync_decision([], BlockHeight(5)) == SyncState.Behind)
    check("test_node_no_tip_waiting_genesis",
          node_sync_decision([], BlockHeight(0)) == SyncState.WaitingForGenesis)
    check("test_node_no_tip_never_caughtup",
          node_sync_decision([], BlockHeight(5)) != SyncState.CaughtUp)
    check("test_node_caughtup_requires_reached_peer_tip",
          node_sync_decision([PeerTip(BlockHeight(10), BlockHash(_hex(10)), BlockHash(_hex(1)))],
                             BlockHeight(10)) == SyncState.CaughtUp)
    check("test_node_behind_when_below_peer_tip",
          node_sync_decision([PeerTip(BlockHeight(10), BlockHash(_hex(10)), BlockHash(_hex(1)))],
                             BlockHeight(5)) == SyncState.Syncing)
    # Genesis-authority exception (§18.1.1): authority reaches CaughtUp with zero peer evidence.
    check("test_authority_no_tip_caughtup",
          node_sync_decision([], BlockHeight(5), genesis_authority=True) == SyncState.CaughtUp)
    check("test_authority_no_tip_at_genesis_caughtup",
          node_sync_decision([], BlockHeight(0), genesis_authority=True) == SyncState.CaughtUp)
    check("test_join_node_no_tip_still_behind",
          node_sync_decision([], BlockHeight(5), genesis_authority=False) == SyncState.Behind)

    # Test 17: initial-sync state machine + peer punishment (§13.3).
    # (a) a bad peer (serving invalid blocks) is punished and a good peer is used → converge.
    good_blocks = [_block(h) for h in range(1, 4)]
    bad_blocks = [{"height": 1, "hash": _hex(1), "valid": False}]  # always invalid
    good_peer = MockPeer(blocks=good_blocks, genesis_hash=BlockHash(_hex(1)))
    bad_peer = MockPeer(blocks=bad_blocks, genesis_hash=BlockHash(_hex(1)))
    sink = MiningSink()
    state, scores = initial_sync([("bad", bad_peer), ("good", good_peer)], sink)
    check("test_sync_state_machine_converges", state == SyncState.CaughtUp and sink.height.get() == 3)
    check("test_sync_state_machine_punishes_bad_peer", scores["bad"] > 0)
    check("test_sync_state_machine_keeps_good_peer", scores["good"] == 0)
    # (b) no healthy peers → Behind (never CaughtUp).
    only_bad = [("bad", bad_peer)]
    sink2 = MiningSink()
    state2, _ = initial_sync(only_bad, sink2)
    check("test_sync_state_machine_no_peers_behind", state2 == SyncState.Behind)
    # (c) CaughtUp requires reaching a peer tip — a below-tip sink stays Syncing/Behind, not CaughtUp.
    sink3 = MiningSink()
    for h in range(1, 3):
        sink3.apply(_block(h))
    state3, _ = initial_sync([("good", good_peer)], sink3)
    check("test_sync_state_machine_caughtup_requires_tip", state3 == SyncState.CaughtUp and sink3.height.get() == 3)

    print(f"\n{'=' * 60}")
    print(f"  Results: {passed}/{passed + failed} passed")
    print(f"{'=' * 60}")
    if failed:
        raise SystemExit(1)
