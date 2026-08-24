"""Tests for Class A vulnerability: Input Reuse Attacks.

Models the upstream DAO proposal input reuse exploit and verifies the fix.
Upstream commit: 1814306ed
Fix pattern: input_nullifier = poseidon_hash(commitment_nullifier, proposal_bulla)
"""

from sim.contract import Caller
from sim.contracts.identity import DaoEscrow


def test_dao_input_reuse_exploit():
    """Without context binding, same inputs can satisfy threshold for
    multiple proposals — bypassing the proposer threshold."""
    dao = DaoEscrow()
    dao.fix_input_reuse = False
    dao.threshold = 100

    gov = Caller("governance", ["governance"])
    alice = Caller("alice", ["member"])
    dao.initialize(gov, "escrow", {})

    inputs = [
        {"nullifier": "commitment-a-null", "amount": 60},
        {"nullifier": "commitment-b-null", "amount": 50},  # total 110 >= 100
    ]

    pid1 = dao.propose_claim(alice, 500, "Proposal 1", inputs)
    assert pid1 is not None

    # VULNERABILITY: same inputs, different proposal — WORKS
    pid2 = dao.propose_claim(alice, 500, "Proposal 2", inputs)
    assert pid2 is not None

    assert len(dao.proposals) == 2
    print("EXPLOIT CONFIRMED: Same inputs (110 stake) reused across 2 proposals")
    print(f"  Threshold: {dao.threshold}, each proposal met it with same commitments")


def test_dao_input_reuse_fix():
    """With context-bound nullifiers, reused inputs are rejected."""
    dao = DaoEscrow()
    dao.fix_input_reuse = True
    dao.threshold = 100

    gov = Caller("governance", ["governance"])
    alice = Caller("alice", ["member"])
    dao.initialize(gov, "escrow", {})

    inputs = [
        {"nullifier": "commitment-a-null", "amount": 60},
        {"nullifier": "commitment-b-null", "amount": 50},
    ]

    pid1 = dao.propose_claim(alice, 500, "Proposal 1", inputs)
    assert pid1 is not None

    # FIX: same inputs blocked
    try:
        dao.propose_claim(alice, 500, "Proposal 2", inputs)
        assert False, "Should have raised"
    except Exception as e:
        assert "already spent" in str(e)
        print(f"FIX CONFIRMED: Input reuse blocked — {e}")


def test_fix_allows_fresh_inputs():
    """Context binding should NOT block legitimate proposals with new inputs."""
    dao = DaoEscrow()
    dao.fix_input_reuse = True
    dao.threshold = 50

    gov = Caller("governance", ["governance"])
    alice = Caller("alice", ["member"])
    dao.initialize(gov, "escrow", {})

    pid1 = dao.propose_claim(alice, 500, "P1", [
        {"nullifier": "n1", "amount": 100},
    ])
    pid2 = dao.propose_claim(alice, 500, "P2", [
        {"nullifier": "n2", "amount": 100},
    ])
    assert pid1 is not None and pid2 is not None

    # Same nullifier, different proposal — blocked
    try:
        dao.propose_claim(alice, 500, "P3", [
            {"nullifier": "n1", "amount": 100},
        ])
        assert False
    except Exception as e:
        assert "already spent" in str(e)

    print("FIX VERIFIED: Fresh inputs allowed, reused inputs blocked")


if __name__ == "__main__":
    test_dao_input_reuse_exploit()
    test_dao_input_reuse_fix()
    test_fix_allows_fresh_inputs()
    print("ALL TESTS PASSED")
