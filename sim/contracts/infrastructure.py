"""Infrastructure and cross-chain contracts.

Covers: Bridge, PoolStake, RelayerEndowment, DrainProtection, Dex, OtcSwap, Stablecoin.
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional

from sim.contract import (
    ANYONE, BACKER, BUYER, GOVERNANCE, ISSUER, MEMBER, ORACLE, RELAYER,
    SELLER, AuthError, Caller, ConstraintError, Contract,
)
from sim.state import StateMachine


# -- Bridge --

class Bridge(Contract):
    """Cross-chain bridge: deposit→withdraw, HTLC swaps, relayer management."""
    name = "bridge"

    def __init__(self):
        super().__init__()
        self.deposits: Dict[str, dict] = {}       # commitment → deposit
        self.withdrawals: Dict[str, dict] = {}     # nullifier → withdrawal
        self.relayers: Dict[str, dict] = {}         # relayer_pub → info
        self.htlcs: Dict[str, dict] = {}            # htlc_id → state
        self._deposit_counter: int = 0

    def deposit(self, caller: Caller, commitment: str, chain: str, amount: int) -> str:
        """Register a deposit from an external chain."""
        if commitment in self.deposits:
            raise ConstraintError(f"Deposit '{commitment}' already exists")
        self._deposit_counter += 1
        did = f"deposit-{self._deposit_counter}"
        self.deposits[did] = {"commitment": commitment, "chain": chain,
                              "amount": amount, "depositor": caller.name}
        return did

    def withdraw(self, caller: Caller, nullifier: str, recipient: str,
                 amount: int, fee: int) -> str:
        """Submit withdrawal request."""
        if nullifier in self.withdrawals:
            raise ConstraintError(f"Nullifier '{nullifier}' already spent — double-spend rejected")
        wid = f"withdraw-{nullifier[:8]}"
        self.withdrawals[wid] = {"nullifier": nullifier, "recipient": recipient,
                                  "amount": amount, "fee": fee, "status": "Pending",
                                  "relayer": None}
        return wid

    def accept_withdrawal(self, caller: Caller, withdrawal_id: str):
        """Relayer accepts a pending withdrawal."""
        self.only(caller, RELAYER)
        w = self.withdrawals.get(withdrawal_id)
        if w is None:
            raise ConstraintError(f"Withdrawal '{withdrawal_id}' not found")
        if w["status"] != "Pending":
            raise ConstraintError(f"Withdrawal is {w['status']}, not Pending")
        if caller.name not in self.relayers:
            raise ConstraintError(f"Relayer '{caller.name}' not registered")
        w["relayer"] = caller.name
        w["status"] = "Accepted"

    def register_relayer(self, caller: Caller, relayer_pub: str):
        """Register a new relayer."""
        self.only(caller, RELAYER)
        self.relayers[relayer_pub] = {"active": True, "reputation": 0}

    def create_htlc(self, caller: Caller, secret_hash: str, amount: int,
                    counterparty: str, timeout: int) -> str:
        """Create HTLC for cross-chain atomic swap."""
        hid = f"htlc-{secret_hash[:8]}"
        sm = StateMachine("Created")
        sm.add_transition("Created", "Claimed", "Refunded")
        self._new_instance(sm, hash=secret_hash, amount=amount,
                          creator=caller.name, counterparty=counterparty, timeout=timeout)
        self.htlcs[hid] = {"secret_hash": secret_hash, "amount": amount,
                           "creator": caller.name, "counterparty": counterparty,
                           "timeout": timeout, "status": "Created"}
        return hid

    def claim_htlc(self, caller: Caller, htlc_id: str, _secret: str):
        """Counterparty claims HTLC by revealing secret."""
        htlc = self.htlcs.get(htlc_id)
        if htlc is None:
            raise ConstraintError(f"HTLC '{htlc_id}' not found")
        if caller.name != htlc["counterparty"]:
            raise AuthError("Only the counterparty can claim HTLC")
        if htlc["status"] != "Created":
            raise ConstraintError(f"HTLC is {htlc['status']}")
        htlc["status"] = "Claimed"

    def refund_htlc(self, caller: Caller, htlc_id: str):
        """Creator refunds after timeout."""
        htlc = self.htlcs.get(htlc_id)
        if htlc is None:
            raise ConstraintError(f"HTLC '{htlc_id}' not found")
        if caller.name != htlc["creator"]:
            raise AuthError("Only the creator can refund")
        if htlc["status"] != "Created":
            raise ConstraintError(f"HTLC is {htlc['status']}")
        if self.block_height < htlc["timeout"]:
            raise ConstraintError("Timeout not reached")
        htlc["status"] = "Refunded"


# -- PoolStake --

class PoolStake(Contract):
    """Relayer shared coverage pool for guaranteed bridge withdrawals."""
    name = "pool_stake"

    def __init__(self):
        super().__init__()
        self.pools: Dict[str, dict] = {}
        self.allocations: Dict[str, dict] = {}

    def create_pool(self, caller: Caller, pool_id: str, min_stake: int) -> str:
        self.only(caller, RELAYER)
        self.pools[pool_id] = {"creator": caller.name, "min_stake": min_stake,
                               "total_stake": 0, "members": {}, "active": True}
        return pool_id

    def join_pool(self, caller: Caller, pool_id: str, amount: int):
        self.only(caller, RELAYER)
        pool = self._get_pool(pool_id)
        pool["members"][caller.name] = pool["members"].get(caller.name, 0) + amount
        pool["total_stake"] += amount

    def allocate_coverage(self, caller: Caller, pool_id: str, withdrawal_id: str, amount: int):
        """Bridge allocates coverage for a withdrawal."""
        pool = self._get_pool(pool_id)
        if amount > pool["total_stake"]:
            raise ConstraintError("Insufficient pool coverage")
        self.allocations[withdrawal_id] = {"pool": pool_id, "amount": amount, "active": True}

    def slash_coverage(self, caller: Caller, withdrawal_id: str):
        """Slash coverage for failed withdrawal."""
        alloc = self.allocations.get(withdrawal_id)
        if alloc is None:
            raise ConstraintError(f"No allocation for {withdrawal_id}")
        pool = self.pools[alloc["pool"]]
        pool["total_stake"] -= alloc["amount"]
        alloc["active"] = False


# -- RelayerEndowment --

class RelayerEndowment(Contract):
    """Backers deploy capital to relayers in exchange for fee share."""
    name = "relayer_endowment"

    def __init__(self):
        super().__init__()
        self.endowments: Dict[str, dict] = {}
        self.deployments: Dict[str, dict] = {}

    def initialize(self, caller: Caller, endowment_id: str, relayer: str, config: dict):
        self.only(caller, RELAYER)
        self.endowments[endowment_id] = {"relayer": relayer, "config": config, "active": True}

    def deploy_capital(self, caller: Caller, endowment_id: str, amount: int, fee_share_bps: int) -> str:
        self.only(caller, BACKER)
        if endowment_id not in self.endowments:
            raise ConstraintError(f"Endowment '{endowment_id}' not found")
        if not self.endowments[endowment_id]["active"]:
            raise ConstraintError("Endowment is inactive")
        did = f"deploy-{caller.name}-{endowment_id}"
        self.deployments[did] = {"backer": caller.name, "endowment": endowment_id,
                                  "amount": amount, "fee_share_bps": fee_share_bps,
                                  "active": True, "earnings": 0}
        return did

    def claim_fees(self, caller: Caller, deployment_id: str):
        deployment = self._get_deployment(deployment_id)
        if deployment["backer"] != caller.name:
            raise AuthError("Not your deployment")

    def deactivate_endowment(self, caller: Caller, endowment_id: str):
        self.only(caller, RELAYER)
        if endowment_id not in self.endowments:
            raise ConstraintError(f"Endowment '{endowment_id}' not found")
        self.endowments[endowment_id]["active"] = False


# -- DrainProtection --

class DrainProtection(Contract):
    """Governance-level protections: rate limiting, vote thresholds, lock/unlock."""
    name = "drain_protection"

    def __init__(self):
        super().__init__()
        self.funds: Dict[str, dict] = {}
        self.proposals: Dict[str, dict] = {}
        self._proposal_counter: int = 0

    def initialize(self, caller: Caller, fund_id: str, config: dict):
        self.only(caller, GOVERNANCE)
        self.funds[fund_id] = {**config, "locked": False, "total_withdrawn": 0}

    def propose(self, caller: Caller, fund_id: str, action: str, params: dict) -> str:
        self.only(caller, MEMBER)
        self._proposal_counter += 1
        pid = f"prop-{self._proposal_counter}"
        self.proposals[pid] = {"fund": fund_id, "action": action, "params": params,
                               "votes_for": 0, "votes_against": 0, "status": "Pending",
                               "proposer": caller.name}
        return pid

    def vote(self, caller: Caller, proposal_id: str, approve: bool):
        self.only(caller, MEMBER)
        prop = self._get_proposal(proposal_id)
        if prop["status"] != "Pending":
            raise ConstraintError(f"Proposal is {prop['status']}")
        if approve:
            prop["votes_for"] += 1
        else:
            prop["votes_against"] += 1

    def execute(self, caller: Caller, proposal_id: str, threshold: int = 3):
        prop = self._get_proposal(proposal_id)
        if prop["status"] != "Pending":
            raise ConstraintError(f"Proposal is {prop['status']}")
        if prop["votes_for"] < threshold:
            raise ConstraintError(f"Votes {prop['votes_for']} below threshold {threshold}")
        prop["status"] = "Executed"

    def lock(self, caller: Caller, fund_id: str):
        self.only(caller, GOVERNANCE)
        fund = self._get_fund(fund_id)
        if fund["locked"]:
            raise ConstraintError("Already locked")
        fund["locked"] = True

    def unlock(self, caller: Caller, fund_id: str):
        self.only(caller, GOVERNANCE)
        fund = self._get_fund(fund_id)
        if not fund["locked"]:
            raise ConstraintError("Not locked")
        fund["locked"] = False

    def exit(self, caller: Caller, fund_id: str, haircut_bps: int = 0):
        """Member exits with optional haircut."""
        self.only(caller, MEMBER)

    # Helpers
    def _get_fund(self, fund_id): return self.funds.get(fund_id) or (_ for _ in ()).throw(ConstraintError(f"Fund '{fund_id}' not found"))
    def _get_proposal(self, pid): return self.proposals.get(pid) or (_ for _ in ()).throw(ConstraintError(f"Proposal '{pid}' not found"))
    def _get_deployment(self, did): return self.deployments.get(did) or (_ for _ in ()).throw(ConstraintError(f"Deployment '{did}' not found"))
    def _get_pool(self, pid): return self.pools.get(pid) or (_ for _ in ()).throw(ConstraintError(f"Pool '{pid}' not found"))


# -- Dex --

class Dex(Contract):
    """Minimal viable DEX — atomic swaps with privacy."""
    name = "dex"

    def __init__(self):
        super().__init__()
        self.swaps: Dict[str, dict] = {}

    def create_swap(self, caller: Caller, offer_token: str, offer_amount: int,
                    ask_token: str, ask_amount: int, timeout: int) -> str:
        """Proposer creates a swap offer."""
        sid = f"swap-{caller.name}-{offer_token}"
        sm = StateMachine("Created")
        sm.add_transition("Created", "Accepted", "Cancelled")
        sm.add_transition("Accepted", "Executed", "Cancelled")
        self._new_instance(sm, proposer=caller.name, offer_token=offer_token,
                          offer_amount=offer_amount, ask_token=ask_token,
                          ask_amount=ask_amount, timeout=timeout)
        self.swaps[sid] = {"state": "Created", "proposer": caller.name}
        return sid

    def accept_swap(self, caller: Caller, swap_id: str):
        self.only_state(swap_id, "Created")
        self.transition(swap_id, "Accepted")

    def execute_swap(self, caller: Caller, swap_id: str):
        self.only_state(swap_id, "Accepted")
        self.transition(swap_id, "Executed")

    def cancel_swap(self, caller: Caller, swap_id: str):
        inst = self._get(swap_id)
        if inst.machine.current not in ("Created", "Accepted"):
            raise ConstraintError(f"Cannot cancel in state {inst.machine.current}")
        self.transition(swap_id, "Cancelled")


# -- OtcSwap --

class OtcSwap(Contract):
    """P2P OTC token swap — two-phase commit with timeout."""
    name = "otc_swap"

    def __init__(self):
        super().__init__()
        self.swaps: Dict[str, dict] = {}

    def create_swap(self, caller: Caller, offer_token: str, offer_amount: int,
                    ask_token: str, ask_amount: int) -> str:
        sid = f"otc-{caller.name}"
        sm = StateMachine("Created")
        sm.add_transition("Created", "Funded", "Cancelled")
        sm.add_transition("Funded", "Executed", "Cancelled")
        self._new_instance(sm, proposer=caller.name, offer_token=offer_token,
                          offer_amount=offer_amount, ask_token=ask_token, ask_amount=ask_amount)
        return sid

    def fund_swap(self, caller: Caller, swap_id: str):
        self.only_state(swap_id, "Created")
        inst = self._get(swap_id)
        if caller.name != inst.metadata["proposer"]:
            raise AuthError("Only the proposer can fund")
        self.transition(swap_id, "Funded")

    def execute_swap(self, caller: Caller, swap_id: str):
        self.only_state(swap_id, "Funded")
        self.transition(swap_id, "Executed")

    def cancel_swap(self, caller: Caller, swap_id: str):
        inst = self._get(swap_id)
        if inst.machine.current not in ("Created", "Funded"):
            raise ConstraintError(f"Cannot cancel in state {inst.machine.current}")
        self.transition(swap_id, "Cancelled")


# -- Stablecoin --

class Stablecoin(Contract):
    """CDP stablecoin with collateral ratio enforcement and liquidation."""
    name = "stablecoin"

    MIN_COLLATERAL_RATIO_BPS = 15_000  # 150%

    def __init__(self):
        super().__init__()
        self.positions: Dict[str, dict] = {}

    def open_position(self, caller: Caller, collateral: int, mint_amount: int) -> str:
        if collateral <= 0 or mint_amount <= 0:
            raise ConstraintError("Amounts must be positive")
        ratio = (collateral * 10000) // mint_amount if mint_amount > 0 else 0
        if ratio < self.MIN_COLLATERAL_RATIO_BPS:
            raise ConstraintError(
                f"Collateral ratio {ratio} bps below minimum {self.MIN_COLLATERAL_RATIO_BPS}"
            )
        pid = f"cdp-{caller.name}"
        self.positions[pid] = {"owner": caller.name, "collateral": collateral,
                               "debt": mint_amount, "active": True}
        return pid

    def add_collateral(self, caller: Caller, position_id: str, amount: int):
        pos = self._get_position(position_id)
        if pos["owner"] != caller.name:
            raise AuthError("Not your position")
        pos["collateral"] += amount

    def mint_stable(self, caller: Caller, position_id: str, amount: int):
        pos = self._get_position(position_id)
        if pos["owner"] != caller.name:
            raise AuthError("Not your position")
        new_debt = pos["debt"] + amount
        ratio = (pos["collateral"] * 10000) // new_debt
        if ratio < self.MIN_COLLATERAL_RATIO_BPS:
            raise ConstraintError(f"Minting would drop ratio to {ratio} bps")
        pos["debt"] = new_debt

    def repay_stable(self, caller: Caller, position_id: str, amount: int):
        pos = self._get_position(position_id)
        if pos["owner"] != caller.name:
            raise AuthError("Not your position")
        pos["debt"] = max(0, pos["debt"] - amount)

    def liquidate(self, caller: Caller, position_id: str):
        pos = self._get_position(position_id)
        ratio = (pos["collateral"] * 10000) // pos["debt"] if pos["debt"] > 0 else 100_000
        if ratio >= self.MIN_COLLATERAL_RATIO_BPS:
            raise ConstraintError(
                f"Position is healthy — ratio {ratio} bps >= {self.MIN_COLLATERAL_RATIO_BPS}"
            )
        pos["active"] = False

    def _get_position(self, pid: str):
        if pid not in self.positions:
            raise ConstraintError(f"Position '{pid}' not found")
        return self.positions[pid]
