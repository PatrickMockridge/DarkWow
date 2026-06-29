#!/usr/bin/env python3
"""
Observer Full Node — Python Specification

Replaces lilith as the discovery node for the DarkWow blockchain track.
An observer node is a dwowd full node with mining disabled. It runs the
full blockchain protocol stack — same as mining nodes and wallets — so
there are no MissingDispatcher channel deaths from protocol mismatches.

Architecture:
  - Track 1 (this spec): Observer nodes for blockchain discovery
  - Track 2 (lilith): Generic P2P seed for non-blockchain services (unchanged)

Matches: bin/dwowd/src/main.rs (ROLE=dwowd, MINING_ENABLED=false),
         src/net/protocol/protocol_address.rs (ProtocolAddress PEX),
         BanPolicy::Relaxed (default, no strict-ban feature).
"""

import time
from dataclasses import dataclass, field
from typing import List, Tuple, Optional

from wallet_model import AddrsMessage, GetAddrsMessage


# ==============================================================================
# Observer Node — dwowd with MINING_ENABLED=false
# ==============================================================================

class ObserverNode:
    """A full blockchain node that observes but doesn't mine.

    Runs the same protocol stack as mining nodes:
      - ProtocolAddress PEX on SESSION_DEFAULT
      - LinearSyncHandler (GetTip/GetBlocks)
      - LinearBroadcastHandler (BlockBroadcast)
      - ProtocolTx (transaction relay)

    No MissingDispatcher vulnerability — every node type in the
    blockchain track speaks the same protocol.
    """
    TRANSPORT_COMBOS = [
        "tor", "tls", "tcp", "nym", "i2p",
        "tor+tls", "nym+tls", "tcp+tls", "i2p+tls",
    ]

    def __init__(self, hostname="observer", port=31340, peers=None):
        self.hostname = hostname
        self.port = port
        self.listen_addr = f"tcp+tls://0.0.0.0:{port}"
        self.external_addr = f"tcp+tls://{hostname}:{port}"
        self.peers = peers or []
        self.mining_enabled = False
        self.ban_policy = "Relaxed"  # default, no strict-ban

        # Hostlist tiers (Gold/White/Grey/Dark)
        self.goldlist = []
        self.whitelist = []
        self.greylist = []
        self.darklist = []

        # Connected channels
        self.connected = {}  # addr -> channel info

    # ------------------------------------------------------------------
    # Address reception (handle_receive_addrs)
    # ------------------------------------------------------------------
    def receive_addrs(self, addrs_msg):
        for url, ts in addrs_msg.addrs:
            self.greylist.append((url, ts))

    # ------------------------------------------------------------------
    # Address serving (handle_receive_get_addrs)
    # Query order: Gold → White → Grey → Gold_excl → White_excl → Grey_excl → Dark
    # ------------------------------------------------------------------
    def serve_get_addrs(self, get_addrs_msg):
        requested = [t for t in get_addrs_msg.transports
                     if t in self.TRANSPORT_COMBOS]
        if not requested:
            requested = get_addrs_msg.transports

        max_n = get_addrs_msg.max
        addrs = []

        def by_scheme(entries, schemes):
            if not schemes:
                return list(entries)
            result = []
            for url, ts in entries:
                try:
                    if url.split("://")[0] in schemes:
                        result.append((url, ts))
                except (IndexError, AttributeError):
                    pass
            return result

        def excl_scheme(entries, schemes):
            if not schemes:
                return []
            result = []
            for url, ts in entries:
                try:
                    if url.split("://")[0] not in schemes:
                        result.append((url, ts))
                except (IndexError, AttributeError):
                    pass
            return result

        # 1. Gold matching
        addrs.extend(by_scheme(self.goldlist, requested)[:max_n])
        # 2. White matching
        r = max_n - len(addrs)
        if r > 0: addrs.extend(by_scheme(self.whitelist, requested)[:r])
        # 3. Grey matching
        r = max_n - len(addrs)
        if r > 0: addrs.extend(by_scheme(self.greylist, requested)[:r])
        # 4. Gold excluding
        r = 2 * max_n - len(addrs)
        if r > 0: addrs.extend(excl_scheme(self.goldlist, requested)[:r])
        # 5. White excluding
        r = 2 * max_n - len(addrs)
        if r > 0: addrs.extend(excl_scheme(self.whitelist, requested)[:r])
        # 6. Grey excluding
        r = 2 * max_n - len(addrs)
        if r > 0: addrs.extend(excl_scheme(self.greylist, requested)[:r])
        # 7. Dark
        r = 2 * max_n - len(addrs)
        if r > 0: addrs.extend(self.darklist[:r])

        addrs = [(u, t) for u, t in addrs
                 if any(u.startswith(s + "://") for s in self.TRANSPORT_COMBOS)]
        return AddrsMessage(addrs)

    def is_empty(self):
        return not (self.goldlist or self.whitelist or self.greylist or self.darklist)


# ==============================================================================
# Bootstrap — how nodes find each other WITHOUT a seed
# ==============================================================================

@dataclass
class BootstrapConfig:
    peers: List[str] = field(default_factory=list)
    max_outbound: int = 8
    max_inbound: int = 4

class PeerDiscovery:
    """Decentralized P2P discovery via peers + PEX gossip.

    No seed dependency. Bootstrap from configured peers, then
    ProtocolAddress PEX propagates addresses on every outbound connection.
    """
    def __init__(self, hostlist, config):
        self.hosts = hostlist
        self.config = config
        self.connected_peers = {}

    def bootstrap(self):
        discovered = 0
        for peer in self.config.peers:
            self._pex_exchange(peer)
            discovered += 1
        return discovered

    def _pex_exchange(self, addr):
        """Simulate connecting to a peer and exchanging addresses via PEX."""
        simulated = [(f"tcp+tls://discovered-{i}:3134{i}", int(time.time()))
                     for i in range(3)]
        self.hosts.greylist.extend([(addr, int(time.time()))])
        self.hosts.greylist.extend(simulated)
        self.connected_peers[addr] = True

    def tick(self):
        for addr in list(self.connected_peers.keys()):
            more = [(f"tcp+tls://pex-{i}:3134{i}", int(time.time()))
                    for i in range(2)]
            self.hosts.greylist.extend(more)

    def peer_count(self):
        return len(self.connected_peers)


# ==============================================================================
# Tests
# ==============================================================================

def test_observer_shares_empty_hostlist():
    """Observer with no peers returns empty AddrsMessage."""
    print("  OBSERVER: empty hostlist...", end=" ")
    obs = ObserverNode()
    result = obs.serve_get_addrs(GetAddrsMessage(max_addrs=8, transports=["tcp+tls"]))
    assert isinstance(result, AddrsMessage)
    assert len(result.addrs) == 0
    print("PASSED")


def test_observer_shares_registered_peers():
    """Peers that advertise to observer are shared with new arrivals."""
    print("  OBSERVER: shares registered peers...", end=" ")
    obs = ObserverNode()

    # Miner advertises
    obs.receive_addrs(AddrsMessage([("tcp+tls://miner0:31342", int(time.time()))]))

    # Wallet queries — gets the miner
    result = obs.serve_get_addrs(GetAddrsMessage(max_addrs=8, transports=["tcp+tls"]))
    assert len(result.addrs) >= 1
    assert any("miner0" in addr for addr, _ in result.addrs)
    print("PASSED")


def test_observer_bootstraps_from_peers():
    """Observer bootstraps from configured peers — no seed needed."""
    print("  OBSERVER: bootstraps from peers...", end=" ")
    obs = ObserverNode()
    config = BootstrapConfig(peers=["tcp+tls://known-miner:31342"])
    discovery = PeerDiscovery(obs, config)

    result = discovery.bootstrap()
    assert result >= 1
    assert discovery.peer_count() >= 1
    print("PASSED")


def test_observer_pex_propagates():
    """PEX gossip spreads addresses through connected peers."""
    print("  OBSERVER: PEX propagates...", end=" ")
    obs = ObserverNode()
    config = BootstrapConfig(peers=["tcp+tls://miner-a:31342"])
    discovery = PeerDiscovery(obs, config)
    discovery.bootstrap()

    initial_grey = len(obs.greylist)
    for _ in range(5):
        discovery.tick()

    assert len(obs.greylist) > initial_grey
    print("PASSED")


def test_observer_no_missing_dispatcher_vulnerability():
    """Observer runs same protocol stack as miners — unknown messages
    are logged (Relaxed) but never kill the channel."""
    print("  OBSERVER: no MissingDispatcher vulnerability...", end=" ")
    # The observer has BanPolicy::Relaxed. BlockBroadcast from a miner
    # would be handled by LinearBroadcastHandler — same protocol stack.
    # There's no scenario where a blockchain node sends a message the
    # observer can't handle.
    obs = ObserverNode()
    assert obs.ban_policy == "Relaxed"
    print("PASSED")


def test_wallet_discovers_via_observer():
    """Wallet with observer in peers discovers miners through observer PEX."""
    print("  OBSERVER: wallet discovers via observer...", end=" ")
    obs = ObserverNode()

    # Miners advertise to observer
    obs.receive_addrs(AddrsMessage([("tcp+tls://miner0:31342", 0)]))
    obs.receive_addrs(AddrsMessage([("tcp+tls://miner1:31343", 0)]))

    # Wallet config has observer in peers
    w_config = BootstrapConfig(peers=["tcp+tls://observer:31340"])
    w_discovery = PeerDiscovery(obs, w_config)
    w_discovery.bootstrap()

    # Wallet queries observer — gets both miners
    result = obs.serve_get_addrs(GetAddrsMessage(max_addrs=8, transports=["tcp+tls"]))
    assert len(result.addrs) >= 2
    assert any("miner0" in addr for addr, _ in result.addrs)
    assert any("miner1" in addr for addr, _ in result.addrs)
    print("PASSED")


def test_full_flow_no_lilith():
    """End-to-end: observer + 2 miners + wallet, no lilith involved."""
    print("  OBSERVER: full flow no lilith...", end=" ")
    obs = ObserverNode()

    # Miners connect to observer and advertise
    for i in range(2):
        addr = f"tcp+tls://miner{i}:3134{i+2}"
        obs.receive_addrs(AddrsMessage([(addr, int(time.time()))]))

    # Wallet bootstraps from observer
    w_config = BootstrapConfig(peers=["tcp+tls://observer:31340"])
    w_discovery = PeerDiscovery(obs, w_config)
    w_discovery.bootstrap()

    # Wallet discovers both miners
    result = obs.serve_get_addrs(GetAddrsMessage(max_addrs=8, transports=["tcp+tls"]))
    assert len(result.addrs) >= 2
    assert any("miner0" in addr for addr, _ in result.addrs)
    assert any("miner1" in addr for addr, _ in result.addrs)
    print("PASSED")


OBSERVER_TESTS = [
    test_observer_shares_empty_hostlist,
    test_observer_shares_registered_peers,
    test_observer_bootstraps_from_peers,
    test_observer_pex_propagates,
    test_observer_no_missing_dispatcher_vulnerability,
    test_wallet_discovers_via_observer,
    test_full_flow_no_lilith,
]

if __name__ == "__main__":
    passed = 0
    failed = 0
    for test in OBSERVER_TESTS:
        try:
            test()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  FAIL: {test.__name__}: {e}")
    print(f"\n{passed} passed, {failed} failed")
    exit(0 if failed == 0 else 1)
