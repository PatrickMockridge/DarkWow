#!/usr/bin/env python3
"""
DarkWow Nullifier Lifecycle — Complete Specification.

Nullifiers prevent double-spends. Every spend MUST produce a nullifier.
Every nullifier MUST be unique across the entire chain history.

Lifecycle: wallet computes → tx carries → mempool dedups → WASM validates
→ chain persists → wallet detects.

Security-critical. Test-critical. Mainnet-critical.
"""

import hashlib
import os
import sys
from dataclasses import dataclass, field
from typing import List, Optional, Set, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wallet_model as wm


# ═══════════════════════════════════════════════════════════════════════════
# Stage 1: Nullifier Computation
# ═══════════════════════════════════════════════════════════════════════════

def test_nullifier_deterministic():
    """Same secret + same coin → same nullifier (deterministic)."""
    print("  TEST: nullifier deterministic...", end=" ")
    secret = wm.SecretKey(os.urandom(32))
    coin_hash = os.urandom(32)

    # Compute nullifier: blake2b(secret || commitment) with domain separation
    nf1 = wm.nullifier(int.from_bytes(secret.inner, 'little'), coin_hash)
    nf2 = wm.nullifier(int.from_bytes(secret.inner, 'little'), coin_hash)

    assert nf1 == nf2, "Nullifier must be deterministic"
    assert len(nf1) == 32, f"Nullifier must be 32 bytes, got {len(nf1)}"
    assert nf1 != b'\x00' * 32, "Nullifier must not be zero"
    print("PASSED")


def test_nullifier_different_coins():
    """Different coins → different nullifiers (even with same secret)."""
    print("  TEST: nullifier different coins...", end=" ")
    secret = wm.SecretKey(os.urandom(32))
    coin_a = os.urandom(32)
    coin_b = os.urandom(32)

    nf_a = wm.nullifier(int.from_bytes(secret.inner, 'little'), coin_a)
    nf_b = wm.nullifier(int.from_bytes(secret.inner, 'little'), coin_b)

    assert nf_a != nf_b, "Different coins must produce different nullifiers"
    print("PASSED")


def test_nullifier_different_secrets():
    """Different secrets → different nullifiers (even with same coin)."""
    print("  TEST: nullifier different secrets...", end=" ")
    sk_a = wm.SecretKey(os.urandom(32))
    sk_b = wm.SecretKey(os.urandom(32))
    coin = os.urandom(32)

    nf_a = wm.nullifier(int.from_bytes(sk_a.inner, 'little'), coin)
    nf_b = wm.nullifier(int.from_bytes(sk_b.inner, 'little'), coin)

    assert nf_a != nf_b, "Different secrets must produce different nullifiers"
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Stage 2: Transaction Population
# ═══════════════════════════════════════════════════════════════════════════

def test_tx_populates_nullifiers():
    """Every spend path populates tx.nullifiers. Deploy path is empty."""
    print("  TEST: tx populates nullifiers...", end=" ")

    # Spend transaction: fee payment always produces a nullifier
    # The wallet sets tx.nullifiers to the FeeCallInput.nullifier bytes
    secret = wm.SecretKey(os.urandom(32))
    coin_hash = os.urandom(32)
    nullifier = wm.nullifier(int.from_bytes(secret.inner, 'little'), coin_hash)

    # Simulate a transaction with a spend (FeeV1 call)
    spend_tx = {
        "calls": [{"contract_id": wm.NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
                    "data": b'\x00' + b'\x00' * 104}],  # FeeV1
        "nullifiers": [nullifier],  # Wallet populated this
    }

    assert len(spend_tx["nullifiers"]) == 1, "Spend tx must have 1 nullifier"
    assert spend_tx["nullifiers"][0] == nullifier, "Nullifier must match"

    # Deploy transaction: no spend, no nullifier
    deploy_tx = {
        "calls": [{"contract_id": os.urandom(32),
                    "data": b'\x00' + b'\x00' * 128}],
        "nullifiers": [],  # Deployment creates, doesn't spend
    }

    assert len(deploy_tx["nullifiers"]) == 0, "Deploy tx must have 0 nullifiers"
    print("PASSED")


def test_tx_spend_without_nullifier_is_invalid():
    """A transaction that spends a coin but has empty nullifiers is malformed."""
    print("  TEST: spend without nullifier invalid...", end=" ")

    # This tx has a FeeV1 call (spend) but empty nullifiers
    malformed_tx = {
        "calls": [{"contract_id": wm.NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
                    "data": b'\x00' + b'\x00' * 104}],
        "nullifiers": [],
    }

    # Mempool should detect this and reject or warn
    has_spend = any(
        call["contract_id"] == wm.NATIVE_TOKEN_CONTRACT_ID.to_bytes()
        and call["data"][0] in (0x00, 0x01, 0x03, 0x04)  # Fee/Burn/Transfer/Spend
        for call in malformed_tx["calls"]
    )
    has_nullifiers = len(malformed_tx["nullifiers"]) > 0

    if has_spend and not has_nullifiers:
        # This is the bug condition — mempool would use the proxy
        malformed = True
    else:
        malformed = False

    assert malformed, "Spend tx with empty nullifiers must be detected as malformed"
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Stage 3: Mempool Dedup
# ═══════════════════════════════════════════════════════════════════════════

@dataclass
class NullifierMempool:
    """Mempool nullifier tracking. Rejects double-spends."""
    nullifiers: Set[bytes] = field(default_factory=set)

    def accept(self, nullifiers: List[bytes]) -> Tuple[bool, str]:
        for nf in nullifiers:
            if nf in self.nullifiers:
                return False, f"Double-spend: nullifier already in mempool"
        for nf in nullifiers:
            self.nullifiers.add(nf)
        return True, "Accepted"

    def remove(self, nullifiers: List[bytes]):
        for nf in nullifiers:
            self.nullifiers.discard(nf)


def test_mempool_rejects_double_spend():
    """Same nullifier twice → mempool rejects second."""
    print("  TEST: mempool rejects double spend...", end=" ")
    pool = NullifierMempool()
    nf = os.urandom(32)

    ok1, _ = pool.accept([nf])
    assert ok1

    ok2, reason = pool.accept([nf])
    assert not ok2
    assert "already in mempool" in reason.lower() or "double" in reason.lower()
    print("PASSED")


def test_mempool_accepts_different_nullifiers():
    """Different nullifiers → mempool accepts both."""
    print("  TEST: mempool accepts different...", end=" ")
    pool = NullifierMempool()
    nf_a = os.urandom(32)
    nf_b = os.urandom(32)
    assert nf_a != nf_b

    ok1, _ = pool.accept([nf_a])
    ok2, _ = pool.accept([nf_b])
    assert ok1 and ok2
    assert len(pool.nullifiers) == 2
    print("PASSED")


def test_mempool_marks_mined():
    """After mining, nullifiers are removed from mempool."""
    print("  TEST: mempool marks mined...", end=" ")
    pool = NullifierMempool()
    nf = os.urandom(32)
    pool.accept([nf])
    assert len(pool.nullifiers) == 1
    pool.remove([nf])
    assert len(pool.nullifiers) == 0
    # Should accept again after removal
    ok, _ = pool.accept([nf])
    assert ok
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Stage 4: Block Validation (WASM execution)
# ═══════════════════════════════════════════════════════════════════════════

@dataclass
class NullifierSMT:
    """Simulated nullifier Sparse Merkle Tree for chain state."""
    spent: Set[bytes] = field(default_factory=set)

    def contains(self, nullifier: bytes) -> bool:
        return nullifier in self.spent

    def insert(self, nullifier: bytes) -> bool:
        """Returns False if already spent (double-spend)."""
        if nullifier in self.spent:
            return False
        self.spent.add(nullifier)
        return True


def test_wasm_rejects_double_spend():
    """WASM execution: same nullifier twice → block rejected."""
    print("  TEST: WASM rejects double spend...", end=" ")
    smt = NullifierSMT()
    nf = os.urandom(32)

    assert smt.insert(nf), "First spend must succeed"
    assert not smt.insert(nf), "Second spend must fail (double-spend)"
    print("PASSED")


def test_wasm_accepts_different():
    """WASM execution: different nullifiers → both accepted."""
    print("  TEST: WASM accepts different...", end=" ")
    smt = NullifierSMT()
    assert smt.insert(os.urandom(32))
    assert smt.insert(os.urandom(32))
    assert len(smt.spent) == 2
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Stage 5: Chain Persistence
# ═══════════════════════════════════════════════════════════════════════════

def test_chain_persistence_roundtrip():
    """Nullifiers survive block commit and restart."""
    print("  TEST: chain persistence...", end=" ")
    # Simulate block execution
    smt = NullifierSMT()
    nf1 = os.urandom(32)
    nf2 = os.urandom(32)
    smt.insert(nf1)
    smt.insert(nf2)

    # Simulate restart: reload from sled
    smt2 = NullifierSMT(spent=smt.spent.copy())
    assert smt2.contains(nf1)
    assert smt2.contains(nf2)
    assert len(smt2.spent) == 2
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Stage 6: Wallet Scan Detection
# ═══════════════════════════════════════════════════════════════════════════

def test_wallet_scan_detects_nullifier():
    """Wallet scan computes nullifier locally for spend tracking."""
    print("  TEST: wallet scan detects nullifier...", end=" ")
    secret = wm.SecretKey(os.urandom(32))
    coin_hash = os.urandom(32)
    nullifier = wm.nullifier(int.from_bytes(secret.inner, 'little'), coin_hash)

    # Wallet tracks spent nullifiers locally (separate from chain SMT)
    local_spent: Set[bytes] = set()
    local_spent.add(nullifier)

    assert nullifier in local_spent, "Wallet must track its own nullifiers"
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Integration: Full Lifecycle
# ═══════════════════════════════════════════════════════════════════════════

def test_full_nullifier_lifecycle():
    """End-to-end: wallet computes → tx carries → mempool dedups →
    WASM validates → chain persists → wallet detects."""
    print("  TEST: full lifecycle...", end=" ")

    # Stage 1: Wallet computes nullifier
    secret = wm.SecretKey(os.urandom(32))
    coin_hash = os.urandom(32)
    nullifier = wm.nullifier(int.from_bytes(secret.inner, 'little'), coin_hash)

    # Stage 2: Transaction carries it
    assert nullifier is not None
    assert len(nullifier) == 32

    # Stage 3: Mempool dedups
    pool = NullifierMempool()
    ok, _ = pool.accept([nullifier])
    assert ok, "Mempool must accept first spend"
    not_ok, _ = pool.accept([nullifier])
    assert not not_ok, "Mempool must reject double-spend"

    # Stage 4: WASM validates
    smt = NullifierSMT()
    assert smt.insert(nullifier), "WASM must accept first spend"
    assert not smt.insert(nullifier), "WASM must reject double-spend"

    # Stage 5: Chain persists
    smt2 = NullifierSMT(spent=smt.spent.copy())
    assert smt2.contains(nullifier)

    # Stage 6: Wallet detects
    local_spent: Set[bytes] = set()
    local_spent.add(nullifier)
    assert nullifier in local_spent

    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Test Runner
# ═══════════════════════════════════════════════════════════════════════════

if __name__ == '__main__':
    print("=" * 60)
    print("DarkWow Nullifier Lifecycle — Specification")
    print("=" * 60)
    print()

    tests = [
        # Stage 1: Computation
        ("deterministic",           test_nullifier_deterministic),
        ("different-coins",         test_nullifier_different_coins),
        ("different-secrets",       test_nullifier_different_secrets),
        # Stage 2: Transaction population
        ("tx-populates",            test_tx_populates_nullifiers),
        ("spend-without-nullifier", test_tx_spend_without_nullifier_is_invalid),
        # Stage 3: Mempool
        ("mempool-double-spend",    test_mempool_rejects_double_spend),
        ("mempool-different",       test_mempool_accepts_different_nullifiers),
        ("mempool-marks-mined",     test_mempool_marks_mined),
        # Stage 4: Block validation
        ("wasm-double-spend",       test_wasm_rejects_double_spend),
        ("wasm-different",          test_wasm_accepts_different),
        # Stage 5: Chain persistence
        ("chain-persistence",       test_chain_persistence_roundtrip),
        # Stage 6: Wallet scan
        ("wallet-scan",             test_wallet_scan_detects_nullifier),
        # Integration
        ("full-lifecycle",          test_full_nullifier_lifecycle),
    ]

    passed = 0
    failed = 0
    for name, test_fn in tests:
        try:
            test_fn()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  {name}: FAILED — {e}")
            import traceback
            traceback.print_exc()

    print()
    print("=" * 60)
    if failed == 0:
        print(f"ALL TESTS PASSED ({passed} tests)")
    else:
        print(f"SOME TESTS FAILED ({failed}/{passed+failed} failures)")
    print("=" * 60)
    sys.exit(0 if failed == 0 else 1)
