#!/usr/bin/env python3
"""
Lilith Seed Node — Python Specification

Models the ACTUAL behavior of lilith (bin/lilith/src/main.rs) plus the ONE
change we make: greylist sharing in GetAddrs responses (Option A).

Read this spec to understand what lilith actually does and what we changed.
"""

from wallet_model import (
    SeedErrorMessage, AddrsMessage, GetAddrsMessage,
    SEED_ERR_VERSION_MISMATCH,
)

# ==============================================================================
# Lilith Seed Node — matches bin/lilith/src/main.rs and src/net/hosts.rs
# ==============================================================================

class LilithSeed:
    """Models a lilith seed node.

    ACTUAL behavior (from bin/lilith/src/main.rs):
      - outbound_connections: 0 — never initiates
      - inbound_connections: MAX — accepts all
      - BanPolicy::Relaxed — never bans
      - Protocols: ProtocolPing + ProtocolAddress
      - WhitelistRefinery: periodic health check, responsive→keep, dead→grey
      - Seeds: [] (lilith IS the seed)
      - Peers: [] (no manual peers)

    Hostlist colors (from src/net/hosts.rs):
      - Gold (index 2): anchorlist — highest priority, guaranteed reachable
      - White (index 1): whitelist — responsive peers, verified by refinery
      - Grey (index 0): greylist — unverified peers, awaiting refinery check
      - Dark (index 4): darklist — non-compatible transports, propagated for diversity
      - Black (index 3): permanent ban list

    GetAddrs query order (from protocol_address.rs):
      Gold(matching) → White(matching) → Gold(excluding) → White(excluding) → Dark

    The Gold and White lists are populated by the refinery from Grey entries.
    Grey entries are mined nodes that connected but haven't been refinery-verified.
    """

    TRANSPORT_COMBOS = [
        "tor", "tls", "tcp", "nym", "i2p",
        "tor+tls", "nym+tls", "tcp+tls", "i2p+tls",
    ]

    def __init__(self, accept_addrs=None):
        self.accept_addrs = accept_addrs or ["tcp+tls://0.0.0.0:31340"]
        self.outbound_connections = 0
        self.inbound_connections = 512
        self.ban_policy = "Relaxed"
        self.goldlist = []      # [(url, last_seen)]
        self.whitelist = []     # [(url, last_seen)]
        self.greylist = []      # [(url, last_seen)]
        self.darklist = []      # [(url, last_seen)]

    # ------------------------------------------------------------------
    # Address reception — matches handle_receive_addrs (protocol_address.rs:116-136)
    # ------------------------------------------------------------------

    def receive_addrs(self, addrs_msg):
        """Store received addresses in the greylist.

        In the real code, these go through filter_addresses() which validates
        scheme, checks blacklist, filters self-addresses, and handles local/production
        boundaries. For the spec, we append directly.
        """
        for url, timestamp in addrs_msg.addrs:
            self.greylist.append((url, timestamp))

    # ------------------------------------------------------------------
    # Address serving — matches handle_receive_get_addrs (protocol_address.rs:141-227)
    # with ONE change: greylist entries are shared (Option A).
    # ------------------------------------------------------------------

    def serve_get_addrs(self, get_addrs_msg):
        """Query hostlist and return peer addresses.

        ACTUAL query order (protocol_address.rs):
          1. Gold (matching transports)
          2. White (matching transports)
          3. Gold (excluding transports)
          4. White (excluding transports)
          5. Dark (fill remaining)

        OPTION A CHANGE: Greylist is queried between White and excluding phases.
        Mining nodes that recently connected (in grey) are immediately
        shareable without waiting for refinery promotion. This is appropriate
        for a seed with BanPolicy::Relaxed — nodes that completed the version
        handshake have proven they can communicate.
        """
        requested = [t for t in get_addrs_msg.transports
                     if t in self.TRANSPORT_COMBOS]
        if not requested and get_addrs_msg.transports:
            # All transports filtered — continue with whatever we have
            requested = get_addrs_msg.transports

        max_addrs = get_addrs_msg.max
        addrs = []

        def by_scheme(entries, schemes):
            """Filter entries whose URL scheme matches any in schemes.
            Uses exact scheme matching like TRANSPORT_COMBOS.contains(&addr.0.scheme())."""
            result = []
            for url, ts in entries:
                try:
                    scheme = url.split("://")[0]
                    if scheme in schemes:
                        result.append((url, ts))
                except (IndexError, AttributeError):
                    pass
            return result

        def excluding_scheme(entries, schemes):
            """Filter entries whose URL scheme does NOT match any in schemes."""
            result = []
            for url, ts in entries:
                try:
                    scheme = url.split("://")[0]
                    if scheme not in schemes:
                        result.append((url, ts))
                except (IndexError, AttributeError):
                    pass
            return result

        # 1. Gold (matching)
        addrs.extend(by_scheme(self.goldlist, requested)[:max_addrs])

        # 2. White (matching)
        remain = max_addrs - len(addrs)
        if remain > 0:
            addrs.extend(by_scheme(self.whitelist, requested)[:remain])

        # OPTION A: 3. Grey (matching) — our change
        remain = max_addrs - len(addrs)
        if remain > 0:
            addrs.extend(by_scheme(self.greylist, requested)[:remain])

        # 4. Gold (excluding) — fill to 2*max
        remain = 2 * max_addrs - len(addrs)
        if remain > 0:
            addrs.extend(excluding_scheme(self.goldlist, requested)[:remain])

        # 5. White (excluding)
        remain = 2 * max_addrs - len(addrs)
        if remain > 0:
            addrs.extend(excluding_scheme(self.whitelist, requested)[:remain])

        # OPTION A: 6. Grey (excluding) — our change
        remain = 2 * max_addrs - len(addrs)
        if remain > 0:
            addrs.extend(excluding_scheme(self.greylist, requested)[:remain])

        # 7. Dark — fill remaining
        remain = 2 * max_addrs - len(addrs)
        if remain > 0:
            addrs.extend(self.darklist[:remain])

        # Filter to only TRANSPORT_COMBOS schemes (line 222 in protocol_address.rs)
        addrs = [(u, t) for u, t in addrs
                 if any(u.startswith(s + "://") for s in self.TRANSPORT_COMBOS)]

        # Real code always returns AddrsMessage, even if empty (not an error)
        return AddrsMessage(addrs)

    # ------------------------------------------------------------------
    # Whitelist refinery — matches whitelist_refinery (bin/lilith/src/main.rs:190-242)
    # ------------------------------------------------------------------

    def refinery_tick(self):
        """Process oldest whitelist entry.

        In the real code: attempts a handshake with the peer.
        - Responsive → stay on whitelist with updated last_seen
        - Unresponsive → downgrade to greylist

        Returns: ("refreshed", url) or ("downgraded", url) or None if empty.
        """
        if not self.whitelist:
            return None

        # Process oldest entry (fetch_last in real code)
        url, _ = self.whitelist.pop(0)

        # For spec: model both paths based on whether entry still exists
        # in any list (simulating refinery's handshake check)
        reachable = any(
            url in [e[0] for e in lst]
            for lst in [self.goldlist, self.greylist]
        )

        if reachable:
            self.whitelist.append((url, self._now()))
            return ("refreshed", url)
        else:
            self.greylist.append((url, self._now()))
            return ("downgraded", url)

    def _now(self):
        import time
        return int(time.time())

    def is_empty(self):
        """True if all hostlist colors are empty."""
        return not (self.goldlist or self.whitelist or self.greylist or self.darklist)


# ==============================================================================
# Seed Connection — models a node connecting to lilith
# ==============================================================================

class SeedConnection:
    """Models a node connecting to lilith via seed protocol.

    Flow:
      1. TCP+TLS connect → version handshake (major.minor compatibility)
      2. Node sends its external address to lilith (send_my_addrs)
         → lilith stores in greylist (handle_receive_addrs)
      3. Node sends GetAddrs → lilith responds with peer addresses
         → lilith's response NOW includes greylist entries (Option A)
      4. Channel closes (seed protocol is one-shot)
    """

    def __init__(self, seed, node_addr, app_name="dwowd", version=(0, 5, 0),
                 external_addrs=None):
        self.seed = seed
        self.node_addr = node_addr
        self.app_name = app_name
        self.version = version
        self.external_addrs = external_addrs or []
        self.connected = False

    def handshake(self, seed_version=(0, 5, 0)):
        """Version handshake. Only major.minor must be compatible.
        app_name is informational — never gated.
        Returns True on success, SeedErrorMessage(401) on version mismatch."""
        if self.version[0] != seed_version[0] or self.version[1] != seed_version[1]:
            return SeedErrorMessage(
                SEED_ERR_VERSION_MISMATCH,
                f"version mismatch: ours={seed_version} peer={self.version}"
            )
        self.connected = True
        return True

    def advertise_and_discover(self, transports=None):
        """Advertise our address to lilith, then request peers.

        Step 1: send_my_addrs — send our external addresses → lilith greylist
        Step 2: send GetAddrs → lilith returns peer addresses (now including grey)
        """
        if not self.connected:
            return None

        # Step 1: Advertise
        if self.external_addrs:
            import time
            msg = AddrsMessage([(addr, int(time.time())) for addr in self.external_addrs])
            self.seed.receive_addrs(msg)

        # Step 2: Request peers
        transports = transports or ["tcp+tls"]
        get_addrs = GetAddrsMessage(max_addrs=8, transports=transports)
        return self.seed.serve_get_addrs(get_addrs)


# ==============================================================================
# Tests — verify spec behavior matches intended architecture
# ==============================================================================

def test_cold_start_returns_empty_not_error():
    """Fresh lilith with no peers returns empty AddrsMessage (not an error).
    This matches the real code — handle_receive_get_addrs always returns
    AddrsMessage, even when empty."""
    print("  LILITH: cold start returns empty...", end=" ")
    lilith = LilithSeed()
    msg = GetAddrsMessage(max_addrs=8, transports=["tcp+tls"])
    result = lilith.serve_get_addrs(msg)
    assert isinstance(result, AddrsMessage)
    assert len(result.addrs) == 0
    print("PASSED")


def test_greylist_shared_after_registration():
    """OPTION A: After a mining node advertises to lilith, its address is
    in the greylist. A subsequent GetAddrs query returns it — no refinery
    wait needed. This is the change from upstream."""
    print("  LILITH: greylist shared after registration...", end=" ")
    lilith = LilithSeed()

    # Mining node connects and advertises
    miner = SeedConnection(lilith, "tcp+tls://miner0:31342",
                           external_addrs=["tcp+tls://miner0:31342"])
    assert miner.handshake() is True
    miner.advertise_and_discover()
    assert len(lilith.greylist) == 1

    # Wallet connects — should find the mining node in greylist
    wallet = SeedConnection(lilith, "tcp+tls://wallet:31360",
                            external_addrs=[])
    assert wallet.handshake() is True
    result = wallet.advertise_and_discover()
    assert isinstance(result, AddrsMessage)
    assert len(result.addrs) >= 1
    assert any("miner0" in addr for addr, _ in result.addrs)
    print("PASSED")


def test_multiple_miners_all_discoverable():
    """N mining nodes advertise. Wallet discovers them all through greylist."""
    print("  LILITH: multiple miners discoverable...", end=" ")
    lilith = LilithSeed()
    N = 5

    for i in range(N):
        addr = f"tcp+tls://miner{i}:31342"
        miner = SeedConnection(lilith, addr, external_addrs=[addr])
        assert miner.handshake() is True
        miner.advertise_and_discover()

    # Wallet should find all miners through greylist
    wallet = SeedConnection(lilith, "tcp+tls://wallet:31360", external_addrs=[])
    assert wallet.handshake() is True
    result = wallet.advertise_and_discover()
    assert len(result.addrs) >= N
    print("PASSED")


def test_app_name_never_gates():
    """Any app_name can connect. app_name is informational only."""
    print("  LILITH: app_name never gates...", end=" ")
    for name in ["dwowd", "darkfid", "dwow-wallet", "darkirc", "custom-tool"]:
        conn = SeedConnection(LilithSeed(), "tcp+tls://node:31342", app_name=name)
        result = conn.handshake()
        assert result is True, f"app_name='{name}' should connect"
    print("PASSED")


def test_version_mismatch_rejected():
    """Incompatible major.minor version produces SeedErrorMessage(401)."""
    print("  LILITH: version mismatch rejected...", end=" ")
    # Major mismatch
    conn = SeedConnection(LilithSeed(), "tcp+tls://node:31342", version=(1, 0, 0))
    result = conn.handshake()
    assert isinstance(result, SeedErrorMessage)
    assert result.code == 401

    # Minor mismatch
    conn2 = SeedConnection(LilithSeed(), "tcp+tls://node:31343", version=(0, 4, 0))
    result2 = conn2.handshake()
    assert isinstance(result2, SeedErrorMessage)
    assert result2.code == 401
    print("PASSED")


def test_refinery_can_downgrade():
    """Refinery downgrades unreachable entries to greylist."""
    print("  LILITH: refinery downgrade...", end=" ")
    lilith = LilithSeed()
    lilith.whitelist = [("tcp+tls://dead-node:31342", 0)]
    # Node is not in any other list → unreachable
    result = lilith.refinery_tick()
    assert result == ("downgraded", "tcp+tls://dead-node:31342")
    assert len(lilith.whitelist) == 0
    assert len(lilith.greylist) == 1
    print("PASSED")


def test_refinery_refreshes_reachable():
    """Refinery keeps reachable entries on whitelist."""
    print("  LILITH: refinery refresh...", end=" ")
    lilith = LilithSeed()
    lilith.whitelist = [("tcp+tls://good-node:31342", 0)]
    lilith.greylist = [("tcp+tls://good-node:31342", 0)]  # still in another list → reachable
    result = lilith.refinery_tick()
    assert result[0] == "refreshed"
    assert "good-node" in result[1]
    print("PASSED")


def test_full_flow_two_miners_one_wallet():
    """End to end: 2 miners advertise via lilith, wallet discovers both."""
    print("  LILITH: full flow...", end=" ")
    lilith = LilithSeed()

    # Mining nodes connect and advertise
    for i in range(2):
        addr = f"tcp+tls://miner{i}:3134{i+2}"
        miner = SeedConnection(lilith, addr, external_addrs=[addr])
        assert miner.handshake() is True
        miner.advertise_and_discover()

    # Wallet connects — discovers both miners via greylist sharing (Option A)
    wallet = SeedConnection(lilith, "tcp+tls://wallet:31360",
                            app_name="dwow-wallet", external_addrs=[])
    assert wallet.handshake() is True
    result = wallet.advertise_and_discover()

    assert isinstance(result, AddrsMessage)
    assert len(result.addrs) >= 2, f"Expected >= 2, got {len(result.addrs)}"
    assert any("miner0" in addr for addr, _ in result.addrs)
    assert any("miner1" in addr for addr, _ in result.addrs)
    print("PASSED")


LILITH_TESTS = [
    test_cold_start_returns_empty_not_error,
    test_greylist_shared_after_registration,
    test_multiple_miners_all_discoverable,
    test_app_name_never_gates,
    test_version_mismatch_rejected,
    test_refinery_can_downgrade,
    test_refinery_refreshes_reachable,
    test_full_flow_two_miners_one_wallet,
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
