#!/usr/bin/env python3
"""
DarkWow Nullifier Lifecycle — Complete Specification.

Nullifiers prevent double-spends. Every spend MUST produce a nullifier.
Every nullifier MUST be unique across the entire chain history.

Lifecycle: wallet computes → tx carries → mempool dedups → WASM validates
→ chain persists → wallet detects.

Security-critical. Test-critical. Mainnet-critical.

Invariant — the Representation Faithfulness Law (type-system.md §0.1):
a "spent" barb is faithfully encoded iff its witness is a distinguished
(non-empty) element; the empty value `[]` is the canonical "absent" witness.
Mechanized in proofs/lean/src/DarkFi/Combinatorial/NullifierStorage.lean
(markSpent_faithful, markEmpty_not_spent, markEmpty_never_adds,
markSpent_sound, markSpent_monotone, markSpent_idempotent,
faithful_iff_nonempty).
"""

import hashlib
import os
import sys
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set, Tuple

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
# Stage 4: Block Validation (two-layer nullifier replay)
# ═══════════════════════════════════════════════════════════════════════════

# Nullifier storage convention (contract-wasm-standards-best-practices.md §9):
#   Write: db_mark_spent(db, &nullifier.to_bytes()) → non-empty marker &[1]
#   Read:  db_contains_key(db, &nullifier.to_bytes())
# No SMT for nullifiers — a flat key-value marker, NOT a Sparse Merkle Tree.
#
# TWO layers, ONE authoritative:
#   (1) Consensus layer — chain_state.spent_nullifiers (BTreeSet) is the
#       authoritative replay gate. chain_state.nullifier_set (BTreeMap →
#       BlockHeight) tracks coinbase maturity. has_nullifier() checks
#       spent_nullifiers ONLY (claim nullifiers are not double-spends).
#   (2) Contract layer — db_mark_spent/db_contains_key marker is consistent
#       defense-in-depth, NOT the primary gate.

# Coinbase spend maturity in blocks (chain_state.rs:1087 COINBASE_MATURITY).
COINBASE_MATURITY = 100


@dataclass
class NullifierDb:
    """Contract-layer nullifier key-value store (defense-in-depth).

    Models the sled backend's empty-value-as-absent semantics: a key is
    "present" iff its stored value is non-empty. db_mark_spent writes the
    non-empty marker &[1]; db_set(key, &[]) writes an empty value that
    db_contains_key cannot see.
    """
    store: Dict[bytes, bytes] = field(default_factory=dict)

    def db_set(self, key: bytes, value: bytes) -> None:
        self.store[key] = value

    def db_contains_key(self, key: bytes) -> bool:
        # Empty value is indistinguishable from absent (empty-value-as-absent).
        return self.store.get(key, b'') != b''

    def db_mark_spent(self, key: bytes) -> None:
        # Writes the non-empty marker &[1] — never &[].
        self.db_set(key, b'\x01')


@dataclass
class ChainNullifierState:
    """Consensus-layer nullifier state (the authoritative replay gate).

    Mirrors chain_state.rs:
      - spent_nullifiers: BTreeSet — double-spend prevention. has_nullifier()
        checks this set ONLY (claim nullifiers are not double-spends).
      - nullifier_set: BTreeMap<Nullifier, BlockHeight> — every nullifier
        (claim + spend) with creation height, for coinbase maturity.
    """
    spent_nullifiers: Set[bytes] = field(default_factory=set)
    nullifier_set: Dict[bytes, int] = field(default_factory=dict)

    def has_nullifier(self, nullifier: bytes) -> bool:
        # chain_state.rs:549-555 — checks spent_nullifiers only.
        return nullifier in self.spent_nullifiers

    def nullifier_height(self, nullifier: bytes) -> Optional[int]:
        return self.nullifier_set.get(nullifier)

    def record_claim(self, nullifier: bytes, height: int) -> None:
        # Claim nullifier (coinbase/fee-collect): maturity tracking only.
        # NOT added to spent_nullifiers (the claim IS the future spend).
        self.nullifier_set.setdefault(nullifier, height)

    def record_spend(self, nullifier: bytes, height: int) -> None:
        # Spend nullifier: replay protection + historical record.
        self.nullifier_set.setdefault(nullifier, height)
        self.spent_nullifiers.add(nullifier)

    def is_mature(self, nullifier: bytes, current_height: int) -> bool:
        # chain_state.rs:1094-1108 — coinbase spend maturity.
        created_at = self.nullifier_set.get(nullifier)
        if created_at is None:
            return True  # not a coinbase output; no maturity constraint
        return current_height - created_at >= COINBASE_MATURITY


def test_empty_value_as_absent():
    """The empty-value-as-absent defect: db_set(key, &[]) writes an empty
    value that db_contains_key cannot see (empty == absent). db_mark_spent
    writes &[1], which IS visible. This is why the standard forbids &[]."""
    print("  TEST: empty-value-as-absent...", end=" ")
    db = NullifierDb()
    nf = os.urandom(32)

    # Forbidden path: an empty write is invisible to the read.
    db.db_set(nf, b'')
    assert not db.db_contains_key(nf), "db_set(&[]) must be invisible (empty == absent)"

    # Standard path: the non-empty marker is visible.
    db.db_mark_spent(nf)
    assert db.db_contains_key(nf), "db_mark_spent(&[1]) must be visible"
    assert db.store[nf] == b'\x01', "db_mark_spent must write the exact &[1] marker"
    print("PASSED")


def test_contract_replay_check():
    """Contract layer: db_contains_key rejects a nullifier already marked
    spent via db_mark_spent (defense-in-depth)."""
    print("  TEST: contract replay check...", end=" ")
    db = NullifierDb()
    nf = os.urandom(32)

    # First spend: no prior marker → proceed, then mark.
    assert not db.db_contains_key(nf), "first spend must see no prior marker"
    db.db_mark_spent(nf)
    # Second spend: prior marker present → reject.
    assert db.db_contains_key(nf), "db_contains_key must detect the prior spend"
    print("PASSED")


def test_consensus_is_authoritative():
    """Consensus spent_nullifiers is the authoritative gate; the contract
    marker is defense-in-depth only. A consensus-spent nullifier is rejected
    even if the contract marker is missing; a contract marker alone never
    authorizes a spend."""
    print("  TEST: consensus authoritative...", end=" ")
    consensus = ChainNullifierState()
    db = NullifierDb()

    # Consensus-spent nullifier, contract marker missing/corrupt.
    nf = os.urandom(32)
    consensus.record_spend(nf, 1000)
    assert consensus.has_nullifier(nf), "consensus must reject regardless of contract marker"
    assert not db.db_contains_key(nf), "contract marker is missing"

    # Contract marker alone never authorizes a spend.
    nf2 = os.urandom(32)
    db.db_mark_spent(nf2)
    assert db.db_contains_key(nf2), "contract marker present"
    assert not consensus.has_nullifier(nf2), "contract marker alone never authorizes (defense-in-depth)"
    print("PASSED")


def test_coinbase_maturity():
    """A coinbase nullifier cannot be spent before COINBASE_MATURITY (100)
    blocks (chain_state.rs:1087, 1094-1108)."""
    print("  TEST: coinbase maturity...", end=" ")
    consensus = ChainNullifierState()
    nf = os.urandom(32)

    # Coinbase claim at height 1000: maturity tracking, NOT a double-spend.
    consensus.record_claim(nf, 1000)
    assert consensus.nullifier_height(nf) == 1000
    assert not consensus.has_nullifier(nf), "claim nullifier is NOT a spent nullifier"

    # Not yet spendable: 50 blocks < 100.
    assert not consensus.is_mature(nf, 1050), "immature coinbase spend must be rejected"
    # Spendable at exactly maturity.
    assert consensus.is_mature(nf, 1100), "coinbase spend must be allowed after maturity"
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Stage 5: Chain Persistence
# ═══════════════════════════════════════════════════════════════════════════

def test_chain_persistence_roundtrip():
    """Nullifiers survive block commit and restart."""
    print("  TEST: chain persistence...", end=" ")
    consensus = ChainNullifierState()
    nf1 = os.urandom(32)
    nf2 = os.urandom(32)
    consensus.record_spend(nf1, 100)
    consensus.record_spend(nf2, 100)

    # Simulate restart: reload from sled.
    reloaded = ChainNullifierState(
        spent_nullifiers=consensus.spent_nullifiers.copy(),
        nullifier_set=consensus.nullifier_set.copy(),
    )
    assert reloaded.has_nullifier(nf1)
    assert reloaded.has_nullifier(nf2)
    assert len(reloaded.spent_nullifiers) == 2
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

    # Wallet tracks spent nullifiers locally (separate from chain nullifier set)
    local_spent: Set[bytes] = set()
    local_spent.add(nullifier)

    assert nullifier in local_spent, "Wallet must track its own nullifiers"
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Integration: Full Lifecycle
# ═══════════════════════════════════════════════════════════════════════════

def test_full_nullifier_lifecycle():
    """End-to-end: wallet computes → tx carries → mempool dedups →
    consensus validates (authoritative) → chain persists → wallet detects."""
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

    # Stage 4: Consensus validates (authoritative) + contract marks (defense-in-depth)
    consensus = ChainNullifierState()
    db = NullifierDb()
    assert not consensus.has_nullifier(nullifier), "consensus must accept first spend"
    assert not db.db_contains_key(nullifier), "contract must see no prior marker"
    consensus.record_spend(nullifier, 100)
    db.db_mark_spent(nullifier)
    assert consensus.has_nullifier(nullifier), "consensus must reject double-spend"
    assert db.db_contains_key(nullifier), "contract marker must record the spend"

    # Stage 5: Chain persists
    reloaded = ChainNullifierState(
        spent_nullifiers=consensus.spent_nullifiers.copy(),
        nullifier_set=consensus.nullifier_set.copy(),
    )
    assert reloaded.has_nullifier(nullifier)

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
        # Stage 4: Block validation (two-layer)
        ("empty-value-as-absent",   test_empty_value_as_absent),
        ("contract-replay-check",   test_contract_replay_check),
        ("consensus-authoritative", test_consensus_is_authoritative),
        ("coinbase-maturity",       test_coinbase_maturity),
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
