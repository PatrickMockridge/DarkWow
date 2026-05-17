"""Discrete-event simulation engine for DarkWow relayer network.

Models block-by-block progression of a Layer 1 blockchain with bridge
and relayer_endowment WASM contracts, plus external relayers.
"""

import random
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Set, Tuple

from .bridge import BridgeContract
from .config import SimConfig
from .endowment import EndowmentContract
from .relayer import RelayerNode, StakeManager, FeedManager, CapitalDeployer, PoolManager


@dataclass
class Block:
    """A single block in the chain."""
    height: int
    transactions: List[Dict] = field(default_factory=list)
    timestamp: float = 0.0

    def add_tx(self, tx_type: str, data: Dict) -> None:
        self.transactions.append({"type": tx_type, "data": data, "block": self.height})


@dataclass
class Event:
    """A scheduled event in the simulation."""
    block: int
    name: str
    callback: Callable
    args: tuple = ()
    kwargs: Dict = field(default_factory=dict)

    def __lt__(self, other: "Event") -> bool:
        return self.block < other.block


class EventQueue:
    """Priority queue of future events ordered by block height."""

    def __init__(self):
        self._events: List[Event] = []

    def schedule(self, block: int, name: str, callback: Callable, *args, **kwargs) -> None:
        self._events.append(Event(block, name, callback, args, kwargs))
        self._events.sort(key=lambda e: e.block)

    def pop_due(self, current_block: int) -> List[Event]:
        due = [e for e in self._events if e.block <= current_block]
        self._events = [e for e in self._events if e.block > current_block]
        return due

    def __len__(self) -> int:
        return len(self._events)


class Network:
    """The simulation network — manages chain state and contract instances."""

    def __init__(self, config: SimConfig, seed: int = 42):
        self.config = config
        self.rng = random.Random(seed)
        self.block_height: int = 0
        self.bridge = BridgeContract(config)
        self.endowment = EndowmentContract(config)
        self.events = EventQueue()
        self.relayers: Dict[str, RelayerNode] = {}
        self.stake_manager = StakeManager(config)
        self.feed_manager = FeedManager(config)
        self.capital_deployer = CapitalDeployer(config)
        self.pool_manager = PoolManager(config)
        self._event_log: List[Dict] = []
        self._tx_counter: int = 0

    def log_event(self, event_type: str, **kwargs) -> None:
        """Record an event for later analysis."""
        self._event_log.append({
            "block": self.block_height,
            "type": event_type,
            **kwargs,
        })

    def next_tx_id(self) -> int:
        self._tx_counter += 1
        return self._tx_counter

    def register_relayer(self, relayer: "RelayerNode") -> None:
        """Register a relayer node on the network."""
        self.relayers[relayer.id] = relayer
        # Initialize endowment account
        self.endowment.initialize(
            relayer_id=relayer.id,
            default_backer_cut_bp=relayer.backer_cut_bp,
            block_height=self.block_height,
        )
        relayer.stake = self.config.initial_relayer_stake
        self.log_event("relayer_registered", relayer_id=relayer.id)

    def advance_block(self) -> Block:
        """Advance the chain by one block, process all transactions and events."""
        self.block_height += 1
        block = Block(height=self.block_height)

        # Process any events due at this block
        for event in self.events.pop_due(self.block_height):
            try:
                event.callback(*event.args, **event.kwargs)
            except Exception as e:
                self.log_event("event_error", name=event.name, error=str(e))

        # Relayers poll for pending withdrawals
        for relayer in self.relayers.values():
            if relayer.online and self.block_height % self.config.relayer_poll_interval_blocks == 0:
                relayer.poll_pending_withdrawals(self)

        return block

    def run(self, blocks: Optional[int] = None) -> List[Dict]:
        """Run the simulation for the given number of blocks."""
        blocks = blocks or self.config.blocks_to_simulate
        for _ in range(blocks):
            self.advance_block()
        return self._event_log

    def get_event_log(self) -> List[Dict]:
        return self._event_log

    def get_pending_withdrawals(self) -> List[Dict]:
        """Return pending withdrawals that relayers can pick up."""
        return [
            w for w in self.bridge.withdrawals.values()
            if w["status"] == "pending" and self.block_height < w["timeout_height"]
        ]

    def execute_withdrawal(
        self,
        nullifier: str,
        relayer_id: str,
        success: bool = True,
    ) -> Dict:
        """Execute a withdrawal (called by relayer after external chain tx)."""
        w = self.bridge.withdrawals.get(nullifier)
        if not w or w["status"] != "pending":
            return {"error": "withdrawal_not_pending"}

        fee = self.feed_manager.compute_fee(w["amount"], w["feed_mode"])

        if success:
            w["status"] = "executed"
            w["executed_by"] = relayer_id
            w["executed_at"] = self.block_height
            w["fee_collected"] = fee
            self.stake_manager.release_stake(relayer_id, w["amount"])
            self.log_event(
                "withdrawal_executed",
                nullifier=nullifier,
                relayer_id=relayer_id,
                amount=w["amount"],
                fee=fee,
            )
            # Settle fees to backer deployments periodically
            self.capital_deployer.accumulate_fees(relayer_id, fee, self)
        else:
            if w["feed_mode"] == "guaranteed":
                # Slash relayer stake, refund premium to user
                slash_amt = self.config.slash_amount
                self.stake_manager.slash(relayer_id, slash_amt, self)
                w["status"] = "slashed"
                w["refund_amount"] = slash_amt
                self.log_event(
                    "withdrawal_slashed",
                    nullifier=nullifier,
                    relayer_id=relayer_id,
                    slash_amount=slash_amt,
                )
            else:
                w["status"] = "failed"
                w["failed_at"] = self.block_height
                self.stake_manager.release_stake(relayer_id, w["amount"])
                self.log_event(
                    "withdrawal_failed",
                    nullifier=nullifier,
                    relayer_id=relayer_id,
                )

        return {"status": w["status"]}

    def cancel_withdrawal(self, nullifier: str) -> Dict:
        """Cancel a timed-out withdrawal."""
        w = self.bridge.withdrawals.get(nullifier)
        if not w:
            return {"error": "not_found"}
        if w["status"] != "pending":
            return {"error": "not_pending"}
        if self.block_height < w["timeout_height"]:
            return {"error": "not_timed_out"}
        w["status"] = "cancelled"
        w["cancelled_at"] = self.block_height
        self.log_event("withdrawal_cancelled", nullifier=nullifier)
        return {"status": "cancelled"}
