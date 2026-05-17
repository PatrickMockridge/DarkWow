"""Scenario 2g: Network Partition.

Tests: Relayer can't reach darkfid for N blocks but CAN reach external chain.
Does relayer execute withdrawals it shouldn't? Does it miss timeout windows?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class NetworkPartitionScenario(BaseScenario):
    name = "network_partition"
    description = "Relayer loses connection to darkfid — tests partition tolerance"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=600,
            num_relayers=3,
            num_users=25,
            num_backers=5,
            withdrawal_timeout_blocks=100,
            relayer_poll_interval_blocks=2,
            initial_relayer_stake=100_000_000,
        )

    def inject_failure(self):
        net = self.network
        # relayer_0 is partitioned: goes offline to darkfid at block 300, recovers at block 450
        # But continues to be "online" to external chain
        r0 = net.relayers.get("relayer_0")
        if r0:
            r0.crash_at_block = 300
            r0.recover_at_block = 450
            net.log_event("scenario_inject", failure="partition",
                          relayer="relayer_0", partition_start=300, partition_end=450)

        # other relayers stay online
        net.run(250)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Architectural: no handoff mechanism exists
        failures.append(
            "HIGH: No withdrawal reassignment mechanism exists. Withdrawals accepted "
            "by a partitioned relayer remain locked until timeout. Other relayers "
            "cannot take over in-flight withdrawals, even if the original relayer "
            "has been offline for many blocks. The bridge contract has no "
            "`reassign_after_blocks` field."
        )

        # Check for withdrawals that timed out during partition
        timed_out_during = [
            e for e in events
            if e["type"] == "withdrawal_cancelled" and 300 <= e["block"] <= 470
        ]
        if timed_out_during:
            failures.append(
                f"HIGH: {len(timed_out_during)} withdrawals timed out during network "
                "partition. Users had to cancel manually."
            )

        # Check if other relayers took over the partitioned relayer's work
        r1_r2_executed = [
            e for e in events
            if e["type"] == "withdrawal_executed"
            and e.get("relayer_id") in ("relayer_1", "relayer_2")
            and 300 <= e["block"] <= 450
        ]
        pending_at_partition = [
            w for w in self.network.bridge.withdrawals.values()
            if w.status == "pending" and w.timeout_height > 300
        ]
        if len(r1_r2_executed) < len(pending_at_partition) * 0.5:
            failures.append(
                "MEDIUM: Other relayers processed less than 50% of pending withdrawals "
                "during the partition. No automatic workload redistribution."
            )

        return failures
