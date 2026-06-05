#!/usr/bin/env python3
"""
DarkWow Merge Mining Dockernet Model.

4 containers: monerod + 3 mining nodes (2 merge-mining, 1 native).
Each merge-mining node is self-contained: dwowd + p2pool sidecar + xmrig sidecar.
No standalone p2pool container needed.

Merge mining flow:
  monerod → ZMQ → p2pool (sidecar) → stratum → xmrig (sidecar) → share → p2pool → mm_rpc → dwowd → DarkWow block
"""

import hashlib, struct, time
from dataclasses import dataclass, field
from typing import Optional, List

from dockernet_model import (
    U32_MAX, PoWConsensus, ChainState, Block, BlockHeader, Transaction,
    derive_key_from_height, _mining_blob, hash_block, check_block_header,
    P2PNetwork, MiningNode,
)

# ============================================================================
# Wallet Derivation — shared-seed dual-key generation
# ============================================================================
# In production, DarkWow uses the Pallas curve and Monero uses Ed25519.
# The model uses blake2b-based deterministic derivation from a shared
# seed: the same seed + different derivation paths → unlinkable keypairs.
# Per-block address cycling: seed + height → unique address per block.

def derive_dual_keys(seed: bytes, height: int):
    """
    Derive both DarkWow and Monero keypairs from a shared seed.
    Different derivation paths ensure the keys are unlinkable.
    Returns (darkwow_address, monero_address) as hex strings.
    """
    # DarkWow: seed || "darkwow" || height (Pallas derivation path)
    dw_material = seed + b"darkwow" + struct.pack("<Q", height)
    dw_key = hashlib.blake2b(dw_material, digest_size=32).digest()
    darkwow_addr = dw_key.hex()

    # Monero: seed || "monero" || height (Ed25519 derivation path)
    xmr_material = seed + b"monero" + struct.pack("<Q", height)
    xmr_key = hashlib.blake2b(xmr_material, digest_size=32).digest()
    monero_addr = xmr_key.hex()

    return darkwow_addr, monero_addr


class MiningWallet:
    """
    Manages a shared seed and derives per-block addresses.
    Each block uses a unique address derived from seed + height,
    preventing linkage across blocks.

    The pipeline Phase 3 generates a 32-byte seed. This wallet
    produces the DarkWow and Monero addresses for each block
    that the miner produces.
    """

    def __init__(self, seed: Optional[bytes] = None):
        self.seed = seed or hashlib.blake2b(b"testnet-wallet-seed", digest_size=32).digest()
        self._cache: dict = {}

    def darkwow_address(self, height: int) -> str:
        return derive_dual_keys(self.seed, height)[0]

    def monero_address(self, height: int) -> str:
        return derive_dual_keys(self.seed, height)[1]

    def coinbase_recipients(self, height: int) -> tuple:
        """Returns (darkwow_addr, monero_addr) for the coinbase at this height."""
        return derive_dual_keys(self.seed, height)


def test_wallet_derivation():
    """Verify wallet derivation properties."""
    w = MiningWallet(b"test-seed-" + b"\x00" * 23)

    # Deterministic: same seed + height → same addresses
    a1 = w.darkwow_address(1)
    a2 = w.darkwow_address(1)
    assert a1 == a2, "Same height must produce same address"

    # Unlinkable: different heights → different addresses
    a3 = w.darkwow_address(2)
    assert a1 != a3, "Different heights must produce different addresses"

    # Cross-chain: DarkWow and Monero addresses differ
    dw, xmr = w.coinbase_recipients(5)
    assert dw != xmr, "DarkWow and Monero addresses must differ"

    # Different seeds → different addresses
    w2 = MiningWallet(b"other-seed-" + b"\x00" * 22)
    assert w.darkwow_address(1) != w2.darkwow_address(1), "Different seeds must differ"

    return True


# ============================================================================
# Monerod
# ============================================================================

MONERO_DIFFICULTY = 1000

@dataclass
class MoneroBlock:
    height: int; previous_hash: bytes; timestamp: int
    difficulty: int = MONERO_DIFFICULTY; nonce: int = 0; hash: bytes = b''

class MoneroNode:
    def __init__(self):
        self.height = 0; self.blocks: List[MoneroBlock] = []
        self.subscribers: List['P2PoolSidecar'] = []
    def subscribe(self, p): self.subscribers.append(p)
    def _hash(self, b): return hashlib.blake2b(struct.pack('<Q',b.height)+b.previous_hash+struct.pack('<QI',b.timestamp,b.difficulty)+struct.pack('<Q',b.nonce),digest_size=32).digest()
    def _check(self, h, d): return struct.unpack('<I',h[0:4])[0] <= d
    def mine(self):
        h = self.height+1; ph = self.blocks[-1].hash if self.blocks else b'\x00'*32
        for n in range(5_000_000):
            b = MoneroBlock(h, ph, int(time.time()), MONERO_DIFFICULTY, n)
            b.hash = self._hash(b)
            if self._check(b.hash, MONERO_DIFFICULTY):
                self.blocks.append(b); self.height = h
                for s in self.subscribers: s.on_block(b)
                return b
        return None

# ============================================================================
# P2Pool + Xmrig (sidecars in merge-mining node container)
# ============================================================================

SHARE_DIFFICULTY = 100

@dataclass
class Share:
    monero_height: int; nonce: int; miner: str = ""

class P2PoolSidecar:
    def __init__(self, nid, chain):
        self.nid = nid; self.chain = chain
        self.block: Optional[MoneroBlock] = None
        self.shares = 0; self.submitted = 0
    def on_block(self, b): self.block = b
    def check(self, s):
        if not self.block or s.monero_height != self.block.height: return False
        d = struct.pack('<Q',s.monero_height)+struct.pack('<Q',s.nonce)+s.miner.encode()
        return struct.unpack('<I',hashlib.blake2b(d,digest_size=32).digest()[0:4])[0] <= SHARE_DIFFICULTY
    def submit(self, s):
        self.shares += 1
        chain = self.chain; cur = chain.get_latest_block()
        if not cur: return False
        h = cur.header.height+1; t = chain.consensus.target
        key = derive_key_from_height(h)
        prev = hashlib.blake2b(_mining_blob(cur.header),digest_size=32).digest()
        header = BlockHeader(previous=prev,height=h,target=t,randomx_key=key,timestamp=int(time.time()))
        block = Block(header=header, transactions=[Transaction(reward=13_837_500_000_000 // max(1,h))])
        try:
            chain.connect_block(block)
            self.submitted += 1
            return True
        except Exception: return False

class XmrigSidecar:
    def __init__(self, nid, p2pool): self.nid = nid; self.p2pool = p2pool; self.found = 0
    def mine(self):
        b = self.p2pool.block
        if not b: return None
        for n in range(50000):
            s = Share(b.height, n, f"miner-{self.nid}")
            if self.p2pool.check(s):
                self.found += 1
                return s
        return None

# ============================================================================
# MergeMiningNode (self-contained container)
# ============================================================================

class MergeMiningNode:
    def __init__(self, nid, genesis, p2p, monerod, wallet: MiningWallet = None):
        self.nid = nid; self.chain = ChainState(); self.p2p = p2p
        self.wallet = wallet or MiningWallet()
        self.p2pool = P2PoolSidecar(nid, self.chain)
        self.xmrig = XmrigSidecar(nid, self.p2pool)
        self.blocks = 0; self.synced = False; self.enabled = False
        p2p.register(nid); monerod.subscribe(self.p2pool)
        if genesis: self._genesis()
    def _genesis(self):
        k = derive_key_from_height(1)
        h = BlockHeader(previous=b'\x00'*32,height=1,target=U32_MAX,randomx_key=k,timestamp=int(time.time()))
        dw_addr, _ = self.wallet.coinbase_recipients(1)
        self.chain.connect_block(Block(header=h,transactions=[Transaction(reward=13_837_500_000_000)]))
    def get_height(self): return self.chain.get_height()
    def start(self): self.synced = True; self.enabled = True
    def mine_one(self):
        if not self.enabled: return None
        # Derive addresses for this block height
        h = self.get_height() + 1
        dw_addr, xmr_addr = self.wallet.coinbase_recipients(h)
        # xmrig mines using the Monero address for this block height
        s = Share(0, 0, xmr_addr[:16])  # placeholder — actual nonce comes from xmrig.mine()
        # xmrig finds shares identified by the Monero address
        actual_s = self.xmrig.mine()
        if actual_s:
            # Attach the wallet-derived Monero address
            actual_s.miner = xmr_addr[:32]
        if actual_s and self.p2pool.submit(actual_s):
            self.blocks += 1
            b = self.chain.get_latest_block()
            if b: self.p2p.broadcast(self.nid, b); return b
        return None
    def sync(self, src):
        for h in range(self.get_height()+1, src.get_height()+1):
            b = src.chain.get_block(h)
            if b:
                try: self.chain.connect_block(b)
                except Exception: pass
    def recv(self, others):
        for t, p in self.p2p.receive(self.nid):
            if t == "BlockBroadcast":
                try: self.chain.connect_block(p)
                except Exception: pass

# ============================================================================
# Simulation
# ============================================================================

def _h(n): return n.get_height() if hasattr(n, 'get_height') else n.chain.get_height()

def run_merge(num=15):
    print(f"=== Merge Mining Dockernet Model ===\n")
    p2p = P2PNetwork(); m = MoneroNode()
    n0 = MergeMiningNode("node0", True, p2p, m)
    n1 = MergeMiningNode("node1", False, p2p, m)
    n2 = MiningNode("node2", False, p2p)
    alln = [n0, n1, n2]

    print("Phase 1: Monerod")
    for _ in range(3): m.mine()

    print("\nPhase 2: Sync")
    n1.sync(n0)
    for h in range(1, n0.get_height()+1):
        b = n0.chain.get_block(h)
        if b:
            try: n2.chain.connect_block(b)
            except Exception: pass
    n1.start(); n2.sync_complete = True; n2.mining_enabled = True
    print(f"  n0={n0.get_height()} n1={n1.get_height()} n2={n2.get_height()}")

    print(f"\nPhase 3: Mining (target {num} blocks)")
    for r in range(1, 5000):
        if r % 5 == 0: m.mine()
        n0.mine_one(); n1.mine_one(); n2.mine_one_block()
        for n in alln:
            if hasattr(n, 'recv'): n.recv(alln)
            else: n.process_p2p_messages(n)
        if r % 10 == 0:
            print(f"  [r{r}] n0={n0.get_height()} n1={n1.get_height()} n2={n2.get_height()} "
                  f"monero={m.height} p2pool=({n0.p2pool.submitted},{n1.p2pool.submitted})")
        if max(_h(n) for n in alln) >= num: break

    print(f"\nPhase 4: Consensus")
    mx = max(_h(n) for n in alln)
    src = max(alln, key=lambda n: _h(n))
    for n in alln:
        for h in range(_h(n)+1, mx+1):
            b = src.chain.get_block(h)
            if b:
                try: n.chain.connect_block(b)
                except Exception: pass

    ok = True
    for h in range(1, mx+1):
        hs = {n.chain.hashes.get(h,"?") for n in alln}
        if len(hs)>1: ok = False
    print(f"  monero={m.height} n0={n0.get_height()} n1={n1.get_height()} n2={n2.get_height()}")
    print(f"  === {'ALL VERIFIED' if ok else 'FAILURE'} ===")
    return ok

if __name__ == "__main__":
    run_merge(15)
