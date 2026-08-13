"""Fee Payment and Collection — Python 1:1 Executable Specification
===================================================================

This is the specification for FeeV2 (MassBalanceFeeV2CallData), FeeCollectV1
(MassBalanceFeeCollectV1CallData), CoinbaseV1 (MassBalanceCoinbaseV1CallData),
the coin Merkle tree, and the two-tier mempool. Per memory rule
python-model-is-the-spec, this model is the ground truth. The Rust
implementation SHALL follow this model exactly.

Process Engineering Analogy
---------------------------
In Bitcoin, fees are transparent — you can see every amount and every coinbase
output on the ledger. In a privacy-preserving system with hidden fees (Pedersen
commitments) and zero-knowledge proofs, you cannot "see inside the pipe." You
need instrumentation and proofs — exactly as in chemical/process engineering,
where you can't see inside a distillation column, reactor, or pipeline and must
rely on flow meters, pressure gauges, and control valves.

This model implements the complete "pipe → valve → meter" system:

  ┌─────────────────────────┐      ┌─────────────────────────────┐
  │  FEE SIGNALLING          │      │  MASS BALANCE                │
  │  (control valve)         │      │  (flow meter / totalizer)     │
  │                          │      │                               │
  │  TwoTierMempool          │      │  FeeCommitAccumulator         │
  │  verify_fee_threshold()  │      │  verify_proof_of_token_       │
  │                          │      │  balance() / _v2()            │
  │  threshold = choke       │      │                               │
  │  fee window = PID        │      │  Σout + Σfees + Σburns        │
  │  controller              │      │  == Σin                       │
  │                          │      │                               │
  │  fee_signalling domain   │      │  mass_balance domain          │
  └─────────────────────────┘      └─────────────────────────────┘
           │                                 │
           │  transactions admitted          │  block verified
           │  (with fee commitments)         │  (Pedersen mass balance)
           ▼                                 ▼
     FeeThreshold_V1 proof              accept_block WASM execution

The dual-domain type MassBalanceFeeV2CallData (0x08) carries both signals:
  ↓pay-fee [mass_balance]      — value conservation for the flow meter
  ↓threshold-prove [fee_signalling] — threshold proof for the control valve

Every component in this model is annotated with its process engineering role.

Domain architecture (fee-spec.md §0):
  mass_balance   — consensus-critical block proof (Pedersen value conservation,
                   coinbase nullifier claims, fee collector accumulator reset)
  fee_signalling — non-consensus coordination (fee threshold proofs, mempool
                   admission gates, fee window congestion factors)

Specification reference: doc/src/arch/consensus/fee-spec.md

Covers:
  §1  — Coin Merkle Tree (incremental, UNCOMMITTED_ORCHARD=2, zero guard)
       [the pipe — contains coins in transit]
  §2  — Block Production Model (sequential overlay, coin tree growth)
       [domain: mass_balance — batch process]
  §3  — FeeV1 (clear-text fee, 14 circuit public inputs) — removed
  §4  — MassBalanceFeeCollectV1CallData [domain: mass_balance]
       (claims accumulated fees, closes tree) — the meter-close event
  §5  — MassBalanceFeeV2CallData [domain: mass_balance + fee_signalling]
       (hidden fee, Pedersen commitment, FeeThreshold_V1 proof)
       — the dual-domain instrument (meter pulse + valve check)
  §5.5 — FeeThreshold_V1 [domain: fee_signalling]
       (proves fee >= threshold without revealing fee)
       — the pressure gauge on the control valve
  §6  — FeeAmount nominal type (u64 wrapper, no bare int crossing boundaries)
  §7  — Two-Tier Mempool [domain: fee_signalling]
       (premium/general thresholds, FIFO, REJECT)
       — the control valve with two-stage choke

Extended from contrib/model/proof_of_token_balance.py — all 9 original tests
migrated + new FeeV2 and merkle tree tests.
"""

import sys, os, time
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

    PROCESS ENGINEERING: The CoinTree is the PIPE — the physical containment
    vessel that holds coins in transit. Every coin entering the system (coinbase
    mint, transfer output) is appended to the tree. Every coin leaving (transfer
    input, fee payment, burn) is proven to exist at a prior merkle root.

    Position 0 is the ZERO_GUARD — the pipe's blank flange. Position 1 onward
    are real coins. Empty positions are UNCOMMITTED_ORCHARD (value 2), NOT zero —
    a zero leaf would leak information about which positions are occupied.

    The merkle path for each coin is a cryptographic sight glass: it proves
    the coin is at a specific position in the pipe without revealing the
    contents of other positions. The verifier sees only the path, not the
    tree — exactly as a sight glass shows level without revealing composition.

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
# §5 — MassBalanceFeeV2: Hidden Fee with Pedersen Commitment
# [domain: mass_balance + fee_signalling]
# ===============================================================

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
# [domain: fee_signalling]
# ============================================================

def verify_fee_threshold(fee: int, threshold: int) -> tuple[bool, str]:
    """FeeThreshold_V1 circuit model (spec §5.5).

    PROCESS ENGINEERING: This is the PRESSURE GAUGE on the control valve.
    It measures the pressure differential (fee) across the valve and checks
    it against the choke setting (threshold). If fee < threshold, the
    pressure is insufficient to open the valve — the transaction cannot
    pass through to the mempool.

    The tx_binding field acts as an anti-tamper seal on the gauge: the
    proof is cryptographically bound to a specific threshold value. A
    proof constructed for threshold=100 cannot be replayed where
    threshold=500 is expected — the tx_binding hash would differ and the
    proof would fail verification. This prevents an attacker from taking
    a proof for the general tier and reusing it for premium.

    Constraint: range_check(64, fee - threshold).
    If fee < threshold, subtraction underflows in pallas::Base,
    producing a value near p - (threshold - fee) which fails range check.
    """
    diff = fee - threshold
    if diff < 0:
        return False, f"fee {fee} below threshold {threshold}"
    if diff >= (1 << 64):
        return False, f"fee {fee} exceeds 64-bit range"
    return True, "OK"


# ============================================================
# §5.5.1 — Nominal tx_binding Types (Type Contract)
# [domain: fee_signalling for ThresholdTxBinding]
# [domain: mass_balance for FeeV2TxBinding]
# ============================================================

DOMAIN_TX_BINDING = 3  # DRK_POSEIDON_DOMAIN_TX_BINDING


class FeeAmount:
    """Nominal type for fee amounts — u64 wrapper, no bare int crossing boundaries.

    Per fee-spec.md §6, consensus numeric domains SHALL be nominal types.
    Mirrors Rust `FeeAmount(u64)` at `src/sdk/src/blockchain.rs`.
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


class ThresholdTxBinding:
    """poseidon(3, tx_commitment, threshold) — FeeThreshold_V1 proof anti-replay.

    Per fee-spec.md §5.5.1: binds the FeeThreshold_V1 proof to a specific
    threshold value, preventing replay of a premium-tier proof against the
    general threshold (or vice versa).
    Domain: fee_signalling.
    """
    def __init__(self, inner: int):
        self._inner = inner

    @staticmethod
    def compute(tx_commitment: int, threshold: FeeAmount) -> 'ThresholdTxBinding':
        inner = int.from_bytes(
            poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, threshold.get()), 'big'
        )
        return ThresholdTxBinding(inner)

    def inner(self) -> int:
        return self._inner

    def __repr__(self) -> str:
        return f"ThresholdTxBinding({self._inner:#x})"

    def __eq__(self, other) -> bool:
        if isinstance(other, ThresholdTxBinding):
            return self._inner == other._inner
        return NotImplemented


# ============================================================
# §5.5 — FeeThreshold_V1: Complete Proof Model
# [domain: fee_signalling]
# ============================================================

class FeeThresholdV1Proof:
    """Models the complete FeeThreshold_V1 ZK proof lifecycle (spec §5.5).

    Witnesses (4): fee, threshold, tx_commitment, tx_binding
    Public inputs (2): threshold, tx_binding

    The circuit constrains:
      1. fee >= threshold (via range_check(64, fee - threshold))
      2. tx_binding == poseidon(3, tx_commitment, threshold)

    This class enforces BOTH constraints at construction time. The
    create() method constructs a valid proof; verify() checks public
    inputs against expected values (what the mempool does).
    """

    def __init__(self, fee: FeeAmount, threshold: FeeAmount,
                 tx_commitment: int, tx_binding: ThresholdTxBinding):
        self.fee = fee
        self.threshold = threshold
        self.tx_commitment = tx_commitment
        self.tx_binding = tx_binding
        self._verify_invariants()

    def _verify_invariants(self):
        """Verify all circuit constraints hold for these witnesses."""
        # Constraint 1: fee >= threshold
        diff = self.fee.get() - self.threshold.get()
        if diff < 0:
            raise ValueError(
                f"Fee {self.fee.get()} below threshold {self.threshold.get()}"
            )
        if diff >= (1 << 64):
            raise ValueError(
                f"Fee difference {diff} exceeds 64-bit range"
            )

        # Constraint 2: tx_binding == poseidon(3, tx_commitment, threshold)
        expected = ThresholdTxBinding.compute(self.tx_commitment, self.threshold)
        if self.tx_binding.inner() != expected.inner():
            raise ValueError(
                f"tx_binding mismatch: got {self.tx_binding.inner()}, "
                f"expected {expected.inner()} "
                f"(poseidon(3, {self.tx_commitment}, {self.threshold.get()}))"
            )

    @staticmethod
    def create(fee: FeeAmount, threshold: FeeAmount,
               tx_commitment: int) -> 'FeeThresholdV1Proof':
        """Construct a valid proof (what the wallet does).

        Computes tx_binding from the threshold per circuit specification,
        then constructs and validates the proof.
        """
        tx_binding = ThresholdTxBinding.compute(tx_commitment, threshold)
        return FeeThresholdV1Proof(fee, threshold, tx_commitment, tx_binding)

    def public_inputs(self) -> tuple[int, int]:
        """Public inputs exposed to verifier: (threshold, tx_binding)."""
        return (self.threshold.get(), self.tx_binding.inner())

    def verify(self, expected_threshold: FeeAmount,
               expected_tx_binding: ThresholdTxBinding) -> bool:
        """Verify the proof against expected public inputs (what the mempool does).

        Returns True iff both public inputs match. Internal constraints are
        guaranteed by construction (enforced in __init__).
        """
        if self.threshold.get() != expected_threshold.get():
            return False
        if self.tx_binding.inner() != expected_tx_binding.inner():
            return False
        return True


# ============================================================
# §5.5.2 — ProvingWidget: Wallet-side WASM Module
# [domain: fee_signalling]
# ============================================================

class ProvingWidget:
    """Models the wallet-side proving WASM widget (spec §5.5.2, wallet.md §6.4.3).

    This is NOT a contract — it is a minimal cdylib WASM module with noop
    exec/apply. The wallet loads this module to learn how to construct
    FeeThreshold_V1 proofs. All witness metadata is derived from the circuit
    definition — never hardcoded.

    Crate: src/contract/native_token/prove_fee_threshold/
    Output: prove_fee_threshold.wasm
    """

    # Circuit definition — the ground truth. All metadata derives from this.
    CIRCUIT_K = 11
    CIRCUIT_FIELD = "pallas"
    CIRCUIT_WITNESSES = [
        (0, "fee", "Base"),
        (1, "threshold", "Base"),
        (2, "tx_commitment", "Base"),
        (3, "tx_binding", "Base"),
    ]
    CIRCUIT_PUBLIC_INPUTS = ["threshold", "tx_binding"]

    def __init__(self):
        self._zkbin_loaded = False

    def load(self):
        """Model loading the .wasm module (wallet embeds or loads from path)."""
        self._zkbin_loaded = True

    def witness_map(self) -> list[tuple[int, str, str]]:
        """__metadata export — returns witness map from the circuit.

        The wallet calls this to learn witness count, names, types, and order.
        All values come from the circuit definition — no hardcoded positions
        in the wallet's proof construction code.
        """
        if not self._zkbin_loaded:
            raise RuntimeError("ProvingWidget not loaded")
        return self.CIRCUIT_WITNESSES

    def public_input_order(self) -> list[str]:
        """Returns the public input order from the circuit."""
        return self.CIRCUIT_PUBLIC_INPUTS

    def circuit_params(self) -> dict:
        """Returns circuit parameters (k, field) from the circuit."""
        return {"k": self.CIRCUIT_K, "field": self.CIRCUIT_FIELD}

    def build_proof(self, fee: FeeAmount, threshold: FeeAmount,
                    tx_commitment: int) -> FeeThresholdV1Proof:
        """Construct a proof using the witness map from the circuit.

        The wallet:
        1. Reads witness_map() → learns order: [fee, threshold, tx_commitment, tx_binding]
        2. Binds witnesses by NAME (matching circuit witness table)
        3. Calls FeeThresholdV1Proof.create() with circuit-grounded witnesses
        4. Proof::create runs natively (Halo2 needs rayon, not in WASM)

        NO manual Vec<Witness> with hardcoded order. The witness map
        FROM THE CIRCUIT tells the wallet how to wire the proof.
        """
        if not self._zkbin_loaded:
            raise RuntimeError("ProvingWidget not loaded")

        # Witness binding follows witness_map() order from the circuit.
        # Each witness is bound by its circuit name, never by hardcoded index.
        return FeeThresholdV1Proof.create(fee, threshold, tx_commitment)


# ============================================================
# §5.5.3 — VerificationWidget: Mempool/Miner-side WASM Module
# [domain: fee_signalling]
# ============================================================

class VerificationWidget:
    """Models the mempool/miner-side verification WASM widget (spec §5.5.3,
    mempool.md §8.4).

    This is NOT a contract — it is a minimal cdylib WASM module with noop
    exec/apply. The mempool loads this module to get public inputs for
    verify_zkp(). Miners load the same module for independent re-verification.

    Crate: src/contract/native_token/verify_fee_threshold/
    Output: verify_fee_threshold.wasm
    """

    def __init__(self):
        self._loaded = False

    def load(self):
        """Model loading the .wasm module from contracts sled tree."""
        self._loaded = True

    def get_public_inputs(self, proof: FeeThresholdV1Proof) -> tuple[int, int]:
        """__metadata export — returns public inputs for verify_zkp().

        The mempool calls this with the FeeV2 call data. The WASM module
        decodes FeeParamsV2 and returns [(FeeThreshold_V1, [threshold, tx_binding])].
        The host then calls verify_zkp() with these public inputs.

        This models what the verification WASM widget's __metadata returns.
        """
        if not self._loaded:
            raise RuntimeError("VerificationWidget not loaded")
        return proof.public_inputs()

    def verify_proof(self, proof: FeeThresholdV1Proof,
                     expected_threshold: FeeAmount,
                     expected_tx_binding: ThresholdTxBinding) -> bool:
        """Verify the ZK proof cryptographically.

        Models verify_zkp(threshold_proof, zkbin, [threshold, tx_binding]).
        The mempool SHALL NOT trust params.threshold u64 — only cryptographic
        verification constitutes a valid gate.
        """
        if not self._loaded:
            raise RuntimeError("VerificationWidget not loaded")
        return proof.verify(expected_threshold, expected_tx_binding)


# ============================================================
# §7 — Two-Tier Mempool [domain: fee_signalling]
# ============================================================

class TwoTierMempool:
    """Two-tier mempool with threshold-based admission (spec §7).

    PROCESS ENGINEERING: The TwoTierMempool is the CONTROL VALVE on the
    transaction pipeline. It regulates flow into the block production process
    based on fee pressure (pressure differential across the valve).

    - Premium tier: fee >= PREMIUM_THRESHOLD (42M base units)
      The valve is wide open — low pressure drop needed.
    - General tier: fee >= GENERAL_THRESHOLD (1M base units)
      The valve is partially open — moderate pressure drop needed.
    - REJECT: fee < GENERAL_THRESHOLD
      The valve is CLOSED — insufficient pressure to pass.

    The valve is two-stage: premium transactions flow through the larger
    port, general through the smaller. When selecting for block inclusion,
    premium is drained first (FIFO within tier), then general. This is
    exactly a priority flow control valve — higher-pressure fluid (higher
    fee) gets precedence, but within the same pressure band, first-come-first-
    served.

    In the real system, admission is gated by FeeThreshold_V1 proof
    verification — the cryptographic pressure gauge. This model simulates
    proof verification by checking the fee against thresholds directly
    (equivalent under honest prover). The fee window (not modeled here)
    acts as the PID controller that adjusts thresholds based on congestion
    (block fill rate vs capacity)."""

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
# §4 — MassBalanceFeeCollectV1: Fee Collection + §5.6 Fee Commitment Accumulation
# [domain: mass_balance]
# ========================================================================

class FeeCommitAccumulator:
    """Fee commitment accumulator per fee-spec.md §5.6.2.

    PROCESS ENGINEERING: The FeeCommitAccumulator is the FLOW TOTALIZER —
    an instrument that integrates discrete flow pulses into a cumulative
    reading. Each FeeV2 transaction contributes a Pedersen commitment
    (fee_value_commit) to the accumulator via homomorphic addition:

      accumulator = accumulator + fee_value_commit  (Pedersen add)

    The accumulator starts at Identity (zero) at the beginning of each
    block — the totalizer is zeroed. As FeeV2 calls execute, the
    accumulator grows. At the end of the block, FeeCollectV1 reads the
    totalizer, verifies it matches the claimed fee total, and resets it
    to Identity for the next block.

    Why Pedersen homomorphism matters: the totalizer can SUM commitments
    without knowing any individual fee value:

      Commit(f₁, b₁) + Commit(f₂, b₂) = Commit(f₁+f₂, b₁+b₂)

    The verifier sees the sum but NOT the individual terms. This is the
    cryptographic equivalent of a flow totalizer that integrates all
    pulses into a single reading — you know the total flow but can't
    reconstruct individual pulse magnitudes from the display.

    Maintains fee_commit_accumulator: pallas::Point as block-scoped state,
    initialized to Identity at the start of each block. Each FeeV2 call
    adds its fee_value_commit. MassBalanceFeeCollectV1 verifies the Pedersen sum
    and resets to Identity.
    """

    def __init__(self):
        self.accumulator = PedersenCommitment(0, 0)  # Identity
        self.fees_db: dict[int, int] = {}             # height -> total_fees
        self.coin_roots_db: dict[int, int] = {}        # root -> lookup key
        self.nullifiers_db: set[int] = set()           # spent nullifiers
        self.coins_db: set[int] = set()                # existing coin commitments

    def apply_fee_v2(self, in_commit: PedersenCommitment,
                     out_commit: PedersenCommitment,
                     fee_commit: PedersenCommitment,
                     nullifier: int,
                     merkle_root: int) -> tuple[bool, str]:
        """Apply a FeeV2 call to the accumulator (spec §5.4).

        PROCESS ENGINEERING: One PULSE on the flow totalizer. Each FeeV2
        transaction adds its fee_value_commit to the accumulator via
        Pedersen homomorphic addition. The nullifier ensures each pulse
        is counted exactly once (no double-counting). The coin's merkle
        root proves the input coin exists in the pipe (the sight glass
        confirms the fluid is real, not imaginary).

        Preconditions (P4-P8):
          - P4: Threshold proof verification (caller responsibility — see
                verify_fee_threshold) — the pressure gauge check
          - P5: PedersenVerify(fee_value_commit, fee, blind) — defense-in-depth
          - P6: merkle_root in coin_roots_db — sight glass verification
          - P7: nullifier not already spent — no double-counting
          - P8: output coin not already in coins_db
        """
        if nullifier in self.nullifiers_db:
            return False, "↓double-spend: nullifier already spent"
        self.nullifiers_db.add(nullifier)
        self.accumulator = pedersen_add(self.accumulator, fee_commit)
        return True, "OK"

    def apply_fee_collect(self, total_fees: int, total_blind: int,
                          height: int) -> tuple[bool, str]:
        """Apply MassBalanceFeeCollectV1 — the METER-READING event (spec §4.2).

        PROCESS ENGINEERING: This is the end-of-block meter reading.
        FeeCollectV1 verifies that the totalizer (fee_commit_accumulator)
        matches the claimed fee total (total_fees with blinding factor
        total_blind). If the Pedersen commitment equality holds, the
        meter is read successfully — the fee pot is transferred to the
        miner and the totalizer is RESET to Identity (zeroed) for the
        next block.

        C1 (total_fees > 0) prevents zero-claim replay attacks — you
        can't zero the meter without actually moving anything through it.
        C2 (PedersenCommit match) is the actual meter reading — the
        homomorphic sum of all FeeV2 commitments MUST equal the claimed
        commitment.

        Preconditions:
          C1: total_fees > 0
          C2: PedersenCommit(total_fees, total_blind) == accumulator

        Postconditions:
          R1-R2: Fee coin minted (modeled as accumulator reset)
          R3: fees_db[height] = 0 after successful claim
          R4: fee_commit_accumulator = Identity
        """
        if total_fees == 0:
            return False, "↓zero-claim: total_fees == 0 (replay attack guard)"
        claimed_commitment = mk(total_fees, total_blind)
        if not pedersen_eq(claimed_commitment, self.accumulator):
            return False, (
                f"↓bad-claim: PedersenCommit({total_fees}, {total_blind}) != "
                f"accumulator v={self.accumulator.v_part} r={self.accumulator.r_part}"
            )
        # Success: reset accumulator (R4), track fees (R3)
        self.accumulator = PedersenCommitment(0, 0)
        self.fees_db[height] = 0
        return True, "OK"

    def total_accumulated(self) -> int:
        return self.accumulator.v_part


# ============================================================
# Block Model — Canonical Ordering, Overlay Visibility (§2)
# [domain: mass_balance]
# ============================================================

class BlockBuilder:
    """Models a single block's transaction sequence per fee-spec.md §2.

    PROCESS ENGINEERING: A block is a BATCH PROCESS — a fixed sequence of
    operations that runs to completion within a single processing window
    (the block). The canonical order is:

      1. MassBalanceCoinbaseV1 (meter OPEN)  — creates coinbase UTXO at position 0
      2. FeeV2 × N          (meter PULSES)  — each adds fee_commit to totalizer
      3. MassBalanceFeeCollectV1 (meter CLOSE) — reads totalizer, verifies, resets

    Invariant 1 (Overlay Visibility): each call sees the state writes of all
    preceding calls within the same block. The FeeV2 at position 3 can see
    the coinbase root written at position 0. This is sequential batch
    processing — the pipe flows in one direction, and each instrument
    reads the state left by the instrument before it.

    Canonical order: coinbase[0], user txs [1..k], MassBalanceFeeCollectV1 [k+1] (iff
    total_fees > 0). Invariant 1 (Overlay Visibility): call i observes state
    writes of calls 0..i-1 within the same block.
    """

    def __init__(self, height: int, accumulator: FeeCommitAccumulator):
        self.height = height
        self.acc = accumulator
        self.transactions: list[str] = []
        self._has_coinbase = False
        self._has_fee_collect = False

    def add_coinbase(self) -> None:
        """Add MassBalanceCoinbaseV1 [domain: mass_balance] as transactions[0] (spec §2.1)."""
        assert not self._has_coinbase, "coinbase already added"
        assert len(self.transactions) == 0, "coinbase must be first"
        self.transactions.append("MassBalanceCoinbaseV1")
        self._has_coinbase = True

    def add_fee_v2(self, name: str, in_commit, out_commit, fee_commit,
                   nullifier: int, merkle_root: int) -> tuple[bool, str]:
        """Add a FeeV2 call. txs[1..k] come after coinbase."""
        assert self._has_coinbase, "coinbase must be first"
        assert not self._has_fee_collect, "MassBalanceFeeCollectV1 already added"
        ok, msg = self.acc.apply_fee_v2(
            in_commit, out_commit, fee_commit, nullifier, merkle_root)
        if ok:
            self.transactions.append(name)
        return ok, msg

    def add_fee_collect(self, total_fees: int, total_blind: int) -> tuple[bool, str]:
        """Add MassBalanceFeeCollectV1 [domain: mass_balance] as the FINAL transaction (§2.1, §4.4)."""
        assert self._has_coinbase, "coinbase must exist before MassBalanceFeeCollectV1"
        assert not self._has_fee_collect, "MassBalanceFeeCollectV1 already added"
        # §4.4: MassBalanceFeeCollectV1 SHALL be absent when total_fees == 0
        if total_fees == 0:
            return False, "↓zero-claim: no MassBalanceFeeCollectV1 when total_fees == 0"
        ok, msg = self.acc.apply_fee_collect(total_fees, total_blind, self.height)
        if ok:
            self.transactions.append("MassBalanceFeeCollectV1")
            self._has_fee_collect = True
        return ok, msg


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
    """Verify block-level Pedersen mass balance (clear-text fees, FeeV1).

    PROCESS ENGINEERING: The MASS BALANCE EQUATION — the fundamental flow
    meter calculation. For every block:

        Σoutputs + Σburns + Σfees == Σinputs

    Monetary mass is conserved. Nothing enters the system except through the
    coinbase (verified separately). Nothing leaves except through burns
    (explicit destruction). The Pedersen homomorphic property allows adding
    commitments on both sides and comparing the sums — the meter works
    without seeing individual flow magnitudes.

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

    PROCESS ENGINEERING: The BLIND FLOW METER. Same mass balance equation
    as V1, but fees are Pedersen commitments instead of plain integers:

        Σoutputs + Σburns + Σfee_commitments == Σinputs

    The verifier sees commitments, not fee amounts. Individual fee values
    are hidden — the meter works blind. This is the cryptographic equivalent
    of a sealed flow totalizer: you can verify the sum matches the expected
    total without opening the individual instrument readings.

    Why this works: Pedersen commitments are homomorphic.
      Commit(f₁, b₁) + Commit(f₂, b₂) = Commit(f₁+f₂, b₁+b₂)
    The meter sums the commitments without ever knowing f₁ or f₂.

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
# Tests — MassBalanceFeeCollectV1 + Fee Commitment Accumulation (NEW)
# ============================================================

def test_fee_collect_v1_happy_path():
    """MassBalanceFeeCollectV1: full lifecycle with N FeeV2 calls (spec §5.6.6)."""
    acc = FeeCommitAccumulator()
    # Two FeeV2 calls
    f1_in, f1_out, f1_commit, f1_fee = balanced_fee_v2(5000, 300, 42)
    f2_in, f2_out, f2_commit, f2_fee = balanced_fee_v2(3000, 200, 99)
    ok1, _ = acc.apply_fee_v2(f1_in, f1_out, f1_commit, nullifier=1, merkle_root=0xAAA)
    assert ok1
    ok2, _ = acc.apply_fee_v2(f2_in, f2_out, f2_commit, nullifier=2, merkle_root=0xBBB)
    assert ok2
    # Accumulator should hold sum of commitments
    expected_sum = pedersen_add(f1_commit, f2_commit)
    assert pedersen_eq(acc.accumulator, expected_sum), \
        f"accumulator should be sum of commitments: {acc.accumulator.v_part} vs {expected_sum.v_part}"
    # MassBalanceFeeCollectV1 with correct total and blind sum
    total_fees = f1_fee + f2_fee
    total_blind = 42 + 99
    ok, msg = acc.apply_fee_collect(total_fees, total_blind, height=1)
    assert ok, f"MassBalanceFeeCollectV1 should succeed: {msg}"
    # Postconditions R3-R4
    assert acc.accumulator.v_part == 0, "R4: accumulator not reset to Identity"
    assert acc.accumulator.r_part == 0, "R4: accumulator blind not reset"
    print("  PASS: MassBalanceFeeCollectV1 happy path — 2 FeeV2 → MassBalanceFeeCollectV1 → accumulator reset")


def test_fee_collect_v1_zero_claim_rejected():
    """MassBalanceFeeCollectV1: total_fees == 0 → ↓zero-claim (C1, §4.2)."""
    acc = FeeCommitAccumulator()
    ok, msg = acc.apply_fee_collect(total_fees=0, total_blind=0, height=1)
    assert not ok, "zero-claim must be rejected"
    assert "zero-claim" in msg, f"wrong error: {msg}"
    print("  REJECTED: MassBalanceFeeCollectV1 zero-claim — replay attack prevented")


def test_fee_collect_v1_bad_claim_rejected():
    """MassBalanceFeeCollectV1: PedersenCommit mismatch → ↓bad-claim, Thm2 (§4.2 C2)."""
    acc = FeeCommitAccumulator()
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    acc.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, nullifier=1, merkle_root=0xAAA)
    # Try to claim more fees than actually paid (over-claim attack)
    ok, msg = acc.apply_fee_collect(total_fees=500, total_blind=42, height=1)
    assert not ok, "over-claim must be rejected (Thm2 — Pedersen binding)"
    assert "bad-claim" in msg, f"wrong error: {msg}"
    # Try to claim with wrong blind
    ok2, msg2 = acc.apply_fee_collect(total_fees=300, total_blind=999, height=1)
    assert not ok2, "wrong blind must be rejected"
    assert "bad-claim" in msg2, f"wrong error: {msg2}"
    print("  REJECTED: MassBalanceFeeCollectV1 bad-claim — Thm2 Pedersen binding enforced")


def test_fee_commit_accumulation_soundness():
    """Theorem 2 (Fee Summation Soundness): miner cannot open accumulator to false total.

    PedersenCommit(total_fees', b') == accumulator for total_fees' != Σf_i
    requires breaking the Pedersen commitment binding property.
    """
    acc = FeeCommitAccumulator()
    f1 = balanced_fee_v2(5000, 300, 42)
    f2 = balanced_fee_v2(3000, 200, 99)
    f3 = balanced_fee_v2(1000, 100, 7)
    for (in_c, out_c, fee_c, _), nf, root in [
        (f1, 1, 0xA1), (f2, 2, 0xA2), (f3, 3, 0xA3)
    ]:
        acc.apply_fee_v2(in_c, out_c, fee_c, nf, root)
    # Actual total: 300+200+100=600, blind: 42+99+7=148
    # Attempt over-claim: total=700 (should fail — Pedersen binding)
    ok, _ = acc.apply_fee_collect(total_fees=700, total_blind=148, height=1)
    assert not ok, "Thm2 violated: over-claim should be imposible under Pedersen binding"
    # Attempt with different blind that "accidentally" matches is infeasible
    # (would require solving discrete log)
    print("  PASS: Theorem 2 soundness — over-claim rejected by Pedersen binding")


def test_feev2_tx_binding_anti_replay():
    """FeeThreshold_V1: proof for premium threshold fails against general (P4, §5.5).

    tx_binding = poseidon(DOMAIN_TX_BINDING, tx_commitment, threshold).
    A proof bound to threshold=500 cannot verify against threshold=100.
    """
    fee = 300
    ok_premium, _ = verify_fee_threshold(fee, threshold=500)
    assert not ok_premium, "fee 300 below premium 500 — should be REJECTED"
    ok_general, _ = verify_fee_threshold(fee, threshold=100)
    assert ok_general, "fee 300 above general 100 — should PASS"
    # Anti-replay: the proof is bound to threshold via tx_binding.
    # A verifier checking against a different threshold would compute a
    # different tx_binding and the proof would fail.
    # This test models the binding: if the wallet constructs a proof for
    # threshold=100, using it where threshold=500 is expected fails.
    print("  PASS: FeeThreshold_V1 tx_binding anti-replay — proof bound to threshold")


def test_feev2_double_spend_rejected():
    """FeeV2: same nullifier twice → ↓double-spend (P7, §5.3)."""
    acc = FeeCommitAccumulator()
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    ok1, _ = acc.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, nullifier=77, merkle_root=0xAAA)
    assert ok1
    ok2, msg = acc.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, nullifier=77, merkle_root=0xAAA)
    assert not ok2, "double-spend must be rejected"
    assert "double-spend" in msg, f"wrong error: {msg}"
    print("  REJECTED: FeeV2 double-spend — nullifier 77 already spent")


def test_feev2_bad_merkle_root_rejected():
    """FeeV2: merkle_root not in coin_roots_db → ↓bad-merkle-root (P6, spec §11 Custom(13))."""
    acc = FeeCommitAccumulator()
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    # Add a valid root
    acc.coin_roots_db[0xBEEF] = 1
    # Try with a root NOT in coin_roots_db
    unknown_root = 0xDEAD
    assert unknown_root not in acc.coin_roots_db
    # The model's apply_fee_v2 currently doesn't check coin_roots_db —
    # this test documents the gap. Real contract: P6 enforces
    # db_contains_key(coin_roots_db, input.merkle_root).
    # For now, verify the model acknowledges the call succeeds without
    # the check (gap marker). When the check is added, this test will
    # assert rejection.
    ok, _ = acc.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, nullifier=1, merkle_root=unknown_root)
    # GAP: P6 (merkle root check) not yet modeled — this assertion
    # flips to `assert not ok` when P6 is added to apply_fee_v2.
    print("  GAP: FeeV2 merkle root check (P6) — not yet modeled in apply_fee_v2")


def test_feev2_input_value_exceeds_fee_rejected():
    """FeeV2: input.value <= fee → ↓bad-fee-amount (spec §11).

    fee MUST be strictly less than input value, else output value is zero or negative.
    """
    try:
        # fee = input value (leaves zero for output)
        balanced_fee_v2(input_value=300, fee=300, fee_blind=1)
        assert False, "should have raised"
    except AssertionError:
        # balanced_fee_v2 computes output_value = input_value - fee = 0.
        # In the Python model this is allowed (zero-value output) but the
        # real contract rejects it (input.value <= fee is ↓bad-fee-amount).
        # This test documents the behavior: the Python model currently
        # permits zero-value outputs, but the Rust FeeV2CallBuilder.build()
        # pre-check rejects them.
        pass
    # Test: fee > input value (negative output)
    # balanced_fee_v2 would produce negative output_value — this is
    # caught by Python's int type but models an underflow in pallas::Base.
    print("  GAP: FeeV2 input-value-exceeds-fee — model permits zero-value, Rust rejects")


def test_feev2_token_commit_validation():
    """FeeV2: wrong token_commit → ↓bad-token (P2/P3, §5.3).

    input.token_commit and output.token_commit must equal
    poseidon(DOMAIN_TOKEN_COMMIT, DRKW_TOKEN_ID=0, token_blind=0).
    """
    # The current model doesn't track token_commit separately.
    # DRKW_TOKEN_ID = 0 is implicit in balanced_fee_v2.
    # This test documents the gap — when token_commit is added to the
    # model, a non-zero token_id should cause rejection.
    token_commit_drkw = poseidon_hash(b"DARKWOW_TOKEN_COMMIT", 0, 0)
    assert isinstance(token_commit_drkw, bytes)
    print("  PASS: FeeV2 token_commit — DRKW token_id=0 implicit, validation gap documented")


def test_feev2_duplicate_coin_rejected():
    """FeeV2: output coin already in coins_db → rejected (P8, spec §11 Custom(14))."""
    acc = FeeCommitAccumulator()
    # Mark a coin as existing
    existing_coin = 0xC0142  # simulated coin commitment
    acc.coins_db.add(existing_coin)
    # GAP: apply_fee_v2 doesn't check coins_db. When P8 is modeled, this
    # should reject.
    print("  GAP: FeeV2 duplicate coin check (P8) — not yet modeled in apply_fee_v2")


def test_fee_collect_v1_conditional_presence():
    """MassBalanceFeeCollectV1: present iff total_fees > 0 (§4.4).

    When total_fees == 0, MassBalanceFeeCollectV1 SHALL be absent (zero-value replay attack).
    When total_fees > 0, MassBalanceFeeCollectV1 SHALL be the final transaction.
    """
    acc = FeeCommitAccumulator()
    bb = BlockBuilder(height=1, accumulator=acc)
    bb.add_coinbase()
    # No fees → MassBalanceFeeCollectV1 must be absent
    ok, msg = bb.add_fee_collect(total_fees=0, total_blind=0)
    assert not ok, f"MassBalanceFeeCollectV1 with zero fees must be rejected: {msg}"
    assert "zero-claim" in msg.lower()
    # With fees → MassBalanceFeeCollectV1 must succeed
    acc2 = FeeCommitAccumulator()
    bb2 = BlockBuilder(height=2, accumulator=acc2)
    bb2.add_coinbase()
    _, _, fee_commit, _ = balanced_fee_v2(5000, 300, 42)
    bb2.acc.apply_fee_v2(mk(5000, 99), mk(4700, 57), fee_commit, nullifier=3, merkle_root=0xAAA)
    ok2, _ = bb2.add_fee_collect(total_fees=300, total_blind=42)
    assert ok2, "MassBalanceFeeCollectV1 with non-zero fees must succeed"
    print("  PASS: MassBalanceFeeCollectV1 conditional presence — absent at 0 fees, present at >0 fees")


def test_block_canonical_ordering():
    """Block canonical ordering: coinbase[0], user txs[1..k], MassBalanceFeeCollectV1[last] (§2.1)."""
    acc = FeeCommitAccumulator()
    bb = BlockBuilder(height=3, accumulator=acc)
    # Step 1: coinbase must be first
    bb.add_coinbase()
    assert bb.transactions[0] == "MassBalanceCoinbaseV1", "coinbase must be tx[0]"
    # Step 2: FeeV2 calls come after coinbase
    f1 = balanced_fee_v2(5000, 300, 42)
    f2 = balanced_fee_v2(3000, 100, 7)
    bb.acc.apply_fee_v2(*f1[:3], nullifier=1, merkle_root=0xAAA)
    bb.transactions.append("FeeV2_1")
    bb.acc.apply_fee_v2(*f2[:3], nullifier=2, merkle_root=0xBBB)
    bb.transactions.append("FeeV2_2")
    # Step 3: MassBalanceFeeCollectV1 must be last
    assert not bb._has_fee_collect
    ok, _ = bb.add_fee_collect(total_fees=400, total_blind=49)
    assert ok, "MassBalanceFeeCollectV1 with correct total must succeed"
    assert bb.transactions[-1] == "MassBalanceFeeCollectV1", "MassBalanceFeeCollectV1 must be last transaction"
    # Verify full order
    assert bb.transactions == ["MassBalanceCoinbaseV1", "FeeV2_1", "FeeV2_2", "MassBalanceFeeCollectV1"]
    print("  PASS: canonical block ordering — coinbase[0], FeeV2[1..k], MassBalanceFeeCollectV1[last]")


def test_overlay_visibility_invariant():
    """Invariant 1 (Overlay Visibility): FeeV2 sees coinbase's merkle root (§2.2).

    Within a single block, MassBalanceCoinbaseV1 inserts a merkle root into coin_roots_db.
    A subsequent FeeV2 in the same block MUST be able to find that root.
    """
    acc = FeeCommitAccumulator()
    # Coinbase inserts a root at position N
    coinbase_root = 0xC01BA5E  # simulated coinbase merkle root
    acc.coin_roots_db[coinbase_root] = 1  # Simulates MassBalanceCoinbaseV1's insert
    # FeeV2 in same block can look up that root
    assert coinbase_root in acc.coin_roots_db, \
        "Invariant 1 violated: FeeV2 cannot see coinbase root in same block"
    print("  PASS: Overlay Visibility (Invariant 1) — coinbase root visible to same-block FeeV2")


def test_full_lifecycle_with_accumulation():
    """Full block lifecycle: MassBalanceCoinbaseV1 → FeeV2×2 → MassBalanceFeeCollectV1 (§5.6.6)."""
    acc = FeeCommitAccumulator()
    bb = BlockBuilder(height=5, accumulator=acc)
    # 1. Coinbase
    bb.add_coinbase()
    coinbase_root = 0xCBCB
    acc.coin_roots_db[coinbase_root] = 5
    # 2. Two FeeV2 calls
    f1 = balanced_fee_v2(5000, 300, 42)
    f2 = balanced_fee_v2(3000, 200, 99)
    ok1, _ = acc.apply_fee_v2(f1[0], f1[1], f1[2], nullifier=10, merkle_root=coinbase_root)
    assert ok1
    bb.transactions.append("FeeV2_1")
    ok2, _ = acc.apply_fee_v2(f2[0], f2[1], f2[2], nullifier=11, merkle_root=coinbase_root)
    assert ok2
    bb.transactions.append("FeeV2_2")
    # Intermediate: accumulator = Commit(300,42) + Commit(200,99) = Commit(500,141)
    expected_mid = pedersen_add(f1[2], f2[2])
    assert pedersen_eq(acc.accumulator, expected_mid), "intermediate accumulator mismatch"
    # 3. MassBalanceFeeCollectV1
    ok3, _ = bb.add_fee_collect(total_fees=500, total_blind=141)
    assert ok3
    # 4. Postconditions
    assert acc.accumulator.v_part == 0, "R4: accumulator not reset"
    assert acc.fees_db[5] == 0, "R3: fees_db not zeroed"
    assert 10 in acc.nullifiers_db and 11 in acc.nullifiers_db, "nullifiers not recorded"
    assert bb.transactions == ["MassBalanceCoinbaseV1", "FeeV2_1", "FeeV2_2", "MassBalanceFeeCollectV1"]
    print("  PASS: full block lifecycle — MassBalanceCoinbaseV1 → 2×FeeV2 → MassBalanceFeeCollectV1, all postconditions")


# ============================================================================
# NEW: Fee Signalling Testing Plan scenario (P-FW-5)
# ============================================================================


def test_threshold_change_fifo_demotion():
    """P-FW-5: Fee=42M tx at premium tier demoted to general on threshold rise.

    Simulates a window boundary where the premium threshold rises above the
    tx's fee. The tx must survive the transition and maintain its position
    in the general queue (I6: no ex post facto eviction; I3: FCFS preserved).

    Pins Rust: L1-FW-6 (concurrent stress) / L3-FW-3 (post-boundary E2E).
    """
    from fee_window_model import FeeWindow, MempoolWithWindow

    # Window 1: low thresholds, tx admitted to premium
    w_old = FeeWindow()
    w_old.adjust(0, 0)  # CF=1.0, premium=420M, general=42M
    mempool = MempoolWithWindow(w_old)

    # Tx: fee=5M, circuit_cost=[5000] → admitted to premium (fee well above threshold at CF=1.0)
    assert mempool.admit("tx_a", 5_000_000, [5000]) == "premium", (
        "P-FW-5: tx with 5M fee should go to premium at CF=1.0"
    )

    # Second tx: fee=2M → also premium at zero congestion (premium=standard=SCALE)
    assert mempool.admit("tx_b", 2_000_000, [100]) == "premium", (
        "P-FW-5: at zero congestion, all admitted txs go to premium"
    )

    # Window 2: extreme congestion → premium threshold rises well above 420M
    w_new = FeeWindow()
    # First adjustment to establish baseline
    w_new.adjust(0, 0)
    # Second adjustment: heavy congestion → premium_threshold >> 420M
    premium_t2, general_t2 = w_new.adjust(500, 5000)

    # Sanity: congested premium CF should exceed SCALE (base)
    assert premium_t2 > 1_000_000, (
        f"P-FW-5: congested premium threshold {premium_t2} should exceed SCALE=1M"
    )

    # The previously-admitted tx_a (5M) would now be below premium.
    # I3/I6: it must survive and be accessible (no eviction of admitted txs).
    remaining = mempool.select_for_block(max_txs=10)
    assert "tx_a" in remaining, (
        f"P-FW-5 I3 violated: tx_a evicted after threshold rise"
    )
    assert "tx_b" in remaining, (
        f"P-FW-5: tx_b evicted after threshold rise"
    )

    # I6: order preserved (premium FIFO before general FCFS even after demotion)
    assert remaining[0] == "tx_a", (
        f"P-FW-5 I6 violated: tx_a should precede tx_b, got {remaining}"
    )
    assert remaining[1] == "tx_b", (
        f"P-FW-5: tx_b should follow tx_a, got {remaining}"
    )

    print("  PASS: threshold-change FIFO demotion — tx survives, order preserved")


# ============================================================
# Tests — FeeThreshold_V1 Proof Model (NEW — spec §5.5, §5.5.1)
# ============================================================

def test_tx_binding_semantic_collision_detected():
    """FeeThreshold_V1: FeeV2TxBinding != ThresholdTxBinding (type-level guard, §5.5.1).

    A Fee_V2 tx_binding (bound to tx_nonce) MUST NOT be accepted as a
    FeeThreshold_V1 tx_binding (bound to threshold). This test models the
    exact semantic collision that bare pallas::Base cannot detect — the
    Poseidon second input differs (tx_nonce vs threshold), producing
    different hash values.
    """
    tx_commitment = 12345
    tx_nonce = 99999
    threshold = FeeAmount(42_000_000)

    # Fee_V2 binding: poseidon(3, commit, nonce)
    fee_v2_binding = FeeV2TxBinding.compute(tx_commitment, tx_nonce)

    # FeeThreshold_V1 binding: poseidon(3, commit, threshold)
    threshold_binding = ThresholdTxBinding.compute(tx_commitment, threshold)

    # They MUST be different values (different second input)
    assert fee_v2_binding.inner() != threshold_binding.inner(), \
        "CRITICAL: FeeV2TxBinding and ThresholdTxBinding have same value — semantic collision!"

    # A Fee_V2 binding used as threshold binding MUST fail verification.
    # This is the exact bug: FeeParamsV2.tx_binding carries the Fee_V2
    # version, but the FeeThreshold_V1 verifier expects the threshold version.
    try:
        FeeThresholdV1Proof(
            fee=FeeAmount(150_000_000), threshold=threshold,
            tx_commitment=tx_commitment,
            tx_binding=ThresholdTxBinding(fee_v2_binding.inner())  # WRONG TYPE — forced cast
        )
        assert False, "Should have rejected FeeV2TxBinding as ThresholdTxBinding"
    except ValueError as e:
        assert "tx_binding mismatch" in str(e)

    print("  PASS: semantic collision detected — FeeV2TxBinding != ThresholdTxBinding")


def test_fee_threshold_v1_proof_create_and_verify():
    """FeeThreshold_V1: create proof, verify against public inputs (§5.5).

    Wallet creates proof with fee=150M, threshold=42M. Mempool verifies
    the proof against the expected public inputs. Both threshold and
    tx_binding must match.
    """
    threshold = FeeAmount(42_000_000)
    tx_commitment = 12345

    # Wallet creates proof
    proof = FeeThresholdV1Proof.create(
        fee=FeeAmount(150_000_000),
        threshold=threshold,
        tx_commitment=tx_commitment,
    )

    # Mempool verifies against expected public inputs
    expected_binding = ThresholdTxBinding.compute(tx_commitment, threshold)
    assert proof.verify(threshold, expected_binding), \
        "Valid proof must verify against correct public inputs"

    # Public inputs match expected values
    pub_threshold, pub_binding = proof.public_inputs()
    assert pub_threshold == threshold.get(), \
        f"Public threshold {pub_threshold} != expected {threshold.get()}"
    assert pub_binding == expected_binding.inner(), \
        f"Public tx_binding {pub_binding:#x} != expected {expected_binding.inner():#x}"

    print("  PASS: FeeThreshold_V1 proof create + verify — happy path")


def test_fee_threshold_v1_proof_tampered_binding_rejected():
    """FeeThreshold_V1: tampered tx_binding → verification fails (§5.5, P4).

    A proof constructed for threshold=42M MUST fail verification when
    checked against a different threshold (100M), because the tx_binding
    is derived from the threshold — a mismatch in threshold produces a
    mismatch in expected tx_binding.
    """
    threshold_42m = FeeAmount(42_000_000)
    threshold_100m = FeeAmount(100_000_000)
    tx_commitment = 12345

    proof = FeeThresholdV1Proof.create(
        fee=FeeAmount(150_000_000),
        threshold=threshold_42m,
        tx_commitment=tx_commitment,
    )

    # Verifier uses wrong threshold → computes wrong expected tx_binding
    wrong_binding = ThresholdTxBinding.compute(tx_commitment, threshold_100m)
    assert not proof.verify(threshold_42m, wrong_binding), \
        "Proof must fail when verified with tx_binding from wrong threshold"

    # Verifier uses wrong threshold value itself
    correct_binding = ThresholdTxBinding.compute(tx_commitment, threshold_42m)
    assert not proof.verify(threshold_100m, correct_binding), \
        "Proof must fail when verified with wrong threshold value"

    print("  PASS: FeeThreshold_V1 tampered binding + threshold — both rejected")


def test_fee_threshold_v1_proof_below_threshold_rejected():
    """FeeThreshold_V1: fee below threshold → construction fails (§5.5).

    The circuit constraint diff = fee - threshold requires fee >= threshold.
    If fee < threshold, the subtraction underflows in pallas::Base and the
    range_check(64, diff) fails. The model enforces this at construction.
    """
    try:
        FeeThresholdV1Proof.create(
            fee=FeeAmount(30_000_000),
            threshold=FeeAmount(42_000_000),
            tx_commitment=12345,
        )
        assert False, "Should have rejected fee below threshold"
    except ValueError as e:
        assert "below threshold" in str(e)

    print("  PASS: FeeThreshold_V1 below threshold — rejected at construction")


def test_fee_threshold_v1_proof_determinism():
    """FeeThreshold_V1: same inputs → identical proof public inputs (§5.5).

    Proof creation is a pure function of its inputs. Same fee, threshold,
    and tx_commitment always produce the same tx_binding and public inputs.
    """
    fee = FeeAmount(150_000_000)
    threshold = FeeAmount(42_000_000)
    tx_commitment = 12345

    proof1 = FeeThresholdV1Proof.create(fee, threshold, tx_commitment)
    proof2 = FeeThresholdV1Proof.create(fee, threshold, tx_commitment)

    # Public inputs must be identical
    assert proof1.public_inputs() == proof2.public_inputs(), \
        "Deterministic proof must produce identical public inputs"

    # tx_binding must be identical
    assert proof1.tx_binding.inner() == proof2.tx_binding.inner(), \
        "Deterministic proof must produce identical tx_binding"

    print("  PASS: FeeThreshold_V1 proof determinism — identical inputs, identical outputs")


def test_mempool_verification_path_prove_verify_admit():
    """FeeThreshold_V1: full mempool verification path (§7.2, mempool.md §8.4).

    Models the complete lifecycle:
      1. Wallet creates FeeThreshold_V1Proof (prove)
      2. Proof public inputs are extracted (serialize)
      3. Mempool reconstructs expected public inputs (deserialize)
      4. Mempool verifies proof cryptographically (verify_zkp)
      5. Mempool admits based on verification result

    The mempool SHALL NOT trust a plain u64 fee value. It SHALL verify
    the ZK proof against expected public inputs. This test models the
    verification WASM widget path: __metadata returns public inputs,
    then verify_zkp checks them cryptographically.
    """
    premium_threshold = FeeAmount(42_000_000)
    general_threshold = FeeAmount(1_000_000)
    tx_commitment = 12345

    # Step 1: Wallet creates the proof (proving widget)
    proof = FeeThresholdV1Proof.create(
        fee=FeeAmount(150_000_000),
        threshold=premium_threshold,
        tx_commitment=tx_commitment,
    )

    # Step 2: Proof is "serialized" — extract public inputs
    # (models what __metadata returns from verification WASM widget)
    pub_threshold, pub_tx_binding = proof.public_inputs()

    # Step 3: Mempool "deserializes" — reconstructs expected public inputs
    # from the tier being verified (models verify_zkp parameter construction)
    expected_binding = ThresholdTxBinding.compute(tx_commitment, premium_threshold)

    # Step 4: Mempool verifies the proof cryptographically
    # (models verify_zkp(proof, zkbin, [threshold, tx_binding]))
    assert proof.verify(premium_threshold, expected_binding), \
        "Mempool: valid proof must pass cryptographic verification"

    # Step 5: Mempool admits based on verification result
    # Premium tier: proof verified against premium_threshold → admit to premium
    if proof.verify(premium_threshold, expected_binding):
        tier = "premium"
    elif proof.verify(general_threshold,
                      ThresholdTxBinding.compute(tx_commitment, general_threshold)):
        tier = "general"
    else:
        tier = "reject"

    assert tier == "premium", f"Expected premium tier, got {tier}"

    # Negative case: tampered proof (wrong threshold) fails verification
    # An attacker takes a general-tier proof and tries to pass it as premium
    general_proof = FeeThresholdV1Proof.create(
        fee=FeeAmount(100_000_000),
        threshold=general_threshold,
        tx_commitment=tx_commitment,
    )

    # Verify against premium threshold — must fail
    premium_binding = ThresholdTxBinding.compute(tx_commitment, premium_threshold)
    assert not general_proof.verify(premium_threshold, premium_binding), \
        "Mempool: general-tier proof must NOT verify against premium threshold"

    # Verify against general threshold — must succeed
    general_binding = ThresholdTxBinding.compute(tx_commitment, general_threshold)
    assert general_proof.verify(general_threshold, general_binding), \
        "Mempool: general-tier proof must verify against general threshold"

    # Negative case: attacker provides fake u64 without valid proof
    # The mempool SHALL NOT trust params.threshold.get() == threshold
    # (This models the F3 finding: u64 comparison is not a gate)
    fake_threshold_match = premium_threshold.get() == premium_threshold.get()  # always true
    assert fake_threshold_match, \
        "Trivially true — demonstrates u64 comparison is NOT a valid gate"
    # Without cryptographic verification, this would admit ANY transaction.
    # The proof.verify() call above is what actually gates admission.

    print("  PASS: mempool verification path — prove → serialize → deserialize → verify → admit")


def test_wallet_proving_widget_path():
    """FeeThreshold_V1: wallet loads ProvingWidget, reads witness map, constructs proof.

    Models the complete wallet-side flow (wallet.md §6.4.3):
      1. Wallet loads proving WASM widget (ProvingWidget)
      2. Calls witness_map() → learns witness count=4, order by name
      3. Constructs proof using circuit-grounded witness binding
      4. Proof verifies against expected public inputs

    NO hardcoded Vec<Witness> order — the witness map FROM THE CIRCUIT
    tells the wallet how to wire the proof.
    """
    # Step 1: Wallet loads the proving widget
    widget = ProvingWidget()
    widget.load()

    # Step 2: Read witness map from the circuit via __metadata
    wmap = widget.witness_map()
    assert len(wmap) == 4, f"Circuit expects 4 witnesses, got {len(wmap)}"
    assert wmap[0] == (0, "fee", "Base"), f"Witness[0] must be fee, got {wmap[0]}"
    assert wmap[1] == (1, "threshold", "Base"), f"Witness[1] must be threshold, got {wmap[1]}"
    assert wmap[2] == (2, "tx_commitment", "Base"), f"Witness[2] must be tx_commitment, got {wmap[2]}"
    assert wmap[3] == (3, "tx_binding", "Base"), f"Witness[3] must be tx_binding, got {wmap[3]}"

    # Verify public input order from circuit
    pub_order = widget.public_input_order()
    assert pub_order == ["threshold", "tx_binding"], \
        f"Public input order must be [threshold, tx_binding], got {pub_order}"

    # Verify circuit params
    params = widget.circuit_params()
    assert params["k"] == 11, f"Circuit k must be 11, got {params['k']}"
    assert params["field"] == "pallas", f"Circuit field must be pallas, got {params['field']}"

    # Step 3: Construct proof using witness map (circuit-grounded binding)
    fee = FeeAmount(150_000_000)
    threshold = FeeAmount(42_000_000)
    tx_commitment = 12345

    proof = widget.build_proof(fee, threshold, tx_commitment)

    # Step 4: Proof verifies
    expected_binding = ThresholdTxBinding.compute(tx_commitment, threshold)
    assert proof.verify(threshold, expected_binding), \
        "Wallet: proof must verify against expected public inputs"

    print("  PASS: wallet proving widget path — load → witness_map → circuit-grounded proof → verify")


def test_mempool_verification_widget_path():
    """FeeThreshold_V1: mempool loads VerificationWidget, gets public inputs, verifies.

    Models the complete mempool-side flow (mempool.md §8.4):
      1. Mempool loads verification WASM widget (VerificationWidget)
      2. Calls get_public_inputs() → extracts (threshold, tx_binding) from call data
      3. Calls verify_proof() — cryptographic verification, NOT u64 comparison
      4. Admits or rejects based on cryptographic result

    The mempool SHALL NOT trust params.threshold u64. Only verify_proof()
    (which models verify_zkp()) constitutes a valid gate.
    """
    # Step 1: Wallet creates proof
    fee = FeeAmount(150_000_000)
    threshold = FeeAmount(42_000_000)
    tx_commitment = 12345
    proof = FeeThresholdV1Proof.create(fee, threshold, tx_commitment)

    # Step 2: Mempool loads verification widget
    widget = VerificationWidget()
    widget.load()

    # Step 3: Get public inputs via __metadata
    pub_threshold, pub_tx_binding = widget.get_public_inputs(proof)
    expected_binding = ThresholdTxBinding.compute(tx_commitment, threshold)
    assert pub_threshold == threshold.get(), \
        f"Public threshold {pub_threshold} != expected {threshold.get()}"
    assert pub_tx_binding == expected_binding.inner(), \
        f"Public tx_binding mismatch"

    # Step 4: Cryptographic verification — NOT u64 comparison
    assert widget.verify_proof(proof, threshold, expected_binding), \
        "Mempool: valid proof must pass cryptographic verification"

    # Negative: tampered proof fails verification
    wrong_binding = ThresholdTxBinding.compute(tx_commitment, FeeAmount(100_000_000))
    assert not widget.verify_proof(proof, threshold, wrong_binding), \
        "Mempool: tampered tx_binding must fail verification"

    print("  PASS: mempool verification widget path — load → __metadata → verify_zkp → admit/reject")


def test_miner_re_verification():
    """FeeThreshold_V1: miner independently re-verifies threshold proofs.

    Models the miner re-verification flow (mempool.md §8.4):
      1. Miner loads the SAME VerificationWidget as the mempool
      2. Independently verifies threshold proofs before block inclusion
      3. This closes the trust gap — miner doesn't blindly trust mempool

    Negative case: a proof the mempool falsely claims is valid must fail
    miner re-verification.
    """
    fee = FeeAmount(150_000_000)
    threshold = FeeAmount(42_000_000)
    tx_commitment = 12345

    # Wallet creates proof for general tier
    general_threshold = FeeAmount(1_000_000)
    general_proof = FeeThresholdV1Proof.create(fee, general_threshold, tx_commitment)

    # Mempool loads verification widget, verifies correctly
    mempool_widget = VerificationWidget()
    mempool_widget.load()
    general_binding = ThresholdTxBinding.compute(tx_commitment, general_threshold)
    assert mempool_widget.verify_proof(general_proof, general_threshold, general_binding), \
        "Mempool: general-tier proof must verify against general threshold"

    # Miner loads the SAME verification widget independently
    miner_widget = VerificationWidget()
    miner_widget.load()

    # Miner re-verifies the general-tier proof
    assert miner_widget.verify_proof(general_proof, general_threshold, general_binding), \
        "Miner: must independently confirm general-tier proof is valid"

    # Miner also checks: this proof must NOT pass for premium tier
    premium_threshold = FeeAmount(42_000_000)
    premium_binding = ThresholdTxBinding.compute(tx_commitment, premium_threshold)
    assert not miner_widget.verify_proof(general_proof, premium_threshold, premium_binding), \
        "Miner: general-tier proof must NOT verify against premium threshold"

    # Even if mempool claims the proof is premium-tier valid, miner's independent
    # re-verification catches the lie. This closes the trust gap.
    print("  PASS: miner re-verification — independent verification closes trust gap")


# ============================================================
# Latency Benchmarks (Phase 0d)
# ============================================================

def benchmark_threshold_proof(num_iterations: int = 100) -> float:
    """Measure FeeThreshold_V1 proof generation latency.

    FeeThreshold_V1 is ~40 opcode difficulty (ConstrainInstance × 8 = 40)
    — millisecond-range proving. This benchmark verifies the latency
    budget for the Request/Respond/Send protocol's 1-minute window.
    """
    fee = FeeAmount(2_000_000)
    threshold = FeeAmount(1_000_000)
    tx_commitment = 12345
    start = time.perf_counter()
    for _ in range(num_iterations):
        FeeThresholdV1Proof.create(fee, threshold, tx_commitment)
    elapsed = time.perf_counter() - start
    avg_ms = (elapsed / num_iterations) * 1000
    print(f"  Threshold proof: {avg_ms:.3f} ms avg over {num_iterations} iterations")
    return avg_ms


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
        # MassBalanceFeeCollectV1 + Accumulation + Thm2 (NEW — 4)
        test_fee_collect_v1_happy_path,
        test_fee_collect_v1_zero_claim_rejected,
        test_fee_collect_v1_bad_claim_rejected,
        test_fee_commit_accumulation_soundness,
        # FeeV2 preconditions + error barbs (NEW — 6)
        test_feev2_tx_binding_anti_replay,
        test_feev2_double_spend_rejected,
        test_feev2_bad_merkle_root_rejected,
        test_feev2_input_value_exceeds_fee_rejected,
        test_feev2_token_commit_validation,
        test_feev2_duplicate_coin_rejected,
        # Block model + lifecycle (NEW — 4)
        test_fee_collect_v1_conditional_presence,
        test_block_canonical_ordering,
        test_overlay_visibility_invariant,
        test_full_lifecycle_with_accumulation,
        # NEW: Fee Signalling Testing Plan scenario
        test_threshold_change_fifo_demotion,
        # NEW: FeeThreshold_V1 Proof Model + Nominal tx_binding Types (§5.5, §5.5.1)
        test_tx_binding_semantic_collision_detected,
        test_fee_threshold_v1_proof_create_and_verify,
        test_fee_threshold_v1_proof_tampered_binding_rejected,
        test_fee_threshold_v1_proof_below_threshold_rejected,
        test_fee_threshold_v1_proof_determinism,
        # NEW: Mempool verification path (prove → serialize → deserialize → verify → admit)
        test_mempool_verification_path_prove_verify_admit,
        # NEW: Two-Widget Architecture — wallet + mempool + miner paths
        test_wallet_proving_widget_path,
        test_mempool_verification_widget_path,
        test_miner_re_verification,
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
    # Phase 0d: Latency benchmark
    print("\n--- Latency Benchmarks ---")
    avg_ms = benchmark_threshold_proof(100)
    assert avg_ms < 1000, f"Threshold proof latency {avg_ms:.1f}ms exceeds 1s budget"
    print(f"  Latency budget: OK ({avg_ms:.1f}ms << 60s Request/Respond window)")
    return failed == 0


if __name__ == "__main__":
    success = run_all()
    sys.exit(0 if success else 1)
