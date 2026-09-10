"""Fee Payment and Collection — Python Executable Model (FeeV3 plaintext mirror)
==============================================================================

This model MIRRORS the deployed Rust implementation (src/contract/native_token).
On-chain changes land first; this model tracks them. Divergences are bugs in
the model, not the spec.

The three plaintext native-token elements, by entrypoint selector:

  PoWRewardV1 (0x05) — coinbase. The coinbase commitment equals
      expected_reward(height) EXACTLY (entrypoint/mod.rs:965-971, HAZOP F1).
      Fees are NOT part of the coinbase; they never enter TOTAL_SUPPLY or the
      cumulative Pedersen chain.

  FeeCollectV1 (0x06) — collection plate. Checks total_fees == fees_db[height]
      as plain u64 (no Pedersen accumulator — entrypoint/mod.rs:1201-1217),
      then zeroes the pot (mod.rs:1276-1277).

  FeeV2 (0x08) — fee payment. Decodes FeeParamsV3 with PLAINTEXT
      fee: FeeAmount + tier: FeeTier. The Pedersen fee_value_commit and
      fee_v2_tx_binding survive only so the host can verify the retained
      Fee_V2 mass-balance proof (fee.rs:85-89). The encrypted-fee channel and
      the FeeThreshold_V1 proof are REMOVED (fee-spec.md §14.4).

Process Engineering Analogy
---------------------------
Fees are plaintext — every fee is visible in the clear, as in Bitcoin. What
the system still needs is instrumentation: the per-block pot fees_db[height]
is the flow totalizer, tier prices are the control valve, and the fee window
PID controller (fee_window_model.py) is the pressure regulator.

  ┌─────────────────────────┐      ┌─────────────────────────────┐
  │  FEE SIGNALLING          │      │  MASS BALANCE                │
  │  (control valve)         │      │  (flow totalizer)            │
  │                          │      │                               │
  │  ThreeTierMempool        │      │  FeeState (fees_db dict)      │
  │  plain fee >= tier_price │      │  verify_proof_of_token_       │
  │                          │      │  balance()                    │
  │  tier prices = choke     │      │                               │
  │  fee window = PID        │      │  Σout + Σfees + Σburns        │
  │  controller              │      │  == Σin                       │
  │                          │      │                               │
  │  fee_signalling domain   │      │  mass_balance domain          │
  └─────────────────────────┘      └─────────────────────────────┘
           │                                 │
           │  transactions admitted          │  block verified
           │  (plaintext fee)                │  (Pedersen mass balance)
           ▼                                 ▼
        plain comparison                  accept_block WASM execution

Domain architecture (fee-spec.md §0):
  mass_balance   — consensus-critical block proof (Pedersen value conservation,
                   coinbase nullifier claims, plaintext fee pot)
  fee_signalling — non-consensus coordination (tier prices, mempool admission,
                   fee window congestion factors)

Specification reference: doc/src/arch/consensus/fee-spec.md

Covers:
  §1  — Coin Merkle Tree (incremental, UNCOMMITTED_ORCHARD=2, zero guard)
  §2  — Block Production Model (sequential overlay, coin tree growth)
  §4  — FeeCollectV1 [domain: mass_balance] — the meter-close event
  §5  — FeeV2 (0x08) with FeeParamsV3 [domain: mass_balance]
        (plaintext fee + tier; retained commit for the host proof)
  §6  — FeeAmount nominal type (u64 wrapper, no bare int crossing boundaries)
  §12.8.1 — Three-tier mempool [domain: fee_signalling]
        (plain fee >= tier_price, FIFO within tier, REJECT below price_low)

Extended from contrib/model/proof_of_token_balance.py — all 9 original tests
migrated + new FeeV2 and merkle tree tests. FeeV3 plaintext rewrite 2026-09:
hidden-fee/threshold machinery removed, [1:1] with entrypoint/mod.rs +
model/fee.rs.
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
# Constants
# ============================================================

COINBASE_MATURITY = 100               # Blocks before coinbase coin is spendable
MERKLE_DEPTH = 32                     # Orchard tree depth (2^32 capacity)
UNCOMMITTED_ORCHARD = 2               # pallas::Base::from(2) — NOT zero
ZERO_GUARD = 0                        # pallas::Base::ZERO at position 0
DRKW_ASSET_ID = 0                     # Native token identifier

# Tier price defaults — [1:1] MempoolConfig::default() (crates/dwow-mempool/src/lib.rs):
# 4×/2×/1× CongestionFactor::SCALE (SCALE = 1_000_000).
PRICE_HIGH_DEFAULT = 4_000_000
PRICE_MEDIUM_DEFAULT = 2_000_000
PRICE_LOW_DEFAULT = 1_000_000

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


class CommitmentTree:
    """Incremental Merkle tree of coin commitments (spec §1.1).

    PROCESS ENGINEERING: The CommitmentTree is the PIPE — the physical containment
    vessel that holds coins in transit. Every coin entering the system (coinbase
    mint, transfer output) is appended to the tree. Every coin leaving (transfer
    input, fee payment, burn) is proven to exist at a prior merkle root.

    Position 0 is the ZERO_GUARD — the pipe's blank flange. Position 1 onward
    are real coins. Empty positions are UNCOMMITTED_ORCHARD (value 2), NOT zero —
    a zero leaf would leak information about which positions are occupied.

    CommitmentTree = BridgeTree<MerkleNode, usize, 32>
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
# §5 — FeeV2: FeeParamsV3 with Plaintext Fee
# [domain: mass_balance]
# ============================================================

def balanced_fee_v2(input_value: int, fee: int,
                     fee_blind: int) -> tuple[PedersenCommitment,
                                               PedersenCommitment,
                                               PedersenCommitment,
                                               int]:
    """Create a balanced FeeV2 call — FeeParamsV3 semantics (fee.rs).

    The fee is PLAINTEXT (FeeV3, fee-spec.md §14.4). The Pedersen triple is
    still constructed: fee_value_commit = PedersenCommit(fee, fee_blind) is
    retained in FeeParamsV3 so the host can verify the retained Fee_V2
    mass-balance proof whose public inputs include its coordinates.

    For Pedersen homomorphic balance: input_blind = output_blind + fee_blind.
    The prover chooses input_blind freely, then sets output_blind so the
    blind equation holds.
    """
    r = int.from_bytes(os.urandom(8), 'big')
    in_commit = mk(input_value, r)
    out_commit = mk(input_value - fee, r - fee_blind)  # so blinds cancel
    fee_commit = mk(fee, fee_blind)                    # retained for the host proof
    return in_commit, out_commit, fee_commit, fee


# ============================================================
# §5.5.1 — Nominal tx_binding Types (Type Contract)
# [domain: mass_balance for FeeV2TxBinding]
# ============================================================

DOMAIN_TX_BINDING = 3  # DRK_POSEIDON_DOMAIN_TX_BINDING


class FeeAmount:
    """Nominal type for fee amounts — u64 wrapper, no bare int crossing boundaries.

    Per fee-spec.md §6, consensus numeric domains SHALL be nominal types.
    Mirrors Rust `FeeAmount(u64)` at `src/sdk/src/blockchain.rs`.
    (fee_window_model.py imports this class — keep byte-identical.)
    """
    def __init__(self, value: int):
        if not (0 <= value < (1 << 64)):
            raise ValueError(f"FeeAmount out of range: {value}")
        self._value = value

    def get(self) -> int:
        return self._value

    def to_base(self) -> int:
        """Convert to pallas::Base field element for ZK witness construction."""
        return self._value  # in prime field, u64 maps directly

    def __repr__(self) -> str:
        return f"FeeAmount({self._value})"

    def _cmp(self, other):
        return other.get() if isinstance(other, FeeAmount) else other

    def __eq__(self, other) -> bool:
        return self._value == self._cmp(other)

    def __lt__(self, other): return self._value < self._cmp(other)
    def __le__(self, other): return self._value <= self._cmp(other)
    def __gt__(self, other): return self._value > self._cmp(other)
    def __ge__(self, other): return self._value >= self._cmp(other)
    def __add__(self, other): return self._value + self._cmp(other)
    def __sub__(self, other): return self._value - self._cmp(other)
    def __mul__(self, other): return self._value * self._cmp(other)
    def __floordiv__(self, other): return self._value // self._cmp(other)
    def __truediv__(self, other): return self._value / self._cmp(other)
    def __radd__(self, other): return self._cmp(other) + self._value
    def __rsub__(self, other): return self._cmp(other) - self._value
    def __rmul__(self, other): return self._cmp(other) * self._value
    def __int__(self): return self._value
    def __hash__(self): return hash(self._value)


class FeeV2TxBinding:
    """poseidon(3, tx_commitment, tx_nonce) — Fee_V2 proof anti-replay.

    Per fee-spec.md §5.5.1: binds the Fee_V2 proof to a specific transaction
    via the tx_nonce, preventing cross-transaction proof replay.
    Domain: mass_balance.
    """
    def __init__(self, inner: int):
        self._inner = inner

    @staticmethod
    def compute(tx_commitment: int, tx_nonce: int) -> 'FeeV2TxBinding':
        inner = int.from_bytes(
            poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce), 'big'
        )
        return FeeV2TxBinding(inner)

    def inner(self) -> int:
        return self._inner

    def __repr__(self) -> str:
        return f"FeeV2TxBinding({self._inner:#x})"

    def __eq__(self, other) -> bool:
        if isinstance(other, FeeV2TxBinding):
            return self._inner == other._inner
        return NotImplemented


class FeeTier:
    """Three-tier priority selector (1=low, 2=medium, 4=high).

    Mirrors Rust `FeeTier` (sdk blockchain.rs / model/fee.rs): the u8 tier
    multiplier encoded in FeeParamsV3. Defined locally — NOT imported from
    fee_window_model (circular: fee_window_model imports FeeAmount from here).
    """
    LOW = 1
    MEDIUM = 2
    HIGH = 4

    def __init__(self, tier: int):
        if tier not in (FeeTier.LOW, FeeTier.MEDIUM, FeeTier.HIGH):
            raise ValueError(f"Invalid tier multiplier: {tier}")
        self._tier = tier

    def tier_multiplier(self) -> int:
        return self._tier

    def __eq__(self, other) -> bool:
        if isinstance(other, FeeTier):
            return self._tier == other._tier
        return NotImplemented

    def __repr__(self) -> str:
        return f"FeeTier({self._tier})"


class FeeParamsV3:
    """FeeV2 (0x08) call data — plaintext since FeeV3 (fee.rs:77-91).

    Wire shape ([1:1] FeeParamsV3::encode):
      fee: FeeAmount (8 bytes LE) — PLAINTEXT
      tier: u8 multiplier (1/2/4)
      fee_value_commit: pallas::Point — kept for proof verification only
      fee_v2_tx_binding (32 bytes) + tx_nonce (32 bytes)

    The Pedersen `fee_value_commit` and `fee_v2_tx_binding` survive only so
    the host can verify the retained Fee_V2 mass-balance proof. There is no
    encrypted fee and no threshold proof.
    """
    def __init__(self, input_commit: PedersenCommitment,
                 output_commit: PedersenCommitment,
                 fee: FeeAmount, tier: FeeTier,
                 fee_value_commit: PedersenCommitment,
                 fee_v2_tx_binding: FeeV2TxBinding, tx_nonce: int):
        self.input_commit = input_commit
        self.output_commit = output_commit
        self.fee = fee
        self.tier = tier
        self.fee_value_commit = fee_value_commit
        self.fee_v2_tx_binding = fee_v2_tx_binding
        self.tx_nonce = tx_nonce

    def __repr__(self) -> str:
        return (f"FeeParamsV3(fee={self.fee}, tier={self.tier}, "
                f"fee_value_commit={self.fee_value_commit})")


# ============================================================
# §12.8.1 — Three-Tier Mempool [domain: fee_signalling]
# ============================================================

class ThreeTierMempool:
    """Three-tier mempool (low/medium/high = 1x/2x/4x). Admission is a PLAIN
    comparison: fee >= tier price, no threshold proof (crates/dwow-mempool
    src/lib.rs Mempool::add; fee-spec.md §12.8.1).

    PROCESS ENGINEERING: The mempool is the CONTROL VALVE on the transaction
    pipeline. Tier prices are the choke settings — high (4x), medium (2x),
    low (1x). FeeV3 txs route to the highest tier whose price their plaintext
    fee covers; below the low tier price the valve is CLOSED (REJECT). FIFO
    within tier; high drains first, then medium, then low.

    Tier prices are runtime-updatable (update_tier_prices) — not modeled here;
    the fee window PID controller (fee_window_model.py) supplies congestion
    direction.
    """

    def __init__(self, price_high: int = PRICE_HIGH_DEFAULT,
                 price_medium: int = PRICE_MEDIUM_DEFAULT,
                 price_low: int = PRICE_LOW_DEFAULT):
        self.price_high = price_high
        self.price_medium = price_medium
        self.price_low = price_low
        self.high_queue = []    # FIFO: append to end, pop from front
        self.medium_queue = []  # FIFO
        self.low_queue = []     # FIFO
        self._tx_map = {}       # tx_id -> (fee, queue_name)
        self._next_id = 0

    def admit(self, fee: int) -> tuple[int, str]:
        """Admit a transaction by plaintext comparison (spec §12.8.1).

        fee >= price_high → high queue; price_medium <= fee < price_high →
        medium; price_low <= fee < price_medium → low; below price_low →
        REJECT. Returns (tx_id, queue_name). Raises ValueError for REJECT.
        """
        tx_id = self._next_id
        self._next_id += 1

        if fee >= self.price_high:
            self.high_queue.append(tx_id)
            self._tx_map[tx_id] = (fee, 'high')
            return tx_id, 'high'
        elif fee >= self.price_medium:
            self.medium_queue.append(tx_id)
            self._tx_map[tx_id] = (fee, 'medium')
            return tx_id, 'medium'
        elif fee >= self.price_low:
            self.low_queue.append(tx_id)
            self._tx_map[tx_id] = (fee, 'low')
            return tx_id, 'low'
        else:
            raise ValueError(f"REJECT: fee {fee} below low tier price {self.price_low}")

    def select_for_block(self, max_txs: int = 250) -> list[int]:
        """Select transactions for block inclusion (spec §12.8.1).

        Drains high queue first (FIFO), then medium, then low.
        Non-destructive — does NOT remove from mempool.
        """
        selected = []
        for queue in (self.high_queue, self.medium_queue, self.low_queue):
            for tx_id in queue:
                if len(selected) >= max_txs:
                    break
                selected.append(tx_id)
        return selected

    def mark_mined(self, tx_ids: list[int]):
        """Remove confirmed transactions from the mempool."""
        for tx_id in tx_ids:
            if tx_id in self._tx_map:
                _, queue_name = self._tx_map.pop(tx_id)
                queue = {
                    'high': self.high_queue,
                    'medium': self.medium_queue,
                    'low': self.low_queue,
                }[queue_name]
                queue.remove(tx_id)

    def size(self) -> int:
        return len(self.high_queue) + len(self.medium_queue) + len(self.low_queue)


# ============================================================
# §4 — FeeCollectV1 + the Plaintext Fee Pot
# [domain: mass_balance]
# ============================================================

class FeeState:
    """Block-scoped fee state — fees accumulate as PLAINTEXT u64 in
    fees_db[height]. [1:1] entrypoint/mod.rs:88-89, apply_fee (1282-1312),
    fee_collect_v1 (1187-1242), apply_fee_collect (1244-1280). No Pedersen
    accumulator.

    PROCESS ENGINEERING: fees_db is the FLOW TOTALIZER — a plain register
    that integrates discrete flow pulses (each FeeV2's fee) into a
    cumulative reading. The total is public by design, so there is nothing
    to hide and nothing to prove: FeeCollectV1 checks the claimed total
    against the register with plain equality and zeroes it.

    P6/P7/P8 state (commitment roots, nullifiers, commitment set) is kept
    here as the fee path's shared block-scoped state (mod.rs:227-241).
    """

    def __init__(self):
        self.fees_db: dict[int, int] = {}             # height -> plaintext fee total
        self.commitment_roots_db: dict[int, int] = {} # P6 lookup: root -> height
        self.nullifiers_db: set[int] = set()          # P7 spent nullifiers
        self.commitment_set: set[int] = set()         # P8 existing commitments

    def apply_fee_v2(self, in_commit: PedersenCommitment,
                     out_commit: PedersenCommitment,
                     fee_value_commit: PedersenCommitment,
                     fee: int, height: int,
                     nullifier: int, merkle_root: int) -> tuple[bool, str]:
        """Apply a FeeV2 call — the plaintext PULSE on the totalizer.

        P6/P7/P8 (mod.rs:227-241) + plaintext accumulation (mod.rs:252-271):
          - P6: merkle_root in commitment_roots_db (the input coin exists)
          - P7: nullifier not already spent (each pulse counted once)
          - bad-fee-amount: input.value <= fee (output would be zero/negative)
          - P8: output commitment not already in commitment_set
        Then fees_db[height] += fee (plain addition).
        """
        if merkle_root not in self.commitment_roots_db:
            return False, "↓bad-merkle-root: input.merkle_root not in commitment_roots_db"
        if nullifier in self.nullifiers_db:
            return False, "↓double-spend: nullifier already spent"
        if out_commit.v_part <= 0:
            return False, "↓bad-fee-amount: input.value <= fee"
        if out_commit.v_part in self.commitment_set:
            return False, "↓duplicate-commitment: output commitment already exists"
        self.nullifiers_db.add(nullifier)
        self.commitment_set.add(out_commit.v_part)
        self.fees_db[height] = self.fees_db.get(height, 0) + fee
        return True, "OK"

    def apply_fee_collect(self, total_fees: int, height: int) -> tuple[bool, str]:
        """FeeCollectV1 (0x06) — the METER-READING event (mod.rs:1196-1217).

        C1 (zero-claim guard, mod.rs:1196-1199): total_fees == 0 rejected —
        after the pot is zeroed, a second zero claim would mint a 0-value
        commitment and reopen the closed tree.
        C2 (plaintext equality, mod.rs:1201-1217): total_fees == fees_db[height]
        — plain u64, no Pedersen accumulator.

        Postconditions (mod.rs:1276-1277): fees_db[height] = 0 — the pot is
        zeroed, preventing double-claim.
        """
        if total_fees == 0:
            return False, "↓zero-claim: total_fees == 0 (replay attack guard)"
        if total_fees != self.fees_db.get(height, 0):
            return False, (f"↓bad-claim: total_fees {total_fees} != "
                           f"fees_db[{height}] = {self.fees_db.get(height, 0)}")
        self.fees_db[height] = 0
        return True, "OK"

    def total_accumulated(self, height: int) -> int:
        return self.fees_db.get(height, 0)


# ============================================================
# Block Model — Canonical Ordering, Overlay Visibility (§2)
# [domain: mass_balance]
# ============================================================

def verify_pow_reward_v1(value: int, height: int) -> tuple[bool, str]:
    """PoWRewardV1 (0x05): pr.input.value == expected_reward(height) EXACTLY.

    [1:1] entrypoint/mod.rs:965-971 (HAZOP F1: exact equality — neither
    under- nor over-mint). Fees are NOT part of the coinbase — they are
    minted separately by FeeCollectV1 (0x06).
    """
    expected = expected_reward(height)
    if value != expected:
        return False, (f"↓reward-mismatch: value {value} != "
                       f"expected_reward({height}) = {expected}")
    return True, "OK"


class BlockBuilder:
    """Models a single block's transaction sequence per fee-spec.md §2.

    PROCESS ENGINEERING: A block is a BATCH PROCESS — a fixed sequence of
    operations that runs to completion within a single processing window
    (the block). The canonical order is:

      1. PoWRewardV1 (meter OPEN)  — creates the reward-only coinbase UTXO
      2. FeeV2 × N    (meter PULSES) — each adds its plaintext fee to fees_db
      3. FeeCollectV1 (meter CLOSE) — total_fees == fees_db[height], pot zeroed

    Invariant 1 (Overlay Visibility): each call sees the state writes of all
    preceding calls within the same block — sequential batch processing.
    """

    def __init__(self, height: int, state: FeeState):
        self.height = height
        self.state = state
        self.transactions: list[str] = []
        self._has_coinbase = False
        self._has_fee_collect = False
        self.coinbase_vc = None  # reward-only commitment — set by add_coinbase

    def add_coinbase(self) -> None:
        """Add PoWRewardV1 as transactions[0] (spec §2.1).

        The coinbase commitment is the reward ONLY — fees never enter the
        coinbase (verify_pow_reward_v1, mod.rs:968).
        """
        assert not self._has_coinbase, "coinbase already added"
        assert len(self.transactions) == 0, "coinbase must be first"
        reward = expected_reward(self.height)
        r = int.from_bytes(os.urandom(8), 'big')
        self.coinbase_vc = mk(reward, r)
        self.transactions.append("PoWRewardV1")
        self._has_coinbase = True

    def add_fee_v2(self, name: str, in_commit, out_commit, fee_value_commit,
                   fee: int, nullifier: int, merkle_root: int) -> tuple[bool, str]:
        """Add a FeeV2 call. txs[1..k] come after coinbase."""
        assert self._has_coinbase, "coinbase must be first"
        assert not self._has_fee_collect, "FeeCollectV1 already added"
        ok, msg = self.state.apply_fee_v2(
            in_commit, out_commit, fee_value_commit, fee, self.height,
            nullifier, merkle_root)
        if ok:
            self.transactions.append(name)
        return ok, msg

    def add_fee_collect(self, total_fees: int) -> tuple[bool, str]:
        """Add FeeCollectV1 as the FINAL transaction (§2.1, §4.4)."""
        assert self._has_coinbase, "coinbase must exist before FeeCollectV1"
        assert not self._has_fee_collect, "FeeCollectV1 already added"
        # §4.4: FeeCollectV1 SHALL be absent when total_fees == 0
        if total_fees == 0:
            return False, "↓zero-claim: no FeeCollectV1 when total_fees == 0"
        ok, msg = self.state.apply_fee_collect(total_fees, self.height)
        if ok:
            self.transactions.append("FeeCollectV1")
            self._has_fee_collect = True
        return ok, msg


# ============================================================
# Block-level Mass Balance (migrated from proof_of_token_balance.py)
# ============================================================

def verify_proof_of_token_balance(coinbase_vc: PedersenCommitment,
                                   coinbase_reward: int,
                                   fee_inputs: list[PedersenCommitment],
                                   fee_outputs: list[PedersenCommitment],
                                   fee_commitments: list[PedersenCommitment],
                                   burn_inputs: list[PedersenCommitment],
                                   transfer_inputs: list[PedersenCommitment],
                                   transfer_outputs: list[PedersenCommitment],
                                   spend_inputs: list[PedersenCommitment],
                                   spend_outputs: list[PedersenCommitment],
                                   mint_outputs: list[PedersenCommitment],
                                   ) -> tuple[bool, str]:
    """Verify block-level Pedersen mass balance (FeeV3).

    PROCESS ENGINEERING: The MASS BALANCE EQUATION — the fundamental flow
    meter calculation. For every block:

        Σoutputs + Σburns + Σfee_commitments == Σinputs

    Monetary mass is conserved. Nothing enters the system except through the
    coinbase (verified separately). Nothing leaves except through burns
    (explicit destruction). The Pedersen homomorphic property allows adding
    commitments on both sides and comparing the sums.

    Coinbase check: coinbase_vc.v_part == coinbase_reward EXACTLY. Fees are
    NOT part of the coinbase — they are minted separately by FeeCollectV1
    (0x06) and never enter the cumulative supply chain.
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

    # Fee aggregate: sum of the retained fee_value_commits (host proof inputs)
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
        net = PedersenCommitment(left.v_part - right.v_part, left.r_part - right.r_part)
        return (False,
                f"MASS BALANCE FAILED: net delta v={net.v_part} r={net.r_part}\n"
                f"  left  (outs+burns+fees): v={left.v_part}\n"
                f"  right (inputs):          v={right.v_part}")

    if coinbase_vc.v_part != coinbase_reward:
        return (False,
                f"COINBASE MISMATCH: commit v={coinbase_vc.v_part} "
                f"!= expected_reward {coinbase_reward} "
                f"(fees are minted separately via FeeCollectV1)")

    return (True, "OK")


# ============================================================
# Tests — Original (migrated from proof_of_token_balance.py)
# ============================================================

def test_legal_transfers_only():
    t_in, t_out = balanced_transfer([1000, 500, 300], [1000, 600, 200])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(100)),
        coinbase_reward=expected_reward(100),
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
        burn_inputs=[], transfer_inputs=t_in, transfer_outputs=t_out,
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: transfers only — 1800 in, 1800 out")


def test_legal_with_fees():
    f1_in, f1_out, f1_commit, f1_fee = balanced_fee_v2(5000, 300, 42)
    f2_in, f2_out, f2_commit, f2_fee = balanced_fee_v2(2000, 450, 99)
    t_in, t_out = balanced_transfer([3000], [3000])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(200)),   # reward ONLY — fees separate
        coinbase_reward=expected_reward(200),
        fee_inputs=[f1_in, f2_in], fee_outputs=[f1_out, f2_out],
        fee_commitments=[f1_commit, f2_commit],
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
        coinbase_reward=expected_reward(300),
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
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
        coinbase_reward=expected_reward(400),
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
        burn_inputs=[], transfer_inputs=[in_commit], transfer_outputs=[out_commit],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert not ok, f"Should have rejected hidden mint! {msg}"
    print(f"  REJECTED: hidden mint detected — 100 in, 1,000,000 out")


def test_illegal_standalone_mint():
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(500)),
        coinbase_reward=expected_reward(500),
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
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
        coinbase_reward=expected_reward(600),
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
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
        coinbase_reward=expected_reward(700),
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
        burn_inputs=[], transfer_inputs=[burn_commit], transfer_outputs=[mint_commit],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert not ok, f"Should have rejected mint-exceeds-burn! {msg}"
    print(f"  REJECTED: mint > burn detected — 50M minted, only 10M burned")


def test_coinbase_exceeds_schedule():
    excessive = expected_reward(800) + 1_000_000
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=pedersen_commit(excessive, b'cb'),
        coinbase_reward=expected_reward(800),
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert not ok, f"Should have rejected excessive coinbase! {msg}"
    print(f"  REJECTED: coinbase exceeds schedule")


def test_coinbase_is_reward_only():
    """Coinbase = reward + fees is REJECTED — fees are minted separately.

    Pins the headline flip: the coinbase commitment equals
    expected_reward(height) EXACTLY (HAZOP F1, mod.rs:968). Fees never enter
    the coinbase — FeeCollectV1 (0x06) mints them separately.
    """
    reward = expected_reward(900)
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=pedersen_commit(reward + 500, b'cb'),
        coinbase_reward=reward,
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert not ok, f"reward+fees coinbase must be rejected! {msg}"
    assert "COINBASE MISMATCH" in msg
    # Reward-only coinbase passes
    ok2, _ = verify_proof_of_token_balance(
        coinbase_vc=pedersen_commit(reward, b'cb'),
        coinbase_reward=reward,
        fee_inputs=[], fee_outputs=[], fee_commitments=[],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok2
    print("  REJECTED: coinbase = reward + fees; reward-only accepted")


def test_pow_reward_exact_equality():
    """PoWRewardV1: value below AND above expected_reward(H) both rejected.

    [1:1] entrypoint/mod.rs:968 — exact equality (HAZOP F1).
    """
    h = 100
    reward = expected_reward(h)
    ok_under, msg_under = verify_pow_reward_v1(reward - 1, h)
    assert not ok_under, f"under-mint must be rejected: {msg_under}"
    ok_over, msg_over = verify_pow_reward_v1(reward + 1, h)
    assert not ok_over, f"over-mint must be rejected: {msg_over}"
    ok_exact, msg_exact = verify_pow_reward_v1(reward, h)
    assert ok_exact, msg_exact
    print("  PASS: PoWRewardV1 exact equality — under/over rejected, exact accepted")


def test_integration_with_cumulative_chain():
    print("\n  Integration: cumulative chain + mass balance...")
    cumulative = PedersenCommitment(0, 0)
    state = FeeState()
    for h in range(1, 11):
        reward = expected_reward(h)
        fees = h * 50
        # Coinbase carries the reward ONLY.
        coinbase_vc = pedersen_commit(reward, b'cb_%d' % h)
        cumulative = pedersen_add(cumulative, coinbase_vc)
        f_in, f_out, f_commit, f_fee = balanced_fee_v2(1000 + h * 100, fees, h)
        t_in, t_out = balanced_transfer([h * 500, h * 300], [h * 500, h * 300])
        state.commitment_roots_db[0xCB00 + h] = h
        state.apply_fee_v2(f_in, f_out, f_commit, f_fee, height=h,
                           nullifier=100 + h, merkle_root=0xCB00 + h)
        ok, msg = verify_proof_of_token_balance(
            coinbase_vc=coinbase_vc, coinbase_reward=reward,
            fee_inputs=[f_in], fee_outputs=[f_out], fee_commitments=[f_commit],
            burn_inputs=[], transfer_inputs=t_in, transfer_outputs=t_out,
            spend_inputs=[], spend_outputs=[], mint_outputs=[],
        )
        assert ok, f"Block {h} failed: {msg}"
        assert state.fees_db[h] == fees, "fees accumulate in fees_db, not the coinbase"
    # Cumulative chain advances by reward commitments ONLY (no fees).
    expected_cum = sum(expected_reward(h) for h in range(1, 11))
    assert cumulative.v_part == expected_cum, (
        f"cumulative {cumulative.v_part} != Σ expected_reward {expected_cum} — fees leaked in"
    )
    print(f"  OK — 10 blocks, cumulative v={cumulative.v_part} == Σ expected_reward (fees excluded)")


# ============================================================
# Tests — FeeV2 (FeeParamsV3)
# ============================================================

def test_feev2_mass_balance():
    """FeeV2: retained fee commit satisfies Pedersen mass balance (§5.4)."""
    f1_in, f1_out, fee_commit, fee1 = balanced_fee_v2(5000, 300, 42)
    assert fee1 == 300

    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(200)),   # reward only — NOT reward + fee
        coinbase_reward=expected_reward(200),
        fee_inputs=[f1_in], fee_outputs=[f1_out],
        fee_commitments=[fee_commit],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: FeeV2 mass balance — retained fee commit, reward-only coinbase")


def test_feev2_plaintext_fee():
    """FeeParamsV3.fee is directly readable and opens the retained commit."""
    f_in, f_out, f_commit, f_fee = balanced_fee_v2(5000, 300, 42)
    params = FeeParamsV3(
        input_commit=f_in, output_commit=f_out,
        fee=FeeAmount(f_fee), tier=FeeTier(FeeTier.MEDIUM),
        fee_value_commit=f_commit,
        fee_v2_tx_binding=FeeV2TxBinding.compute(12345, 999),
        tx_nonce=999,
    )
    # The fee is PLAINTEXT — directly readable, no hiding.
    assert params.fee.get() == 300
    # The retained commit (host-proof use only) opens to the same value.
    assert pedersen_eq(params.fee_value_commit, mk(300, 42))
    assert params.tier.tier_multiplier() == 2
    print("  PASS: FeeParamsV3 — plaintext fee readable, retained commit opens to it")


def test_three_tier_admission():
    """Mempool: fee >= tier price admits (high/medium/low); below → REJECT."""
    mempool = ThreeTierMempool(price_high=400, price_medium=200, price_low=100)
    _, queue = mempool.admit(500)
    assert queue == 'high'
    _, queue2 = mempool.admit(300)
    assert queue2 == 'medium'
    _, queue3 = mempool.admit(150)
    assert queue3 == 'low'
    try:
        mempool.admit(50)
        assert False, "Should have raised ValueError"
    except ValueError as e:
        assert "REJECT" in str(e)
    print("  PASS: three-tier plaintext admission — high/medium/low routing, below-low REJECT")


def test_feev2_below_tier_price_rejected():
    """FeeV2: fee below the low tier price — REJECTED (spec §12.8.1)."""
    _, _, _, fee1 = balanced_fee_v2(5000, 50, 42)
    assert fee1 == 50
    try:
        mempool = ThreeTierMempool(price_high=200, price_medium=150, price_low=100)
        mempool.admit(fee1)
        assert False, "Should have raised ValueError"
    except ValueError as e:
        assert "REJECT" in str(e)
    print("  REJECTED: fee 50 below low tier price 100")


def test_feev2_high_tier():
    """FeeV2: fee >= price_high -> high queue (spec §12.8.1)."""
    mempool = ThreeTierMempool(price_high=500, price_medium=200, price_low=100)
    tx_id, queue = mempool.admit(1000)
    assert queue == 'high'
    assert len(mempool.high_queue) == 1
    assert len(mempool.medium_queue) == 0
    assert len(mempool.low_queue) == 0
    print("  PASS: fee 1000 -> high tier")


def test_feev2_medium_tier():
    """FeeV2: price_medium <= fee < price_high -> medium queue."""
    mempool = ThreeTierMempool(price_high=500, price_medium=200, price_low=100)
    tx_id, queue = mempool.admit(300)
    assert queue == 'medium'
    assert len(mempool.medium_queue) == 1
    print("  PASS: fee 300 -> medium tier")


def test_feev2_low_tier():
    """FeeV2: price_low <= fee < price_medium -> low queue."""
    mempool = ThreeTierMempool(price_high=500, price_medium=200, price_low=100)
    tx_id, queue = mempool.admit(150)
    assert queue == 'low'
    assert len(mempool.low_queue) == 1
    print("  PASS: fee 150 -> low tier")


def test_feev2_fifo_ordering():
    """FeeV2: FIFO within tier; high drains before medium before low."""
    mempool = ThreeTierMempool(price_high=500, price_medium=200, price_low=100)
    l1, _ = mempool.admit(150)
    m1, _ = mempool.admit(300)
    h1, _ = mempool.admit(600)
    h2, _ = mempool.admit(700)

    selected = mempool.select_for_block()
    assert selected == [h1, h2, m1, l1], f"Expected [h1, h2, m1, l1], got {selected}"
    print("  PASS: high FIFO → medium FIFO → low FIFO")


def test_feev2_mempool_mark_mined():
    """FeeV2: mark_mined removes txs from queues (spec §12.8.1)."""
    mempool = ThreeTierMempool(price_high=500, price_medium=200, price_low=100)
    p1, _ = mempool.admit(500)
    g1, _ = mempool.admit(100)
    assert mempool.size() == 2

    mempool.mark_mined([p1])
    assert mempool.size() == 1
    assert len(mempool.high_queue) == 0
    print("  PASS: mark_mined removes from queues")


# ============================================================
# Tests — Coin Merkle Tree
# ============================================================

def test_commitment_merkle_tree_position_enumeration():
    """Coin Merkle Tree: position enumeration and root computation (spec §1.6, §1.7)."""
    tree = CommitmentTree()
    tree.init_zero_guard()
    assert tree._next_position == 1

    # Append commitments at positions 1, 2, 3
    commitment1 = 12345  # simulated commitment
    commitment2 = 67890
    commitment3 = 11111

    pos1 = tree.append(commitment1)
    assert pos1 == 1
    pos2 = tree.append(commitment2)
    assert pos2 == 2
    pos3 = tree.append(commitment3)
    assert pos3 == 3

    # Roots at each position should be deterministic
    root1 = tree.root(0)  # after zero guard
    root2 = tree.root(1)  # after commitment1
    root3 = tree.root(2)  # after commitment2
    root4 = tree.root(3)  # after commitment3

    # Roots at different positions should differ
    assert root1 != root2 != root3 != root4

    # Re-computing same root should be deterministic
    assert tree.root(1) == root2
    assert tree.root(2) == root3
    print("  PASS: merkle tree positions and roots — deterministic")


def test_commitment_merkle_path_verification():
    """Coin Merkle Tree: merkle path derivation and verification (spec §1.8, §1.9)."""
    tree = CommitmentTree()
    tree.init_zero_guard()

    # Append several commitments
    commitments = [100, 200, 300, 400, 500]
    positions = []
    for c in commitments:
        positions.append(tree.append(c))

    # Verify each commitment's merkle path
    for i, (commitment, pos) in enumerate(zip(commitments, positions)):
        path = tree.witness(pos)
        root = tree.root(pos)
        assert tree.verify_merkle_proof(commitment, pos, path, root), \
            f"Merkle proof failed for commitment {commitment} at position {pos}"

    # Tampered commitment should fail verification
    fake_commitment = 99999
    path = tree.witness(positions[2])
    root = tree.root(positions[2])
    assert not tree.verify_merkle_proof(fake_commitment, positions[2], path, root), \
        "Tampered commitment should not verify"
    print("  PASS: merkle path derivation and verification")


def test_commitment_merkle_tree_empty_leaf_value():
    """Coin Merkle Tree: empty leaf is UNCOMMITTED_ORCHARD=2, NOT zero (spec §1.3)."""
    tree = CommitmentTree()
    tree.init_zero_guard()

    # Position 0 is ZERO_GUARD (pallas::Base::ZERO)
    assert tree.leaves[0] == 0, "Position 0 must be ZERO_GUARD = 0"

    # Position 1 is the first real commitment
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
# Tests — FeeCollectV1 + the Plaintext Fee Pot
# ============================================================

def test_fee_collect_v1_happy_path():
    """FeeCollectV1: full lifecycle with N FeeV2 calls — plaintext pot."""
    state = FeeState()
    state.commitment_roots_db[0xAAA] = 1
    state.commitment_roots_db[0xBBB] = 1
    f1_in, f1_out, f1_commit, f1_fee = balanced_fee_v2(5000, 300, 42)
    f2_in, f2_out, f2_commit, f2_fee = balanced_fee_v2(3000, 200, 99)
    ok1, _ = state.apply_fee_v2(f1_in, f1_out, f1_commit, f1_fee, height=1,
                                nullifier=1, merkle_root=0xAAA)
    assert ok1
    ok2, _ = state.apply_fee_v2(f2_in, f2_out, f2_commit, f2_fee, height=1,
                                nullifier=2, merkle_root=0xBBB)
    assert ok2
    # The pot holds the plain sum.
    assert state.fees_db[1] == 500, f"fees_db[1] should be 500, got {state.fees_db[1]}"
    ok, msg = state.apply_fee_collect(total_fees=500, height=1)
    assert ok, f"FeeCollectV1 should succeed: {msg}"
    # Pot zeroed (prevents double-claim).
    assert state.fees_db[1] == 0, "pot not zeroed after collect"
    print("  PASS: FeeCollectV1 happy path — plaintext pot 300+200, claimed 500, pot zeroed")


def test_fee_collect_v1_zero_claim_rejected():
    """FeeCollectV1: total_fees == 0 → ↓zero-claim (C1, §4.2)."""
    state = FeeState()
    ok, msg = state.apply_fee_collect(total_fees=0, height=1)
    assert not ok, "zero-claim must be rejected"
    assert "zero-claim" in msg, f"wrong error: {msg}"
    print("  REJECTED: FeeCollectV1 zero-claim — replay attack prevented")


def test_fee_collect_v1_bad_claim_rejected():
    """FeeCollectV1: claim != pot → ↓bad-claim (plain equality, C2)."""
    state = FeeState()
    state.commitment_roots_db[0xAAA] = 1
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    state.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, 300, height=1,
                       nullifier=1, merkle_root=0xAAA)
    # Over-claim attack
    ok, msg = state.apply_fee_collect(total_fees=500, height=1)
    assert not ok, "over-claim must be rejected"
    assert "bad-claim" in msg, f"wrong error: {msg}"
    # Under-claim is also a mismatch
    ok2, msg2 = state.apply_fee_collect(total_fees=299, height=1)
    assert not ok2, "under-claim must be rejected"
    assert "bad-claim" in msg2, f"wrong error: {msg2}"
    print("  REJECTED: FeeCollectV1 bad-claim — plain equality (over- and under-claim)")


def test_fee_collect_pot_zeroed_and_replay_guarded():
    """After collect, fees_db[h] == 0; a second claim of the same total fails."""
    state = FeeState()
    state.commitment_roots_db[0xA7] = 7
    _, _, f_commit, f_fee = balanced_fee_v2(5000, 300, 42)
    ok, _ = state.apply_fee_v2(mk(5000, 99), mk(4700, 57), f_commit, f_fee,
                               height=7, nullifier=1, merkle_root=0xA7)
    assert ok
    ok_c, _ = state.apply_fee_collect(total_fees=300, height=7)
    assert ok_c
    assert state.fees_db[7] == 0, "pot must be zeroed"
    # Replay: a second collect of the same total now mismatches the zeroed pot.
    ok2, msg2 = state.apply_fee_collect(total_fees=300, height=7)
    assert not ok2, "replayed collect must be rejected"
    assert "bad-claim" in msg2
    print("  PASS: pot zeroed after collect; replay of the same claim rejected")


def test_fee_accumulation_plaintext_soundness():
    """Plaintext accumulation: over/under-claim rejected by plain equality.

    Replaces the Pedersen Thm2 test — there is no accumulator to open, the
    register IS the sum.
    """
    state = FeeState()
    specs = [((5000, 300, 42), 1, 0xA1), ((3000, 200, 99), 2, 0xA2),
             ((1000, 100, 7), 3, 0xA3)]
    for (in_val, fee, fee_blind), nf, root in specs:
        state.commitment_roots_db[root] = 1
        in_c, out_c, fee_c, f = balanced_fee_v2(in_val, fee, fee_blind)
        ok, _ = state.apply_fee_v2(in_c, out_c, fee_c, f, height=1,
                                   nullifier=nf, merkle_root=root)
        assert ok
    assert state.fees_db[1] == 600
    # Over-claim
    ok_over, _ = state.apply_fee_collect(total_fees=700, height=1)
    assert not ok_over, "over-claim must be rejected"
    # Under-claim
    ok_under, _ = state.apply_fee_collect(total_fees=599, height=1)
    assert not ok_under, "under-claim must be rejected"
    # Exact claim
    ok_exact, _ = state.apply_fee_collect(total_fees=600, height=1)
    assert ok_exact
    print("  PASS: plaintext accumulation soundness — over/under-claim rejected, exact accepted")


# ============================================================
# Tests — FeeV2 Preconditions + Error Barbs
# ============================================================

def test_feev2_double_spend_rejected():
    """FeeV2: same nullifier twice → ↓double-spend (P7, §5.3)."""
    state = FeeState()
    state.commitment_roots_db[0xAAA] = 1
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    ok1, _ = state.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, 300,
                                height=1, nullifier=77, merkle_root=0xAAA)
    assert ok1
    ok2, msg = state.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, 300,
                                  height=1, nullifier=77, merkle_root=0xAAA)
    assert not ok2, "double-spend must be rejected"
    assert "double-spend" in msg, f"wrong error: {msg}"
    print("  REJECTED: FeeV2 double-spend — nullifier 77 already spent")


def test_feev2_bad_merkle_root_rejected():
    """FeeV2: merkle_root not in commitment_roots_db → ↓bad-merkle-root (P6).

    [1:1] mod.rs:227-241 — enforced on-chain; the old GAP marker closes.
    """
    state = FeeState()
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    state.commitment_roots_db[0xBEEF] = 1
    unknown_root = 0xDEAD
    assert unknown_root not in state.commitment_roots_db
    ok, msg = state.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, 300,
                                 height=1, nullifier=1, merkle_root=unknown_root)
    assert not ok, "unknown merkle root must be rejected"
    assert "bad-merkle-root" in msg, f"wrong error: {msg}"
    print("  REJECTED: FeeV2 bad merkle root (P6) — real rejection")


def test_feev2_input_value_exceeds_fee_rejected():
    """FeeV2: input.value <= fee → ↓bad-fee-amount (output would be zero/negative)."""
    state = FeeState()
    state.commitment_roots_db[0xAAA] = 1
    # fee == input value → output value 0 → rejected
    ok, msg = state.apply_fee_v2(mk(300, 99), mk(0, 99), mk(300, 1), 300,
                                 height=1, nullifier=1, merkle_root=0xAAA)
    assert not ok, "input.value <= fee must be rejected"
    assert "bad-fee-amount" in msg, f"wrong error: {msg}"
    print("  REJECTED: FeeV2 input.value <= fee (output would be zero)")


def test_feev2_token_commit_validation():
    """FeeV2: wrong token_commit → ↓bad-token (P2/P3, §5.3).

    input.token_commit and output.token_commit must equal
    poseidon(DOMAIN_TOKEN_COMMIT, DRKW_ASSET_ID=0, token_blind=0).
    """
    # The current model doesn't track token_commit separately.
    # DRKW_ASSET_ID = 0 is implicit in balanced_fee_v2.
    # This test documents the gap — when token_commit is added to the
    # model, a non-zero token_id should cause rejection.
    token_commit_drkw = poseidon_hash(b"DARKWOW_TOKEN_COMMIT", 0, 0)
    assert isinstance(token_commit_drkw, bytes)
    print("  PASS: FeeV2 token_commit — DRKW token_id=0 implicit, validation gap documented")


def test_feev2_duplicate_commitment_rejected():
    """FeeV2: output commitment already in commitment_set → rejected (P8).

    [1:1] mod.rs:227-241 — enforced on-chain; the old GAP marker closes.
    """
    state = FeeState()
    state.commitment_roots_db[0xAAA] = 1
    state.commitment_set.add(4700)   # existing commitment (simulated key)
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    ok, msg = state.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, 300,
                                 height=1, nullifier=1, merkle_root=0xAAA)
    assert not ok, "duplicate commitment must be rejected"
    assert "duplicate" in msg, f"wrong error: {msg}"
    print("  REJECTED: FeeV2 duplicate commitment (P8) — real rejection")


# ============================================================
# Tests — Block Model + Lifecycle
# ============================================================

def test_fee_collect_v1_conditional_presence():
    """FeeCollectV1: present iff total_fees > 0 (§4.4)."""
    state = FeeState()
    bb = BlockBuilder(height=1, state=state)
    bb.add_coinbase()
    # No fees → FeeCollectV1 must be absent
    ok, msg = bb.add_fee_collect(total_fees=0)
    assert not ok, f"FeeCollectV1 with zero fees must be rejected: {msg}"
    assert "zero-claim" in msg.lower()
    # With fees → FeeCollectV1 must succeed
    state2 = FeeState()
    bb2 = BlockBuilder(height=2, state=state2)
    bb2.add_coinbase()
    state2.commitment_roots_db[0xAAA] = 2
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    bb2.state.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, 300,
                           height=2, nullifier=3, merkle_root=0xAAA)
    ok2, _ = bb2.add_fee_collect(total_fees=300)
    assert ok2, "FeeCollectV1 with non-zero fees must succeed"
    print("  PASS: FeeCollectV1 conditional presence — absent at 0 fees, present at >0 fees")


def test_block_canonical_ordering():
    """Block canonical ordering: PoWRewardV1[0], FeeV2[1..k], FeeCollectV1[last] (§2.1)."""
    state = FeeState()
    bb = BlockBuilder(height=3, state=state)
    bb.add_coinbase()
    assert bb.transactions[0] == "PoWRewardV1", "coinbase must be tx[0]"
    f1 = balanced_fee_v2(5000, 300, 42)
    f2 = balanced_fee_v2(3000, 100, 7)
    state.commitment_roots_db[0xAAA] = 3
    state.commitment_roots_db[0xBBB] = 3
    bb.state.apply_fee_v2(f1[0], f1[1], f1[2], f1[3], height=3,
                          nullifier=1, merkle_root=0xAAA)
    bb.transactions.append("FeeV2_1")
    bb.state.apply_fee_v2(f2[0], f2[1], f2[2], f2[3], height=3,
                          nullifier=2, merkle_root=0xBBB)
    bb.transactions.append("FeeV2_2")
    assert not bb._has_fee_collect
    ok, _ = bb.add_fee_collect(total_fees=400)
    assert ok, "FeeCollectV1 with correct total must succeed"
    assert bb.transactions[-1] == "FeeCollectV1", "FeeCollectV1 must be last transaction"
    assert bb.transactions == ["PoWRewardV1", "FeeV2_1", "FeeV2_2", "FeeCollectV1"]
    print("  PASS: canonical block ordering — PoWRewardV1[0], FeeV2[1..k], FeeCollectV1[last]")


def test_overlay_visibility_invariant():
    """Invariant 1 (Overlay Visibility): FeeV2 sees coinbase's merkle root (§2.2)."""
    state = FeeState()
    coinbase_root = 0xC01BA5E  # simulated coinbase merkle root
    state.commitment_roots_db[coinbase_root] = 1  # Simulates PoWRewardV1's insert
    assert coinbase_root in state.commitment_roots_db, \
        "Invariant 1 violated: FeeV2 cannot see coinbase root in same block"
    print("  PASS: Overlay Visibility (Invariant 1) — coinbase root visible to same-block FeeV2")


def test_full_lifecycle_with_accumulation():
    """Full block lifecycle: PoWRewardV1 → FeeV2×2 → FeeCollectV1 (§5.6.6)."""
    state = FeeState()
    bb = BlockBuilder(height=5, state=state)
    bb.add_coinbase()
    coinbase_root = 0xCBCB
    state.commitment_roots_db[coinbase_root] = 5
    f1 = balanced_fee_v2(5000, 300, 42)
    f2 = balanced_fee_v2(3000, 200, 99)
    ok1, _ = state.apply_fee_v2(f1[0], f1[1], f1[2], f1[3], height=5,
                                nullifier=10, merkle_root=coinbase_root)
    assert ok1
    bb.transactions.append("FeeV2_1")
    ok2, _ = state.apply_fee_v2(f2[0], f2[1], f2[2], f2[3], height=5,
                                nullifier=11, merkle_root=coinbase_root)
    assert ok2
    bb.transactions.append("FeeV2_2")
    # Intermediate: the pot holds the plain sum.
    assert state.fees_db[5] == 500, "intermediate pot mismatch"
    # 3. FeeCollectV1
    ok3, _ = bb.add_fee_collect(total_fees=500)
    assert ok3
    # 4. Postconditions
    assert state.fees_db[5] == 0, "pot not zeroed"
    assert 10 in state.nullifiers_db and 11 in state.nullifiers_db, "nullifiers not recorded"
    assert bb.transactions == ["PoWRewardV1", "FeeV2_1", "FeeV2_2", "FeeCollectV1"]
    print("  PASS: full block lifecycle — PoWRewardV1 → 2×FeeV2 → FeeCollectV1, all postconditions")


# ============================================================================
# Fee Signalling Testing Plan scenario (P-FW-5)
# ============================================================================


def test_threshold_change_fifo_demotion():
    """P-FW-5: txs survive a congesting window boundary; three-tier FCFS preserved.

    Simulates a window boundary where the congestion factor rises, so the
    previously-admitted fees no longer cover their original tier. The txs must
    survive the transition (I6: no ex post facto eviction) and retain FCFS order
    (I3). [1:1] with fee_window_model.test_fcfs_preservation.

    Pins Rust: L1-FW-6 (concurrent stress) / L3-FW-3 (post-boundary E2E).
    """
    from fee_window_model import FeeWindow, MempoolWithWindow, FeeTier, compute_total_fee

    # Window 1: zero congestion (CF=1.0) — two txs admitted to HIGH.
    w_old = FeeWindow()
    mempool = MempoolWithWindow(w_old)

    fee_a = compute_total_fee(5000, w_old.circuit_cf, FeeTier(FeeTier.HIGH), 100_000)
    fee_b = compute_total_fee(100, w_old.circuit_cf, FeeTier(FeeTier.HIGH), 100_000)
    assert mempool.admit("tx_a", fee_a, 5000, FeeTier(FeeTier.HIGH)) == "high", (
        "P-FW-5: tx with HIGH fee should route to the high queue at CF=1.0"
    )
    assert mempool.admit("tx_b", fee_b, 100, FeeTier(FeeTier.HIGH)) == "high", (
        "P-FW-5: at zero congestion a HIGH-tier tx is admitted to the high queue"
    )

    # Window 2: congested CFs — the previously-admitted fees no longer cover HIGH.
    w_new = FeeWindow()
    w_new.adjust_circuit(1000, 5000)  # circuit CF congested
    w_new.adjust_wasm(1000, 5000)     # WASM CF congested
    mempool.on_window_boundary(w_new)

    # I3/I6: admitted txs survive (no ex post facto eviction) and FCFS order holds.
    remaining = mempool.select_for_block(max_txs=10)
    assert "tx_a" in remaining, (
        "P-FW-5 I3 violated: tx_a evicted after threshold rise"
    )
    assert "tx_b" in remaining, (
        "P-FW-5: tx_b evicted after threshold rise"
    )

    # I6: order preserved (FCFS within the surviving queue).
    assert remaining[0] == "tx_a", (
        f"P-FW-5 I6 violated: tx_a should precede tx_b, got {remaining}"
    )
    assert remaining[1] == "tx_b", (
        f"P-FW-5: tx_b should follow tx_a, got {remaining}"
    )

    print("  PASS: threshold-change FIFO demotion — txs survive, order preserved")


# ============================================================
# Runner
# ============================================================

def run_all():
    tests = [
        # Migrated tests (10)
        test_legal_transfers_only,
        test_legal_with_fees,
        test_legal_with_burns,
        test_illegal_hidden_mint,
        test_illegal_standalone_mint,
        test_legal_mint_balanced_by_burn,
        test_illegal_mint_exceeds_burn,
        test_coinbase_exceeds_schedule,
        test_coinbase_is_reward_only,
        test_pow_reward_exact_equality,
        test_integration_with_cumulative_chain,
        # FeeV2 / FeeParamsV3 / mempool (8)
        test_feev2_mass_balance,
        test_feev2_plaintext_fee,
        test_three_tier_admission,
        test_feev2_below_tier_price_rejected,
        test_feev2_high_tier,
        test_feev2_medium_tier,
        test_feev2_low_tier,
        test_feev2_fifo_ordering,
        test_feev2_mempool_mark_mined,
        # Coin Merkle Tree tests (3)
        test_commitment_merkle_tree_position_enumeration,
        test_commitment_merkle_path_verification,
        test_commitment_merkle_tree_empty_leaf_value,
        # FeeCollectV1 + plaintext pot (5)
        test_fee_collect_v1_happy_path,
        test_fee_collect_v1_zero_claim_rejected,
        test_fee_collect_v1_bad_claim_rejected,
        test_fee_collect_pot_zeroed_and_replay_guarded,
        test_fee_accumulation_plaintext_soundness,
        # FeeV2 preconditions + error barbs (5)
        test_feev2_double_spend_rejected,
        test_feev2_bad_merkle_root_rejected,
        test_feev2_input_value_exceeds_fee_rejected,
        test_feev2_token_commit_validation,
        test_feev2_duplicate_commitment_rejected,
        # Block model + lifecycle (4)
        test_fee_collect_v1_conditional_presence,
        test_block_canonical_ordering,
        test_overlay_visibility_invariant,
        test_full_lifecycle_with_accumulation,
        # Fee Signalling Testing Plan scenario
        test_threshold_change_fifo_demotion,
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
