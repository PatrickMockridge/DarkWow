"""Scenario 2c: Fee Settlement Evasion.

Tests: Relayer earns fees but never calls SettleFeesV1.
Can backers detect this? What recourse do they have?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class FeeSettlementEvasionScenario(BaseScenario):
    name = "fee_settlement_evasion"
    description = "Relayer collects fees but never settles to backers"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=800,
            num_relayers=2,
            num_users=20,
            num_backers=5,
            fee_settlement_interval_blocks=50,
            initial_relayer_stake=100_000_000,
            initial_backer_capital=50_000_000,
            initial_user_balance=20_000_000,
        )

    def inject_failure(self):
        net = self.network
        # relayer_0 stops settling fees but keeps processing withdrawals
        r0 = net.relayers.get("relayer_0")
        if r0:
            r0.skip_settlement = True
            net.log_event("scenario_inject", failure="fee_evasion", relayer="relayer_0")

        # Let it run for 300 more blocks while relayer_0 earns fees but never settles
        net.run(300)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Check relayer_0's unsettled fees
        pending = self.network.capital_deployer.get_pending("relayer_0")
        if pending > 0:
            failures.append(
                f"CRITICAL: relayer_0 accumulated {pending} in fees but never settled. "
                "No on-chain enforcement of settlement — backers rely entirely on "
                "relayer honesty. No timeout or automatic settlement mechanism exists."
            )

        # Check if backers earned any fees from relayer_0
        r0_backer_fees = sum(
            e.get("amount", 0) for e in events
            if e["type"] == "fees_claimed" and any(
                d.relayer_id == "relayer_0"
                for d in self.network.endowment.deployments.values()
                if d.deployment_id == e.get("deployment_id", "")
            )
        )
        if r0_backer_fees == 0 and pending > 0:
            failures.append(
                "HIGH: Backers of relayer_0 earned zero fees despite relayer processing "
                "withdrawals. The SettleFeesV1 call is entirely voluntary. Backers have "
                "no on-chain mechanism to force settlement or exit with accrued fees."
            )

        # Check backer ROI
        for backer in self.backers:
            r0_deployments = [
                d for d in backer.deployments
                if self.network.endowment.deployments.get(d)
                and self.network.endowment.deployments[d].relayer_id == "relayer_0"
            ]
            if r0_deployments and backer.total_fees_earned < sum(
                self.network.endowment.deployments[d].amount * 0.01
                for d in r0_deployments
            ):
                failures.append(
                    "MEDIUM: Backer ROI is zero or negative for relayer_0 deployments. "
                    "No mechanism to detect evasion early or auto-withdraw from dishonest relayers."
                )

        return failures
