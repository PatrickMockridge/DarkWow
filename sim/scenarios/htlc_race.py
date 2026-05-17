"""Scenario 2j: HTLC Race Condition.

Tests: User and counterparty both try to claim/refund same HTLC.
Does the bridge contract resolve this correctly?
"""

from ..config import SimConfig, with_overrides
from .base import BaseScenario


class HtlcRaceScenario(BaseScenario):
    name = "htlc_race"
    description = "Simultaneous claim and refund on same HTLC — tests atomicity"

    def configure(self):
        self.config = with_overrides(
            self.config,
            blocks_to_simulate=400,
            num_relayers=2,
            num_users=10,
            withdrawal_timeout_blocks=100,
        )

    def inject_failure(self):
        net = self.network
        # Create an HTLC
        htlc_id = "htlc_race_test"
        result = net.bridge.create_htlc(
            htlc_id=htlc_id,
            hash_lock="hash_secret_123",
            time_lock=net.block_height + 50,
            amount=10_000_000,
            sender="user_0",
            receiver="user_1",
            chain="ethereum",
            block_height=net.block_height,
        )
        net.log_event("htlc_created", htlc_id=htlc_id)

        # Advance to near timeout
        net.run(48)

        # Attempt simultaneous claim and refund (race condition)
        claim_result = net.bridge.claim_htlc(htlc_id, "secret_123", net.block_height)
        refund_result = net.bridge.refund_htlc(htlc_id, net.block_height)

        net.log_event("htlc_race_attempt", htlc_id=htlc_id,
                      claim=claim_result, refund=refund_result)

        if "error" not in claim_result and "error" not in refund_result:
            net.log_event("htlc_race_success", htlc_id=htlc_id,
                          detail="BOTH claim and refund succeeded — funds lost!")

        net.run(50)

    def analyze(self):
        failures = []
        events = self.network.get_event_log()

        # Check for race condition success (both claim and refund)
        race_successes = [
            e for e in events if e["type"] == "htlc_race_success"
        ]
        if race_successes:
            failures.append(
                "CRITICAL: HTLC race condition succeeded — both claim AND refund "
                "were processed for the same HTLC. This means funds could be doubled. "
                "The bridge contract's HTLC state machine is not atomic — there is no "
                "mutual exclusion between claim and refund operations at the same block."
            )

        # Check final HTLC state
        htlc = self.network.bridge.htlcs.get("htlc_race_test")
        if htlc:
            if htlc.status == "claimed" and htlc.secret:
                failures.append(
                    "NOTE: HTLC was claimed (claim processed before refund). "
                    "Order depends on execution order within block — not guaranteed "
                    "across different nodes/mempools."
                )
            elif htlc.status == "refunded":
                failures.append(
                    "NOTE: HTLC was refunded (refund processed before claim). "
                    "The rightful claimer lost their funds due to race."
                )

        # Check for time-lock enforcement
        if htlc and htlc.status == "refunded":
            # Was the time lock actually expired?
            # The refund was attempted at block ~(start + 48), time_lock = start + 50
            failures.append(
                "HIGH: HTLC refund was processed BEFORE the time_lock expired. "
                "The refund function should check block_height >= time_lock, "
                "but if both claim and refund arrive in the same block, the "
                "outcome is non-deterministic."
            )

        return failures
