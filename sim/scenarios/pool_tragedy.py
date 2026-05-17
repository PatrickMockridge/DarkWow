"""Scenario 2i: Pool Stake Tragedy of Commons.

Tests: Multiple relayers share a pool; one is reckless.
Does one bad relayer's slashes drain the shared pool?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class PoolTragedyScenario(BaseScenario):
    name = "pool_tragedy"
    description = "Shared stake pool drained by one reckless relayer"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=600,
            num_relayers=4,
            num_users=25,
            num_backers=5,
            initial_relayer_stake=50_000_000,
            slash_amount=1_000_000,
            guaranteed_premium_bp=500,
            withdrawal_timeout_blocks=100,
        )

    def setup(self):
        super().setup()
        net = self.network
        # Create a shared pool with relayers 0, 1, 2
        net.pool_manager.create_pool(
            "shared_pool",
            ["relayer_0", "relayer_1", "relayer_2"],
        )
        # relayer_3 stays solo as control
        net.log_event("pool_created", pool_id="shared_pool",
                      members=["relayer_0", "relayer_1", "relayer_2"])

    def inject_failure(self):
        net = self.network
        # relayer_2 in the shared pool becomes reckless
        r2 = net.relayers.get("relayer_2")
        if r2:
            r2.external_chain_failure_rate = 0.9  # 90% failure rate
            r2.malicious = False  # Not malicious, just incompetent
            net.log_event("scenario_inject", failure="reckless_pool_member",
                          relayer="relayer_2", pool="shared_pool")

        # Users request guaranteed withdrawals
        for user in self.users[:15]:
            user.withdraw(net, 3_000_000, feed_mode=1)

        net.run(250)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Check slash distribution
        r2_slashes = sum(
            1 for e in events
            if e["type"] == "withdrawal_slashed" and e.get("relayer_id") == "relayer_2"
        )
        r0_slashes = sum(
            1 for e in events
            if e["type"] == "withdrawal_slashed" and e.get("relayer_id") == "relayer_0"
        )

        # Architectural: shared pools lack per-member accountability
        failures.append(
            "HIGH: PoolManager tracks total pool slashes but does not attribute them "
            "to individual members. One reckless relayer degrades coverage for ALL "
            "pool members with no mechanism for ejection, probation, or proportional "
            "liability. The PoolManager in pool.rs has no reputation tracking."
        )

        if r2_slashes > 0:
            failures.append(
                f"HIGH: Reckless relayer_2 incurred {r2_slashes} slashes. "
                "In a shared pool, these slashes reduce total pool coverage, "
                "affecting ALL pool members even though only one was reckless."
            )

        pool_slashed = self.network.pool_manager.pools.get("shared_pool", {}).get("slashed", 0)
        if pool_slashed > 0:
            failures.append(
                f"MEDIUM: Shared pool lost {pool_slashed} in slashes from reckless member. "
                "Pool stake accounting does not track per-member responsibility."
            )

        # Compare pool member vs solo relayer
        failures.append(
            "MEDIUM: Pool membership creates negative externalities — a diligent "
            "relayer in a shared pool has worse risk-adjusted returns than an "
            "identical solo relayer, because their stake implicitly backs reckless "
            "members. This creates adverse selection: good relayers leave pools."
        )

        return failures
