"""Scenario 2e: Guaranteed Withdrawal Slash Loop.

Tests: Relayer accepts guaranteed withdrawals, consistently fails.
Does slashing actually compensate users? What if stake is insufficient?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class SlashLoopScenario(BaseScenario):
    name = "slash_loop"
    description = "Relayer repeatedly fails guaranteed withdrawals — tests slash exhaustion"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=500,
            num_relayers=2,
            num_users=20,
            initial_relayer_stake=20_000_000,  # Small stake to accelerate exhaustion
            slash_amount=1_000_000,
            guaranteed_premium_bp=500,
            withdrawal_timeout_blocks=100,
        )

    def inject_failure(self):
        net = self.network
        # relayer_0 has high external chain failure rate
        r0 = net.relayers.get("relayer_0")
        if r0:
            r0.external_chain_failure_rate = 0.8  # 80% failure
            net.log_event("scenario_inject", failure="high_failure_rate", relayer="relayer_0")

        # Users request many guaranteed withdrawals to relayer_0's chain
        for user in self.users[:15]:
            user.withdraw(net, 5_000_000, feed_mode=1)

        net.run(250)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        slashes = [
            e for e in events if e["type"] == "withdrawal_slashed"
        ]
        executed = [
            e for e in events if e["type"] == "withdrawal_executed"
        ]
        total_slashed = 0  # Initialize before conditional

        if slashes:
            total_slashed = sum(e.get("slash_amount", 0) for e in slashes)
            r0 = self.network.stake_manager.records.get("relayer_0")
            remaining = r0.available_stake if r0 else 0

            failures.append(
                f"HIGH: {len(slashes)} guaranteed withdrawals slashed (total: {total_slashed}). "
                f"Remaining relayer_0 stake: {remaining}. "
            )

            if remaining <= 0:
                failures.append(
                    "CRITICAL: Relayer stake fully exhausted. Subsequent guaranteed "
                    "withdrawals have ZERO coverage. Users pay premium but get no "
                    "protection. The slash_amount constant does not scale with risk."
                )

        # Check user experience
        users_refunded = set()
        for e in slashes:
            # Track which nullifiers were refunded
            nullifier = e.get("nullifier")
            if nullifier:
                users_refunded.add(nullifier)

        total_guaranteed = sum(
            1 for e in events
            if e["type"] == "withdrawal_requested" and e.get("feed_mode") == 1
        )
        if len(users_refunded) < total_guaranteed and total_guaranteed > 0:
            failures.append(
                f"MEDIUM: Only {len(users_refunded)}/{total_guaranteed} guaranteed withdrawals "
                "received slash refunds. Remaining users lost their premium with no compensation."
            )

        # Check if slash amount is adequate
        total_withdrawal_amount = sum(
            e.get("amount", 0) for e in events
            if e["type"] == "withdrawal_requested" and e.get("feed_mode") == 1
        )
        if total_slashed < total_withdrawal_amount * 0.01:
            failures.append(
                "MEDIUM: Total slashed amount is less than 1% of guaranteed withdrawal volume. "
                "Slash amount is a flat constant, not proportional to withdrawal size."
            )

        return failures
