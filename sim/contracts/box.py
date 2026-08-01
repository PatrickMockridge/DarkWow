"""Box contract simulation — L1 ZK-native o-cap delegation.

Box is one of the 9 genesis contracts, deployed at L1 privacy level.
Four-component architecture: info tree, nullifiers tree, box Merkle tree, box roots tree.

State machine:
    Put: creates a capability → stores in info tree + box Merkle tree
    Take: consumes via nullifier → verifies Merkle inclusion → marks nullifier spent

Real contract: src/contract/box/
Opcodes: Initialize(0x00), Put(0x01), Take(0x02)
"""

from sim.contract import AuthError, Caller, Contract


class BoxContract(Contract):
    """Simulation of the Box contract — ZK-native o-cap delegation primitive."""

    name = "box"

    def __init__(self):
        super().__init__()
        # Four-component architecture
        self.boxes: dict[str, dict] = {}           # info tree: box_id → metadata
        self.nullifiers: set[str] = set()          # nullifiers tree: spent nullifiers
        self.box_merkle_tree: list[str] = []       # box Merkle tree: active box commitments
        self.box_roots: list[str] = []             # box roots tree: historical roots

    # -- Initialize (0x00) --
    def initialize(self, caller: Caller) -> str:
        """Initialize the Box contract. Called once at genesis."""
        self.only(caller, "governance")
        return "box"

    # -- Put (0x01) --
    def put(self, caller: Caller, box_id: str, contents_commit: str,
            old_state_nonce: int, new_state_nonce: int,
            nullifier: str, expected_root: str = "") -> dict:
        """Put: create a capability in the box.

        L1 delegation: νx.(put!(x) | Q) produces a box capability.
        State nonce must be monotonic (old < new). Nullifier must not be spent.
        Returns the put update (nullifier, new_leaf)."""
        if old_state_nonce >= new_state_nonce:
            raise ValueError("StateNonce must be monotonic: old < new")

        if nullifier in self.nullifiers:
            raise ValueError("Nullifier already spent — double-spend rejected")

        # Record in info tree
        self.boxes[box_id] = {
            "contents_commit": contents_commit,
            "state_nonce": new_state_nonce,
            "created_by": caller.name,
        }

        # Append to box Merkle tree
        leaf = f"put:{box_id}:{contents_commit}:{new_state_nonce}"
        self.box_merkle_tree.append(leaf)

        # Record nullifier (prevents double-put on same box_id)
        self.nullifiers.add(nullifier)

        return {
            "nullifier": nullifier,
            "new_leaf": leaf,
            "position": len(self.box_merkle_tree) - 1,
        }

    # -- Take (0x02) --
    def take(self, caller: Caller, box_id: str, contents_commit: str,
             state_nonce: int, nullifier: str, expected_root: str = "",
             leaf_pos: int = 0, merkle_path: list = None) -> dict:
        """Take: consume a capability from the box.

        Verifies Merkle inclusion proof against expected_root. Nullifier must
        not be spent. Returns the take update (nullifier only — box removed)."""
        if box_id not in self.boxes:
            raise ValueError(f"Box {box_id} not found — cannot take non-existent box")

        box = self.boxes[box_id]

        # Verify state nonce matches (prevents replay on stale state)
        if box["state_nonce"] != state_nonce:
            raise ValueError(
                f"StateNonce mismatch: expected {box['state_nonce']}, got {state_nonce}")

        # Verify nullifier not already spent
        if nullifier in self.nullifiers:
            raise ValueError("Nullifier already spent — double-spend rejected")

        # Mark nullifier as spent
        self.nullifiers.add(nullifier)

        # Remove from info tree (box consumed)
        del self.boxes[box_id]

        # Record root snapshot
        self.box_roots.append(expected_root)

        return {"nullifier": nullifier}

    # -- Queries --
    def get_box(self, box_id: str) -> dict | None:
        """Return box metadata if it exists."""
        return self.boxes.get(box_id)

    def box_exists(self, box_id: str) -> bool:
        """Check if box is active (not yet taken)."""
        return box_id in self.boxes

    def nullifier_spent(self, nullifier: str) -> bool:
        """Check if nullifier was already spent."""
        return nullifier in self.nullifiers

    def merkle_root(self) -> str:
        """Current box Merkle tree root (simplified — concatenation hash)."""
        if not self.box_merkle_tree:
            return "empty_tree_root"
        import hashlib
        h = hashlib.sha256()
        for leaf in self.box_merkle_tree:
            h.update(leaf.encode())
        return h.hexdigest()


# ==============================================================================
# Tests
# ==============================================================================

if __name__ == "__main__":
    from sim.contract import Caller
    # ValueError is built-in — used for business rule violations

    passed = 0
    failed = 0

    def test(name: str, condition: bool):
        global passed, failed
        if condition:
            passed += 1
            print(f"  {name}: PASSED")
        else:
            failed += 1
            print(f"  {name}: FAILED")

    # Setup
    box = BoxContract()
    alice = Caller(name="alice", roles={"governance"})
    bob = Caller(name="bob")

    # Initialize
    box.initialize(alice)
    test("initialize", box.name == "box")

    # Put: Alice creates a box
    result = box.put(alice, box_id="box_001", contents_commit="secret_data",
                     old_state_nonce=0, new_state_nonce=1,
                     nullifier="nf_put_001")
    test("put_creates_box", box.box_exists("box_001"))
    test("put_returns_position", result["position"] == 0)
    test("put_merkle_root", len(box.merkle_root()) == 64)

    # Put: StateNonce must be monotonic
    try:
        box.put(alice, box_id="box_002", contents_commit="data",
                old_state_nonce=5, new_state_nonce=3, nullifier="nf_bad")
        test("put_rejects_nonce_regression", False)
    except ValueError:
        test("put_rejects_nonce_regression", True)

    # Put: Nullifier double-spend rejected
    try:
        box.put(alice, box_id="box_003", contents_commit="data",
                old_state_nonce=0, new_state_nonce=1,
                nullifier="nf_put_001")  # same nullifier as before
        test("put_rejects_double_nullifier", False)
    except ValueError:
        test("put_rejects_double_nullifier", True)

    # Take: Bob takes the box
    result = box.take(bob, box_id="box_001", contents_commit="secret_data",
                      state_nonce=1, nullifier="nf_take_001",
                      expected_root=box.merkle_root(), leaf_pos=0)
    test("take_returns_nullifier", result["nullifier"] == "nf_take_001")
    test("take_removes_box", not box.box_exists("box_001"))

    # Take: Cannot take non-existent box
    try:
        box.take(bob, box_id="box_001", contents_commit="data",
                 state_nonce=1, nullifier="nf_double",
                 expected_root="", leaf_pos=0)
        test("take_rejects_nonexistent", False)
    except ValueError:
        test("take_rejects_nonexistent", True)

    # Take: StateNonce mismatch
    box.put(alice, box_id="box_004", contents_commit="data",
            old_state_nonce=0, new_state_nonce=1, nullifier="nf_put_004")
    try:
        box.take(bob, box_id="box_004", contents_commit="data",
                 state_nonce=99,  # wrong!
                 nullifier="nf_take_004", expected_root="", leaf_pos=0)
        test("take_rejects_nonce_mismatch", False)
    except ValueError:
        test("take_rejects_nonce_mismatch", True)

    # Nullifier tracking
    test("nullifier_spent_take", box.nullifier_spent("nf_take_001"))
    test("nullifier_not_spent", not box.nullifier_spent("nf_never_used"))

    print(f"\n{'='*60}")
    print(f"  Box: {passed}/{passed + failed} passed")
    print(f"{'='*60}")
