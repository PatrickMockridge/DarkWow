"""Scenario 2f: Backer Bank Run.

Tests: Multiple backers withdraw simultaneously.
Is capital accounting correct? Can total_deployed drop below active coverage?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class BackerBankRunScenario(BaseScenario):
    name = "backer_bank_run"
    description = "All backers withdraw simultaneously — tests capital accounting under stress"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=600,
            num_relayers=2,
            num_users=20,
            num_backers=10,
            initial_backer_capital=50_000_000,
            initial_relayer_stake=100_000_000,
            withdrawal_timeout_blocks=100,
        )

    def run_operational_phase(self, blocks=200):
        net = self.network
        # Backers deploy capital
        for i, backer in enumerate(self.backers):
            relayer_id = f"relayer_{i % 2}"
            backer.deploy_capital(net, relayer_id, 20_000_000)

        # Users deposit and withdraw
        for user in self.users:
            user.deposit(net, 10_000_000)
        net.run(50)
        for user in self.users[:10]:
            user.withdraw(net, 5_000_000)
        net.run(150)

    def inject_failure(self):
        net = self.network
        # All backers withdraw simultaneously
        for backer in self.backers:
            for dep_id in list(backer.deployments.keys()):
                result = backer.withdraw_deployment(net, dep_id)
                if "error" in result:
                    net.log_event("bank_run_error",
                                  backer_id=backer.id,
                                  deployment_id=dep_id,
                                  error=result["error"])

        net.run(200)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Check if capital accounting broke
        for relayer_id in self.network.relayers:
            account = self.network.endowment.registry.get(relayer_id)
            if account:
                actual_deployed = self.network.endowment.get_total_deployed(relayer_id)
                if actual_deployed != account.total_deployed:
                    failures.append(
                        f"CRITICAL: Capital accounting mismatch for {relayer_id}: "
                        f"account says {account.total_deployed}, actual is {actual_deployed}"
                    )

        # Check if any relayer became under-collateralized
        for relayer_id, relayer in self.network.relayers.items():
            total_deployed = self.network.endowment.get_total_deployed(relayer_id)
            active_withdrawal_value = sum(
                w.amount for w in self.network.bridge.withdrawals.values()
                if w.status == "pending" and w.executed_by is None
            )
            if active_withdrawal_value > total_deployed + relayer.stake and active_withdrawal_value > 0:
                failures.append(
                    f"HIGH: {relayer_id} is under-collateralized after bank run. "
                    f"Active withdrawals: {active_withdrawal_value}, "
                    f"Deployed capital: {total_deployed}, Own stake: {relayer.stake}"
                )

        # Check if any backer was unable to withdraw
        stuck_backers = [
            e for e in events if e["type"] == "bank_run_error"
        ]
        if stuck_backers:
            failures.append(
                f"MEDIUM: {len(stuck_backers)} backer withdrawals failed during bank run. "
                "Possible causes: double-withdraw protection working correctly, or "
                "capital accounting preventing legitimate withdrawals."
            )

        # Verify total_deployed never went negative
        for relayer_id, account in self.network.endowment.registry.items():
            if account.total_deployed < 0:
                failures.append(
                    f"CRITICAL: {relayer_id} total_deployed went negative ({account.total_deployed}). "
                    "Integer underflow in capital accounting."
                )

        return failures
