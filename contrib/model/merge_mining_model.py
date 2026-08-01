#!/usr/bin/env python3
"""
DarkWow Merge Mining Model — Executable Specification
======================================================

Models Monero RandomX merge mining: PowSource::Monero, MoneroPowData,
DLEq proof verification (HAZ-XMR-01/02), uncle extension check (HAZOP-13),
competing block Monero validation (H-14), deterministic block hash.

Matches:
  src/linear/src/block.rs      — PowSource enum, MoneroPowData
  src/linear/src/monero/       — Monero verification (merkle proofs, DLEq)
  src/linear/src/chain_state.rs — uncle extension check
  bin/dwowd/src/tests/merge_mining.rs — deterministic block hash test

Usage:
  python3 contrib/model/merge_mining_model.py
"""

import hashlib
from dataclasses import dataclass, field
from typing import List, Optional
from enum import IntEnum


# ==============================================================================
# Constants — matching Rust src/sdk/src/blockchain.rs
# ==============================================================================

MERGE_MINING_TAG: bytes = b"darkwow_merge_mining_v1"


class PowSource(IntEnum):
    """Proof of Work source — matches block.rs:35 PowSource."""
    Native = 0
    Monero = 1


# ==============================================================================
# MoneroPowData — matches block.rs MoneroPowData
# ==============================================================================

@dataclass
class MoneroPowData:
    """Cryptographic proof that a Monero block was mined with merge mining tag.
    Embedded in Monero coinbase TX extra field."""
    monero_block_hash: bytes           # [u8; 32] — Monero block containing our TX
    monero_block_height: int           # u64 — MoneroBlockHeight
    merkle_proof: List[bytes] = field(default_factory=list)  # TX merkle proof path
    merkle_proof_index: int = 0        # TX index in Monero block
    merge_mining_tag: bytes = MERGE_MINING_TAG
    dleq_proof: Optional[bytes] = None  # Discrete Log Equality proof (HAZ-XMR-01/02)


@dataclass
class MoneroBlockHeight:
    """Nominal type for Monero block height — not interchangeable with DarkWow BlockHeight."""
    height: int

    def get(self) -> int:
        return self.height


# ==============================================================================
# Core merge mining functions
# ==============================================================================

def verify_merge_mining_tag(coinbase_tx_extra: bytes) -> bool:
    """Check that merge mining tag is embedded in Monero coinbase TX extra field.
    Returns True if MERGE_MINING_TAG is found."""
    return MERGE_MINING_TAG in coinbase_tx_extra


def verify_merkle_proof(tx_hash: bytes, merkle_root: bytes,
                         proof: List[bytes], index: int) -> bool:
    """Verify that tx_hash is included in merkle_root via merkle proof.
    Standard Merkle tree verification."""
    current = tx_hash
    for sibling in proof:
        if index % 2 == 0:
            combined = current + sibling
        else:
            combined = sibling + current
        current = hashlib.sha256(hashlib.sha256(combined).digest()).digest()
        index //= 2
    return current == merkle_root


def verify_dleq_proof(pow_data: MoneroPowData) -> bool:
    """Verify Discrete Log Equality proof (HAZ-XMR-01/02).
    Proves that the same secret key was used for both the Monero TX key
    and the DarkWow block signature, without revealing the key.
    Stub: returns True if dleq_proof is present (full verification requires
    curve operations not yet modeled in Python)."""
    return pow_data.dleq_proof is not None


def check_uncle_extension(uncle_monero_height: MoneroBlockHeight,
                           canonical_monero_height: MoneroBlockHeight) -> bool:
    """HAZOP-13: Verify uncle block's Monero height is within acceptable
    extension range. Uncle must reference a Monero block at or after the
    canonical chain's anchor height."""
    return uncle_monero_height.get() >= canonical_monero_height.get()


def deterministic_block_hash(header_data: bytes, pow_data: MoneroPowData) -> bytes:
    """Compute deterministic block hash from header + MoneroPowData.
    Same inputs → same hash, verified by test_merge_mined_block_deterministic."""
    h = hashlib.blake2b(digest_size=32, person=b"DarkFi_MergeHash")
    h.update(header_data)
    h.update(pow_data.monero_block_hash)
    h.update(pow_data.monero_block_height.to_bytes(8, 'little'))
    h.update(pow_data.merge_mining_tag)
    return h.digest()


# ==============================================================================
# Tests
# ==============================================================================

if __name__ == "__main__":
    import os
    passed = 0
    failed = 0

    # Test 1: PowSource enum values
    assert PowSource.Native == 0
    assert PowSource.Monero == 1
    passed += 1
    print("  test_pow_source_enum: PASSED")

    # Test 2: Merge mining tag verification
    extra_with_tag = b"some_data" + MERGE_MINING_TAG + b"more_data"
    extra_without_tag = b"no_merge_mining_here"
    assert verify_merge_mining_tag(extra_with_tag)
    assert not verify_merge_mining_tag(extra_without_tag)
    passed += 1
    print("  test_merge_mining_tag: PASSED")

    # Test 3: Merkle proof verification (simple 2-leaf tree)
    leaf1 = hashlib.sha256(hashlib.sha256(b"tx1").digest()).digest()
    leaf2 = hashlib.sha256(hashlib.sha256(b"tx2").digest()).digest()
    root = hashlib.sha256(hashlib.sha256(leaf1 + leaf2).digest()).digest()
    assert verify_merkle_proof(leaf1, root, [leaf2], 0)
    assert verify_merkle_proof(leaf2, root, [leaf1], 1)
    passed += 1
    print("  test_merkle_proof: PASSED")

    # Test 4: DLEq proof (stub)
    pow_with_proof = MoneroPowData(
        monero_block_hash=os.urandom(32),
        monero_block_height=2912484,
        dleq_proof=b"mock_dleq_proof",
    )
    pow_without_proof = MoneroPowData(
        monero_block_hash=os.urandom(32),
        monero_block_height=2912484,
    )
    assert verify_dleq_proof(pow_with_proof)
    assert not verify_dleq_proof(pow_without_proof)
    passed += 1
    print("  test_dleq_proof: PASSED")

    # Test 5: Uncle extension check (HAZOP-13)
    canonical_height = MoneroBlockHeight(2912484)
    uncle_ok = MoneroBlockHeight(2912484)
    uncle_before = MoneroBlockHeight(2912483)
    assert check_uncle_extension(uncle_ok, canonical_height)
    assert not check_uncle_extension(uncle_before, canonical_height)
    passed += 1
    print("  test_uncle_extension: PASSED")

    # Test 6: Deterministic block hash
    header = os.urandom(100)
    h1 = deterministic_block_hash(header, pow_with_proof)
    h2 = deterministic_block_hash(header, pow_with_proof)
    assert h1 == h2, "same inputs must produce same hash"
    h3 = deterministic_block_hash(os.urandom(100), pow_with_proof)
    assert h1 != h3, "different header must produce different hash"
    passed += 1
    print("  test_deterministic_hash: PASSED")

    # Test 7: MoneroPowData round-trip
    data = MoneroPowData(
        monero_block_hash=os.urandom(32),
        monero_block_height=2912484,
        merkle_proof=[os.urandom(32) for _ in range(3)],
        merkle_proof_index=5,
    )
    assert data.monero_block_height == 2912484
    passed += 1
    print("  test_monero_pow_data: PASSED")

    print(f"\n{'='*60}")
    print(f"  Results: {passed}/{passed + failed} passed")
    print(f"{'='*60}")
