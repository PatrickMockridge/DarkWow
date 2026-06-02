"""State machine utilities for contract simulations.

A StateMachine tracks legal transitions and rejects illegal ones.
Contracts compose multiple state machines — e.g. auction has an Auction
machine and a Bid machine, each with independent lifecycles.
"""

from dataclasses import dataclass, field
from typing import Dict, Optional, Set


class StateError(Exception):
    """Raised when an illegal state transition is attempted."""

    def __init__(self, current: str, attempted: str, legal: Set[str]):
        self.current = current
        self.attempted = attempted
        self.legal = legal
        super().__init__(
            f"Cannot transition from '{current}' to '{attempted}'. "
            f"Legal transitions from '{current}': {sorted(legal)}"
        )


class AuthError(Exception):
    """Raised when an unauthorized caller attempts an action."""
    pass


class ConstraintError(Exception):
    """Raised when a business rule constraint fails (e.g. coverage below min)."""
    pass


@dataclass
class StateMachine:
    """A directed graph of legal state transitions.

    Usage:
        sm = StateMachine("Created")
        sm.add_transition("Created", "Funded")
        sm.add_transition("Funded", "Claimed", "Refunded")
        sm.transition("Created", "Funded")  # OK
        sm.transition("Funded", "Created")  # raises StateError
    """

    initial: str
    current: str = field(init=False)
    _transitions: Dict[str, Set[str]] = field(default_factory=dict)

    def __post_init__(self):
        self.current = self.initial
        self._transitions[self.initial] = set()

    def add_transition(self, from_state: str, *to_states: str):
        """Register legal transitions from a state."""
        if from_state not in self._transitions:
            self._transitions[from_state] = set()
        self._transitions[from_state].update(to_states)
        # Ensure target states exist in the graph
        for s in to_states:
            if s not in self._transitions:
                self._transitions[s] = set()

    def can_transition(self, to: str) -> bool:
        """Check if a transition is legal from the current state."""
        return to in self._transitions.get(self.current, set())

    def transition(self, to: str):
        """Execute a state transition, raising StateError if illegal."""
        if not self.can_transition(to):
            raise StateError(self.current, to, self._transitions.get(self.current, set()))
        self.current = to

    def is_terminal(self) -> bool:
        """Check if the current state is terminal (no outgoing transitions)."""
        return len(self._transitions.get(self.current, set())) == 0

    def __repr__(self):
        return f"StateMachine({self.current})"


@dataclass
class Instance:
    """A contract instance with an ID and a state machine.

    Multiple instances can coexist — e.g. multiple escrows, multiple auctions.
    """

    instance_id: str
    machine: StateMachine
    metadata: Dict = field(default_factory=dict)

    def transition(self, to: str):
        self.machine.transition(to)
