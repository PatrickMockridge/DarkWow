"""Scenario 2a: Relayer Crash/Restart.

Tests: Do pending withdrawals timeout when a relayer crashes?
Does another relayer pick them up? Are backer funds safe?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class RelayerCrashScenario(BaseScenario):
    name = "relayer_crash"
    description = "Relayer goes offline for 150 blocks — tests timeout and handoff"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=600,
            num_relayers=3,
            num_users=30,
            withdrawal_timeout_blocks=100,
            relayer_poll_interval_blocks=2,
        )

    def inject_failure(self):
        net = self.network
        # Crash relayer_0 at block 250 for ~150 blocks
        r0 = net.relayers.get("relayer_0")
        if r0:
            r0.crash_at_block = 250
            r0.recover_at_block = 400
            net.log_event("scenario_inject", failure="crash", relayer="relayer_0",
                          crash_block=250, recover_block=400)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Check if any withdrawals timed out during crash window (250-400)
        timed_out = [
            e for e in events
            if e["type"] == "withdrawal_cancelled" and 250 <= e["block"] <= 420
        ]
        if timed_out:
            failures.append(
                f"CRITICAL: {len(timed_out)} withdrawals timed out during relayer crash. "
                "No automatic handoff to other relayers — users had to cancel manually."
            )

        # Check if other relayers picked up the slack
        r1_processed = sum(
            1 for e in events
            if e["type"] == "withdrawal_executed" and e.get("relayer_id") == "relayer_1"
            and 250 <= e["block"] <= 400
        )
        r2_processed = sum(
            1 for e in events
            if e["type"] == "withdrawal_executed" and e.get("relayer_id") == "relayer_2"
            and 250 <= e["block"] <= 400
        )
        if r1_processed == 0 and r2_processed == 0:
            failures.append(
                "HIGH: Other relayers did not process withdrawals during relayer_0 crash. "
                "No multi-relayer redundancy in practice."
            )

        # Check backer fund safety
        backer_losses = sum(
            1 for e in events
            if e["type"] == "stake_slashed" and e.get("relayer_id") == "relayer_0"
            and 250 <= e["block"] <= 400
        )
        if backer_losses > 0:
            failures.append(
                "MEDIUM: Backer stake was slashed while relayer was crashed. "
                "Unfair penalty for infrastructure failure."
            )

        return failures
