"""Scenario 2b: Capital Exhaustion.

Tests: What happens when withdrawal volume exceeds total deployed capital?
Does stake coverage prevent overcommitment?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class CapitalExhaustionScenario(BaseScenario):
    name = "capital_exhaustion"
    description = "Withdrawal volume exceeds total available stake — tests coverage limits"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=600,
            num_relayers=2,
            num_users=30,
            num_backers=5,
            initial_relayer_stake=10_000_000,  # Only 10 DAI each
            initial_backer_capital=5_000_000,  # Only 5 DAI each
            initial_user_balance=100_000_000,  # 100 DAI each — lots of withdrawal demand
            coverage_ratio=1.5,
            withdrawal_timeout_blocks=100,
        )

    def run_operational_phase(self, blocks=200):
        net = self.network
        # Backers deploy small amounts
        for i, backer in enumerate(self.backers):
            relayer_id = f"relayer_{i % 2}"
            backer.deploy_capital(net, relayer_id, 5_000_000)

        # Users make large deposits
        for user in self.users:
            user.deposit(net, 50_000_000)

        net.run(blocks)
        # Users request many withdrawals — more than stake can cover
        for user in self.users[:20]:
            user.withdraw(net, 30_000_000)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Check for insufficient coverage events
        insufficient = [
            e for e in events if e["type"] == "insufficient_coverage"
        ]

        # Check if any withdrawals were stuck pending
        pending_at_end = sum(
            1 for w in self.network.bridge.withdrawals.values()
            if w.status == "pending"
        )

        # System-wide coverage ratio
        total_stake = sum(
            r.stake for r in self.network.relayers.values()
        )
        total_pending_amount = sum(
            w.amount for w in self.network.bridge.withdrawals.values()
            if w.status == "pending"
        )

        if insufficient:
            failures.append(
                f"CRITICAL: {len(insufficient)} withdrawals rejected due to insufficient "
                "stake coverage. Users cannot access their funds when relayer capital "
                "is exhausted. No fallback mechanism exists."
            )

        if pending_at_end > 0:
            failures.append(
                f"HIGH: {pending_at_end} withdrawals stuck in pending state at simulation end. "
                "No mechanism to increase relayer capacity dynamically."
            )

        if total_pending_amount > total_stake:
            failures.append(
                f"MEDIUM: Pending withdrawals ({total_pending_amount}) exceed total "
                f"relayer stake ({total_stake}). Coverage ratio violated at system level."
            )

        # Always check: does the system have ANY mechanism to handle overflow?
        if total_stake < self.config.initial_user_balance * self.config.num_users * 0.1:
            failures.append(
                f"HIGH: Total relayer stake ({total_stake}) is less than 10% of user "
                f"deposit capacity. The system has no dynamic capital scaling — a "
                f"sudden withdrawal surge would exhaust coverage immediately."
            )

        return failures
