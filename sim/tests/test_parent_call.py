"""Tests for Class B vulnerability: Parent Call Validation.

Models the upstream DAO auth_xfer parent validation fix.
Upstream commit: 3b73ab4e1
Fix pattern: validate parent contract_id + function_code, not just opcode.

This is an instance of safety.md Lesson 2 — validate the target, not just
the action. Checking data[0] tells you what function will run, but not what
contract will run it.
"""

from sim.contract import Caller
from sim.contracts.identity import DaoEscrow


def test_auth_xfer_without_parent_check():
    """Without fix, auth_xfer succeeds regardless of parent call context."""
    dao = DaoEscrow()
    dao.fix_parent_validation = False

    gov = Caller("governance", ["governance"])
    alice = Caller("alice", ["member"])
    dao.initialize(gov, "escrow", {})

    # Call auth_xfer with a MALICIOUS parent (wrong contract_id)
    result = dao.auth_xfer(alice, {
        "contract_id": "malicious_contract",
        "function_code": 0x99,
    })
    assert result == "auth_xfer_ok"
    print("VULNERABILITY CONFIRMED: auth_xfer succeeds with malicious parent")

    # Call auth_xfer with NO parent at all (direct call)
    result2 = dao.auth_xfer(alice, None)
    assert result2 == "auth_xfer_ok"
    print("VULNERABILITY CONFIRMED: auth_xfer succeeds with no parent")


def test_auth_xfer_with_parent_check():
    """With fix, auth_xfer validates parent is dao::exec()."""
    dao = DaoEscrow()
    dao.fix_parent_validation = True

    gov = Caller("governance", ["governance"])
    alice = Caller("alice", ["member"])
    dao.initialize(gov, "escrow", {})

    # Valid parent: dao_escrow::exec()
    result = dao.auth_xfer(alice, {
        "contract_id": "dao_escrow",
        "function_code": 0x00,
    })
    assert result == "auth_xfer_ok"
    print("FIX: Valid parent (dao_escrow::exec) allowed")

    # Wrong contract_id
    try:
        dao.auth_xfer(alice, {
            "contract_id": "malicious_contract",
            "function_code": 0x00,
        })
        assert False
    except Exception as e:
        assert "not dao_escrow" in str(e)
        print(f"FIX: Wrong contract_id blocked — {e}")

    # Wrong function_code (right contract, wrong function)
    try:
        dao.auth_xfer(alice, {
            "contract_id": "dao_escrow",
            "function_code": 0x05,
        })
        assert False
    except Exception as e:
        assert "not dao::exec" in str(e)
        print(f"FIX: Wrong function_code blocked — {e}")


if __name__ == "__main__":
    test_auth_xfer_without_parent_check()
    test_auth_xfer_with_parent_check()
    print("ALL PARENT CALL VALIDATION TESTS PASSED")
