"""Escrow contract simulation.

Reference implementation for the simulation framework.
Simplest state machine in the codebase: Created → Funded → Claimed/Refunded.

State machine:
    Created --[Fund]--> Funded --[Claim]--> Claimed
             |                   |
             |                   +--[Refund]--> Refunded
             |
             +--[Cancel]--> Cancelled

Real contract: src/contract/escrow/
Opcodes: InitializeV1(0x00), CreateEscrowV1(0x01), FundV1(0x02),
         ClaimV1(0x03), RefundV1(0x04), CancelV1(0x05)
"""

from sim.contract import (
    ANYONE, BUYER, SELLER, AuthError, Caller, ConstraintError, Contract,
    StateError,
)
from sim.state import StateMachine


class Escrow(Contract):
    """Simulation of the escrow contract."""

    name = "escrow"

    def __init__(self):
        super().__init__()
        self.funders: dict[str, str] = {}  # escrow_id → caller who funded

    # -- InitializeV1 (0x00) --
    def initialize(self, caller: Caller) -> str:
        """Initialize the escrow contract. Governance only."""
        self.only(caller, "governance")
        return "escrow"

    # -- CreateEscrowV1 (0x01) --
    def create_escrow(
        self,
        caller: Caller,
        buyer: str,
        seller: str,
        amount: int,
        timeout_block: int,
    ) -> str:
        """Buyer creates a new escrow with terms."""
        self.only(caller, BUYER)
        sm = StateMachine("Created")
        sm.add_transition("Created", "Funded", "Cancelled")
        sm.add_transition("Funded", "Claimed", "Refunded")
        # Claimed, Refunded, Cancelled are terminal
        return self._new_instance(
            sm,
            buyer=buyer,
            seller=seller,
            amount=amount,
            timeout=timeout_block,
            creator=caller.name,
        )

    # -- FundV1 (0x02) --
    def fund(self, caller: Caller, escrow_id: str):
        """Lock funds into the escrow. Seller (or buyer) funds it."""
        self.only_state(escrow_id, "Created")
        # In the real contract, either party can fund — we check basic auth
        inst = self._get(escrow_id)
        if caller.name not in (inst.metadata["buyer"], inst.metadata["seller"]):
            raise AuthError(
                f"Caller '{caller.name}' is not buyer or seller of escrow {escrow_id}"
            )
        self.funders[escrow_id] = caller.name
        self.transition(escrow_id, "Funded")

    # -- ClaimV1 (0x03) --
    def claim(self, caller: Caller, escrow_id: str):
        """Seller claims funds by proving knowledge of secret."""
        self.only(caller, SELLER)
        self.only_state(escrow_id, "Funded")
        inst = self._get(escrow_id)
        if caller.name != inst.metadata["seller"]:
            raise AuthError(
                f"Only the seller '{inst.metadata['seller']}' can claim escrow {escrow_id}"
            )
        self.transition(escrow_id, "Claimed")

    # -- RefundV1 (0x04) --
    def refund(self, caller: Caller, escrow_id: str):
        """Buyer claims refund after timeout."""
        self.only(caller, BUYER)
        self.only_state(escrow_id, "Funded")
        inst = self._get(escrow_id)
        if caller.name != inst.metadata["buyer"]:
            raise AuthError(
                f"Only the buyer '{inst.metadata['buyer']}' can refund escrow {escrow_id}"
            )
        # In the real contract, refund only works after timeout.
        # Simulate timeout check.
        if self.block_height < inst.metadata["timeout"]:
            raise ConstraintError(
                f"Refund not available until block {inst.metadata['timeout']} "
                f"(current: {self.block_height})"
            )
        self.transition(escrow_id, "Refunded")

    # -- CancelV1 (0x05) --
    def cancel(self, caller: Caller, escrow_id: str):
        """Buyer cancels escrow before funding."""
        self.only(caller, BUYER)
        self.only_state(escrow_id, "Created")
        inst = self._get(escrow_id)
        if caller.name != inst.metadata["buyer"]:
            raise AuthError(
                f"Only the buyer '{inst.metadata['buyer']}' can cancel escrow {escrow_id}"
            )
        self.transition(escrow_id, "Cancelled")
