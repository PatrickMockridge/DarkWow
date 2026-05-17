"""Relayer Endowment contract model.

Models the on-chain capital deployment system:
- Registry: relayer_id -> {total_deployed, active_deployments, accumulated_fees, backer_cut_bp}
- Deployments: deployment_id -> {relayer_id, backer_id, amount, backer_cut_bp, accumulated_fees, withdrawn}
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional

from .config import SimConfig


@dataclass
class EndowmentAccount:
    relayer_id: str
    total_deployed: int = 0
    active_deployments: int = 0
    accumulated_fees: int = 0
    default_backer_cut_bp: int = 5000
    is_active: bool = True
    created_at: int = 0


@dataclass
class EndowmentDeployment:
    deployment_id: str
    relayer_id: str
    backer_id: str
    amount: int
    backer_cut_bp: int
    accumulated_fees: int = 0
    withdrawn: bool = False
    deployed_at: int = 0
    withdrawn_at: Optional[int] = None

    @property
    def is_active(self) -> bool:
        return not self.withdrawn


class EndowmentContract:
    """On-chain relayer endowment contract state."""

    def __init__(self, config: SimConfig):
        self.config = config
        self.registry: Dict[str, EndowmentAccount] = {}
        self.deployments: Dict[str, EndowmentDeployment] = {}
        self._deployment_counter: int = 0

    def _next_deployment_id(self) -> str:
        self._deployment_counter += 1
        return f"deploy_{self._deployment_counter}"

    def initialize(self, relayer_id: str, default_backer_cut_bp: int, block_height: int) -> Dict:
        """InitializeV1: Relayer creates an endowment account."""
        if relayer_id in self.registry:
            return {"error": "already_initialized"}
        if default_backer_cut_bp > self.config.bp_precision:
            return {"error": "invalid_bp"}

        account = EndowmentAccount(
            relayer_id=relayer_id,
            default_backer_cut_bp=default_backer_cut_bp,
            created_at=block_height,
        )
        self.registry[relayer_id] = account
        return {"status": "initialized", "relayer_id": relayer_id}

    def deploy_capital(
        self,
        relayer_id: str,
        backer_id: str,
        amount: int,
        backer_cut_bp: int,
        block_height: int,
    ) -> Dict:
        """DeployCapitalV1: Backer deploys capital to a relayer."""
        account = self.registry.get(relayer_id)
        if not account:
            return {"error": "endowment_not_found"}
        if not account.is_active:
            return {"error": "endowment_inactive"}
        if amount < self.config.min_deploy:
            return {"error": "insufficient_deploy"}

        deployment_id = self._next_deployment_id()
        deployment = EndowmentDeployment(
            deployment_id=deployment_id,
            relayer_id=relayer_id,
            backer_id=backer_id,
            amount=amount,
            backer_cut_bp=backer_cut_bp,
            deployed_at=block_height,
        )
        self.deployments[deployment_id] = deployment
        account.total_deployed += amount
        account.active_deployments += 1
        return {"status": "deployed", "deployment_id": deployment_id}

    def settle_fees(
        self,
        relayer_id: str,
        total_fees: int,
        allocations: List[Dict],
        block_height: int,
    ) -> Dict:
        """SettleFeesV1: Relayer distributes fees to backer deployments."""
        account = self.registry.get(relayer_id)
        if not account:
            return {"error": "endowment_not_found"}
        if not account.is_active:
            return {"error": "endowment_inactive"}

        total_allocated = sum(a["fee_amount"] for a in allocations)
        if total_allocated != total_fees:
            return {"error": "allocation_mismatch"}

        for alloc in allocations:
            deployment = self.deployments.get(alloc["deployment_id"])
            if not deployment:
                return {"error": f"deployment_not_found: {alloc['deployment_id']}"}
            if deployment.relayer_id != relayer_id:
                return {"error": f"wrong_relayer: {alloc['deployment_id']}"}
            if deployment.withdrawn:
                return {"error": f"deployment_withdrawn: {alloc['deployment_id']}"}
            deployment.accumulated_fees += alloc["fee_amount"]

        account.accumulated_fees += total_fees
        return {"status": "settled", "total_fees": total_fees}

    def claim_fees(self, deployment_id: str, backer_id: str, block_height: int) -> Dict:
        """ClaimRelayerFeesV1: Backer claims accumulated fees."""
        deployment = self.deployments.get(deployment_id)
        if not deployment:
            return {"error": "deployment_not_found"}
        if deployment.backer_id != backer_id:
            return {"error": "unauthorized"}
        if deployment.withdrawn:
            return {"error": "already_withdrawn"}
        if deployment.accumulated_fees == 0:
            return {"error": "no_fees"}

        claimed = deployment.accumulated_fees
        deployment.accumulated_fees = 0
        return {"status": "claimed", "amount": claimed}

    def withdraw_deployment(self, deployment_id: str, backer_id: str, block_height: int) -> Dict:
        """WithdrawDeploymentV1: Backer withdraws principal + fees."""
        deployment = self.deployments.get(deployment_id)
        if not deployment:
            return {"error": "deployment_not_found"}
        if deployment.backer_id != backer_id:
            return {"error": "unauthorized"}
        if deployment.withdrawn:
            return {"error": "already_withdrawn"}

        account = self.registry.get(deployment.relayer_id)
        if account:
            account.total_deployed -= deployment.amount
            account.active_deployments -= 1

        total = deployment.amount + deployment.accumulated_fees
        deployment.withdrawn = True
        deployment.withdrawn_at = block_height
        deployment.accumulated_fees = 0
        return {"status": "withdrawn", "total_returned": total}

    def get_active_deployments(self, relayer_id: str) -> List[EndowmentDeployment]:
        """Get all active (non-withdrawn) deployments for a relayer."""
        return [
            d for d in self.deployments.values()
            if d.relayer_id == relayer_id and d.is_active
        ]

    def get_total_deployed(self, relayer_id: str) -> int:
        """Get total active capital deployed to a relayer."""
        return sum(
            d.amount for d in self.deployments.values()
            if d.relayer_id == relayer_id and d.is_active
        )
