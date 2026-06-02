"""Auction contract simulation.

Dual state machines: Auction (Created→Active→Closed→Settled) and Bid
(Active→Outbid→Refunded / Active→Won→Claimed).

Real contract: src/contract/auction/
Opcodes: CreateAuction(0x00), PlaceBid(0x01), CloseAuction(0x02),
         ClaimWinnings(0x03), SettleAuction(0x04), RefundBid(0x05)
"""

from dataclasses import dataclass, field
from typing import Dict, Optional

from sim.contract import (
    ANYONE, BIDDER, BUYER, SELLER, AuthError, Caller, ConstraintError, Contract,
)
from sim.state import StateMachine


class Auction(Contract):
    name = "auction"

    def __init__(self):
        super().__init__()
        self.auctions: Dict[str, dict] = {}  # auction_id → metadata
        self._bid_machines: Dict[str, StateMachine] = {}  # bid_id → machine

    def create_auction(self, caller: Caller, reserve_price: int, deadline_block: int) -> str:
        """Seller creates auction."""
        self.only(caller, SELLER)
        sm = StateMachine("Created")
        sm.add_transition("Created", "Active", "Cancelled")
        sm.add_transition("Active", "Closed")
        sm.add_transition("Closed", "Settled")
        aid = self._new_instance(sm, seller=caller.name, reserve=reserve_price,
                                  deadline=deadline_block, high_bid=0, high_bidder=None)
        self.auctions[aid] = {"bids": {}}
        return aid

    def place_bid(self, caller: Caller, auction_id: str, amount: int) -> str:
        """Bidder places bid — must exceed current high bid and reserve."""
        self.only(caller, BIDDER)
        inst = self._get(auction_id)
        if inst.machine.current == "Created":
            self.transition(auction_id, "Active")
        self.only_state(auction_id, "Active")
        if self.block_height > inst.metadata["deadline"]:
            raise ConstraintError("Auction deadline has passed")
        if amount <= inst.metadata["reserve"]:
            raise ConstraintError(f"Bid {amount} below reserve {inst.metadata['reserve']}")
        if amount <= inst.metadata["high_bid"]:
            raise ConstraintError(f"Bid {amount} must exceed current high bid {inst.metadata['high_bid']}")

        # Outbid previous high bidder
        prev_bidder = inst.metadata["high_bidder"]
        if prev_bidder is not None:
            old_bid_id = f"{auction_id}:bid:{prev_bidder}"
            if old_bid_id in self._bid_machines:
                self._bid_machines[old_bid_id].transition("Outbid")

        inst.metadata["high_bid"] = amount
        inst.metadata["high_bidder"] = caller.name

        bid_id = f"{auction_id}:bid:{caller.name}"
        bsm = StateMachine("Active")
        bsm.add_transition("Active", "Won", "Outbid")
        bsm.add_transition("Outbid", "Refunded")
        bsm.add_transition("Won", "Claimed")
        self._bid_machines[bid_id] = bsm
        self.auctions[auction_id]["bids"][caller.name] = amount
        return bid_id

    def close_auction(self, caller: Caller, auction_id: str):
        """Seller closes auction at/after deadline."""
        self.only(caller, SELLER)
        self.only_state(auction_id, "Active")
        inst = self._get(auction_id)
        if self.block_height < inst.metadata["deadline"]:
            raise ConstraintError("Cannot close before deadline")
        self.transition(auction_id, "Closed")
        # Winning bid transitions to Won
        winner = inst.metadata["high_bidder"]
        if winner is not None:
            bid_id = f"{auction_id}:bid:{winner}"
            if bid_id in self._bid_machines:
                self._bid_machines[bid_id].transition("Won")

    def claim_winnings(self, caller: Caller, auction_id: str):
        """Winner claims the auction item."""
        inst = self._get(auction_id)
        self.only_state(auction_id, "Closed")
        winner = inst.metadata["high_bidder"]
        if caller.name != winner:
            raise AuthError(f"Only the winner '{winner}' can claim")
        bid_id = f"{auction_id}:bid:{caller.name}"
        if bid_id in self._bid_machines:
            self._bid_machines[bid_id].transition("Claimed")

    def settle_auction(self, caller: Caller, auction_id: str):
        """Seller receives payment."""
        self.only(caller, SELLER)
        self.only_state(auction_id, "Closed")
        self.transition(auction_id, "Settled")

    def refund_bid(self, caller: Caller, auction_id: str):
        """Outbid bidder claims refund."""
        self.only(caller, BIDDER)
        bid_id = f"{auction_id}:bid:{caller.name}"
        if bid_id not in self._bid_machines:
            raise ConstraintError(f"No bid from {caller.name} in auction {auction_id}")
        bsm = self._bid_machines[bid_id]
        if bsm.current != "Outbid":
            raise ConstraintError(f"Bid is {bsm.current}, not Outbid — cannot refund")
        bsm.transition("Refunded")
