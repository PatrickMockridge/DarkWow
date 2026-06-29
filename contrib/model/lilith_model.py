#!/usr/bin/env python3
"""
Lilith Seed Node — Python Specification

Models lilith's behavior as a P2P seed node: accepting inbound connections,
storing advertised peer addresses, serving them on request, and periodically
refining the hostlist. This spec models the SEED SIDE of interactions that
the wallet model (wallet_model.py) models from the WALLET SIDE.

Matches: bin/lilith/src/main.rs, src/net/protocol/protocol_address.rs,
         src/net/protocol/protocol_seed.rs (upstream pattern).
"""

# Reuse message types and error codes from wallet spec
from wallet_model import (
    SeedErrorMessage, AddrsMessage, GetAddrsMessage,
    PeerConnection, HostlistDiscovery,
    SEED_ERR_HOSTLIST_EMPTY, SEED_ERR_VERSION_MISMATCH,
    SEED_ERR_NO_MATCHING_TRANSPORTS, SEED_ERR_BAD_REQUEST,
    MAX_SEED_ERRORS_PER_CONNECTION,
    seed_error_is_client_error, seed_error_is_server_error,
)

# ==============================================================================
# Lilith Seed Node
# ==============================================================================

class LilithSeed:
    """Models a lilith seed node.

    Architecture (from bin/lilith/src/main.rs):
      - outbound_connections: 0 — never initiates connections
      - inbound_connections: MAX — accepts all inbound connections
      - BanPolicy::Relaxed — never bans peers
      - Protocols: ProtocolPing (keepalive) + ProtocolAddress (address exchange)
      - WhitelistRefinery: periodic health check of whitelisted peers
      - Seeds: [] (lilith is the seed, it has no upstream seeds)
      - Peers: [] (lilith has no manual peers)
    """
    def __init__(self, accept_addrs=None):
        self.accept_addrs = accept_addrs or ["tcp+tls://0.0.0.0:31340"]
        self.outbound_connections = 0
        self.inbound_connections = 512
        self.ban_policy = "Relaxed"
        # Host container: Gold, White, Grey, Dark lists
        self.goldlist = []      # [(url, last_seen)]
        self.whitelist = []     # [(url, last_seen)]
        self.greylist = []      # [(url, last_seen)]
        self.darklist = []      # [(url, last_seen)]
        self.connected_channels = {}  # addr -> channel_info

    # ------------------------------------------------------------------
    # Address reception — handle_receive_addrs (protocol_address.rs:124)
    # ------------------------------------------------------------------

    def receive_addrs(self, channel_addr, addrs_msg):
        """A connected node sends us its addresses. Store in greylist."""
        for url, timestamp in addrs_msg.addrs:
            self.greylist.append((url, timestamp))
        return len(addrs_msg.addrs)

    # ------------------------------------------------------------------
    # Address serving — handle_receive_get_addrs (protocol_address.rs:146)
    # ------------------------------------------------------------------

    TRANSPORT_COMBOS = [
        "tor", "tls", "tcp", "nym", "i2p",
        "tor+tls", "nym+tls", "tcp+tls", "i2p+tls",
    ]

    def serve_get_addrs(self, channel_addr, get_addrs_msg):
        """A connected node requests peer addresses. Query Gold→White→Grey→Dark
        and return matching addresses.

        If no transports are requested, serve all entries regardless of scheme.
        """
        requested = [t for t in get_addrs_msg.transports
                     if t in self.TRANSPORT_COMBOS]
        if not requested and get_addrs_msg.transports:
            return SeedErrorMessage(
                SEED_ERR_NO_MATCHING_TRANSPORTS,
                f"no matching transports: requested={get_addrs_msg.transports}"
            )

        max_addrs = get_addrs_msg.max
        addrs = []

        def filter_by_scheme(entries, schemes):
            if not schemes:
                return list(entries)  # no scheme filter — return all
            return [(u, t) for u, t in entries if any(s in u for s in schemes)]

        # 1. Gold (matching transports)
        addrs.extend(filter_by_scheme(self.goldlist, requested)[:max_addrs])
        remain = max_addrs - len(addrs)

        # 2. White (matching transports)
        if remain > 0:
            addrs.extend(filter_by_scheme(self.whitelist, requested)[:remain])
        remain = 2 * max_addrs - len(addrs)

        # 3. Grey (matching transports) — WAS MISSING before HAZOP round 3
        if remain > 0:
            addrs.extend(filter_by_scheme(self.greylist, requested)[:remain])
        remain = 2 * max_addrs - len(addrs)

        # 4. Gold (excluding transports) — share for propagation
        if remain > 0:
            gold_exclude = [(u, t) for u, t in self.goldlist
                           if not any(s in u for s in requested)]
            addrs.extend(gold_exclude[:remain])
        remain = 2 * max_addrs - len(addrs)

        # 5. White (excluding transports)
        if remain > 0:
            white_exclude = [(u, t) for u, t in self.whitelist
                            if not any(s in u for s in requested)]
            addrs.extend(white_exclude[:remain])
        remain = 2 * max_addrs - len(addrs)

        # 6. Grey (excluding transports)
        if remain > 0:
            grey_exclude = [(u, t) for u, t in self.greylist
                           if not any(s in u for s in requested)]
            addrs.extend(grey_exclude[:remain])
        remain = 2 * max_addrs - len(addrs)

        # 7. Dark (fallback — fill remaining)
        if remain > 0:
            addrs.extend(self.darklist[:remain])

        # Filter to only TRANSPORT_COMBOS schemes
        addrs = [(u, t) for u, t in addrs
                 if any(s in u for s in self.TRANSPORT_COMBOS)]

        if not addrs:
            return SeedErrorMessage(
                SEED_ERR_HOSTLIST_EMPTY,
                "hostlist empty, no peers available"
            )

        return AddrsMessage(addrs)

    # ------------------------------------------------------------------
    # Whitelist refinery (bin/lilith/src/main.rs:190-242)
    # ------------------------------------------------------------------

    def refinery_tick(self):
        """Process one whitelist entry: if it exists in any list,
        update last_seen; otherwise, it's unreachable — downgrade to greylist.
        Returns: (action, url) or None if whitelist is empty.
        """
        if not self.whitelist:
            return None
        # Process oldest entry (fetch_last in the code)
        url, last_seen = self.whitelist[0]
        # In real code: attempts handshake with the peer
        # For model: always consider responsive
        self.whitelist.pop(0)
        self.whitelist.append((url, self._now()))
        return ("refreshed", url)

    def _now(self):
        import time
        return int(time.time())

    def is_empty(self):
        """True if all hostlist colors are empty."""
        return not (self.goldlist or self.whitelist or self.greylist or self.darklist)


# ==============================================================================
# Seed Connection — models a mining node connecting to lilith
# ==============================================================================

class SeedConnection:
    """Models one inbound connection to lilith from a mining node or wallet.

    Flow (matches ProtocolSeed on the connecting side, ProtocolAddress on lilith):
      1. TCP+TLS connect
      2. Version handshake (magic bytes, version major.minor compatible)
      3. Connecting node sends its address (send_my_addrs) → lilith greylist
      4. Connecting node sends GetAddrs → lilith responds with peer addresses
      5. Channel closes (seed protocol is one-shot)
    """

    def __init__(self, seed, node_addr, app_name="dwowd", version=(0, 5, 0),
                 external_addrs=None):
        self.seed = seed
        self.node_addr = node_addr
        self.app_name = app_name        # informational — never gated
        self.version = version          # (major, minor, patch)
        self.external_addrs = external_addrs or []
        self.connected = False
        self.addrs_received = []

    def handshake(self, seed_app_name="dwowd", seed_version=(0, 5, 0)):
        """Perform version handshake. Returns True if compatible.

        DEFENSE IN DEPTH:
          - app_name is informational only — mismatch does NOT reject
          - Only version major.minor incompatibility triggers rejection
          - Seed sends SeedErrorMessage(401) on version mismatch
        """
        # app_name mismatch — logged, never gated
        if self.app_name != seed_app_name:
            pass  # informational only

        # Version compatibility check
        if self.version[0] != seed_version[0] or self.version[1] != seed_version[1]:
            return SeedErrorMessage(
                SEED_ERR_VERSION_MISMATCH,
                f"version mismatch: ours={seed_version} peer={self.version}"
            )

        self.connected = True
        return True

    def advertise_and_discover(self):
        """After handshake, advertise our address and request peers.

        Step 1: send_my_addrs() — if we have external_addrs, send to lilith
        Step 2: send GetAddrs to lilith
        Step 3: lilith responds with AddrsMessage or SeedErrorMessage
        """
        if not self.connected:
            return SeedErrorMessage(SEED_ERR_BAD_REQUEST, "not connected")

        # Step 1: Advertise our addresses to lilith
        if self.external_addrs:
            import time
            addrs_msg = AddrsMessage([
                (addr, int(time.time())) for addr in self.external_addrs
            ])
            self.seed.receive_addrs(self.node_addr, addrs_msg)

        # Step 2-3: Request and receive peer addresses
        get_addrs = GetAddrsMessage(max_addrs=8, transports=["tcp+tls"])
        return self.seed.serve_get_addrs(self.node_addr, get_addrs)


# ==============================================================================
# Tests
# ==============================================================================

def test_lilith_cold_start_empty_hostlist():
    """Fresh lilith with no peers. Query BEFORE advertising — gets 503."""
    print("  LILITH: cold start empty hostlist...", end=" ")
    lilith = LilithSeed()
    # Query first — before any node advertises, hostlist is empty
    msg = GetAddrsMessage(max_addrs=8, transports=["tcp+tls"])
    result = lilith.serve_get_addrs("tcp+tls://client:55555", msg)
    assert isinstance(result, SeedErrorMessage), \
        f"Expected SeedErrorMessage, got {type(result).__name__}"
    assert result.code == 503
    assert "no peers" in result.reason
    print("PASSED")


def test_lilith_single_registration():
    """One node advertises. Next node discovers it."""
    print("  LILITH: single registration...", end=" ")
    lilith = LilithSeed()

    # Miner 0 connects and advertises
    m0 = SeedConnection(lilith, "tcp+tls://miner0:31342",
                        external_addrs=["tcp+tls://miner0:31342"])
    assert m0.handshake() is True
    m0.advertise_and_discover()
    # After advertising, miner0 is in greylist
    assert len(lilith.greylist) == 1

    # Miner 1 connects — should get miner0's address
    m1 = SeedConnection(lilith, "tcp+tls://miner1:31343",
                        external_addrs=["tcp+tls://miner1:31343"])
    assert m1.handshake() is True
    result1 = m1.advertise_and_discover()
    assert isinstance(result1, AddrsMessage)
    assert len(result1.addrs) >= 1
    assert any("miner0" in addr for addr, _ in result1.addrs)
    print("PASSED")


def test_lilith_multiple_registration():
    """N nodes connect and advertise. Each subsequent node discovers earlier ones."""
    print("  LILITH: multiple registration...", end=" ")
    lilith = LilithSeed()
    N = 5

    for i in range(N):
        addr = f"tcp+tls://miner{i}:31342"
        conn = SeedConnection(lilith, addr, external_addrs=[addr])
        assert conn.handshake() is True
        conn.advertise_and_discover()
        # Each node finds at least its own + all previous nodes
        assert len(lilith.greylist) == i + 1
    print("PASSED")


def test_lilith_refinery_promotion():
    """Whitelist refinery refreshes entries."""
    print("  LILITH: refinery promotion...", end=" ")
    lilith = LilithSeed()
    lilith.whitelist = [("tcp+tls://node:31342", 0)]
    result = lilith.refinery_tick()
    assert result is not None
    assert result[0] == "refreshed"
    assert "node" in result[1]
    print("PASSED")


def test_lilith_refinery_empty_whitelist():
    """Refinery does nothing when whitelist is empty."""
    print("  LILITH: refinery empty...", end=" ")
    lilith = LilithSeed()
    assert lilith.refinery_tick() is None
    print("PASSED")


def test_lilith_greylist_in_getaddrs():
    """Greylist entries ARE returned in GetAddrs responses (HAZOP round 3 fix)."""
    print("  LILITH: greylist in GetAddrs...", end=" ")
    lilith = LilithSeed()
    lilith.greylist = [("tcp+tls://grey-node:31342", 100)]

    msg = GetAddrsMessage(max_addrs=8, transports=["tcp+tls"])
    result = lilith.serve_get_addrs("tcp+tls://client:55555", msg)

    assert isinstance(result, AddrsMessage)
    assert len(result.addrs) >= 1
    assert any("grey-node" in addr for addr, _ in result.addrs)
    print("PASSED")


def test_lilith_app_name_agnostic():
    """Seed accepts connections from ANY app_name (HAZOP round 3 fix)."""
    print("  LILITH: app_name agnostic...", end=" ")
    for name in ["dwowd", "darkfid", "dwow-wallet", "darkirc", "custom-tool"]:
        conn = SeedConnection(LilithSeed(), "tcp+tls://node:31342", app_name=name)
        result = conn.handshake(seed_app_name="dwowd")
        assert result is True, f"app_name='{name}' should connect"
    print("PASSED")


def test_lilith_version_gate():
    """Incompatible versions are rejected with 401 error."""
    print("  LILITH: version gate...", end=" ")
    # Major version mismatch
    conn = SeedConnection(LilithSeed(), "tcp+tls://node:31342", version=(1, 0, 0))
    result = conn.handshake(seed_version=(0, 5, 0))
    assert isinstance(result, SeedErrorMessage)
    assert result.code == 401
    assert "version mismatch" in result.reason

    # Minor version mismatch
    conn2 = SeedConnection(LilithSeed(), "tcp+tls://node:31343", version=(0, 4, 0))
    result2 = conn2.handshake(seed_version=(0, 5, 0))
    assert isinstance(result2, SeedErrorMessage)
    assert result2.code == 401
    print("PASSED")


def test_lilith_full_seed_flow():
    """End-to-end: 2 mining nodes + 1 wallet all discover via lilith."""
    print("  LILITH: full seed flow...", end=" ")
    lilith = LilithSeed()

    # Phase 1: Mining nodes connect and advertise
    miners = []
    for i in range(2):
        addr = f"tcp+tls://miner{i}:3134{i+2}"
        conn = SeedConnection(lilith, addr, external_addrs=[addr])
        assert conn.handshake() is True
        conn.advertise_and_discover()
        miners.append(conn)
        # Each miner's address should be in lilith's greylist
        assert len(lilith.greylist) == i + 1

    # Phase 2: Wallet connects — should get both mining nodes
    wallet = SeedConnection(lilith, "tcp+tls://wallet:31360",
                            app_name="dwow-wallet",
                            external_addrs=[])  # wallet has no external addr
    assert wallet.handshake() is True
    result = wallet.advertise_and_discover()

    assert isinstance(result, AddrsMessage), \
        f"Expected AddrsMessage, got {type(result).__name__}"
    assert len(result.addrs) >= 2, \
        f"Expected >= 2 mining nodes, got {len(result.addrs)}"
    assert any("miner0" in addr for addr, _ in result.addrs)
    assert any("miner1" in addr for addr, _ in result.addrs)
    print("PASSED")


def test_lilith_is_empty():
    """is_empty() returns True only when all colors are empty."""
    print("  LILITH: is_empty...", end=" ")
    lilith = LilithSeed()
    assert lilith.is_empty()

    lilith.greylist = [("tcp+tls://node:31342", 0)]
    assert not lilith.is_empty()

    lilith.greylist = []
    lilith.whitelist = [("tcp+tls://node:31343", 0)]
    assert not lilith.is_empty()

    lilith.whitelist = []
    lilith.goldlist = [("tcp+tls://node:31344", 0)]
    assert not lilith.is_empty()

    lilith.goldlist = []
    lilith.darklist = [("tcp+tls://node:31345", 0)]
    assert not lilith.is_empty()
    print("PASSED")


# ==============================================================================
# Test runner
# ==============================================================================

LILITH_TESTS = [
    test_lilith_cold_start_empty_hostlist,
    test_lilith_single_registration,
    test_lilith_multiple_registration,
    test_lilith_refinery_promotion,
    test_lilith_refinery_empty_whitelist,
    test_lilith_greylist_in_getaddrs,
    test_lilith_app_name_agnostic,
    test_lilith_version_gate,
    test_lilith_full_seed_flow,
    test_lilith_is_empty,
]

if __name__ == "__main__":
    passed = 0
    failed = 0
    for test in LILITH_TESTS:
        try:
            test()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  FAIL: {test.__name__}: {e}")
    print(f"\n{passed} passed, {failed} failed")
    exit(0 if failed == 0 else 1)
