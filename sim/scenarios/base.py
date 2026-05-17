"""Base scenario class for failure mode simulations."""

import json
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from ..config import SimConfig
from ..engine import Network
from ..agents import setup_default_agents, User, Backer, RelayerOperator
from ..metrics import MetricsCollector


@dataclass
class ScenarioResult:
    """Results from running a single scenario."""
    name: str
    description: str
    passed: bool = False
    failure_modes_found: List[str] = field(default_factory=list)
    metrics: Dict[str, Any] = field(default_factory=dict)
    events: List[Dict] = field(default_factory=list)
    remediation_suggestions: List[str] = field(default_factory=list)


class BaseScenario:
    """Base class for failure mode scenarios."""

    name: str = "base"
    description: str = "Base scenario"

    def __init__(self, config: Optional[SimConfig] = None, seed: int = 42):
        self.config = config or SimConfig()
        self.seed = seed
        self.network: Optional[Network] = None
        self.collector = MetricsCollector()
        self.users: List[User] = []
        self.backers: List[Backer] = []
        self.operators: List[RelayerOperator] = []
        self.result = ScenarioResult(name=self.name, description=self.description)

    def setup(self) -> None:
        """Initialize network and agents."""
        self.network = Network(self.config, seed=self.seed)
        agents = setup_default_agents(self.network)
        self.users = agents["users"]
        self.backers = agents["backers"]
        self.operators = agents["operators"]

    def configure(self) -> None:
        """Override to inject failure conditions."""
        pass

    def run_initial_phase(self) -> None:
        """Run the initial operational phase (deposits, deployments)."""
        net = self.network
        # Backers deploy capital to relayers
        for i, backer in enumerate(self.backers):
            relayer_id = f"relayer_{i % len(self.operators)}"
            result = backer.deploy_capital(
                net, relayer_id, net.config.initial_backer_capital // 2,
            )
            if "error" in result:
                net.log_event("deploy_error", backer_id=backer.id, error=result["error"])

        # Users make deposits
        for i, user in enumerate(self.users):
            deposit_amt = net.config.initial_user_balance // 2
            user.deposit(net, deposit_amt)

        # Run some blocks to process setup
        net.run(20)

    def run_operational_phase(self, blocks: int = 200) -> None:
        """Run normal operations: users withdraw, relayers process, fees settle."""
        net = self.network
        for block_offset in range(0, blocks, 20):
            net.run(20)
            # Some users request withdrawals
            for user in self.users:
                if net.rng.random() < 0.05:  # 5% per 20-block window
                    amt = min(
                        user.wrapped_balance,
                        net.rng.randint(
                            net.config.min_withdrawal_amount,
                            net.config.min_withdrawal_amount * 10,
                        ),
                    )
                    if amt >= net.config.min_withdrawal_amount:
                        feed_mode = 1 if net.rng.random() < 0.2 else 0  # 20% guaranteed
                        user.withdraw(net, amt, feed_mode)

    def inject_failure(self) -> None:
        """Override to inject the specific failure mode."""
        pass

    def run_recovery_phase(self, blocks: int = 200) -> None:
        """Run after failure injection to observe recovery behavior."""
        self.network.run(blocks)

    def collect_metrics(self) -> None:
        """Process event log and populate result metrics."""
        events = self.network.get_event_log()
        for event in events:
            self.collector.record_event(event)

        self.collector.metrics.total_blocks = self.network.block_height
        self.collector.metrics.compute_derived()

        # Compute relayer uptime
        total_blocks = self.network.block_height
        for relayer_id in self.network.relayers:
            offline_count = sum(
                1 for e in events
                if e["type"] in ("relayer_offline",) and e.get("relayer_id") == relayer_id
            )
            online_count = sum(
                1 for e in events
                if e["type"] in ("relayer_online", "relayer_recovered") and e.get("relayer_id") == relayer_id
            )
            # Simplified: if never went offline, uptime is 100%
            self.collector.metrics.relayer_uptime_pct[relayer_id] = (
                1.0 if offline_count == 0 else max(0.0, (total_blocks - offline_count * 10) / total_blocks)
            )

        # Compute backer ROIs
        for backer in self.backers:
            self.collector.metrics.backer_rois.append(backer.roi)

        self.result.events = events
        self.result.metrics = {
            "withdrawal_success_rate": self.collector.metrics.withdrawal_success_rate,
            "total_withdrawals_requested": self.collector.metrics.total_withdrawals_requested,
            "total_withdrawals_executed": self.collector.metrics.total_withdrawals_executed,
            "total_withdrawals_failed": self.collector.metrics.total_withdrawals_failed,
            "total_withdrawals_slashed": self.collector.metrics.total_withdrawals_slashed,
            "total_withdrawals_cancelled": self.collector.metrics.total_withdrawals_cancelled,
            "avg_withdrawal_latency_blocks": self.collector.metrics.avg_withdrawal_latency_blocks,
            "total_fees_collected": self.collector.metrics.total_fees_collected,
            "total_fees_settled": self.collector.metrics.total_fees_settled,
            "total_settlement_events": self.collector.metrics.total_settlement_events,
            "total_stake_slashed": self.collector.metrics.total_stake_slashed,
            "slash_events": self.collector.metrics.slash_events,
            "total_capital_deployed": self.collector.metrics.total_capital_deployed,
            "total_backer_fees_earned": self.collector.metrics.total_backer_fees_earned,
            "avg_backer_roi": self.collector.metrics.avg_backer_roi,
            "relayer_uptime_pct": self.collector.metrics.relayer_uptime_pct,
            "relayer_withdrawals_processed": self.collector.metrics.relayer_withdrawals_processed,
            "relayer_slashes": self.collector.metrics.relayer_slashes,
        }

    def analyze(self) -> List[str]:
        """Analyze results and identify failure modes. Override in subclasses."""
        return []

    def run(self) -> ScenarioResult:
        """Run the full scenario lifecycle."""
        self.configure()  # Apply config overrides BEFORE creating network
        self.setup()
        self.run_initial_phase()
        self.run_operational_phase()
        self.inject_failure()
        self.run_recovery_phase()
        self.collect_metrics()
        self.result.failure_modes_found = self.analyze()
        self.result.passed = len(self.result.failure_modes_found) == 0
        return self.result

    def save_results(self, filepath: str) -> None:
        """Save results to JSON files."""
        # Save event log
        with open(filepath.replace(".json", "_events.jsonl"), "w") as f:
            for event in self.result.events:
                f.write(json.dumps(event) + "\n")
        # Save metrics
        with open(filepath.replace(".json", "_metrics.json"), "w") as f:
            json.dump(self.result.metrics, f, indent=2, default=str)
