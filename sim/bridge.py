"""Bridge contract model.

Models the on-chain DarkWow Bridge WASM contract state:
- Deposits tree: deposit_id -> {secret_hash, amount, chain, recipient}
- Withdrawals tree: nullifier -> {amount, recipient_hash, timeout_height, feed_mode}
- Pending withdrawals: queryable list for relayers
- Nullifier set: double-spend prevention
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional

from .config import SimConfig


@dataclass
class DepositRecord:
    deposit_id: str
    secret_hash: str
    amount: int
    chain: str  # "ethereum", "monero", etc.
    recipient: str
    block_height: int
    merkle_root: str = ""


@dataclass
class WithdrawalRecord:
    nullifier: str
    amount: int
    recipient_hash: str
    timeout_height: int
    feed_mode: int  # 0 = standard, 1 = guaranteed
    status: str = "pending"  # pending, executed, failed, cancelled, slashed
    executed_by: Optional[str] = None
    executed_at: Optional[int] = None
    fee_collected: int = 0
    refund_amount: int = 0
    cancelled_at: Optional[int] = None
    failed_at: Optional[int] = None


@dataclass
class HtlcRecord:
    htlc_id: str
    hash_lock: str
    time_lock: int
    amount: int
    sender: str
    receiver: str
    chain: str
    status: str = "pending"  # pending, claimed, refunded
    secret: Optional[str] = None


class BridgeContract:
    """On-chain bridge contract state."""

    def __init__(self, config: SimConfig):
        self.config = config
        self.deposits: Dict[str, DepositRecord] = {}
        self.withdrawals: Dict[str, WithdrawalRecord] = {}
        self.nullifiers: set = set()
        self.commitments: set = set()
        self.htlcs: Dict[str, HtlcRecord] = {}
        self.total_deposited: int = 0
        self.total_withdrawn: int = 0
        self.total_fees_collected: int = 0

    def deposit(
        self,
        deposit_id: str,
        secret_hash: str,
        amount: int,
        chain: str,
        recipient: str,
        block_height: int,
    ) -> Dict:
        """Process a DepositV1 transaction."""
        if deposit_id in self.deposits:
            return {"error": "double_deposit"}
        if amount < self.config.min_withdrawal_amount:
            return {"error": "amount_too_low"}

        commitment = f"cm_{secret_hash}_{amount}"
        if commitment in self.commitments:
            return {"error": "commitment_exists"}
        self.commitments.add(commitment)

        record = DepositRecord(
            deposit_id=deposit_id,
            secret_hash=secret_hash,
            amount=amount,
            chain=chain,
            recipient=recipient,
            block_height=block_height,
        )
        self.deposits[deposit_id] = record
        self.total_deposited += amount
        return {"status": "deposited", "deposit_id": deposit_id}

    def request_withdrawal(
        self,
        nullifier: str,
        amount: int,
        recipient_hash: str,
        feed_mode: int = 0,
        block_height: int = 0,
        fee_bp: int = 100,
    ) -> Dict:
        """Process a WithdrawV1 transaction."""
        if nullifier in self.nullifiers:
            return {"error": "nullifier_spent"}
        if amount < self.config.min_withdrawal_amount:
            return {"error": "amount_too_low"}
        if amount > self.config.max_withdrawal_amount:
            return {"error": "amount_too_high"}

        self.nullifiers.add(nullifier)

        timeout = block_height + self.config.withdrawal_timeout_blocks
        record = WithdrawalRecord(
            nullifier=nullifier,
            amount=amount,
            recipient_hash=recipient_hash,
            timeout_height=timeout,
            feed_mode=feed_mode,
        )
        self.withdrawals[nullifier] = record
        return {
            "status": "pending",
            "nullifier": nullifier,
            "timeout_height": timeout,
        }

    def cancel_withdrawal(self, nullifier: str, block_height: int) -> Dict:
        """Cancel a timed-out withdrawal."""
        w = self.withdrawals.get(nullifier)
        if not w:
            return {"error": "not_found"}
        if w.status != "pending":
            return {"error": "not_pending"}
        if block_height < w.timeout_height:
            return {"error": "not_timed_out"}
        w.status = "cancelled"
        w.cancelled_at = block_height
        return {"status": "cancelled"}

    def get_pending_withdrawals(self, current_block: int) -> List[WithdrawalRecord]:
        """Return withdrawals that relayers should process."""
        return [
            w for w in self.withdrawals.values()
            if w.status == "pending" and current_block < w.timeout_height
        ]

    def get_expired_withdrawals(self, current_block: int) -> List[WithdrawalRecord]:
        """Return withdrawals past their timeout."""
        return [
            w for w in self.withdrawals.values()
            if w.status == "pending" and current_block >= w.timeout_height
        ]

    def execute_withdrawal(self, nullifier: str, relayer_id: str, block_height: int, fee: int) -> Dict:
        """Mark a withdrawal as executed by a relayer."""
        w = self.withdrawals.get(nullifier)
        if not w or w.status != "pending":
            return {"error": "not_pending"}
        w.status = "executed"
        w.executed_by = relayer_id
        w.executed_at = block_height
        w.fee_collected = fee
        self.total_withdrawn += w.amount
        self.total_fees_collected += fee
        return {"status": "executed"}

    def create_htlc(
        self,
        htlc_id: str,
        hash_lock: str,
        time_lock: int,
        amount: int,
        sender: str,
        receiver: str,
        chain: str,
        block_height: int,
    ) -> Dict:
        """Create an HTLC for cross-chain atomic swap."""
        if htlc_id in self.htlcs:
            return {"error": "htlc_exists"}
        record = HtlcRecord(
            htlc_id=htlc_id,
            hash_lock=hash_lock,
            time_lock=time_lock,
            amount=amount,
            sender=sender,
            receiver=receiver,
            chain=chain,
        )
        self.htlcs[htlc_id] = record
        return {"status": "created", "htlc_id": htlc_id}

    def claim_htlc(self, htlc_id: str, secret: str, block_height: int) -> Dict:
        """Claim an HTLC by revealing the secret."""
        h = self.htlcs.get(htlc_id)
        if not h:
            return {"error": "not_found"}
        if h.status != "pending":
            return {"error": f"already_{h.status}"}
        # In real system: poseidon_hash(secret) == hash_lock
        h.status = "claimed"
        h.secret = secret
        return {"status": "claimed"}

    def refund_htlc(self, htlc_id: str, block_height: int) -> Dict:
        """Refund an HTLC after time_lock expires."""
        h = self.htlcs.get(htlc_id)
        if not h:
            return {"error": "not_found"}
        if h.status != "pending":
            return {"error": f"already_{h.status}"}
        if block_height < h.time_lock:
            return {"error": "time_lock_not_expired"}
        h.status = "refunded"
        return {"status": "refunded"}
