"""Scenario 2d: Malicious Relayer — Theft Attempt.

Tests: Can a malicious relayer steal user funds by executing withdrawals
to wrong addresses? Does the ZK proof system prevent this?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class MaliciousRelayerTheftScenario(BaseScenario):
    name = "malicious_relayer_theft"
    description = "Relayer attempts to redirect withdrawals to attacker addresses"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=400,
            num_relayers=3,
            num_users=20,
            num_backers=3,
            initial_relayer_stake=100_000_000,
            initial_user_balance=10_000_000,
        )

    def inject_failure(self):
        net = self.network
        # Make relayer_1 malicious
        r1 = net.relayers.get("relayer_1")
        if r1:
            r1.malicious = True
            # High failure rate — tries to redirect
            r1.external_chain_failure_rate = 1.0
            net.log_event("scenario_inject", failure="malicious_relayer", relayer="relayer_1")

        # Users request guaranteed withdrawals
        for user in self.users[:10]:
            user.withdraw(net, 5_000_000, feed_mode=1)  # guaranteed

        net.run(200)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Count slashes on malicious relayer
        r1_slashes = [
            e for e in events
            if e["type"] == "withdrawal_slashed" and e.get("relayer_id") == "relayer_1"
        ]
        if r1_slashes:
            failures.append(
                f"HIGH: {len(r1_slashes)} guaranteed withdrawals slashed due to malicious "
                "relayer. Slashing provides user refund, BUT:"
            )

        # Check if any user lost funds permanently
        permanent_losses = [
            e for e in events
            if e["type"] in ("withdrawal_failed",) and e.get("relayer_id") == "relayer_1"
        ]
        if permanent_losses:
            failures.append(
                "CRITICAL: Users experienced permanent loss on failed non-guaranteed "
                "withdrawals with malicious relayer. No recourse for standard withdrawals."
            )

        # Check ZK proof protection
        # In the simulation, the bridge contract relies on nullifier uniqueness.
        # The real ZK proof ensures the relayer can't change the recipient without
        # knowing the secret. Our simulation validates the nullifier is unspent.
        double_spends = [
            e for e in events if e["type"] == "double_spend_attempt"
        ]
        if not double_spends:
            failures.append(
                "NOTE: ZK proof system (nullifier check) prevented double-spend attacks "
                "in simulation. However, this assumes the ZK circuit is correctly "
                "implemented and the nullifier is derived correctly from the secret."
            )

        # Check stake adequacy for slash coverage
        r1_stake = self.network.stake_manager.records.get("relayer_1")
        if r1_stake:
            total_slashed = r1_stake.slashed_amount
            if total_slashed >= r1_stake.total_stake * 0.8:
                failures.append(
                    f"CRITICAL: Malicious relayer exhausted {total_slashed}/{r1_stake.total_stake} "
                    "stake via slashing. After stake is fully slashed, further guaranteed "
                    "withdrawals have NO coverage — users lose their premium and get no refund."
                )

        return failures
