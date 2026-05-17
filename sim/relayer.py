"""Relayer node model.

Models the universal_relayer behavior:
- StakeManager: Locks/releases/slashes stake for withdrawal coverage
- FeedManager: Prices withdrawals (standard % or guaranteed + premium)
- CapitalDeployer: Tracks backer capital, computes fee shares
- PoolManager: Shared coverage pools
- RelayerNode: Main loop — poll, check, price, execute, settle
"""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Set, Tuple

from .config import SimConfig


@dataclass
class StakeRecord:
    relayer_id: str
    total_stake: int
    locked_stake: int = 0
    slashed_amount: int = 0
    slash_count: int = 0

    @property
    def available_stake(self) -> int:
        return self.total_stake - self.locked_stake - self.slashed_amount


class StakeManager:
    """Manages relayer stake for withdrawal coverage."""

    def __init__(self, config: SimConfig):
        self.config = config
        self.records: Dict[str, StakeRecord] = {}

    def register(self, relayer_id: str, initial_stake: int) -> None:
        self.records[relayer_id] = StakeRecord(
            relayer_id=relayer_id,
            total_stake=initial_stake,
        )

    def can_cover(self, relayer_id: str, amount: int) -> bool:
        record = self.records.get(relayer_id)
        if not record:
            return False
        return record.available_stake >= amount * self.config.coverage_ratio

    def lock_stake(self, relayer_id: str, amount: int) -> bool:
        record = self.records.get(relayer_id)
        if not record:
            return False
        required = int(amount * self.config.coverage_ratio)
        if record.available_stake < required:
            return False
        record.locked_stake += required
        return True

    def release_stake(self, relayer_id: str, amount: int) -> None:
        record = self.records.get(relayer_id)
        if record:
            required = int(amount * self.config.coverage_ratio)
            record.locked_stake = max(0, record.locked_stake - required)

    def slash(self, relayer_id: str, slash_amount: int, network: Any) -> None:
        record = self.records.get(relayer_id)
        if not record:
            return
        actual_slash = min(slash_amount, record.total_stake - record.slashed_amount)
        record.slashed_amount += actual_slash
        record.slash_count += 1
        record.locked_stake = max(0, record.locked_stake - actual_slash)
        network.log_event(
            "stake_slashed",
            relayer_id=relayer_id,
            amount=actual_slash,
            total_slashed=record.slashed_amount,
            remaining_stake=record.available_stake,
        )

    def get_available_stake(self, relayer_id: str) -> int:
        record = self.records.get(relayer_id)
        return record.available_stake if record else 0


class FeedManager:
    """Prices withdrawals based on feed mode."""

    def __init__(self, config: SimConfig):
        self.config = config

    def compute_fee(self, amount: int, feed_mode: int) -> int:
        """Compute the relayer fee for a withdrawal."""
        base_fee = amount * self.config.standard_fee_bp // self.config.bp_precision
        if feed_mode == 1:  # guaranteed
            premium = amount * self.config.guaranteed_premium_bp // self.config.bp_precision
            return base_fee + premium
        return base_fee

    def compute_backer_share(self, fee: int, backer_cut_bp: int) -> int:
        """Compute backer's share of a fee."""
        return fee * backer_cut_bp // self.config.bp_precision


class CapitalDeployer:
    """Tracks backer capital deployments and computes fee shares."""

    def __init__(self, config: SimConfig):
        self.config = config
        self.pending_fees: Dict[str, int] = {}  # relayer_id -> total_accumulated_fees

    def accumulate_fees(self, relayer_id: str, fee: int, network: Any) -> None:
        """Accumulate fees for later settlement."""
        self.pending_fees[relayer_id] = self.pending_fees.get(relayer_id, 0) + fee
        network.log_event("fee_accumulated", relayer_id=relayer_id, fee=fee)

    def settle(self, relayer_id: str, network: Any, feed_manager: FeedManager) -> Optional[Dict]:
        """Settle accumulated fees to backer deployments."""
        total = self.pending_fees.get(relayer_id, 0)
        if total == 0:
            return None

        deployments = network.endowment.get_active_deployments(relayer_id)
        if not deployments:
            self.pending_fees[relayer_id] = 0
            return None

        # Build allocations proportional to deployment amount
        total_deployed = sum(d.amount for d in deployments)
        allocations = []
        for dep in deployments:
            share = total * dep.amount // total_deployed
            backer_share = feed_manager.compute_backer_share(share, dep.backer_cut_bp)
            if backer_share > 0:
                allocations.append({
                    "deployment_id": dep.deployment_id,
                    "fee_amount": backer_share,
                })

        if allocations:
            total_allocated = sum(a["fee_amount"] for a in allocations)
            result = network.endowment.settle_fees(
                relayer_id, total_allocated, allocations, network.block_height,
            )
            network.log_event(
                "fees_settled",
                relayer_id=relayer_id,
                total=total_allocated,
                num_backers=len(allocations),
            )
            self.pending_fees[relayer_id] = max(0, total - total_allocated)
            return result

        return None

    def get_pending(self, relayer_id: str) -> int:
        return self.pending_fees.get(relayer_id, 0)


class PoolManager:
    """Shared coverage pools across relayers."""

    def __init__(self, config: SimConfig):
        self.config = config
        self.pools: Dict[str, Dict] = {}  # pool_id -> {members, total_stake, slashed}
        self.relayer_pool: Dict[str, str] = {}  # relayer_id -> pool_id

    def create_pool(self, pool_id: str, members: List[str]) -> None:
        self.pools[pool_id] = {
            "members": set(members),
            "total_stake": 0,
            "slashed": 0,
        }
        for relayer_id in members:
            self.relayer_pool[relayer_id] = pool_id

    def get_pool_members(self, relayer_id: str) -> Set[str]:
        pool_id = self.relayer_pool.get(relayer_id)
        if pool_id:
            return self.pools[pool_id]["members"]
        return {relayer_id}

    def record_slash(self, relayer_id: str, amount: int) -> None:
        pool_id = self.relayer_pool.get(relayer_id)
        if pool_id:
            self.pools[pool_id]["slashed"] += amount


@dataclass
class RelayerNode:
    """A relayer node in the network."""

    id: str
    online: bool = True
    stake: int = 0
    backer_cut_bp: int = 5000
    active_withdrawals: Set[str] = field(default_factory=set)
    settlement_timer: int = 0
    # Failure injection
    crash_at_block: Optional[int] = None
    recover_at_block: Optional[int] = None
    malicious: bool = False
    skip_settlement: bool = False
    fee_multiplier: float = 1.0  # for fee manipulation testing
    external_chain_failure_rate: float = 0.0

    def poll_pending_withdrawals(self, network: Any) -> None:
        """Main relayer loop: poll for pending withdrawals and process them."""
        # Check crash/recover schedule
        if self.crash_at_block and network.block_height >= self.crash_at_block:
            if self.online:
                self.online = False
                network.log_event("relayer_offline", relayer_id=self.id, reason="crash")
        if self.recover_at_block and network.block_height >= self.recover_at_block:
            if not self.online:
                self.online = True
                network.log_event("relayer_online", relayer_id=self.id, reason="recovery")

        if not self.online:
            return

        # Periodic fee settlement
        self.settlement_timer += 1
        if self.settlement_timer >= network.config.fee_settlement_interval_blocks:
            if not self.skip_settlement:
                network.capital_deployer.settle(
                    self.id, network, network.feed_manager,
                )
            self.settlement_timer = 0

        # Poll pending withdrawals
        pending = network.bridge.get_pending_withdrawals(network.block_height)
        available = network.stake_manager.get_available_stake(self.id)

        for w in pending:
            # Skip if already assigned to this relayer
            if w.nullifier in self.active_withdrawals:
                continue

            # Skip if relayer is at capacity
            if len(self.active_withdrawals) >= network.config.max_concurrent_withdrawals:
                break

            # Check if any relayer is already processing this
            already_assigned = any(
                w.nullifier in r.active_withdrawals
                for rid, r in network.relayers.items()
                if rid != self.id
            )
            if already_assigned:
                continue

            # Check stake coverage
            if not network.stake_manager.can_cover(self.id, w.amount):
                network.log_event(
                    "insufficient_coverage",
                    relayer_id=self.id,
                    nullifier=w.nullifier,
                    amount=w.amount,
                    available=available,
                )
                continue

            # Lock stake and accept withdrawal
            network.stake_manager.lock_stake(self.id, w.amount)
            self.active_withdrawals.add(w.nullifier)

            # Simulate external chain execution
            latency = network.config.external_chain_latency_blocks
            success = self._simulate_execution(network)

            network.events.schedule(
                network.block_height + latency,
                "withdrawal_complete",
                self._complete_withdrawal,
                w.nullifier,
                success,
            )

            network.log_event(
                "withdrawal_accepted",
                relayer_id=self.id,
                nullifier=w.nullifier,
                amount=w.amount,
                feed_mode=w.feed_mode,
                success_expected=success,
            )

    def _simulate_execution(self, network: Any) -> bool:
        """Simulate external chain execution — may fail based on conditions."""
        base_rate = network.config.external_chain_failure_rate
        relayer_rate = self.external_chain_failure_rate
        failure_rate = max(base_rate, relayer_rate)

        if self.malicious:
            # Malicious relayer fails guaranteed withdrawals on purpose
            return False

        if failure_rate > 0:
            return network.rng.random() > failure_rate
        return True

    def _complete_withdrawal(self, nullifier: str, success: bool) -> None:
        """Callback when external chain execution completes."""
        # Access network through the event system
        self.active_withdrawals.discard(nullifier)
