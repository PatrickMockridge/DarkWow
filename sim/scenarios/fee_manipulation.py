"""Scenario 2h: Fee Market Manipulation.

Tests: Relayer sets extortionate fees on guaranteed withdrawals.
Is there any price discovery? Can users switch relayers?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class FeeManipulationScenario(BaseScenario):
    name = "fee_manipulation"
    description = "Relayer gouges fees on guaranteed withdrawals — tests market dynamics"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=500,
            num_relayers=3,
            num_users=25,
            standard_fee_bp=100,  # 1% normal
            guaranteed_premium_bp=500,  # 5% premium normal
            withdrawal_timeout_blocks=100,
            initial_user_balance=10_000_000,
        )

    def inject_failure(self):
        net = self.network
        # relayer_0 becomes the dominant relayer (monopoly)
        # and sets extortionate fees
        r0 = net.relayers.get("relayer_0")
        if r0:
            r0.fee_multiplier = 10.0  # 10x normal fees
            net.log_event("scenario_inject", failure="fee_gouging",
                          relayer="relayer_0", multiplier=10.0)

        # Make relayers 1 and 2 less capable (smaller stake)
        for rid in ("relayer_1", "relayer_2"):
            r = net.relayers.get(rid)
            if r:
                r.stake = 10_000_000  # Much less stake, can't handle large withdrawals

        net.run(100)

        # Users request guaranteed withdrawals
        for user in self.users[:15]:
            user.withdraw(net, 3_000_000, feed_mode=1)

        net.run(150)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Check relayer_0's fee income vs others
        r0_fees = sum(
            e.get("fee", 0) for e in events
            if e["type"] == "withdrawal_executed" and e.get("relayer_id") == "relayer_0"
        )
        r1_fees = sum(
            e.get("fee", 0) for e in events
            if e["type"] == "withdrawal_executed" and e.get("relayer_id") == "relayer_1"
        )
        r2_fees = sum(
            e.get("fee", 0) for e in events
            if e["type"] == "withdrawal_executed" and e.get("relayer_id") == "relayer_2"
        )

        # Architectural check: does the protocol enforce any fee cap?
        # In the real system, each relayer sets fees independently via FeedManager
        # and nothing prevents a monopolist from charging 50%+ fees.
        failures.append(
            "HIGH: No fee cap mechanism exists in the protocol. A relayer with "
            "dominant stake can set extortionate fees and users have no alternative "
            "if smaller relayers lack sufficient coverage. The fee is determined "
            "unilaterally by the relayer at execution time."
        )

        failures.append(
            "MEDIUM: No fee discovery or comparison mechanism exists. Users cannot "
            "query relayer fees before committing to a withdrawal. There is no "
            "commitment to a specific fee rate before the withdrawal is accepted."
        )

        if r0_fees > 0 and r1_fees == 0 and r2_fees == 0:
            failures.append(
                f"CRITICAL: relayer_0 captured 100% of fee revenue ({r0_fees}). "
                "Stake concentration creates natural monopolies with no competitive "
                "pressure. Barriers to entry (large initial stake) prevent new relayers "
                "from competing."
            )

        return failures
