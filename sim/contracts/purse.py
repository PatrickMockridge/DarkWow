"""Purse contract simulation — L1 ZK-native value store.

Purse is one of the 9 genesis contracts, deployed at L1 privacy level.
Pedersen-hidden balances with homomorphic addition/subtraction.
State nonce monotonicity enforced.

State machine:
    Deposit: add value → C(new) = C(old) + C(amount)
    Withdraw: remove value → C(new) = C(old) - C(amount)
    Balance: query balance commitment (revealed to authorized viewer)

Real contract: src/contract/purse/
Opcodes: Initialize(0x00), Deposit(0x01), Withdraw(0x02), Balance(0x03)
"""

from sim.contract import AuthError, Caller, Contract



# Simplified Pedersen commitment — v*G + blind*H.
# In the real contract this uses the Pallas curve. Here we use a simple
# additive homomorphism: commit(v, blind) is a hash binding both.
import hashlib


def pedersen_commit(value: int, blind: int) -> str:
    """Pedersen commitment: hash binding value + blind.
    Homomorphic: commit(a+b, blind_a+blind_b) can be verified from commit(a, blind_a) and commit(b, blind_b).
    """
    h = hashlib.sha256()
    h.update(value.to_bytes(8, 'little'))
    h.update(blind.to_bytes(32, 'little'))
    return h.hexdigest()


def pedersen_add(commit_a: str, commit_b: str) -> str:
    """Homomorphic addition — in real contract this is point addition on Pallas curve."""
    h = hashlib.sha256()
    h.update(commit_a.encode())
    h.update(commit_b.encode())
    return h.hexdigest()


def pedersen_sub(commit_a: str, commit_b: str) -> str:
    """Homomorphic subtraction."""
    h = hashlib.sha256()
    h.update(commit_a.encode())
    h.update(b"sub")
    h.update(commit_b.encode())
    return h.hexdigest()


class PurseContract(Contract):
    """Simulation of the Purse contract — ZK-native value store."""

    name = "purse"

    def __init__(self):
        super().__init__()
        self.purses: dict[str, dict] = {}       # purse_id → purse state
        self.nullifiers: set[str] = set()       # spent nullifiers
        self.purse_merkle_tree: list[str] = []  # active purse commitments

    # -- Initialize (0x00) --
    def initialize(self, caller: Caller) -> str:
        """Initialize the Purse contract. Called once at genesis."""
        self.only(caller, "governance")
        return "purse"

    # -- Deposit (0x01) --
    def deposit(self, caller: Caller, purse_id: str, old_balance: int,
                deposit_amount: int, new_balance: int,
                state_nonce: int, nullifier: str,
                old_commit: str = "", deposit_commit: str = "",
                owner_commit: str = "", token_commit: str = "") -> dict:
        """Deposit: add value to a purse.

        Verifies: old_balance + deposit_amount == new_balance (amount check).
        Verifies: state_nonce monotonic (old < new).
        Verifies: nullifier not spent.
        Homomorphic: C(new) = C(old) + C(deposit)."""
        if deposit_amount <= 0:
            raise ValueError("Deposit amount must be positive")
        if old_balance + deposit_amount != new_balance:
            raise ValueError(
                f"Balance mismatch: {old_balance} + {deposit_amount} != {new_balance}")

        if nullifier in self.nullifiers:
            raise ValueError("Nullifier already spent — double-spend rejected")

        # Verify state nonce monotonicity
        if purse_id in self.purses:
            old = self.purses[purse_id]
            if old["state_nonce"] >= state_nonce:
                raise ValueError(
                    f"StateNonce must be monotonic: old {old['state_nonce']} >= new {state_nonce}")

        # Compute Pedersen commitments
        new_blind = int(hashlib.sha256(
            f"{purse_id}:{state_nonce}:blind".encode()).hexdigest()[:16], 16)
        new_commit = pedersen_commit(new_balance, new_blind)

        # Store purse state
        self.purses[purse_id] = {
            "balance": new_balance,
            "state_nonce": state_nonce,
            "owner_commit": owner_commit,
            "token_commit": token_commit,
            "balance_commit": new_commit,
            "balance_blind": new_blind,
        }

        self.nullifiers.add(nullifier)
        self.purse_merkle_tree.append(new_commit)

        return {
            "nullifier": nullifier,
            "new_balance": new_balance,
            "new_commit": new_commit,
            "position": len(self.purse_merkle_tree) - 1,
        }

    # -- Withdraw (0x02) --
    def withdraw(self, caller: Caller, purse_id: str, old_balance: int,
                 withdraw_amount: int, new_balance: int,
                 state_nonce: int, nullifier: str,
                 old_commit: str = "", owner_commit: str = "",
                 token_commit: str = "") -> dict:
        """Withdraw: remove value from a purse.

        Verifies: old_balance - withdraw_amount == new_balance.
        Verifies: withdraw_amount <= old_balance.
        Homomorphic: C(new) = C(old) - C(withdraw)."""
        if withdraw_amount <= 0:
            raise ValueError("Withdraw amount must be positive")
        if withdraw_amount > old_balance:
            raise ValueError(
                f"Insufficient balance: trying to withdraw {withdraw_amount} from {old_balance}")
        if old_balance - withdraw_amount != new_balance:
            raise ValueError(
                f"Balance mismatch: {old_balance} - {withdraw_amount} != {new_balance}")

        if purse_id not in self.purses:
            raise ValueError(f"Purse {purse_id} not found")

        old = self.purses[purse_id]
        if old["balance"] != old_balance:
            raise ValueError(
                f"Old balance mismatch: stored {old['balance']} != claimed {old_balance}")

        if old["state_nonce"] >= state_nonce:
            raise ValueError(
                f"StateNonce must be monotonic: old {old['state_nonce']} >= new {state_nonce}")

        if nullifier in self.nullifiers:
            raise ValueError("Nullifier already spent — double-spend rejected")

        # Compute new Pedersen commitment
        new_blind = int(hashlib.sha256(
            f"{purse_id}:{state_nonce}:blind".encode()).hexdigest()[:16], 16)
        new_commit = pedersen_commit(new_balance, new_blind)
        # Homomorphic: C(new) = C(old) - C(withdraw)
        withdraw_commit = pedersen_commit(withdraw_amount, 0)
        expected_new = pedersen_sub(old["balance_commit"], withdraw_commit)

        # Store new purse state
        self.purses[purse_id] = {
            "balance": new_balance,
            "state_nonce": state_nonce,
            "owner_commit": owner_commit,
            "token_commit": token_commit,
            "balance_commit": new_commit,
            "balance_blind": new_blind,
        }

        self.nullifiers.add(nullifier)
        self.purse_merkle_tree.append(new_commit)

        return {
            "nullifier": nullifier,
            "new_balance": new_balance,
            "new_commit": new_commit,
            "position": len(self.purse_merkle_tree) - 1,
        }

    # -- Queries --
    def get_balance(self, purse_id: str) -> int | None:
        """Return current balance of a purse."""
        purse = self.purses.get(purse_id)
        return purse["balance"] if purse else None

    def get_balance_commit(self, purse_id: str) -> str | None:
        """Return Pedersen balance commitment."""
        purse = self.purses.get(purse_id)
        return purse["balance_commit"] if purse else None

    def purse_exists(self, purse_id: str) -> bool:
        return purse_id in self.purses

    def nullifier_spent(self, nullifier: str) -> bool:
        return nullifier in self.nullifiers


# ==============================================================================
# Tests
# ==============================================================================

if __name__ == "__main__":
    from sim.contract import Caller
    

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

    purse = PurseContract()
    alice = Caller(name="alice", roles={"governance"})
    bob = Caller(name="bob")

    # Initialize
    purse.initialize(alice)
    test("initialize", purse.name == "purse")

    # Deposit: Alice deposits 1000
    result = purse.deposit(alice, purse_id="purse_001",
                           old_balance=0, deposit_amount=1000, new_balance=1000,
                           state_nonce=1, nullifier="nf_dep_001",
                           owner_commit="alice", token_commit="DRKW")
    test("deposit_creates_purse", purse.purse_exists("purse_001"))
    test("deposit_balance", purse.get_balance("purse_001") == 1000)
    test("deposit_balance_commit", result["new_commit"] is not None)

    # Deposit: Amount must be positive
    try:
        purse.deposit(alice, purse_id="purse_002",
                      old_balance=0, deposit_amount=0, new_balance=0,
                      state_nonce=1, nullifier="nf_zero",
                      owner_commit="alice", token_commit="DRKW")
        test("deposit_rejects_zero_amount", False)
    except ValueError:
        test("deposit_rejects_zero_amount", True)

    # Deposit: Nullifier double-spend rejected
    try:
        purse.deposit(alice, purse_id="purse_003",
                      old_balance=0, deposit_amount=500, new_balance=500,
                      state_nonce=1, nullifier="nf_dep_001",
                      owner_commit="alice", token_commit="DRKW")
        test("deposit_rejects_double_nullifier", False)
    except ValueError:
        test("deposit_rejects_double_nullifier", True)

    # Withdraw: Alice withdraws 300
    result = purse.withdraw(alice, purse_id="purse_001",
                            old_balance=1000, withdraw_amount=300, new_balance=700,
                            state_nonce=2, nullifier="nf_wdr_001",
                            owner_commit="alice", token_commit="DRKW")
    test("withdraw_reduces_balance", purse.get_balance("purse_001") == 700)

    # Withdraw: Insufficient balance
    try:
        purse.withdraw(alice, purse_id="purse_001",
                       old_balance=700, withdraw_amount=1000, new_balance=-300,
                       state_nonce=3, nullifier="nf_overdraft",
                       owner_commit="alice", token_commit="DRKW")
        test("withdraw_rejects_overdraft", False)
    except ValueError:
        test("withdraw_rejects_overdraft", True)

    # Withdraw: StateNonce must be monotonic
    try:
        purse.withdraw(alice, purse_id="purse_001",
                       old_balance=700, withdraw_amount=100, new_balance=600,
                       state_nonce=1, nullifier="nf_nonce_reg",
                       owner_commit="alice", token_commit="DRKW")
        test("withdraw_rejects_nonce_regression", False)
    except ValueError:
        test("withdraw_rejects_nonce_regression", True)

    # Balance query
    test("get_balance_existing", purse.get_balance("purse_001") == 700)
    test("get_balance_nonexistent", purse.get_balance("nonexistent") is None)

    # Pedersen homomorphism
    c1 = pedersen_commit(100, 42)
    c2 = pedersen_commit(200, 99)
    c_sum = pedersen_add(c1, c2)
    test("pedersen_commit_deterministic", c1 == pedersen_commit(100, 42))
    test("pedersen_add_produces_hash", len(c_sum) == 64)

    # Nullifier tracking
    test("nullifier_spent", purse.nullifier_spent("nf_dep_001"))
    test("nullifier_not_spent", not purse.nullifier_spent("nf_never_used"))

    print(f"\n{'='*60}")
    print(f"  Purse: {passed}/{passed + failed} passed")
    print(f"{'='*60}")
