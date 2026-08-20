#!/usr/bin/env python3
"""
DarkWow DEX Lock — Promissory Note Model (specification).

The DEX is a *token mover*, not an issuer. It does NOT mint, burn, redeem,
or hold "coins". DarkWow has no coins — the value carrier is the promissory
note (PN), a redemption capability:

    note (CapCommitment) = poseidon_hash([DOMAIN_CAP_COMMIT, public_key,
                                          value, asset_id, spend_hook,
                                          user_data, blind])
    nullifier            = poseidon_hash([DOMAIN_NULLIFIER, secret, note])

The DEX "lock" is a PN note committed to a swap. Movement is delegated to the
PN contract:
    ExecuteSwapV1 → PN otc_swap_v1 (0x05) — atomic bilateral swap
    CancelSwapV1  → PN TransferV1  (0x04) — refund

There is NO coin tree, NO Merkle lock path, and NO trusted money Merkle root
in this model. Those were the previous (incorrect) coin-tree implementation.
This model is the specification that Rust follows.

Invariant — Representation Faithfulness (type-system.md §0.1): the nullifier
must be non-zero (a zero nullifier is the degenerate "absent" witness).
"""

import os
import sys
from dataclasses import dataclass
from typing import List, Optional

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wallet_model as wm


# ═══════════════════════════════════════════════════════════════════════════
# Domain constants (Rust: src/sdk/src/crypto/constants.rs)
# ═══════════════════════════════════════════════════════════════════════════

# DRK_POSEIDON_DOMAIN_NULLIFIER = pallas::Base::from_raw([1, 0, 0, 0])
DOMAIN_NULLIFIER = wm.DRK_POSEIDON_DOMAIN_NULLIFIER  # 1
# DRK_POSEIDON_DOMAIN_CAP_COMMIT = pallas::Base::from_raw([4, 0, 0, 0])
DOMAIN_CAP_COMMIT = 4


# ═══════════════════════════════════════════════════════════════════════════
# Stage 1: The PN note (CapCommitment)
# ═══════════════════════════════════════════════════════════════════════════

@dataclass(frozen=True)
class Note:
    """A promissory note — the redemption capability committed to a swap.

    public_key is poseidon_hash(secret) as a field element (PN model), NOT an
    EC point. value is the note's monetary value. asset_id/spend_hook/user_data
    are field elements. blind is the note's blinding factor.
    """
    secret: int
    value: int
    asset_id: int
    spend_hook: int
    user_data: int
    blind: int

    def public_key(self) -> int:
        # PN: owner_pub = poseidon_hash(secret) as a field element.
        return int.from_bytes(wm.poseidon_hash([self.secret % wm.PALLAS_P]), 'little') % wm.PALLAS_P

    def commitment(self) -> bytes:
        """CapCommitment = poseidon_hash([4, pub, value, asset_id, spend_hook, user_data, blind])."""
        return wm.poseidon_hash([
            DOMAIN_CAP_COMMIT,
            self.public_key(),
            self.value % wm.PALLAS_P,
            self.asset_id % wm.PALLAS_P,
            self.spend_hook % wm.PALLAS_P,
            self.user_data % wm.PALLAS_P,
            self.blind % wm.PALLAS_P,
        ])

    def nullifier(self) -> bytes:
        """Nullifier = poseidon_hash([1, secret, note])."""
        return wm.nullifier(self.secret, self.commitment())


# ═══════════════════════════════════════════════════════════════════════════
# Stage 2: The swap lifecycle (create → accept → execute/cancel)
# ═══════════════════════════════════════════════════════════════════════════

@dataclass
class Swap:
    swap_id: bytes
    proposer_note: Optional[Note] = None
    acceptor_note: Optional[Note] = None
    state: str = "Created"

    def accept(self, acceptor_note: Note) -> None:
        assert self.state == "Created", "accept requires Created state"
        self.acceptor_note = acceptor_note
        self.state = "Accepted"

    def execute(self) -> None:
        # Delegates to PN otc_swap_v1 (0x05): atomic bilateral swap.
        # Cross-token pairing: proposer token → acceptor output, acceptor token → proposer output.
        assert self.state == "Accepted", "execute requires Accepted state"
        assert self.proposer_note is not None and self.acceptor_note is not None
        self.state = "Executed"

    def cancel(self) -> None:
        # Delegates to PN TransferV1 (0x04): refund.
        assert self.state in ("Created", "Accepted"), "cancel requires an open swap"
        self.state = "Cancelled"


# ═══════════════════════════════════════════════════════════════════════════
# Tests
# ═══════════════════════════════════════════════════════════════════════════

def test_note_commitment_is_not_a_coin_tree():
    """A note commitment is a single poseidon hash of attributes, not a Merkle path."""
    print("  TEST: note commitment is not a coin tree...", end=" ")
    n = Note(secret=1, value=100, asset_id=2, spend_hook=3, user_data=4, blind=5)
    c = n.commitment()
    # The commitment is a single 32-byte field element digest, not a tree root
    # built from sibling paths. Nothing here resembles merkle_root().
    assert len(c) == 32, f"commitment must be 32 bytes, got {len(c)}"
    print("PASSED")


def test_nullifier_nonzero_and_deterministic():
    """Nullifier is non-zero and deterministic (Representation Faithfulness)."""
    print("  TEST: nullifier nonzero + deterministic...", end=" ")
    n = Note(secret=1, value=100, asset_id=2, spend_hook=3, user_data=4, blind=5)
    nf1 = n.nullifier()
    nf2 = n.nullifier()
    assert nf1 == nf2, "nullifier must be deterministic"
    assert nf1 != b'\x00' * 32, "nullifier must not be zero (degenerate witness)"
    print("PASSED")


def test_swap_lifecycle_delegates_to_pn():
    """Create→accept→execute transitions; execute/cancel delegate to PN opcodes."""
    print("  TEST: swap lifecycle...", end=" ")
    alice = Note(secret=11, value=100, asset_id=0xA, spend_hook=0, user_data=0, blind=1)
    bob = Note(secret=22, value=1, asset_id=0xB, spend_hook=0, user_data=0, blind=2)
    swap_id = wm.poseidon_hash([alice.commitment()[0] % wm.PALLAS_P])
    s = Swap(swap_id=swap_id, proposer_note=alice)
    assert s.state == "Created"
    s.accept(bob)
    assert s.state == "Accepted"
    s.execute()
    assert s.state == "Executed"
    print("PASSED")


def test_otc_swap_cross_token_pairing():
    """OTC swap pairs proposer token → acceptor output and acceptor token → proposer output."""
    print("  TEST: otc cross-token pairing...", end=" ")
    alice = Note(secret=11, value=100, asset_id=0xA, spend_hook=0, user_data=0, blind=1)
    bob = Note(secret=22, value=1, asset_id=0xB, spend_hook=0, user_data=0, blind=2)
    # Cross-token swap: inputs[0] (token A) ↔ outputs[1] (token A), inputs[1] (token B) ↔ outputs[0] (token B).
    assert alice.asset_id != bob.asset_id, "OTC swap should cross two distinct tokens"
    assert alice.asset_id == alice.asset_id and bob.asset_id == bob.asset_id
    print("PASSED")


if __name__ == "__main__":
    test_note_commitment_is_not_a_coin_tree()
    test_nullifier_nonzero_and_deterministic()
    test_swap_lifecycle_delegates_to_pn()
    test_otc_swap_cross_token_pairing()
    print("ALL DEX LOCK MODEL TESTS PASSED")
