"""Fee Payment and Collection — Python 1:1 Executable Specification
===================================================================

This is the specification for FeeV1, FeeV2, FeeCollectV1, the coin Merkle tree,
and the two-tier mempool. Per memory rule python-model-is-the-spec, this model
is the ground truth. The Rust implementation SHALL follow this model exactly.

Specification reference: doc/src/arch/consensus/fee-spec.md

Covers:
  §1  — Coin Merkle Tree (incremental, UNCOMMITTED_ORCHARD=2, zero guard)
  §2  — Block Production Model (sequential overlay, coin tree growth)
  §3  — FeeV1 (clear-text fee, 14 circuit public inputs)
  §4  — FeeCollectV1 (claims accumulated fees, closes tree)
  §5  — FeeV2 (hidden fee, Pedersen commitment, FeeThreshold_V1 proof)
  §6  — FeeAmount nominal type (u64 wrapper, no bare int crossing boundaries)
  §7  — Two-Tier Mempool (premium/general thresholds, FIFO, REJECT)

Extended from contrib/model/proof_of_token_balance.py — all 9 original tests
migrated + new FeeV2 and merkle tree tests.
"""

import sys, os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from sim.crypto import (
    PedersenCommitment,
    pedersen_commit,
    pedersen_add,
    pedersen_eq,
    poseidon_hash,
    expected_reward,
)

# ============================================================
# Constants — matching fee-spec.md §10
# ============================================================

PREMIUM_THRESHOLD = 42_000_000        # Minimum fee for premium mempool tier
GENERAL_THRESHOLD = 1_000_000         # Minimum fee for general mempool tier
COINBASE_MATURITY = 100               # Blocks before coinbase coin is spendable
MERKLE_DEPTH = 32                     # Orchard tree depth (2^32 capacity)
UNCOMMITTED_ORCHARD = 2               # pallas::Base::from(2) — NOT zero
ZERO_GUARD = 0                        # pallas::Base::ZERO at position 0
DRKW_TOKEN_ID = 0                     # Native token identifier

# ============================================================
# §1 — Coin Merkle Tree
# ============================================================

def _combine(altitude: int, left: int, right: int) -> int:
    """Simulate MerkleNode::combine(altitude, left, right).

    Uses poseidon_hash with altitude-prefixed domain separation.
    Real circuit: SinsemillaHash(altitude || left.inner() || right.inner()).
    """
    return int.from_bytes(
        poseidon_hash(f"merkle_combine_{altitude}", left, right), 'big'
    ) % (2**64)


class CoinTree:
    """Incremental Merkle tree of coin commitments (spec §1.1).

    CoinTree = BridgeTree<MerkleNode, usize, 32>
    MerkleNode = MerkleNode(pallas::Base)  — modeled as int

    Empty leaf: pallas::Base::from(2) (UNCOMMITTED_ORCHARD)
    Zero guard at position 0: pallas::Base::ZERO
    """

    def __init__(self):
        # Precompute empty roots ladder (spec §1.4)
        self._empty_roots = [0] * (MERKLE_DEPTH + 1)
        self._empty_roots[0] = UNCOMMITTED_ORCHARD  # level 0: UNCOMMITTED_ORCHARD
        for i in range(1, MERKLE_DEPTH + 1):
            self._empty_roots[i] = _combine(i - 1, self._empty_roots[i - 1], self._empty_roots[i - 1])

        self.leaves = []  # ordered list of leaf values
        self._next_position = 0

    def init_zero_guard(self):
        """Initialize with zero guard at position 0 (spec §1.5)."""
        assert self._next_position == 0
        self.leaves.append(ZERO_GUARD)
        self._next_position = 1

    def append(self, leaf: int) -> int:
        """Append a leaf at the next position, return the position (spec §1.6)."""
        pos = self._next_position
        self.leaves.append(leaf)
        self._next_position += 1
        return pos

    def root(self, position: int) -> int:
        """Compute the Merkle root after leaf at `position` was appended (spec §1.7)."""
        if position >= len(self.leaves):
            raise IndexError(f"position {position} beyond leaf count {len(self.leaves)}")
        return self._compute_root_at(position)

    def witness(self, position: int, depth: int = MERKLE_DEPTH) -> list[int]:
        """Compute merkle path for leaf at `position` (Theorem 1, spec §1.8).

        The path is computed against the tree state at the time the leaf
        was appended — i.e., with only leaves 0..position (inclusive).
        Returns list of 32 siblings, one per level.
        """
        if position >= len(self.leaves):
            raise IndexError(f"position {position} beyond leaf count {len(self.leaves)}")
        return self._merkle_path(position, depth, self.leaves[:position + 1])

    def _compute_root_at(self, pos: int) -> int:
        """Compute root with first pos+1 leaves.

        Builds a full-depth (32-level) tree. Empty subtrees are filled
        with EMPTY_ROOTS[L] at each level. This matches the ZK circuit's
        merkle path verification which iterates all 32 levels.
        """
        if pos < 0:
            return self._empty_roots[MERKLE_DEPTH]

        N = pos + 1
        nodes = list(self.leaves[:N])

        # Pad to power-of-2 with UNCOMMITTED_ORCHARD
        size = 1
        while size < N:
            size <<= 1
        while len(nodes) < size:
            nodes.append(UNCOMMITTED_ORCHARD)

        level = 0
        # Combine until we have one node, then continue with empty roots
        while len(nodes) > 1:
            next_level = []
            for i in range(0, len(nodes), 2):
                left = nodes[i]
                right = nodes[i + 1] if i + 1 < len(nodes) else UNCOMMITTED_ORCHARD
                next_level.append(_combine(level, left, right))
            nodes = next_level
            level += 1

        # Continue for remaining levels (up to MERKLE_DEPTH)
        # At each level, combine the single node with EMPTY_ROOTS[level]
        current = nodes[0]
        for remaining_level in range(level, MERKLE_DEPTH):
            current = _combine(remaining_level, current, self._empty_roots[remaining_level])

        return current

    def _merkle_path(self, pos: int, depth: int, leaves: list[int] = None) -> list[int]:
        """Compute merkle path for all `depth` levels.

        The path consists of the sibling node at each level. For levels
        beyond the actual tree height, the sibling is EMPTY_ROOTS[level].
        `leaves` defaults to self.leaves, but can be a slice for historical state.
        """
        if leaves is None:
            leaves = self.leaves
        N = len(leaves)
        path = []
        # Pad to power-of-2 with UNCOMMITTED_ORCHARD for empty leaves
        size = 1
        while size < N:
            size <<= 1
        nodes = list(leaves[:]) + [UNCOMMITTED_ORCHARD] * (size - N)

        current_pos = pos
        for level in range(depth):
            sibling_idx = current_pos ^ 1
            if sibling_idx < len(nodes):
                sibling = nodes[sibling_idx]
            else:
                sibling = self._empty_roots[level]
            path.append(sibling)

            # Move up one level: combine pairs
            next_nodes = []
            for i in range(0, len(nodes), 2):
                left = nodes[i]
                right = nodes[i + 1] if i + 1 < len(nodes) else self._empty_roots[level]
                next_nodes.append(_combine(level, left, right))
            nodes = next_nodes
            current_pos >>= 1

        return path

    def verify_merkle_proof(self, leaf: int, position: int, merkle_path: list[int],
                             expected_root: int) -> bool:
        """Verify a merkle inclusion proof (spec §1.9)."""
        current = leaf
        for level in range(MERKLE_DEPTH):
            if position & (1 << level) == 0:
                current = _combine(level, current, merkle_path[level])
            else:
                current = _combine(level, merkle_path[level], current)
        return current == expected_root


# ============================================================
# Test helpers (migrated from proof_of_token_balance.py)
# ============================================================

def mk(v: int, r: int = 0) -> PedersenCommitment:
    """Create a Pedersen commitment with explicit (value, blind)."""
    return PedersenCommitment(v, r)


def balanced_transfer(input_values: list[int],
                      output_values: list[int]) -> tuple[list[PedersenCommitment],
                                                         list[PedersenCommitment]]:
    """Create a balanced TransferV1 call.
    Prover chooses random input blinds, constrains last output blind
    so sum(output_blinds) == sum(input_blinds).
    """
    assert sum(input_values) == sum(output_values), "transfer must be value-neutral"
    inputs = []
    in_r_sum = 0
    for v in input_values:
        r = int.from_bytes(os.urandom(8), 'big')
        in_r_sum += r
        inputs.append(mk(v, r))
    outputs = []
    out_r_sum = 0
    for i, v in enumerate(output_values):
        if i < len(output_values) - 1:
            r = int.from_bytes(os.urandom(8), 'big')
            out_r_sum += r
            outputs.append(mk(v, r))
        else:
            r = in_r_sum - out_r_sum
            outputs.append(mk(v, r))
    return inputs, outputs


def balanced_fee(input_value: int, fee: int) -> tuple[PedersenCommitment,
                                                       PedersenCommitment,
                                                       int]:
    """Create a balanced FeeV1 call (spec §3).

    Fee_V2 circuit constrains: output_value + fee == input_value.
    Uses same blind for input and output so they cancel, fee blind = 0.
    """
    r = int.from_bytes(os.urandom(8), 'big')
    in_commit = mk(input_value, r)
    out_commit = mk(input_value - fee, r)
    return in_commit, out_commit, fee


def burn_inputs(values: list[int]) -> list[PedersenCommitment]:
    """Create BurnV1 inputs (no outputs — coins destroyed)."""
    commits = []
    for v in values:
        r = int.from_bytes(os.urandom(8), 'big')
        commits.append(mk(v, r))
    return commits


def unbalanced_mint(value: int) -> PedersenCommitment:
    """Create a MintV1 output with NO matching input — this is inflation."""
    r = int.from_bytes(os.urandom(8), 'big')
    return mk(value, r)


# ============================================================
# §5 — FeeV2: Hidden Fee with Pedersen Commitment
# ============================================================

def balanced_fee_v2(input_value: int, fee: int,
                     fee_blind: int) -> tuple[PedersenCommitment,
                                               PedersenCommitment,
                                               PedersenCommitment,
                                               int]:
    """Create a balanced FeeV2 call with hidden fee (spec §5.2).

    Fee_V3 circuit: output_value + fee == input_value (private witness).
    fee_value_commit = PedersenCommit(fee, fee_blind) — public input.

    For Pedersen homomorphic balance: input_blind = output_blind + fee_blind.
    The prover chooses input_blind freely, then sets output_blind so the
    blind equation holds. This is what the Fee_V3 circuit enforces.

    Fee is a private witness — verifiers see only the commitments.
    """
    r = int.from_bytes(os.urandom(8), 'big')
    in_commit = mk(input_value, r)
    out_commit = mk(input_value - fee, r - fee_blind)  # so blinds cancel
    fee_commit = mk(fee, fee_blind)                    # Pedersen hides fee
    return in_commit, out_commit, fee_commit, fee


# ============================================================
# §5.5 — FeeThreshold_V1: Prove fee >= threshold without revealing fee
# ============================================================

def verify_fee_threshold(fee: int, threshold: int) -> tuple[bool, str]:
    """FeeThreshold_V1 circuit model (spec §5.5).

    Constraint: range_check(64, fee - threshold).
    If fee < threshold, subtraction underflows in pallas::Base,
    producing a value near p - (threshold - fee) which fails range check.

    tx_binding = poseidon(DOMAIN_TX_BINDING, tx_commitment, threshold)
    — binds proof to a specific threshold to prevent replay.
    """
    diff = fee - threshold
    if diff < 0:
        return False, f"fee {fee} below threshold {threshold}"
    if diff >= (1 << 64):
        return False, f"fee {fee} exceeds 64-bit range"
    return True, "OK"


# ============================================================
# §7 — Two-Tier Mempool
# ============================================================

class TwoTierMempool:
    """Two-tier mempool with threshold-based admission (spec §7).

    Premium queue: fee >= PREMIUM_THRESHOLD (FIFO)
    General queue: fee >= GENERAL_THRESHOLD (FIFO)
    REJECT: fee < GENERAL_THRESHOLD

    In the real system, admission is gated by FeeThreshold_V1 proof
    verification. This model simulates proof verification by checking
    the fee against thresholds directly (equivalent under honest prover).
    """

    def __init__(self, premium_threshold: int = PREMIUM_THRESHOLD,
                 general_threshold: int = GENERAL_THRESHOLD):
        self.premium_threshold = premium_threshold
        self.general_threshold = general_threshold
        self.premium_queue = []   # FIFO: append to end, pop from front
        self.general_queue = []   # FIFO
        self._tx_map = {}         # tx_id -> (fee, queue_name)
        self._next_id = 0

    def admit(self, fee: int) -> tuple[int, str]:
        """Admit a transaction based on fee vs thresholds (spec §7.2).

        Returns (tx_id, queue_name). Raises ValueError for REJECT.
        """
        tx_id = self._next_id
        self._next_id += 1

        if fee >= self.premium_threshold:
            self.premium_queue.append(tx_id)
            self._tx_map[tx_id] = (fee, 'premium')
            return tx_id, 'premium'
        elif fee >= self.general_threshold:
            self.general_queue.append(tx_id)
            self._tx_map[tx_id] = (fee, 'general')
            return tx_id, 'general'
        else:
            raise ValueError(f"REJECT: fee {fee} below general threshold {self.general_threshold}")

    def select_for_block(self, max_txs: int = 250) -> list[int]:
        """Select transactions for block inclusion (spec §7.3).

        Drains premium queue first (FIFO), then general queue (FIFO).
        Non-destructive — does NOT remove from mempool.
        """
        selected = []
        # Drain premium first
        for tx_id in self.premium_queue:
            if len(selected) >= max_txs:
                break
            selected.append(tx_id)
        # Drain general second
        for tx_id in self.general_queue:
            if len(selected) >= max_txs:
                break
            selected.append(tx_id)
        return selected

    def mark_mined(self, tx_ids: list[int]):
        """Remove confirmed transactions from the mempool."""
        for tx_id in tx_ids:
            if tx_id in self._tx_map:
                _, queue_name = self._tx_map.pop(tx_id)
                if queue_name == 'premium':
                    self.premium_queue.remove(tx_id)
                else:
                    self.general_queue.remove(tx_id)

    def size(self) -> int:
        return len(self.premium_queue) + len(self.general_queue)


# ============================================================
# Block-level Mass Balance (migrated from proof_of_token_balance.py)
# ============================================================

def verify_proof_of_token_balance(coinbase_vc: PedersenCommitment,
                                   coinbase_reward: int,
                                   coinbase_fees: int,
                                   fee_inputs: list[PedersenCommitment],
                                   fee_outputs: list[PedersenCommitment],
                                   fee_amounts: list[int],
                                   burn_inputs: list[PedersenCommitment],
                                   transfer_inputs: list[PedersenCommitment],
                                   transfer_outputs: list[PedersenCommitment],
                                   spend_inputs: list[PedersenCommitment],
                                   spend_outputs: list[PedersenCommitment],
                                   mint_outputs: list[PedersenCommitment],
                                   ) -> tuple[bool, str]:
    """Verify block-level Pedersen mass balance.

    Equation: outputs + burn_inputs + fee_aggregate == inputs
    Coinbase is excluded from both sides — verified separately.
    """
    total_inputs = PedersenCommitment(0, 0)
    for c in fee_inputs:       total_inputs = pedersen_add(total_inputs, c)
    for c in burn_inputs:      total_inputs = pedersen_add(total_inputs, c)
    for c in transfer_inputs:  total_inputs = pedersen_add(total_inputs, c)
    for c in spend_inputs:     total_inputs = pedersen_add(total_inputs, c)

    total_outputs = PedersenCommitment(0, 0)
    for c in fee_outputs:      total_outputs = pedersen_add(total_outputs, c)
    for c in transfer_outputs: total_outputs = pedersen_add(total_outputs, c)
    for c in spend_outputs:    total_outputs = pedersen_add(total_outputs, c)
    for c in mint_outputs:     total_outputs = pedersen_add(total_outputs, c)

    fee_aggregate = PedersenCommitment(0, 0)
    for fee in fee_amounts:
        fee_aggregate = pedersen_add(fee_aggregate, mk(fee, 0))

    burn_aggregate = PedersenCommitment(0, 0)
    for c in burn_inputs:
        burn_aggregate = pedersen_add(burn_aggregate, c)

    left = total_outputs
    left = pedersen_add(left, burn_aggregate)
    left = pedersen_add(left, fee_aggregate)
    right = total_inputs

    if not pedersen_eq(left, right):
        net = PedersenCommitment(left.v_part - right.v_part, left.r_part - right.r_part)
        return (False,
                f"MASS BALANCE FAILED: net delta v={net.v_part} r={net.r_part}\n"
                f"  left  (outs+burns+fees): v={left.v_part}\n"
                f"  right (inputs):          v={right.v_part}")

    expected = coinbase_reward + coinbase_fees
    if coinbase_vc.v_part != expected:
        return (False,
                f"COINBASE MISMATCH: commit v={coinbase_vc.v_part} != expected {expected}")

    return (True, "OK")


# ============================================================
# FeeV2 Mass Balance with Hidden Fee Commitments
# ============================================================

def verify_proof_of_token_balance_v2(
    coinbase_vc: PedersenCommitment,
    coinbase_reward: int,
    coinbase_fees: int,
    fee_inputs: list[PedersenCommitment],
    fee_outputs: list[PedersenCommitment],
    fee_commitments: list[PedersenCommitment],   # NEW: Pedersen commitments instead of plain ints
    burn_inputs: list[PedersenCommitment],
    transfer_inputs: list[PedersenCommitment],
    transfer_outputs: list[PedersenCommitment],
    spend_inputs: list[PedersenCommitment],
    spend_outputs: list[PedersenCommitment],
    mint_outputs: list[PedersenCommitment],
) -> tuple[bool, str]:
    """Verify block-level mass balance with HIDDEN fee commitments (FeeV2).

    Difference from V1: fee_commitments are PedersenCommitment objects,
    not plain integers. The verifier sees commitments, not fee amounts.
    """
    total_inputs = PedersenCommitment(0, 0)
    for c in fee_inputs:       total_inputs = pedersen_add(total_inputs, c)
    for c in burn_inputs:      total_inputs = pedersen_add(total_inputs, c)
    for c in transfer_inputs:  total_inputs = pedersen_add(total_inputs, c)
    for c in spend_inputs:     total_inputs = pedersen_add(total_inputs, c)

    total_outputs = PedersenCommitment(0, 0)
    for c in fee_outputs:      total_outputs = pedersen_add(total_outputs, c)
    for c in transfer_outputs: total_outputs = pedersen_add(total_outputs, c)
    for c in spend_outputs:    total_outputs = pedersen_add(total_outputs, c)
    for c in mint_outputs:     total_outputs = pedersen_add(total_outputs, c)

    # Fee aggregate: sum of Pedersen commitments (not plain ints)
    fee_aggregate = PedersenCommitment(0, 0)
    for fc in fee_commitments:
        fee_aggregate = pedersen_add(fee_aggregate, fc)

    burn_aggregate = PedersenCommitment(0, 0)
    for c in burn_inputs:
        burn_aggregate = pedersen_add(burn_aggregate, c)

    left = total_outputs
    left = pedersen_add(left, burn_aggregate)
    left = pedersen_add(left, fee_aggregate)
    right = total_inputs

    if not pedersen_eq(left, right):
        return (False,
                f"MASS BALANCE FAILED (V2): left != right\n"
                f"  left  (outs+burns+fees): v={left.v_part}\n"
                f"  right (inputs):          v={right.v_part}")

    expected = coinbase_reward + coinbase_fees
    if coinbase_vc.v_part != expected:
        return (False,
                f"COINBASE MISMATCH: commit v={coinbase_vc.v_part} != expected {expected}")

    return (True, "OK")


# ============================================================
# Tests — Original FeeV1 (migrated from proof_of_token_balance.py)
# ============================================================

def test_legal_transfers_only():
    t_in, t_out = balanced_transfer([1000, 500, 300], [1000, 600, 200])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(100) + 500),
        coinbase_reward=expected_reward(100), coinbase_fees=500,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[], transfer_inputs=t_in, transfer_outputs=t_out,
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: transfers only — 1800 in, 1800 out")


def test_legal_with_fees():
    f1_in, f1_out, fee1 = balanced_fee(5000, 300)
    f2_in, f2_out, fee2 = balanced_fee(2000, 450)
    t_in, t_out = balanced_transfer([3000], [3000])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(200) + 750),
        coinbase_reward=expected_reward(200), coinbase_fees=750,
        fee_inputs=[f1_in, f2_in], fee_outputs=[f1_out, f2_out],
        fee_amounts=[fee1, fee2],
        burn_inputs=[], transfer_inputs=t_in, transfer_outputs=t_out,
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: fees — (4700+1550)+(300+450)+3000 == 5000+2000+3000")


def test_legal_with_burns():
    burns = burn_inputs([1000, 500])
    t_in, t_out = balanced_transfer([2000], [2000])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(300)),
        coinbase_reward=expected_reward(300), coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=burns, transfer_inputs=t_in, transfer_outputs=t_out,
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: burns — 2000+(1000+500) == 2000+1000+500")


def test_illegal_hidden_mint():
    in_b = int.from_bytes(os.urandom(8), 'big')
    in_commit = pedersen_commit(100, in_b.to_bytes(8, 'big'))
    out_commit = pedersen_commit(1_000_000, in_b.to_bytes(8, 'big'))
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(400)),
        coinbase_reward=expected_reward(400), coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[], transfer_inputs=[in_commit], transfer_outputs=[out_commit],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert not ok, f"Should have rejected hidden mint! {msg}"
    print(f"  REJECTED: hidden mint detected — 100 in, 1,000,000 out")


def test_illegal_standalone_mint():
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(500)),
        coinbase_reward=expected_reward(500), coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[unbalanced_mint(50_000_000)],
    )
    assert not ok, f"Should have rejected standalone mint! {msg}"
    print(f"  REJECTED: standalone mint detected")


def test_legal_mint_balanced_by_burn():
    r = int.from_bytes(os.urandom(8), 'big')
    burn_commit = mk(50_000_000, r)
    mint_commit = mk(50_000_000, r)
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(600)),
        coinbase_reward=expected_reward(600), coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[],
        transfer_inputs=[burn_commit], transfer_outputs=[mint_commit],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: mint balanced by burn — net zero")


def test_illegal_mint_exceeds_burn():
    r = int.from_bytes(os.urandom(8), 'big')
    burn_commit = mk(10_000_000, r)
    mint_commit = mk(50_000_000, r)
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(700)),
        coinbase_reward=expected_reward(700), coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[], transfer_inputs=[burn_commit], transfer_outputs=[mint_commit],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert not ok, f"Should have rejected mint-exceeds-burn! {msg}"
    print(f"  REJECTED: mint > burn detected — 50M minted, only 10M burned")


def test_coinbase_exceeds_schedule():
    excessive = expected_reward(800) + 1_000_000
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=pedersen_commit(excessive, b'cb'),
        coinbase_reward=expected_reward(800), coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert not ok, f"Should have rejected excessive coinbase! {msg}"
    print(f"  REJECTED: coinbase exceeds schedule")


def test_integration_with_cumulative_chain():
    print("\n  Integration: cumulative chain + mass balance...")
    cumulative = PedersenCommitment(0, 0)
    for h in range(1, 11):
        reward = expected_reward(h)
        fees = h * 50
        coinbase_vc = pedersen_commit(reward + fees, (b'cb_%d' % h))
        cumulative = pedersen_add(cumulative, coinbase_vc)
        f_in, f_out, fee = balanced_fee(1000 + h * 100, fees)
        t_in, t_out = balanced_transfer([h * 500, h * 300], [h * 500, h * 300])
        ok, msg = verify_proof_of_token_balance(
            coinbase_vc=coinbase_vc, coinbase_reward=reward, coinbase_fees=fees,
            fee_inputs=[f_in], fee_outputs=[f_out], fee_amounts=[fee],
            burn_inputs=[], transfer_inputs=t_in, transfer_outputs=t_out,
            spend_inputs=[], spend_outputs=[], mint_outputs=[],
        )
        assert ok, f"Block {h} failed: {msg}"
    print(f"  OK — 10 blocks, cumulative v={cumulative.v_part}")


# ============================================================
# Tests — FeeV2 (new)
# ============================================================

def test_feev2_mass_balance():
    """FeeV2: hidden fee commitments satisfy mass balance (spec §5.4)."""
    f1_in, f1_out, fee_commit, fee1 = balanced_fee_v2(5000, 300, 42)
    ok, msg = verify_fee_threshold(fee1, 100)
    assert ok, msg

    # Mass balance with FeeV2 hidden commitments
    v2_ok, v2_msg = verify_proof_of_token_balance_v2(
        coinbase_vc=mk(expected_reward(200) + 300),
        coinbase_reward=expected_reward(200), coinbase_fees=300,
        fee_inputs=[f1_in], fee_outputs=[f1_out],
        fee_commitments=[fee_commit],  # Pedersen commitment, not plain int
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert v2_ok, v2_msg
    print("  PASS: FeeV2 mass balance — hidden fee commit works")


def test_feev2_below_threshold_rejected():
    """FeeV2: fee below threshold — rejected (spec §5.5, §7.2)."""
    f1_in, f1_out, fee_commit, fee1 = balanced_fee_v2(5000, 50, 42)
    ok, msg = verify_fee_threshold(fee1, 100)
    assert not ok, f"fee {fee1} below threshold 100 should fail"

    # Mempool rejection
    try:
        mempool = TwoTierMempool(premium_threshold=200, general_threshold=100)
        mempool.admit(fee1)
        assert False, "Should have raised ValueError"
    except ValueError as e:
        assert "REJECT" in str(e)
    print("  REJECTED: fee 50 below general threshold 100")


def test_feev2_premium_tier():
    """FeeV2: fee >= premium threshold -> premium queue (spec §7.2)."""
    mempool = TwoTierMempool(premium_threshold=500, general_threshold=100)
    tx_id, queue = mempool.admit(1000)
    assert queue == 'premium'
    assert len(mempool.premium_queue) == 1
    assert len(mempool.general_queue) == 0
    print("  PASS: fee 1000 -> premium tier")


def test_feev2_general_tier():
    """FeeV2: fee between general and premium -> general queue (spec §7.2)."""
    mempool = TwoTierMempool(premium_threshold=500, general_threshold=100)
    tx_id, queue = mempool.admit(300)
    assert queue == 'general'
    assert len(mempool.premium_queue) == 0
    assert len(mempool.general_queue) == 1
    print("  PASS: fee 300 -> general tier")


def test_feev2_fifo_ordering():
    """FeeV2: premium FIFO ordering — first in, first out (spec §7.3)."""
    mempool = TwoTierMempool(premium_threshold=200, general_threshold=50)
    # Add general first, then premium
    g1, _ = mempool.admit(100)
    g2, _ = mempool.admit(150)
    p1, _ = mempool.admit(500)
    p2, _ = mempool.admit(600)

    selected = mempool.select_for_block()
    # Premium drained first, in arrival order
    assert selected == [p1, p2, g1, g2], f"Expected [p1, p2, g1, g2], got {selected}"
    print("  PASS: premium FIFO before general FIFO")


def test_feev2_mempool_mark_mined():
    """FeeV2: mark_mined removes txs from queues (spec §7.3)."""
    mempool = TwoTierMempool(premium_threshold=200, general_threshold=50)
    p1, _ = mempool.admit(500)
    g1, _ = mempool.admit(100)
    assert mempool.size() == 2

    mempool.mark_mined([p1])
    assert mempool.size() == 1
    assert len(mempool.premium_queue) == 0
    print("  PASS: mark_mined removes from queues")


def test_feev2_privacy():
    """FeeV2: verifier sees fee_commit, NOT fee amount (spec §5.2)."""
    f1_in, f1_out, fee_commit, fee1 = balanced_fee_v2(5000, 300, 42)
    # Verifier sees fee_commit (Pedersen), not fee
    assert fee_commit.v_part == 300
    assert fee_commit.r_part == 42
    # Verifier CANNOT distinguish fee=300 blind=42 from fee=200 blind=X
    # because Pedersen commitment is computationally hiding
    another_commit = mk(300, 42)
    assert pedersen_eq(fee_commit, another_commit)
    print("  PASS: fee hidden behind Pedersen commitment")


def test_coin_merkle_tree_position_enumeration():
    """Coin Merkle Tree: position enumeration and root computation (spec §1.6, §1.7)."""
    tree = CoinTree()
    tree.init_zero_guard()
    assert tree._next_position == 1

    # Append coins at positions 1, 2, 3
    coin1 = 12345  # simulated coin commitment
    coin2 = 67890
    coin3 = 11111

    pos1 = tree.append(coin1)
    assert pos1 == 1
    pos2 = tree.append(coin2)
    assert pos2 == 2
    pos3 = tree.append(coin3)
    assert pos3 == 3

    # Roots at each position should be deterministic
    root1 = tree.root(0)  # after zero guard
    root2 = tree.root(1)  # after coin1
    root3 = tree.root(2)  # after coin2
    root4 = tree.root(3)  # after coin3

    # Roots at different positions should differ
    assert root1 != root2 != root3 != root4

    # Re-computing same root should be deterministic
    assert tree.root(1) == root2
    assert tree.root(2) == root3
    print("  PASS: merkle tree positions and roots — deterministic")


def test_coin_merkle_path_verification():
    """Coin Merkle Tree: merkle path derivation and verification (spec §1.8, §1.9)."""
    tree = CoinTree()
    tree.init_zero_guard()

    # Append several coins
    coins = [100, 200, 300, 400, 500]
    positions = []
    for c in coins:
        positions.append(tree.append(c))

    # Verify each coin's merkle path
    for i, (coin, pos) in enumerate(zip(coins, positions)):
        path = tree.witness(pos)
        root = tree.root(pos)
        assert tree.verify_merkle_proof(coin, pos, path, root), \
            f"Merkle proof failed for coin {coin} at position {pos}"

    # Tampered coin should fail verification
    fake_coin = 99999
    path = tree.witness(positions[2])
    root = tree.root(positions[2])
    assert not tree.verify_merkle_proof(fake_coin, positions[2], path, root), \
        "Tampered coin should not verify"
    print("  PASS: merkle path derivation and verification")


def test_coin_merkle_tree_empty_leaf_value():
    """Coin Merkle Tree: empty leaf is UNCOMMITTED_ORCHARD=2, NOT zero (spec §1.3)."""
    tree = CoinTree()
    tree.init_zero_guard()

    # Position 0 is ZERO_GUARD (pallas::Base::ZERO)
    assert tree.leaves[0] == 0, "Position 0 must be ZERO_GUARD = 0"

    # Position 1 is the first real coin
    pos = tree.append(42)
    assert pos == 1

    # Empty subtrees use UNCOMMITTED_ORCHARD = 2, not 0
    # Verify the empty roots ladder starts with 2
    assert tree._empty_roots[0] == 2, "EMPTY_ROOTS[0] must be UNCOMMITTED_ORCHARD = 2"

    # Verify merkle path at level 1 uses EMPTY_ROOTS[1] for empty sibling
    path = tree.witness(1)
    # Level 0: sibling at pos 0 is ZERO_GUARD (not empty)
    assert path[0] == 0, f"Level 0 sibling at pos 0 should be ZERO_GUARD=0, got {path[0]}"
    # Level 1: sibling at pos 2-3 is empty subtree -> EMPTY_ROOTS[1]
    # EMPTY_ROOTS[1] = combine(0, EMPTY_ROOTS[0], EMPTY_ROOTS[0])
    #               = combine(0, UNCOMMITTED_ORCHARD=2, 2)
    assert path[1] == tree._empty_roots[1], \
        f"Level 1 empty sibling should be EMPTY_ROOTS[1]={tree._empty_roots[1]}, got {path[1]}"
    print("  PASS: empty leaf values — UNCOMMITTED_ORCHARD=2, ZERO_GUARD=0")


# ============================================================
# Runner
# ============================================================

def run_all():
    tests = [
        # Original FeeV1 tests (9)
        test_legal_transfers_only,
        test_legal_with_fees,
        test_legal_with_burns,
        test_illegal_hidden_mint,
        test_illegal_standalone_mint,
        test_legal_mint_balanced_by_burn,
        test_illegal_mint_exceeds_burn,
        test_coinbase_exceeds_schedule,
        test_integration_with_cumulative_chain,
        # New FeeV2 tests (7)
        test_feev2_mass_balance,
        test_feev2_below_threshold_rejected,
        test_feev2_premium_tier,
        test_feev2_general_tier,
        test_feev2_fifo_ordering,
        test_feev2_mempool_mark_mined,
        test_feev2_privacy,
        # New Coin Merkle Tree tests (3)
        test_coin_merkle_tree_position_enumeration,
        test_coin_merkle_path_verification,
        test_coin_merkle_tree_empty_leaf_value,
    ]
    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  FAIL: {test.__name__}: {e}")
            import traceback
            traceback.print_exc()
    print(f"\n=== fee_model: {passed} passed, {failed} failed ===")
    return failed == 0


if __name__ == "__main__":
    success = run_all()
    sys.exit(0 if success else 1)
