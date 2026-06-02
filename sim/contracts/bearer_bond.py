"""Bearer Bond contract simulation.

Fixed-interest staking contract. Models:
- Series lifecycle: Active → Voided → Matured
- Two-step interest flow: RequestInterestV1 → PayInterestV1
- Coverage enforcement: ProveCoverageV1, VerifyCoverageV1
- Emergency unstake when coverage drops below minimum
- Deterministic interest: principal × rate × blocks / (bp × blocks_per_year)

Real contract: src/contract/bearer_bond/
State machine:
    Series: Active --[void]--> Voided | Active --[mature]--> Matured
    Claim: Pending --[pay]--> Paid
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, Optional

from sim.contract import (
    ANYONE, HOLDER, ISSUER, AuthError, Caller, ConstraintError, Contract,
)
from sim.state import StateMachine


# Constants matching the real contract
BP_PRECISION: int = 10000
BLOCKS_PER_YEAR: int = 15_768_000
MIN_COVERAGE_RATIO_BPS: int = 10000  # 100%


class SeriesStatus(Enum):
    ACTIVE = "Active"
    VOIDED = "Voided"
    MATURED = "Matured"


class ClaimStatus(Enum):
    PENDING = "Pending"
    PAID = "Paid"


@dataclass
class BondSeries:
    """Per-series configuration and state."""
    series_token_id: str
    interest_rate_bps: int       # Annual rate in basis points (500 = 5%)
    maturity_block: int
    status: SeriesStatus = SeriesStatus.ACTIVE
    issuer_contract: str = ""
    total_staked: int = 0
    reserve_amount: int = 0      # Issuer's reserve balance


@dataclass
class StakeCoin:
    """A single stake position."""
    token_commit: str
    owner: str                   # Caller name who holds this coin
    principal: int
    last_claim_block: int        # Block of last paid interest claim
    maturity_block: int
    issuer_contract: str
    series_token_id: str


@dataclass
class CoverageReport:
    """On-chain record of a coverage proof."""
    series_token_id: str
    total_outstanding: int
    total_interest_obligation: int
    reserve_amount: int
    coverage_ratio_bps: int
    report_block: int

    @property
    def is_voided(self) -> bool:
        return self.coverage_ratio_bps < MIN_COVERAGE_RATIO_BPS


@dataclass
class RequestedClaim:
    """Pending interest claim awaiting issuer payment."""
    interest_amount: int
    payment_key: str
    status: ClaimStatus = ClaimStatus.PENDING


def calculate_interest(principal: int, interest_rate_bps: int, blocks_elapsed: int) -> int:
    """Deterministic interest formula matching the real contract."""
    if blocks_elapsed == 0:
        return 0
    return (principal * interest_rate_bps * blocks_elapsed) // (BP_PRECISION * BLOCKS_PER_YEAR)


class BearerBond(Contract):
    """Simulation of the Bearer Bond fixed-interest staking contract."""

    name = "bearer_bond"

    def _init(self):
        self.series: Dict[str, BondSeries] = {}       # series_token_id → BondSeries
        self.stakes: Dict[str, StakeCoin] = {}         # token_commit → StakeCoin
        self.claims: Dict[str, RequestedClaim] = {}    # (token_commit, claim_block) → RequestedClaim
        self.coverage_reports: Dict[str, CoverageReport] = {}  # series_token_id → latest report

    # -- IssueStakeV1 (0x00) --
    def issue_stake(
        self,
        caller: Caller,
        series_token_id: str,
        token_commit: str,
        owner: str,
        principal: int,
        min_claim: int = 1,
    ) -> str:
        """Issuer creates a staking pool and mints a stake coin to the staker."""
        self.only(caller, ISSUER)

        if series_token_id not in self.series:
            raise ConstraintError(f"Series '{series_token_id}' not found")
        series = self.series[series_token_id]
        if series.status != SeriesStatus.ACTIVE:
            raise ConstraintError(f"Series '{series_token_id}' is {series.status.value}")

        if principal <= 0:
            raise ConstraintError("Principal must be positive")

        coin = StakeCoin(
            token_commit=token_commit,
            owner=owner,
            principal=principal,
            last_claim_block=self.block_height,
            maturity_block=series.maturity_block,
            issuer_contract=series.issuer_contract,
            series_token_id=series_token_id,
        )
        self.stakes[token_commit] = coin
        series.total_staked += principal

        self._db_set("coins", token_commit, coin)
        return token_commit

    # -- TransferStakeV1 (0x01) --
    def transfer_stake(self, caller: Caller, token_commit: str, new_owner: str):
        """Holder transfers stake position. Last_claim_block preserved."""
        coin = self._get_stake(token_commit)
        self.only(caller, HOLDER)
        if coin.owner != caller.name:
            raise AuthError(f"Caller '{caller.name}' does not own stake {token_commit}")
        if self.block_height >= coin.maturity_block:
            raise ConstraintError("Cannot transfer after maturity")
        series = self.series[coin.series_token_id]
        if series.status != SeriesStatus.ACTIVE:
            raise ConstraintError(f"Series is {series.status.value}")
        coin.owner = new_owner

    # -- RequestInterestV1 (0x02) --
    def request_interest(
        self,
        caller: Caller,
        token_commit: str,
        claim_block: int,
        payment_key: str,
        min_claim: int = 1,
    ) -> int:
        """Holder requests interest payment. Creates Pending claim record."""
        coin = self._get_stake(token_commit)
        self.only(caller, HOLDER)
        if coin.owner != caller.name:
            raise AuthError(f"Caller '{caller.name}' does not own stake {token_commit}")

        series = self.series[coin.series_token_id]
        if series.status != SeriesStatus.ACTIVE:
            raise ConstraintError(f"Series is {series.status.value}")

        if claim_block <= coin.last_claim_block:
            raise ConstraintError(
                f"claim_block ({claim_block}) must be > last_claim_block ({coin.last_claim_block})"
            )

        claim_key = f"{token_commit}:{claim_block}"
        if claim_key in self.claims:
            raise ConstraintError(f"Pending claim already exists for {claim_key}")

        blocks_elapsed = claim_block - coin.last_claim_block
        interest = calculate_interest(coin.principal, series.interest_rate_bps, blocks_elapsed)

        if interest < min_claim:
            raise ConstraintError(
                f"Interest {interest} below minimum claim {min_claim}"
            )

        claim = RequestedClaim(
            interest_amount=interest,
            payment_key=payment_key,
            status=ClaimStatus.PENDING,
        )
        self.claims[claim_key] = claim
        self._db_set("bonds_info", claim_key, claim)
        return interest

    # -- EmergencyUnstakeV1 (0x03) --
    def emergency_unstake(self, caller: Caller, token_commit: str):
        """Holder exits before maturity when coverage < 100%."""
        coin = self._get_stake(token_commit)
        self.only(caller, HOLDER)
        if coin.owner != caller.name:
            raise AuthError(f"Caller '{caller.name}' does not own stake {token_commit}")

        series = self.series[coin.series_token_id]
        report = self.coverage_reports.get(series.series_token_id)
        if report is None:
            raise ConstraintError("No coverage report — cannot prove under-collateralization")
        if not report.is_voided:
            raise ConstraintError(
                f"Coverage ratio {report.coverage_ratio_bps} bps >= "
                f"{MIN_COVERAGE_RATIO_BPS} bps — emergency unstake not allowed"
            )

        # Void the series on first emergency unstake
        if series.status == SeriesStatus.ACTIVE:
            series.status = SeriesStatus.VOIDED

        series.total_staked -= coin.principal
        del self.stakes[token_commit]

    # -- UnstakeV1 (0x04) --
    def unstake(self, caller: Caller, token_commit: str):
        """Holder withdraws principal at or after maturity."""
        coin = self._get_stake(token_commit)
        self.only(caller, HOLDER)
        if coin.owner != caller.name:
            raise AuthError(f"Caller '{caller.name}' does not own stake {token_commit}")
        if self.block_height < coin.maturity_block:
            raise ConstraintError(
                f"Stake not yet matured — current block {self.block_height} < "
                f"maturity block {coin.maturity_block}"
            )
        series = self.series[coin.series_token_id]
        series.total_staked -= coin.principal
        del self.stakes[token_commit]

    # -- BurnStakeV1 (0x05) --
    def burn_stake(self, caller: Caller, series_token_id: str):
        """Issuer retires the staking pool."""
        self.only(caller, ISSUER)
        series = self._get_series(series_token_id)
        series.status = SeriesStatus.VOIDED

    # -- ProveCoverageV1 (0x06) --
    def prove_coverage(
        self,
        caller: Caller,
        series_token_id: str,
        total_outstanding: int,
        total_interest_obligation: int,
        reserve_amount: int,
        report_block: int,
    ):
        """Submit ZK proof of reserves (callable by issuer or any holder).

        Accepts any coverage report whose arithmetic is ZK-proven. If the
        coverage ratio is below minimum, the series is auto-voided,
        enabling EmergencyUnstakeV1.
        """
        self.only(caller, ISSUER, HOLDER)
        series = self._get_series(series_token_id)

        total_obligation = total_outstanding + total_interest_obligation
        if total_obligation == 0:
            raise ConstraintError("Total obligation is zero")

        coverage_ratio_bps = (reserve_amount * BP_PRECISION) // total_obligation

        report = CoverageReport(
            series_token_id=series.series_token_id,
            total_outstanding=total_outstanding,
            total_interest_obligation=total_interest_obligation,
            reserve_amount=reserve_amount,
            coverage_ratio_bps=coverage_ratio_bps,
            report_block=report_block,
        )
        self.coverage_reports[series.series_token_id] = report
        series.reserve_amount = reserve_amount

        # Auto-void the series if coverage is below minimum
        if report.is_voided and series.status == SeriesStatus.ACTIVE:
            series.status = SeriesStatus.VOIDED

    # -- VerifyCoverageV1 (0x07) --
    def verify_coverage(self, caller: Caller, series_token_id: str) -> Optional[CoverageReport]:
        """Read latest coverage report (read-only)."""
        return self.coverage_reports.get(series_token_id)

    # -- PayInterestV1 (0x08) --
    def pay_interest(self, caller: Caller, token_commit: str, claim_block: int):
        """Issuer pays a pending interest claim."""
        self.only(caller, ISSUER)

        claim_key = f"{token_commit}:{claim_block}"
        claim = self.claims.get(claim_key)
        if claim is None:
            raise ConstraintError(f"No claim found for {claim_key}")
        if claim.status != ClaimStatus.PENDING:
            raise ConstraintError(f"Claim {claim_key} is already {claim.status.value}")

        coin = self._get_stake(token_commit)
        series = self._get_series(coin.series_token_id)

        if series.status == SeriesStatus.VOIDED:
            raise ConstraintError("Series is voided — cannot pay interest")

        # Enforce ringfencing: issuer must have filed a coverage report
        report = self.coverage_reports.get(series.series_token_id)
        if report is None:
            raise ConstraintError("No coverage report — issuer must prove reserves before paying")

        # Update last_claim_block and mark claim paid
        coin.last_claim_block = claim_block
        claim.status = ClaimStatus.PAID

    # -- Helpers --

    def _get_stake(self, token_commit: str) -> StakeCoin:
        if token_commit not in self.stakes:
            raise ConstraintError(f"Stake '{token_commit}' not found")
        return self.stakes[token_commit]

    def _get_series(self, series_token_id: str) -> BondSeries:
        if series_token_id not in self.series:
            raise ConstraintError(f"Series '{series_token_id}' not found")
        return self.series[series_token_id]

    def create_series(
        self,
        caller: Caller,
        series_token_id: str,
        interest_rate_bps: int,
        maturity_block: int,
        initial_reserve: int = 0,
    ) -> BondSeries:
        """Convenience: create a bond series (normally done via contract init)."""
        self.only(caller, ISSUER)
        series = BondSeries(
            series_token_id=series_token_id,
            interest_rate_bps=interest_rate_bps,
            maturity_block=maturity_block,
            issuer_contract=caller.name,
            reserve_amount=initial_reserve,
        )
        self.series[series_token_id] = series
        self._db_set("bonds_info", series_token_id, series)
        return series
