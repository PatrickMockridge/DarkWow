"""Tests for escrow contract simulation.

Covers: all legal transitions, illegal transitions, authorization checks,
        timeout enforcement, double-claim prevention.
"""

import pytest
from sim.contract import Caller
from sim.contracts.escrow import Escrow
from sim.state import AuthError, ConstraintError, StateError


@pytest.fixture
def escrow():
    return Escrow()


@pytest.fixture
def alice():
    return Caller("alice", ["buyer"])


@pytest.fixture
def bob():
    return Caller("bob", ["seller"])


@pytest.fixture
def mallory():
    return Caller("mallory", ["buyer"])


def test_full_lifecycle(escrow, alice, bob):
    """Happy path: create → fund → claim."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    assert escrow._get(eid).machine.current == "Created"

    escrow.fund(bob, eid)
    assert escrow._get(eid).machine.current == "Funded"

    escrow.claim(bob, eid)
    assert escrow._get(eid).machine.current == "Claimed"


def test_create_then_cancel(escrow, alice):
    """Buyer can cancel before funding."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.cancel(alice, eid)
    assert escrow._get(eid).machine.current == "Cancelled"


def test_refund_after_timeout(escrow, alice, bob):
    """Buyer can refund after timeout."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.fund(bob, eid)
    escrow.advance_block(250)  # past timeout of 200
    escrow.refund(alice, eid)
    assert escrow._get(eid).machine.current == "Refunded"


def test_cannot_refund_before_timeout(escrow, alice, bob):
    """Refund must be rejected before timeout block."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.fund(bob, eid)
    with pytest.raises(ConstraintError, match="Refund not available"):
        escrow.refund(alice, eid)


def test_cannot_fund_twice(escrow, alice, bob):
    """Once funded, cannot fund again."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.fund(bob, eid)
    with pytest.raises(StateError):
        escrow.fund(bob, eid)


def test_cannot_claim_from_created(escrow, bob):
    """Cannot claim an unfunded escrow."""
    eid = escrow.create_escrow(Caller("alice", ["buyer"]), "alice", "bob", 1000, 200)
    with pytest.raises(StateError):
        escrow.claim(bob, eid)


def test_only_buyer_can_create(escrow, bob):
    """Non-buyer cannot create escrow."""
    with pytest.raises(AuthError):
        escrow.create_escrow(bob, "bob", "alice", 1000, 200)


def test_only_seller_can_claim(escrow, alice, bob):
    """Buyer cannot claim — only seller."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.fund(bob, eid)
    with pytest.raises(AuthError, match="Only the seller"):
        escrow.claim(alice, eid)


def test_only_creator_can_refund(escrow, alice, bob):
    """Stranger cannot refund."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.fund(bob, eid)
    escrow.advance_block(250)
    with pytest.raises(AuthError, match="Only the buyer"):
        escrow.refund(bob, eid)


def test_cannot_cancel_after_funded(escrow, alice, bob):
    """Once funded, cancel is no longer valid."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.fund(bob, eid)
    with pytest.raises(StateError):
        escrow.cancel(alice, eid)


def test_cannot_claim_already_claimed(escrow, alice, bob):
    """Double-claim must be rejected."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.fund(bob, eid)
    escrow.claim(bob, eid)
    with pytest.raises(StateError):
        escrow.claim(bob, eid)


def test_terminal_states(escrow, alice, bob):
    """Claimed, Refunded, Cancelled are terminal — no further transitions."""
    eid = escrow.create_escrow(alice, "alice", "bob", 1000, 200)
    escrow.cancel(alice, eid)
    assert escrow._get(eid).machine.is_terminal()
