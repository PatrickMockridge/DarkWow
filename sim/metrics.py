"""Metrics collection for simulation analysis."""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional


@dataclass
class SimulationMetrics:
    """Aggregated metrics from a simulation run."""

    # Withdrawal metrics
    total_withdrawals_requested: int = 0
    total_withdrawals_executed: int = 0
    total_withdrawals_failed: int = 0
    total_withdrawals_slashed: int = 0
    total_withdrawals_cancelled: int = 0
    total_withdrawals_timed_out: int = 0
    withdrawal_success_rate: float = 0.0
    avg_withdrawal_latency_blocks: float = 0.0
    withdrawal_latencies: List[int] = field(default_factory=list)

    # Fee metrics
    total_fees_collected: int = 0
    total_fees_settled: int = 0
    total_settlement_events: int = 0
    avg_settlement_interval_blocks: float = 0.0

    # Stake metrics
    total_stake_slashed: int = 0
    slash_events: int = 0
    stake_utilization_pct: float = 0.0  # avg % of stake locked

    # Backer metrics
    total_capital_deployed: int = 0
    total_backer_fees_earned: int = 0
    total_backer_withdrawals: int = 0
    avg_backer_roi: float = 0.0
    backer_rois: List[float] = field(default_factory=list)

    # Relayer metrics
    relayer_uptime_pct: Dict[str, float] = field(default_factory=dict)
    relayer_withdrawals_processed: Dict[str, int] = field(default_factory=dict)
    relayer_slashes: Dict[str, int] = field(default_factory=dict)

    # HTLC metrics
    total_htlcs_created: int = 0
    total_htlcs_claimed: int = 0
    total_htlcs_refunded: int = 0
    htlc_race_attempts: int = 0
    htlc_race_successes: int = 0

    # Attack metrics
    double_spend_attempts: int = 0
    double_spend_successes: int = 0
    fee_evasion_events: int = 0
    manipulation_events: int = 0

    # Network
    total_blocks: int = 0
    total_transactions: int = 0

    def compute_derived(self) -> None:
        """Compute derived metrics from raw counts."""
        total = self.total_withdrawals_requested
        if total > 0:
            self.withdrawal_success_rate = (
                self.total_withdrawals_executed / total
            )
        if self.withdrawal_latencies:
            self.avg_withdrawal_latency_blocks = (
                sum(self.withdrawal_latencies) / len(self.withdrawal_latencies)
            )
        if self.backer_rois:
            self.avg_backer_roi = sum(self.backer_rois) / len(self.backer_rois)


class MetricsCollector:
    """Collects metrics during a simulation run."""

    def __init__(self):
        self.metrics = SimulationMetrics()
        self._withdrawal_request_blocks: Dict[str, int] = {}
        self._settlement_blocks: List[int] = []
        self._stake_utilization_samples: List[float] = []

    def record_event(self, event: Dict) -> None:
        """Process a simulation event and update metrics."""
        etype = event.get("type", "")

        if etype == "withdrawal_requested":
            self.metrics.total_withdrawals_requested += 1
            self._withdrawal_request_blocks[event["nullifier"]] = event.get("block", 0)

        elif etype == "withdrawal_executed":
            self.metrics.total_withdrawals_executed += 1
            self.metrics.total_fees_collected += event.get("fee", 0)
            req_block = self._withdrawal_request_blocks.get(event["nullifier"])
            if req_block is not None:
                latency = event.get("block", 0) - req_block
                self.metrics.withdrawal_latencies.append(latency)

            # Track per-relayer processing
            rid = event.get("relayer_id", "")
            if rid:
                self.metrics.relayer_withdrawals_processed[rid] = (
                    self.metrics.relayer_withdrawals_processed.get(rid, 0) + 1
                )

        elif etype == "withdrawal_failed":
            self.metrics.total_withdrawals_failed += 1

        elif etype == "withdrawal_slashed":
            self.metrics.total_withdrawals_slashed += 1
            self.metrics.total_stake_slashed += event.get("slash_amount", 0)
            self.metrics.slash_events += 1
            rid = event.get("relayer_id", "")
            if rid:
                self.metrics.relayer_slashes[rid] = (
                    self.metrics.relayer_slashes.get(rid, 0) + 1
                )

        elif etype == "withdrawal_cancelled":
            self.metrics.total_withdrawals_cancelled += 1

        elif etype == "fees_settled":
            self.metrics.total_fees_settled += event.get("total", 0)
            self.metrics.total_settlement_events += 1
            self._settlement_blocks.append(event.get("block", 0))

        elif etype == "stake_slashed":
            self.metrics.total_stake_slashed += event.get("amount", 0)
            self.metrics.slash_events += 1

        elif etype == "capital_deployed":
            self.metrics.total_capital_deployed += event.get("amount", 0)

        elif etype == "fees_claimed":
            self.metrics.total_backer_fees_earned += event.get("amount", 0)

        elif etype == "deployment_withdrawn":
            self.metrics.total_backer_withdrawals += 1

        elif etype == "fee_accumulated":
            pass  # Tracked via settlements

        elif etype == "fee_evasion_detected":
            self.metrics.fee_evasion_events += 1

        elif etype == "fee_manipulation":
            self.metrics.manipulation_events += 1

        elif etype == "htlc_created":
            self.metrics.total_htlcs_created += 1

        elif etype == "htlc_claimed":
            self.metrics.total_htlcs_claimed += 1

        elif etype == "htlc_refunded":
            self.metrics.total_htlcs_refunded += 1

        elif etype == "htlc_race_attempt":
            self.metrics.htlc_race_attempts += 1

        elif etype == "htlc_race_success":
            self.metrics.htlc_race_successes += 1

        elif etype == "double_spend_attempt":
            self.metrics.double_spend_attempts += 1
            if event.get("success"):
                self.metrics.double_spend_successes += 1

        elif etype == "relayer_offline":
            rid = event.get("relayer_id", "")
            if rid in self.metrics.relayer_uptime_pct:
                # Will be corrected in finalize
                pass

    def finalize(self, total_blocks: int, relayers: Dict[str, Any]) -> SimulationMetrics:
        """Compute final metrics after simulation ends."""
        self.metrics.total_blocks = total_blocks
        self.metrics.compute_derived()

        # Compute relayer uptime from event log
        # This is handled by the scenario code which has full event log access

        return self.metrics
