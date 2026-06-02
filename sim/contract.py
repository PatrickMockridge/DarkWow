"""Base class for all contract simulations.

Provides the shared infrastructure every contract needs:
- Instance management (create, lookup, transition)
- Authorization (caller → role checking)
- Database tree abstraction (Python dicts)
- Block height tracking
"""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple

from sim.state import AuthError, ConstraintError, Instance, StateError, StateMachine


@dataclass
class Caller:
    """Identity of the caller invoking a contract function."""
    name: str
    roles: List[str] = field(default_factory=list)

    def has_role(self, role: str) -> bool:
        return role in self.roles


class Contract:
    """Base class for all contract simulations.

    Subclasses define:
    - init(): set up initial state
    - Function methods: one per contract function, named after the function
    """

    name: str = "base"

    def __init__(self):
        self.instances: Dict[str, Instance] = {}
        self.db: Dict[str, Dict[str, Any]] = {}
        self.block_height: int = 0
        self._next_id: int = 0
        self._init()

    def _init(self):
        """Override in subclasses to set up initial state."""
        pass

    # -- Instance management --

    def _new_instance(self, machine: StateMachine, **meta) -> str:
        """Create a new instance with a unique ID and state machine."""
        self._next_id += 1
        iid = f"{self.name}-{self._next_id}"
        self.instances[iid] = Instance(iid, machine, dict(meta))
        return iid

    def _get(self, instance_id: str) -> Instance:
        """Get an instance by ID, raising if not found."""
        if instance_id not in self.instances:
            raise ConstraintError(f"Instance '{instance_id}' not found")
        return self.instances[instance_id]

    # -- Authorization --

    def only(self, caller: Caller, *roles: str):
        """Assert the caller has at least one of the required roles."""
        if not any(caller.has_role(r) for r in roles):
            raise AuthError(
                f"Caller '{caller.name}' lacks required role(s) {roles}. "
                f"Held roles: {caller.roles}"
            )

    def only_state(self, instance_id: str, *allowed: str):
        """Assert the instance is in one of the allowed states."""
        inst = self._get(instance_id)
        if inst.machine.current not in allowed:
            raise StateError(
                inst.machine.current,
                f"one of {allowed}",
                set(allowed),
            )

    def transition(self, instance_id: str, to: str):
        """Execute a state transition on an instance."""
        inst = self._get(instance_id)
        inst.machine.transition(to)

    # -- Database abstraction --

    def _tree(self, name: str) -> Dict[str, Any]:
        """Get or create a database tree."""
        if name not in self.db:
            self.db[name] = {}
        return self.db[name]

    def _db_get(self, tree: str, key: str) -> Optional[Any]:
        """Read a value from a database tree."""
        return self._tree(tree).get(key)

    def _db_set(self, tree: str, key: str, value: Any):
        """Write a value to a database tree."""
        self._tree(tree)[key] = value

    def _db_contains(self, tree: str, key: str) -> bool:
        """Check if a key exists in a database tree."""
        return key in self._tree(tree)

    # -- Block height --

    def advance_block(self, n: int = 1):
        """Advance the simulated block height."""
        self.block_height += n


# -- Common roles used across contracts --

ISSUER = "issuer"
HOLDER = "holder"
ANYONE = "anyone"
GOVERNANCE = "governance"
BUYER = "buyer"
SELLER = "seller"
BIDDER = "bidder"
ORACLE = "oracle"
HOUSE = "house"
PLAYER = "player"
RELAYER = "relayer"
BACKER = "backer"
UNDERWRITER = "underwriter"
MEMBER = "member"
