"""Identity, credential, and labor contracts.

Covers: Attestation, Identity, Tender, LaborMarket, Oracle,
         Subscription, InsuranceMarket, DaoEscrow, DarkbetExchange.
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional

from sim.contract import (
    ANYONE, BUYER, GOVERNANCE, ISSUER, MEMBER, ORACLE, SELLER,
    AuthError, Caller, ConstraintError, Contract,
)
from sim.state import StateMachine


# -- Attestation --

class Attestation(Contract):
    """Claims and attestations — used by tender, labor_market, and identity."""
    name = "attestation"

    def __init__(self):
        super().__init__()
        self.attestations: Dict[str, dict] = {}
        self.claims: Dict[str, dict] = {}

    def create_attestation(self, caller: Caller, claim_type: str, data: dict) -> str:
        aid = f"att-{caller.name}-{claim_type}"
        sm = StateMachine("Active")
        sm.add_transition("Active", "Revoked", "Expired")
        self._new_instance(sm, attestor=caller.name, claim_type=claim_type, data=data)
        self.attestations[aid] = {"attestor": caller.name, "type": claim_type, "status": "Active"}
        return aid

    def revoke_attestation(self, caller: Caller, attestation_id: str):
        att = self.attestations.get(attestation_id)
        if att is None:
            raise ConstraintError(f"Attestation '{attestation_id}' not found")
        if att["attestor"] != caller.name:
            raise AuthError("Only the attestor can revoke")
        self.transition(attestation_id, "Revoked")

    def create_claim(self, caller: Caller, attestation_id: str, claim_data: dict) -> str:
        cid = f"claim-{caller.name}-{attestation_id}"
        sm = StateMachine("Pending")
        sm.add_transition("Pending", "Verified", "Rejected")
        sm.add_transition("Verified", "Consumed")
        self._new_instance(sm, claimer=caller.name, attestation=attestation_id, data=claim_data)
        self.claims[cid] = {"claimer": caller.name, "status": "Pending"}
        return cid

    def verify_claim(self, caller: Caller, claim_id: str, valid: bool):
        claim = self.claims.get(claim_id)
        if claim is None:
            raise ConstraintError(f"Claim '{claim_id}' not found")
        if claim["status"] != "Pending":
            raise ConstraintError(f"Claim is {claim['status']}")
        self.transition(claim_id, "Verified" if valid else "Rejected")

    def consume_claim(self, caller: Caller, claim_id: str):
        self.only_state(claim_id, "Verified")
        inst = self._get(claim_id)
        if caller.name != inst.metadata["claimer"]:
            raise AuthError("Only the claimer can consume")
        self.transition(claim_id, "Consumed")


# -- Identity --

class Identity(Contract):
    """Selective disclosure of attributes via ZK proofs."""
    name = "identity"

    def __init__(self):
        super().__init__()
        self.credentials: Dict[str, dict] = {}
        self.issuers: Dict[str, dict] = {}

    def register_issuer(self, caller: Caller, issuer_pub: str):
        self.only(caller, GOVERNANCE)
        self.issuers[issuer_pub] = {"active": True}

    def issue_credential(self, caller: Caller, holder: str, schema: str,
                         attributes: dict, expires_at: int) -> str:
        if caller.name not in self.issuers:
            raise AuthError(f"Issuer '{caller.name}' not registered")
        cid = f"cred-{caller.name}-{holder}-{schema}"
        self.credentials[cid] = {"issuer": caller.name, "holder": holder,
                                 "schema": schema, "attributes": attributes,
                                 "expires_at": expires_at, "active": True}
        return cid

    def revoke_credential(self, caller: Caller, credential_id: str):
        cred = self.credentials.get(credential_id)
        if cred is None:
            raise ConstraintError(f"Credential '{credential_id}' not found")
        if cred["issuer"] != caller.name:
            raise AuthError("Only the issuer can revoke")
        cred["active"] = False

    def verify_capability(self, caller: Caller, capability_id: str,
                          required_schema: str, threshold: int) -> bool:
        """Verify a capability against credentials. Read-only, returns bool."""
        for cid, cred in self.credentials.items():
            if cred["schema"] == required_schema and cred["active"]:
                for attr, value in cred["attributes"].items():
                    if isinstance(value, int) and value >= threshold:
                        return True
        return False


# -- Tender --

class Tender(Contract):
    """Sealed-bid tendering with identity/competency integration."""
    name = "tender"

    def __init__(self):
        super().__init__()
        self.tenders: Dict[str, dict] = {}
        self.bids: Dict[str, dict] = {}

    def create_tender(self, caller: Caller, specs: dict, deadline: int) -> str:
        tid = f"tender-{caller.name}"
        sm = StateMachine("Created")
        sm.add_transition("Created", "Bidding")
        sm.add_transition("Bidding", "Revealed", "Cancelled")
        sm.add_transition("Revealed", "Awarded", "Cancelled")
        self._new_instance(sm, requester=caller.name, specs=specs, deadline=deadline)
        self.tenders[tid] = {"requester": caller.name, "status": "Created", "bids": {}}
        return tid

    def submit_bid(self, caller: Caller, tender_id: str, amount: int):
        tender = self.tenders.get(tender_id)
        if tender is None:
            raise ConstraintError(f"Tender '{tender_id}' not found")
        if tender["status"] == "Created":
            tender["status"] = "Bidding"
        if tender["status"] != "Bidding":
            raise ConstraintError(f"Tender is {tender['status']}")
        bid_id = f"{tender_id}:bid:{caller.name}"
        self.bids[bid_id] = {"tender": tender_id, "bidder": caller.name,
                             "amount": amount, "status": "Sealed"}
        tender["bids"][caller.name] = amount

    def close_tender(self, caller: Caller, tender_id: str):
        tender = self.tenders.get(tender_id)
        if tender is None:
            raise ConstraintError(f"Tender '{tender_id}' not found")
        if tender["requester"] != caller.name:
            raise AuthError("Only the requester can close")
        tender["status"] = "Revealed"

    def select_winner(self, caller: Caller, tender_id: str, winner: str):
        tender = self.tenders.get(tender_id)
        if tender["requester"] != caller.name:
            raise AuthError("Only the requester can select")
        if winner not in tender["bids"]:
            raise ConstraintError(f"No bid from '{winner}'")
        tender["status"] = "Awarded"
        bid_id = f"{tender_id}:bid:{winner}"
        if bid_id in self.bids:
            self.bids[bid_id]["status"] = "Accepted"


# -- LaborMarket --

class LaborMarket(Contract):
    """Job market with escrow payments, milestones, and dispute resolution."""
    name = "labor_market"

    class JobState(Enum):
        CREATED = "Created"
        ACTIVE = "Active"
        SUBMITTED = "Submitted"
        COMPLETED = "Completed"
        DISPUTED = "Disputed"
        REFUNDED = "Refunded"
        CANCELLED = "Cancelled"

    def __init__(self):
        super().__init__()
        self.jobs: Dict[str, dict] = {}

    def create_job(self, caller: Caller, description: str, payment: int,
                   deadline: int, milestones: Optional[List[dict]] = None) -> str:
        jid = f"job-{caller.name}"
        sm = StateMachine("Created")
        sm.add_transition("Created", "Active", "Cancelled")
        sm.add_transition("Active", "Submitted", "Disputed", "Cancelled")
        sm.add_transition("Submitted", "Completed", "Disputed")
        sm.add_transition("Disputed", "Completed", "Refunded")
        self._new_instance(sm, employer=caller.name, description=description,
                          payment=payment, deadline=deadline, worker=None,
                          milestones=milestones or [])
        self.jobs[jid] = {"employer": caller.name, "status": "Created"}
        return jid

    def accept_job(self, caller: Caller, job_id: str):
        self.only_state(job_id, "Created")
        inst = self._get(job_id)
        inst.metadata["worker"] = caller.name
        self.transition(job_id, "Active")

    def submit_deliverable(self, caller: Caller, job_id: str, deliverable_hash: str):
        self.only_state(job_id, "Active", "Submitted")
        inst = self._get(job_id)
        if caller.name != inst.metadata["worker"]:
            raise AuthError("Only the worker can submit")
        self.transition(job_id, "Submitted")

    def confirm_delivery(self, caller: Caller, job_id: str):
        self.only_state(job_id, "Submitted")
        inst = self._get(job_id)
        if caller.name != inst.metadata["employer"]:
            raise AuthError("Only the employer can confirm")
        self.transition(job_id, "Completed")

    def dispute(self, caller: Caller, job_id: str):
        self.only_state(job_id, "Active", "Submitted", "Disputed")
        self.transition(job_id, "Disputed")

    def refund(self, caller: Caller, job_id: str):
        inst = self._get(job_id)
        if caller.name != inst.metadata["employer"]:
            raise AuthError("Only employer can refund")
        self.transition(job_id, "Refunded")

    def cancel(self, caller: Caller, job_id: str):
        self.only_state(job_id, "Created")
        inst = self._get(job_id)
        if caller.name != inst.metadata["employer"]:
            raise AuthError("Only employer can cancel")
        self.transition(job_id, "Cancelled")


# -- Oracle --

class Oracle(Contract):
    """Push-model oracle for price feeds and data attestation."""
    name = "oracle"

    def __init__(self):
        super().__init__()
        self.oracles: Dict[str, dict] = {}
        self.values: Dict[str, List[dict]] = {}

    def register_oracle(self, caller: Caller, oracle_id: str, data_type: str):
        self.oracles[oracle_id] = {"operator": caller.name, "type": data_type, "active": True}
        self.values[oracle_id] = []

    def push_value(self, caller: Caller, oracle_id: str, value: int):
        oracle = self.oracles.get(oracle_id)
        if oracle is None:
            raise ConstraintError(f"Oracle '{oracle_id}' not found")
        if oracle["operator"] != caller.name:
            raise AuthError("Not your oracle")
        if not oracle["active"]:
            raise ConstraintError("Oracle is inactive")
        self.values[oracle_id].append({"block": self.block_height, "value": value})

    def set_active(self, caller: Caller, oracle_id: str, active: bool):
        oracle = self.oracles.get(oracle_id)
        if oracle["operator"] != caller.name:
            raise AuthError("Not your oracle")
        oracle["active"] = active


# -- Subscription --

class Subscription(Contract):
    """Member subscription service with time-locked access."""
    name = "subscription"

    def __init__(self):
        super().__init__()
        self.subscriptions: Dict[str, dict] = {}

    def subscribe(self, caller: Caller, plan_id: str, duration_blocks: int,
                  payment: int) -> str:
        sid = f"sub-{caller.name}-{plan_id}"
        sm = StateMachine("Active")
        sm.add_transition("Active", "Cancelled", "Expired")
        sm.add_transition("Cancelled", "Active")  # Renew
        self._new_instance(sm, subscriber=caller.name, plan=plan_id,
                          expires_at=self.block_height + duration_blocks)
        self.subscriptions[sid] = {"status": "Active", "subscriber": caller.name}
        return sid

    def cancel(self, caller: Caller, subscription_id: str):
        sub = self.subscriptions.get(subscription_id)
        if sub is None or sub["subscriber"] != caller.name:
            raise AuthError("Not your subscription")
        self.transition(subscription_id, "Cancelled")

    def renew(self, caller: Caller, subscription_id: str, duration_blocks: int):
        self.only_state(subscription_id, "Cancelled", "Active")
        self.transition(subscription_id, "Active")


# -- InsuranceMarket --

class InsuranceMarket(Contract):
    """Decentralized insurance with underwriters and risk buyers."""
    name = "insurance_market"

    def __init__(self):
        super().__init__()
        self.underwriters: Dict[str, dict] = {}
        self.policies: Dict[str, dict] = {}

    def underwrite(self, caller: Caller, bond_amount: int, risk_type: str,
                   premium_bps: int) -> str:
        self.only(caller, "underwriter")
        uid = f"uw-{caller.name}-{risk_type}"
        self.underwriters[uid] = {"underwriter": caller.name, "bond": bond_amount,
                                  "risk_type": risk_type, "premium_bps": premium_bps,
                                  "active": True}
        return uid

    def purchase_coverage(self, caller: Caller, underwriter_id: str,
                          coverage_amount: int, duration_blocks: int) -> str:
        uw = self.underwriters.get(underwriter_id)
        if uw is None:
            raise ConstraintError(f"Underwriter '{underwriter_id}' not found")
        if not uw["active"]:
            raise ConstraintError("Underwriter is inactive")
        pid = f"policy-{caller.name}-{underwriter_id}"
        sm = StateMachine("Active")
        sm.add_transition("Active", "Expired", "Claimed", "Cancelled")
        self._new_instance(sm, buyer=caller.name, underwriter=underwriter_id,
                          amount=coverage_amount,
                          expires_at=self.block_height + duration_blocks)
        return pid

    def deactivate_underwriter(self, caller: Caller, underwriter_id: str):
        self.only(caller, GOVERNANCE)
        uw = self.underwriters.get(underwriter_id)
        if uw is None:
            raise ConstraintError(f"Underwriter '{underwriter_id}' not found")
        uw["active"] = False


# -- DaoEscrow --

class DaoEscrow(Contract):
    """Multi-mode DAO: escrow, treasury, endowment with governance."""
    name = "dao_escrow"

    MODE_ESCROW = "escrow"
    MODE_TREASURY = "treasury"
    MODE_ENDOWMENT = "endowment"

    def __init__(self):
        super().__init__()
        self.mode: str = self.MODE_ESCROW
        self.members: Dict[str, dict] = {}
        self.proposals: Dict[str, dict] = {}

    def initialize(self, caller: Caller, mode: str, config: dict):
        self.only(caller, GOVERNANCE)
        self.mode = mode

    def pay_premium(self, caller: Caller, amount: int) -> str:
        self.only(caller, MEMBER)
        mid = f"member-{caller.name}"
        self.members[mid] = {"member": caller.name, "premium_paid": amount, "active": True}
        return mid

    def propose_claim(self, caller: Caller, amount: int, reason: str) -> str:
        self.only(caller, MEMBER)
        pid = f"prop-{caller.name}"
        sm = StateMachine("Pending")
        sm.add_transition("Pending", "Executed", "Cancelled")
        self._new_instance(sm, proposer=caller.name, amount=amount, reason=reason,
                          votes_for=0, votes_against=0)
        return pid

    def vote_claim(self, caller: Caller, proposal_id: str, approve: bool):
        self.only(caller, MEMBER)
        inst = self._get(proposal_id)
        if approve:
            inst.metadata["votes_for"] += 1
        else:
            inst.metadata["votes_against"] += 1

    def execute_claim(self, caller: Caller, proposal_id: str, threshold: int = 3):
        inst = self._get(proposal_id)
        if inst.metadata["votes_for"] < threshold:
            raise ConstraintError("Votes below threshold")
        self.transition(proposal_id, "Executed")

    def set_governance_active(self, caller: Caller, active: bool):
        self.only(caller, GOVERNANCE)


# -- DarkbetExchange --

class DarkbetExchange(Contract):
    """Betting exchange with order-book and AMM pool modes."""
    name = "darkbet_exchange"

    def __init__(self):
        super().__init__()
        self.markets: Dict[str, dict] = {}
        self.orders: Dict[str, dict] = {}

    def create_market(self, caller: Caller, description: str, close_block: int) -> str:
        mid = f"market-{caller.name}"
        sm = StateMachine("Open")
        sm.add_transition("Open", "Resolved")
        sm.add_transition("Resolved", "Settled")
        self._new_instance(sm, creator=caller.name, description=description,
                          close_block=close_block)
        return mid

    def place_back(self, caller: Caller, market_id: str, amount: int, outcome: str) -> str:
        oid = f"back-{caller.name}-{market_id}"
        self.orders[oid] = {"trader": caller.name, "market": market_id,
                            "amount": amount, "outcome": outcome, "type": "back",
                            "status": "Open"}
        return oid

    def place_lay(self, caller: Caller, market_id: str, amount: int, outcome: str) -> str:
        oid = f"lay-{caller.name}-{market_id}"
        self.orders[oid] = {"trader": caller.name, "market": market_id,
                            "amount": amount, "outcome": outcome, "type": "lay",
                            "status": "Open"}
        return oid

    def resolve_market(self, caller: Caller, market_id: str, winning_outcome: str):
        self.only(caller, ORACLE)
        self.only_state(market_id, "Open")
        self.transition(market_id, "Resolved")

    def settle_market(self, caller: Caller, market_id: str):
        self.only_state(market_id, "Resolved")
        self.transition(market_id, "Settled")

    def cancel_order(self, caller: Caller, order_id: str):
        order = self.orders.get(order_id)
        if order is None or order["trader"] != caller.name:
            raise AuthError("Not your order")
        if order["status"] != "Open":
            raise ConstraintError(f"Order is {order['status']}")
        order["status"] = "Cancelled"
