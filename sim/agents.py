"""Participant agents in the simulation.

Models the behavior of Users, Backers, RelayerOperators, and Attackers.
Each agent type generates transactions based on its strategy.
"""

import random
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from .config import SimConfig


@dataclass
class User:
    """A bridge user who deposits and withdraws."""

    id: str
    balance: int = 0  # balance on external chain
    wrapped_balance: int = 0  # wrapped tokens on DarkWow
    active_nullifiers: List[str] = field(default_factory=list)

    def deposit(
        self,
        network: Any,
        amount: int,
        chain: str = "ethereum",
    ) -> Dict:
        """Deposit tokens from external chain to bridge."""
        if amount > self.balance:
            return {"error": "insufficient_balance"}

        deposit_id = f"{self.id}_dep_{network.next_tx_id()}"
        secret = f"secret_{deposit_id}"
        result = network.bridge.deposit(
            deposit_id=deposit_id,
            secret_hash=f"hash_{secret}",
            amount=amount,
            chain=chain,
            recipient=self.id,
            block_height=network.block_height,
        )
        if "error" not in result:
            self.balance -= amount
            self.wrapped_balance += amount
            network.log_event(
                "user_deposited",
                user_id=self.id,
                amount=amount,
                chain=chain,
            )
        return result

    def withdraw(
        self,
        network: Any,
        amount: int,
        feed_mode: int = 0,
    ) -> Dict:
        """Request a withdrawal from bridge to external chain."""
        if amount > self.wrapped_balance:
            return {"error": "insufficient_wrapped"}
        if amount <= 0:
            return {"error": "invalid_amount"}

        nullifier = f"null_{self.id}_{network.next_tx_id()}"
        result = network.bridge.request_withdrawal(
            nullifier=nullifier,
            amount=amount,
            recipient_hash=f"recipient_{self.id}",
            feed_mode=feed_mode,
            block_height=network.block_height,
        )
        if "error" not in result:
            self.wrapped_balance -= amount
            self.active_nullifiers.append(nullifier)
            network.log_event(
                "withdrawal_requested",
                user_id=self.id,
                nullifier=nullifier,
                amount=amount,
                feed_mode=feed_mode,
            )
        return result

    def cancel_withdrawal(self, network: Any, nullifier: str) -> Dict:
        """Cancel a timed-out withdrawal."""
        result = network.cancel_withdrawal(nullifier)
        if "error" not in result:
            self.wrapped_balance += network.bridge.withdrawals[nullifier].amount
            self.active_nullifiers.remove(nullifier)
        return result


@dataclass
class Backer:
    """A capital provider who deploys to relayers and earns fees."""

    id: str
    capital: int = 0
    deployments: Dict[str, int] = field(default_factory=dict)  # deployment_id -> amount
    total_fees_earned: int = 0
    total_withdrawn: int = 0

    def deploy_capital(
        self,
        network: Any,
        relayer_id: str,
        amount: int,
        backer_cut_bp: Optional[int] = None,
    ) -> Dict:
        """Deploy capital to a relayer's endowment."""
        if amount > self.capital:
            return {"error": "insufficient_capital"}

        bp = backer_cut_bp or network.config.default_backer_cut_bp
        result = network.endowment.deploy_capital(
            relayer_id=relayer_id,
            backer_id=self.id,
            amount=amount,
            backer_cut_bp=bp,
            block_height=network.block_height,
        )
        if "error" not in result:
            self.capital -= amount
            self.deployments[result["deployment_id"]] = amount
            network.log_event(
                "capital_deployed",
                backer_id=self.id,
                relayer_id=relayer_id,
                amount=amount,
                deployment_id=result["deployment_id"],
            )
        return result

    def claim_fees(self, network: Any, deployment_id: str) -> Dict:
        """Claim accumulated fees from a deployment."""
        result = network.endowment.claim_fees(
            deployment_id=deployment_id,
            backer_id=self.id,
            block_height=network.block_height,
        )
        if "error" not in result:
            self.capital += result["amount"]
            self.total_fees_earned += result["amount"]
            network.log_event(
                "fees_claimed",
                backer_id=self.id,
                deployment_id=deployment_id,
                amount=result["amount"],
            )
        return result

    def withdraw_deployment(self, network: Any, deployment_id: str) -> Dict:
        """Withdraw principal + fees from a deployment."""
        result = network.endowment.withdraw_deployment(
            deployment_id=deployment_id,
            backer_id=self.id,
            block_height=network.block_height,
        )
        if "error" not in result:
            self.capital += result["total_returned"]
            self.total_withdrawn += result["total_returned"]
            del self.deployments[deployment_id]
            network.log_event(
                "deployment_withdrawn",
                backer_id=self.id,
                deployment_id=deployment_id,
                total_returned=result["total_returned"],
            )
        return result

    @property
    def roi(self) -> float:
        """Return on investment: fees earned / capital still deployed."""
        deployed = sum(self.deployments.values())
        if deployed == 0:
            return 0.0
        return self.total_fees_earned / deployed


@dataclass
class RelayerOperator:
    """Controls a relayer node's behavior."""

    relayer_id: str
    node: Any = None  # RelayerNode reference

    def crash(self, network: Any) -> None:
        """Force relayer offline."""
        if self.node:
            self.node.online = False
            network.log_event("relayer_crashed", relayer_id=self.relayer_id)

    def recover(self, network: Any) -> None:
        """Bring relayer back online."""
        if self.node:
            self.node.online = True
            network.log_event("relayer_recovered", relayer_id=self.relayer_id)

    def set_malicious(self, malicious: bool = True) -> None:
        """Enable/disable malicious behavior."""
        if self.node:
            self.node.malicious = malicious

    def set_fee_multiplier(self, multiplier: float) -> None:
        """Manipulate fee pricing."""
        if self.node:
            self.node.fee_multiplier = multiplier

    def disable_settlement(self) -> None:
        """Stop settling fees to backers."""
        if self.node:
            self.node.skip_settlement = True


class Attacker:
    """Adversarial agent attempting to exploit the system."""

    def __init__(self, id: str):
        self.id = id
        self.attack_log: List[Dict] = []

    def attempt_double_spend(self, network: Any, nullifier: str, new_recipient: str) -> Dict:
        """Try to withdraw already-spent nullifier."""
        w = network.bridge.withdrawals.get(nullifier)
        if not w:
            return {"status": "no_such_withdrawal"}

        result = network.bridge.request_withdrawal(
            nullifier=nullifier,
            amount=w.amount,
            recipient_hash=new_recipient,
            block_height=network.block_height,
        )
        self.attack_log.append({
            "type": "double_spend",
            "nullifier": nullifier,
            "result": result,
            "block": network.block_height,
        })
        return result

    def attempt_htlc_race(
        self, network: Any, htlc_id: str, secret: str,
    ) -> Dict:
        """Try to claim and refund same HTLC simultaneously."""
        claim_result = network.bridge.claim_htlc(
            htlc_id, secret, network.block_height,
        )
        refund_result = network.bridge.refund_htlc(
            htlc_id, network.block_height,
        )
        self.attack_log.append({
            "type": "htlc_race",
            "htlc_id": htlc_id,
            "claim": claim_result,
            "refund": refund_result,
            "block": network.block_height,
        })
        return {"claim": claim_result, "refund": refund_result}

    def attempt_backer_bank_run(self, network: Any, deployment_ids: List[str]) -> Dict:
        """Withdraw all deployments simultaneously."""
        results = {}
        for dep_id in deployment_ids:
            results[dep_id] = network.endowment.withdraw_deployment(
                dep_id, self.id, network.block_height,
            )
        self.attack_log.append({
            "type": "bank_run",
            "deployments": deployment_ids,
            "results": results,
            "block": network.block_height,
        })
        return results


def setup_default_agents(network: Any) -> Dict[str, List]:
    """Create and configure a default set of agents for a simulation."""
    config = network.config
    agents: Dict[str, List] = {
        "users": [],
        "backers": [],
        "operators": [],
    }

    # Create relayers
    for i in range(config.num_relayers):
        rid = f"relayer_{i}"
        relayer = network.relayers.get(rid)
        if not relayer:
            from .relayer import RelayerNode
            relayer = RelayerNode(id=rid, backer_cut_bp=config.default_backer_cut_bp)
            network.register_relayer(relayer)
        network.stake_manager.register(rid, config.initial_relayer_stake)
        op = RelayerOperator(relayer_id=rid, node=relayer)
        agents["operators"].append(op)

    # Create backers
    for i in range(config.num_backers):
        bid = f"backer_{i}"
        backer = Backer(id=bid, capital=config.initial_backer_capital)
        agents["backers"].append(backer)

    # Create users
    for i in range(config.num_users):
        uid = f"user_{i}"
        user = User(id=uid, balance=config.initial_user_balance)
        agents["users"].append(user)

    return agents
