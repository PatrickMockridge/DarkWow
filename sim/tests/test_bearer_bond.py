"""Tests for bearer bond contract simulation.

Covers: two-step interest flow, coverage void, emergency unstake,
        maturity enforcement, authorization checks, edge cases from safety.md.
"""

import pytest
from sim.contract import Caller
from sim.contracts.bearer_bond import (
    MIN_COVERAGE_RATIO_BPS, BearerBond, ClaimStatus, calculate_interest,
)
from sim.state import AuthError, ConstraintError


@pytest.fixture
def bond():
    return BearerBond()


@pytest.fixture
def issuer():
    return Caller("issuer_co", ["issuer"])


@pytest.fixture
def holder():
    return Caller("holder_alice", ["holder"])


@pytest.fixture
def holder2():
    return Caller("holder_bob", ["holder"])


def create_series_and_stake(bond, issuer, holder, rate=500, maturity=1_000_000, principal=100_000):
    """Helper: create a series and issue a stake coin."""
    bond.create_series(issuer, "series-1", rate, maturity, initial_reserve=1_000_000)
    tk = bond.issue_stake(issuer, "series-1", "coin-1", holder.name, principal)
    return tk


# -- Lifecycle: issue → transfer → request interest → pay → unstake --

def test_full_lifecycle(bond, issuer, holder, holder2):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.advance_block(1000)

    # Transfer
    bond.transfer_stake(holder, tk, holder2.name)
    assert bond.stakes[tk].owner == holder2.name

    # Request interest
    bond.advance_block(1000)
    interest = bond.request_interest(holder2, tk, bond.block_height, "paykey-1")
    assert interest > 0

    # File coverage
    bond.prove_coverage(issuer, "series-1",
                        total_outstanding=100_000,
                        total_interest_obligation=interest,
                        reserve_amount=200_000,
                        report_block=bond.block_height)

    # Pay
    bond.pay_interest(issuer, tk, bond.block_height)

    # Advance past maturity and unstake
    bond.advance_block(2_000_000)
    bond.unstake(holder2, tk)
    assert tk not in bond.stakes


# -- Two-step interest flow --

def test_request_creates_pending_claim(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.advance_block(100)
    interest = bond.request_interest(holder, tk, bond.block_height, "paykey-1")
    claim_key = f"{tk}:{bond.block_height}"
    assert bond.claims[claim_key].status == ClaimStatus.PENDING
    assert bond.claims[claim_key].interest_amount == interest


def test_cannot_request_duplicate_claim(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.advance_block(100)
    bond.request_interest(holder, tk, bond.block_height, "paykey-1")
    with pytest.raises(ConstraintError, match="Pending claim already exists"):
        bond.request_interest(holder, tk, bond.block_height, "paykey-2")


def test_last_claim_block_not_advanced_until_pay(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    original = bond.stakes[tk].last_claim_block

    bond.advance_block(100)
    bond.request_interest(holder, tk, bond.block_height, "paykey-1")
    # last_claim_block should NOT have changed after request
    assert bond.stakes[tk].last_claim_block == original

    # After issuer pays, last_claim_block advances
    bond.prove_coverage(issuer, "series-1", 100_000, 50, 200_000, bond.block_height)
    bond.pay_interest(issuer, tk, bond.block_height)
    assert bond.stakes[tk].last_claim_block == bond.block_height


def test_cannot_pay_without_coverage_report(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.advance_block(100)
    bond.request_interest(holder, tk, bond.block_height, "paykey-1")
    with pytest.raises(ConstraintError, match="No coverage report"):
        bond.pay_interest(issuer, tk, bond.block_height)


# -- Coverage and emergency unstake --

def test_emergency_unstake_when_coverage_below_min(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    # File a covere report below minimum
    bond.prove_coverage(issuer, "series-1",
                        total_outstanding=100_000,
                        total_interest_obligation=5000,
                        reserve_amount=80_000,  # below 105_000 obligation
                        report_block=bond.block_height)
    bond.emergency_unstake(holder, tk)
    assert tk not in bond.stakes
    assert bond.series["series-1"].status.value == "Voided"


def test_emergency_unstake_not_allowed_when_covered(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.prove_coverage(issuer, "series-1",
                        total_outstanding=100_000,
                        total_interest_obligation=5000,
                        reserve_amount=200_000,  # well above 105_000
                        report_block=bond.block_height)
    with pytest.raises(ConstraintError, match="emergency unstake not allowed"):
        bond.emergency_unstake(holder, tk)


def test_emergency_unstake_without_report(bond, holder):
    # No coverage report filed at all
    bond.series["series-1"] = __import__("sim.contracts.bearer_bond", fromlist=["BondSeries"]).BondSeries(
        "series-1", 500, 1_000_000)
    bond.stakes["coin-1"] = __import__("sim.contracts.bearer_bond", fromlist=["StakeCoin"]).StakeCoin(
        "coin-1", holder.name, 100_000, 0, 1_000_000, "", "series-1")
    with pytest.raises(ConstraintError, match="No coverage report"):
        bond.emergency_unstake(holder, "coin-1")


# -- Maturity enforcement --

def test_cannot_unstake_before_maturity(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder, maturity=1_000_000)
    with pytest.raises(ConstraintError, match="not yet matured"):
        bond.unstake(holder, tk)


def test_can_unstake_at_maturity(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder, maturity=500)
    bond.advance_block(600)
    bond.unstake(holder, tk)
    assert tk not in bond.stakes


# -- Authorization --

def test_only_issuer_can_issue(bond, holder):
    bond.create_series(Caller("issuer_co", ["issuer"]), "series-1", 500, 1_000_000)
    with pytest.raises(AuthError):
        bond.issue_stake(holder, "series-1", "coin-1", holder.name, 100_000)


def test_only_holder_can_request_interest(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.advance_block(100)
    stranger = Caller("stranger", ["holder"])
    with pytest.raises(ConstraintError):  # stranger doesn't own the coin
        bond.request_interest(stranger, tk, bond.block_height, "paykey-1")


def test_only_issuer_can_pay(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.advance_block(100)
    bond.request_interest(holder, tk, bond.block_height, "paykey-1")
    bond.prove_coverage(issuer, "series-1", 100_000, 50, 200_000, bond.block_height)
    with pytest.raises(AuthError):
        bond.pay_interest(holder, tk, bond.block_height)


# -- Interest formula --

def test_calculate_interest():
    # 5% annual rate on 100,000 for 1 year ≈ 5,000
    interest = calculate_interest(100_000, 500, 15_768_000)
    assert interest == 5000

    # Zero blocks = zero interest
    assert calculate_interest(100_000, 500, 0) == 0


def test_interest_below_min_claim(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder, rate=1)  # 0.01% — tiny rate
    bond.advance_block(1)  # just one block
    with pytest.raises(ConstraintError, match="below minimum"):
        bond.request_interest(holder, tk, bond.block_height, "paykey-1", min_claim=1000)


# -- What if issuer never pays? --

def test_pending_claim_blocks_further_requests(bond, issuer, holder):
    tk = create_series_and_stake(bond, issuer, holder)
    bond.advance_block(100)
    bond.request_interest(holder, tk, bond.block_height, "paykey-1")
    # Advance blocks but issuer never pays
    bond.advance_block(200)
    # Can't request again — claim still pending for same block range
    with pytest.raises(ConstraintError, match="Pending claim already exists"):
        bond.request_interest(holder, tk, bond.block_height, "paykey-2")
