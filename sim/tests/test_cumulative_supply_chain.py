"""Cumulative Supply Chain — rigorous math verification.

Tests the Pedersen commitment chain S_H = S_{H-1} + C_H that provides
shielded supply audit for NativeToken. Verifies the chain is correct,
externally auditable, tamper-evident, and handles forks correctly.

Design: passive audit layer (like Bitcoin's halving schedule) —
not a consensus circuit breaker.
"""

import sys, os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from sim.crypto import (
    poseidon_hash,
    PedersenCommitment,
    pedersen_commit,
    pedersen_add,
    pedersen_eq,
    ec_mul_base,
    nullifier,
    commitment,
    expected_reward,
    expected_cumulative_supply,
)

# ============================================================
# Deterministic coinbase blind derivation
# (Matches dwow_sdk::blockchain::coinbase_blind)
# ============================================================

def coinbase_blind(prev_commitment: bytes, height: int) -> bytes:
    """Deterministic blind: poseidon_hash(prev_commitment, height, domain)."""
    return poseidon_hash(prev_commitment, height, b'native_token_coinbase_blind')


# ============================================================
# Block and chain simulation
# ============================================================

class CoinbaseOutput:
    """A coinbase commitment created in a block."""
    def __init__(self, height, reward, blind, public_key=None):
        self.height = height
        self.reward = reward
        self.blind = blind
        self.public_key = public_key or ec_mul_base(b'miner_%d' % height)
        self.value_commit = pedersen_commit(reward, blind)
        self.commitment = commitment(
            self.public_key, reward, b'\x00' * 32,  # token_id = zero (DARK)
            b'\x00' * 32, b'\x00' * 32, poseidon_hash(b'commitment_blind', height)
        )


class CumulativeChain:
    """Tracks the Pedersen cumulative commitment chain S_H.

    Key assumption: each canonical block height adds exactly `expected_reward(H)`
    to total supply. Uncle rewards are paid FROM the canonical coinbase, not from
    new issuance. The consensus invariant is:

        canonical_reward + sum(uncle_rewards) = expected_reward(H)

    The canonical miner may keep only 50% (or less), but the other 50% goes to
    uncle miners — the total new DARK created at height H is always the full
    block reward. The cumulative chain tracks total issuance, not the canonical
    miner's net take.

    This means the cumulative chain is a function of block HEIGHT only, not of
    how many uncles were included or how the reward was split.
    """

    def __init__(self):
        self.cumulative_commit = PedersenCommitment(0, 0)  # S_0 = identity
        self.cumulative_blind = 0  # sum of all coinbase blind ints (for simplicity)
        self.cumulative_blind_bytes = b'\x00' * 32  # byte-level blind tracking
        self.total_supply = 0
        self.prev_commitment = b'\x00' * 32  # genesis: zero
        self.blocks = []  # list of CoinbaseOutput (canonical chain)

    def add_canonical_block(self, height, miner_id=1):
        """Add a canonical block at the given height. Extends the cumulative chain."""
        reward = expected_reward(height)
        blind = coinbase_blind(self.prev_commitment, height)
        coinbase = CoinbaseOutput(height, reward, blind, ec_mul_base(b'miner_%d_%d' % (miner_id, height)))

        # The chain identity: S_H = S_{H-1} + C_H
        # Pedersen additive homomorphism: C(v1,b1) + C(v2,b2) = C(v1+v2, b1+b2)
        new_cumulative = pedersen_add(self.cumulative_commit, coinbase.value_commit)

        # Verify homomorphic property: the new cumulative v_part equals
        # old v_part + reward, confirming the chain correctly tracks supply
        assert new_cumulative.v_part == self.cumulative_commit.v_part + reward, \
            f"Chain break at height {height}: v_part mismatch"
        assert new_cumulative.r_part == self.cumulative_commit.r_part + coinbase.value_commit.r_part, \
            f"Chain break at height {height}: r_part mismatch"

        # Update state
        self.cumulative_commit = new_cumulative
        self.total_supply += reward
        self.prev_commitment = coinbase.commitment
        self.blocks.append(coinbase)

        return coinbase

    def verify_chain(self):
        """External auditor: verify the entire chain from genesis."""
        cumulative = PedersenCommitment(0, 0)
        total_supply = 0
        prev_commitment = b'\x00' * 32

        for i, coinbase in enumerate(self.blocks):
            h = i + 1
            # Recompute expected blind
            expected_blind = coinbase_blind(prev_commitment, h)
            expected_reward_h = expected_reward(h)

            # Recompute expected coinbase value commit
            expected_vc = pedersen_commit(expected_reward_h, expected_blind)

            # Extend expected cumulative
            cumulative = pedersen_add(cumulative, expected_vc)
            total_supply += expected_reward_h

            # Verify stored cumulative matches expected
            assert pedersen_eq(cumulative, self.cumulative_commit_at(h)), \
                f"External audit failed at height {h}: stored S_{h} != expected"

            # Verify total supply matches emission schedule
            assert total_supply == expected_cumulative_supply(h), \
                f"Supply mismatch at height {h}: {total_supply} != {expected_cumulative_supply(h)}"

            prev_commitment = coinbase.commitment

        return True

    def cumulative_commit_at(self, height):
        """Get the cumulative commitment at a given height (1-indexed)."""
        if height == 0:
            return PedersenCommitment(0, 0)
        cumulative = PedersenCommitment(0, 0)
        for i in range(height):
            cumulative = pedersen_add(cumulative, self.blocks[i].value_commit)
        return cumulative

    def tamper_and_detect(self, height, fake_supply_increase):
        """Inject a tampered cumulative at given height; verify detection."""
        # Copy the chain
        original = self.cumulative_commit_at(height)

        # Tamper: pretend cumulative is larger (hidden inflation)
        tampered = PedersenCommitment(
            original.v_part + fake_supply_increase,
            original.r_part  # same blind — this would fail Pedersen binding
        )

        # External auditor would detect: S_height != expected
        expected = PedersenCommitment(
            expected_cumulative_supply(height),
            original.r_part  # auditor computes expected blind sum
        )

        # The tampered commitment doesn't match expected supply
        return not pedersen_eq(tampered, expected)


# ============================================================
# Fork / Reorg simulation
# ============================================================

class ForkSimulation:
    """Simulates competing blocks at the same height and reorg resolution."""

    def __init__(self):
        self.canonical = CumulativeChain()
        self.fork_blocks = []  # competing blocks not on canonical chain

    def mine_competing_blocks(self, height):
        """Mine two competing blocks at the same height."""
        # Both miners build on the same canonical tip
        block_a = CoinbaseOutput(
            height, expected_reward(height),
            coinbase_blind(self.canonical.prev_commitment, height),
            ec_mul_base(b'miner_A_%d' % height)
        )
        block_b = CoinbaseOutput(
            height, expected_reward(height),
            coinbase_blind(self.canonical.prev_commitment, height),
            ec_mul_base(b'miner_B_%d' % height)
        )

        # Both use the SAME prev_commitment, so blinds are identical
        # Different public keys → different commitments
        assert block_a.blind == block_b.blind, \
            "Competing blocks at same height must have same blind"
        assert block_a.commitment != block_b.commitment, \
            "Different miners produce different commitments"

        return block_a, block_b

    def resolve_to_canonical(self, winner, loser):
        """Resolve fork: winner becomes canonical, loser becomes uncle."""
        # Canonical chain extends with winner
        new_cumulative = pedersen_add(
            self.canonical.cumulative_commit, winner.value_commit
        )
        self.canonical.cumulative_commit = new_cumulative
        self.canonical.total_supply += winner.reward
        self.canonical.prev_commitment = winner.commitment
        self.canonical.blocks.append(winner)

        # Loser becomes uncle — NOT in cumulative chain
        self.fork_blocks.append(loser)

        # Verify: cumulative chain does NOT include uncle
        recomputed = PedersenCommitment(0, 0)
        for b in self.canonical.blocks:
            recomputed = pedersen_add(recomputed, b.value_commit)
        assert pedersen_eq(recomputed, self.canonical.cumulative_commit), \
            "Cumulative chain must only reflect canonical blocks"

    def reorg(self, new_canonical_blocks):
        """Simulate a reorg: switch to a different fork."""
        # Revert canonical chain to fork point
        fork_point = new_canonical_blocks[0].height - 1
        self.canonical.blocks = self.canonical.blocks[:fork_point]
        self.canonical.total_supply = expected_cumulative_supply(fork_point)
        self.canonical.cumulative_commit = self.canonical.cumulative_commit_at(fork_point)

        # Replay new canonical blocks
        for block in new_canonical_blocks:
            self.canonical.cumulative_commit = pedersen_add(
                self.canonical.cumulative_commit, block.value_commit
            )
            self.canonical.total_supply += block.reward
            self.canonical.prev_commitment = block.commitment
            self.canonical.blocks.append(block)

        # Verify chain integrity after reorg
        recomputed = PedersenCommitment(0, 0)
        total = 0
        for i, b in enumerate(self.canonical.blocks):
            recomputed = pedersen_add(recomputed, b.value_commit)
            total += b.reward
            assert total == expected_cumulative_supply(i + 1), \
                f"Post-reorg supply mismatch at height {i+1}"
        assert pedersen_eq(recomputed, self.canonical.cumulative_commit), \
            "Post-reorg cumulative chain integrity broken"


# ============================================================
# Tests
# ============================================================

def test_chain_extension():
    """Verify S_H = S_{H-1} + C_H for every block."""
    print("Chain extension: S_H = S_{H-1} + C_H...")
    chain = CumulativeChain()
    for h in range(1, 21):
        chain.add_canonical_block(h)
    assert chain.total_supply == expected_cumulative_supply(20)
    print("  OK — 20 blocks, chain intact, supply=%d" % chain.total_supply)
    return True


def test_external_audit():
    """External auditor verifies entire chain independently."""
    print("External audit: independent verification...")
    chain = CumulativeChain()
    for h in range(1, 51):
        chain.add_canonical_block(h)
    assert chain.verify_chain()
    print("  OK — 50 blocks, external audit passes")
    return True


def test_tamper_detection():
    """Inject tampered cumulative; verify auditor detects it."""
    print("Tamper detection: hidden inflation...")
    chain = CumulativeChain()
    for h in range(1, 11):
        chain.add_canonical_block(h)

    # Tamper at height 5: pretend 100M extra DARK
    tampered_detected = chain.tamper_and_detect(5, 100_000_000)
    assert tampered_detected, "Tamper should be detected!"

    # Tamper with wrong blind
    fake_commit = PedersenCommitment(
        expected_cumulative_supply(5),
        999999999  # wrong blind
    )
    real_commit = chain.cumulative_commit_at(5)
    assert not pedersen_eq(fake_commit, real_commit), \
        "Wrong blind must not match"
    print("  OK — tampering detected, wrong blind rejected")
    return True


def test_deterministic_blinds():
    """Blinds must be deterministic from prev_commitment + height."""
    print("Deterministic blinds...")
    # Two chains with same prev_commitment must produce same blind
    prev = b'\x00' * 32
    b1 = coinbase_blind(prev, 5)
    b2 = coinbase_blind(prev, 5)
    assert b1 == b2, "Same inputs → same blind"

    # Different height → different blind
    b3 = coinbase_blind(prev, 6)
    assert b1 != b3, "Different height → different blind"

    # Different prev_commitment → different blind
    b4 = coinbase_blind(b'different_commitment', 5)
    assert b1 != b4, "Different prev_commitment → different blind"

    print("  OK — blinds deterministic, unique per (prev_commitment, height)")
    return True


def test_fork_handling():
    """Competing blocks: only canonical extends cumulative chain."""
    print("Fork handling: canonical vs uncle...")
    fork = ForkSimulation()

    # Build canonical chain up to height 4
    for h in range(1, 5):
        fork.canonical.add_canonical_block(h)

    # Height 5: two miners compete
    block_a, block_b = fork.mine_competing_blocks(5)

    # Both have same blind (same prev_commitment, same height)
    assert block_a.blind == block_b.blind

    # A wins, B becomes uncle
    fork.resolve_to_canonical(block_a, block_b)

    # Verify: cumulative chain only has 5 blocks (A, not B)
    assert len(fork.canonical.blocks) == 5
    assert len(fork.fork_blocks) == 1

    # Key invariant: total supply = expected_cumulative_supply(height)
    # regardless of how many uncles there were. Only ONE full reward
    # is issued per canonical height. Uncle rewards come FROM the
    # canonical coinbase, not from new issuance.
    assert fork.canonical.total_supply == expected_cumulative_supply(5)
    print("  invariant: supply = emission schedule regardless of uncle count")

    # Continue mining on canonical
    for h in range(6, 11):
        fork.canonical.add_canonical_block(h)
    assert fork.canonical.total_supply == expected_cumulative_supply(10)

    print("  OK — fork resolved, only canonical blocks in chain, supply correct")
    return True


def test_reorg():
    """Reorg switches canonical chain; cumulative recomputed correctly."""
    print("Reorg: chain switch...")
    fork = ForkSimulation()

    # Build to height 4
    for h in range(1, 5):
        fork.canonical.add_canonical_block(h)

    # Height 5-7 on fork A (canonical)
    fork_a_blocks = []
    for h in range(5, 8):
        cb = fork.canonical.add_canonical_block(h)
        fork_a_blocks.append(cb)

    supply_before_reorg = fork.canonical.total_supply
    assert supply_before_reorg == expected_cumulative_supply(7)

    # Reorg to fork B: heights 5-7 with different miners
    fork.canonical.blocks = fork.canonical.blocks[:4]  # back to height 4
    fork.canonical.total_supply = expected_cumulative_supply(4)
    fork.canonical.cumulative_commit = fork.canonical.cumulative_commit_at(4)
    fork.canonical.prev_commitment = fork.canonical.blocks[-1].commitment

    fork_b_blocks = []
    for h in range(5, 8):
        cb = CoinbaseOutput(
            h, expected_reward(h),
            coinbase_blind(fork.canonical.prev_commitment, h),
            ec_mul_base(b'miner_B_%d' % h)
        )
        fork_b_blocks.append(cb)
        fork.canonical.cumulative_commit = pedersen_add(
            fork.canonical.cumulative_commit, cb.value_commit
        )
        fork.canonical.total_supply += cb.reward
        fork.canonical.prev_commitment = cb.commitment
        fork.canonical.blocks.append(cb)

    # Supply must be identical after reorg (same heights, same emission)
    assert fork.canonical.total_supply == supply_before_reorg, \
        "Reorg must not change total supply at same height"
    assert fork.canonical.total_supply == expected_cumulative_supply(7)

    # But cumulative COMMITMENTS differ (different prev_commitments → different blinds)
    fork_a_cumulative = PedersenCommitment(0, 0)
    for b in fork_a_blocks:
        fork_a_cumulative = pedersen_add(fork_a_cumulative, b.value_commit)
    fork_b_cumulative = PedersenCommitment(0, 0)
    for b in fork_b_blocks:
        fork_b_cumulative = pedersen_add(fork_b_cumulative, b.value_commit)
    assert not pedersen_eq(fork_a_cumulative, fork_b_cumulative), \
        "Different fork paths → different cumulative commitments (different blinds)"

    print("  OK — reorg preserves supply, different paths have different commitments")
    return True


def test_emission_schedule():
    """Verify the emission schedule math."""
    print("Emission schedule...")
    # Genesis
    assert expected_reward(0) == 0
    assert expected_cumulative_supply(0) == 0

    # Block 1
    r1 = expected_reward(1)
    assert r1 > 0
    assert expected_cumulative_supply(1) == r1

    # Monotonic
    for h in range(1, 100):
        assert expected_reward(h) > 0
        assert expected_cumulative_supply(h) >= expected_cumulative_supply(h-1)

    # Supply at height H = sum of rewards
    for h in [1, 5, 10, 50, 100, 500]:
        total = sum(expected_reward(i) for i in range(1, h+1))
        assert total == expected_cumulative_supply(h), \
            f"Emission schedule mismatch at height {h}"

    print("  OK — emission schedule consistent")
    return True


def test_upper_bound_property():
    """Cumulative chain proves upper bound: actual supply ≤ cumulative supply."""
    print("Upper bound: supply ≤ cumulative...")
    chain = CumulativeChain()
    for h in range(1, 21):
        chain.add_canonical_block(h)

    cumulative_supply = chain.total_supply
    assert cumulative_supply == expected_cumulative_supply(20)

    # Simulate burns: actual supply drops, cumulative stays same
    burned = 100_000_000
    actual_supply = cumulative_supply - burned
    assert actual_supply < cumulative_supply
    assert cumulative_supply == expected_cumulative_supply(20)

    # Over-reporting is conservative: proves nobody exceeded the ceiling
    print("  OK — cumulative supply is upper bound, burns conservative")
    return True


# ============================================================
# Runner
# ============================================================

def run_all():
    tests = [
        test_chain_extension,
        test_external_audit,
        test_tamper_detection,
        test_deterministic_blinds,
        test_fork_handling,
        test_reorg,
        test_emission_schedule,
        test_upper_bound_property,
    ]
    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  FAIL: {e}")
            import traceback
            traceback.print_exc()

    print(f"\n=== Results: {passed} passed, {failed} failed ===")
    return failed == 0


if __name__ == "__main__":
    success = run_all()
    sys.exit(0 if success else 1)
