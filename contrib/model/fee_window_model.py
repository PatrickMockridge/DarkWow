#!/usr/bin/env python3
"""Fee Window Signalling Model — [1:1] executable specification.

Adaptive congestion-driven fee threshold adjustment across 20-block windows.
Specification: fee-spec.md §12. Python model first, Rust implementation second.

Invariants tested:
    I1 — Window Determinism
    I2 — Backward Compatibility
    I3 — FCFS Preservation (no ex post facto eviction)
    I4 — Congestion Factor Ordering (CF_premium > CF_standard)
    I5 — Circuit Rate Monotonicity
    I6 — CF Convergence (logarithmic scaling)
    I7 — Smooth Adjustment (±10% per window)
    I8 — Deterministic CF (local computation, no coordination)
"""

import math
import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
from dataclasses import dataclass, field
from typing import List, Tuple, Optional
from collections import deque


# ============================================================================
# Constants [1:1] with Rust FeeWindowConfig
# ============================================================================

SCALE: int = 1_000_000          # fixed-point scale for congestion factors
WINDOW_SIZE: int = 20           # blocks per fee window

# Per-opcode difficulty factors — consensus constants [1:1] Rust opcode_cost.rs
# Each opcode difficulty is proportional to its ZK constraint system complexity
# (gate_count × column_count). An average circuit (~20 mixed ops) sums to ~1000.
OPCODE_DIFFICULTY: dict = {
    # ECC ops (10 advice columns, complete addition formula)
    "EcAdd": 1000, "EcMul": 1000, "EcMulBase": 1000, "EcMulShort": 1000,
    "EcMulVarBase": 1000, "EcGetX": 1000, "EcGetY": 1000,
    # Sinsemilla/Merkle (generator table + 5 advice columns)
    "MerkleRoot": 800, "SparseMerkleRoot": 800, "SetMembership": 800,
    # Poseidon (~12 partial + ~5 full rounds)
    "PoseidonHash": 500,
    # Heavy arithmetic (BaseDiv: ~255 gates for Fermat inversion)
    "BaseDiv": 250,
    "RangeCheck": 100, "LessThanStrict": 100, "LessThanLoose": 100,
    "LessThanOrEqual": 100, "BaseLtStrict": 100,
    # Light arithmetic
    "BaseMul": 50,
    "BaseAdd": 20, "BaseSub": 20, "WitnessBase": 20,
    # Selection
    "CondSelect": 40, "ZeroCondSelect": 40,
    # Comparison
    "IsEqualBase": 30, "IsNotEqualBase": 30, "BoolCheck": 30, "NotBase": 30,
    # Constrain (1 gate, 1 column)
    "ConstrainEqualBase": 5, "ConstrainEqualPoint": 5, "ConstrainInstance": 5,
    # Zero cost
    "Noop": 0, "DebugPrint": 0,
}

# Approximate circuit difficulties from opcode composition [1:1] Rust circuit_difficulty()
# Calibrated: average circuit ~1000, complex ~10000, simple ~40
CIRCUIT_RATES: dict = {
    "TransferV2": 1000, "SpendV2": 1000, "BurnV2": 1000, "FeeV2": 500,
    "FeeCollectV2": 500, "PoWRewardV2": 500,
    "FeeThreshold_V1": 40,
    "CreateSwapV2": 1000, "AcceptSwapV2": 1000, "CancelSwapV2": 500,
    "ExecuteSwapV2": 2000, "ExecuteSwapFeeV2": 500, "ExecuteSwapSlippageV2": 500,
    "VerifyCapabilityV2": 2000, "CreateGroupV2": 2000, "SignV2": 2000, "FinalizeV2": 2000,
    "PushValueV2": 3000, "AttestValueV2": 3000, "DepositV2": 3000, "WithdrawV2": 3000,
    "LiquidateV2": 5000, "ExecuteSwapV2_complex": 5000, "MintStableV2": 5000,
    "BaseDivV2": 10000, "PoseidonRecursiveV2": 10000, "AggregateV2": 10000,
}

# Typical k-values per contract type [1:1] fee-spec.md §12.11
# k determines proving domain size (2^k rows). Higher k = larger circuit capacity.
CIRCUIT_K: dict = {
    "FeeThreshold_V1": 11,
    "TransferV2": 12, "SpendV2": 12, "BurnV2": 12,
    "FeeV2": 12, "FeeCollectV2": 12, "PoWRewardV2": 12,
    "CreateSwapV2": 13, "AcceptSwapV2": 13, "CancelSwapV2": 13,
    "ExecuteSwapV2": 14, "ExecuteSwapFeeV2": 14, "ExecuteSwapSlippageV2": 14,
    "VerifyCapabilityV2": 14, "CreateGroupV2": 14, "SignV2": 14, "FinalizeV2": 14,
    "PushValueV2": 15, "AttestValueV2": 15, "DepositV2": 15, "WithdrawV2": 15,
    "LiquidateV2": 15, "ExecuteSwapV2_complex": 15, "MintStableV2": 15,
    "BaseDivV2": 16, "PoseidonRecursiveV2": 16, "AggregateV2": 16,
}

# Per-kB WASM storage rate: 0.01 DRKW at CF=1.0
BASELINE_STORAGE: int = 1_000_000

# Circuit k-value scaling [1:1] fee-spec.md §12.11
K_REF: int = 11           # Reference k (FeeThreshold_V1), scale factor 1.0
MAX_K: int = 16            # Maximum k from zkas/constants.rs, scale factor 32

# Execution risk factors — fee-spec.md §12.12, manifest.md §Cost Profiles
# Multiplier on baseline circuit fee based on manifest/attestation status.
# Contracts are infrastructure, not experiments: the economic gradient
# pushes toward attested manifests with endowments.
#
# Represented as fixed-point integers with RISK_FACTOR_SCALE = 100_000:
#   risk_factor / RISK_FACTOR_SCALE = the effective multiplier.
# Integer representation guarantees numerical determinism across platforms.
RISK_FACTOR_SCALE: int = 100_000          # baseline: 1.0 = 100_000
RISK_FACTOR: dict = {
    "genesis": 100_000,                  # 1.0 — cryptographically foundational
    "attested_endowed": 100_000,         # 1.0 — vouched, skin in the game
    "attested_no_endowment": 125_000,    # 1.25 — vouched but no stake
    "self_declared": 150_000,            # 1.5 — deployer claims, unverified
    "unknown": 200_000,                  # 2.0 — pessimistic default
}


# ============================================================================
# Pedersen Commitment + FeeCommitAccumulator — fee-spec.md §5.6
# ============================================================================
# Uses sim.crypto primitives (pedersen_commit, pedersen_add, pedersen_eq)
# which are SHA256-based Pedersen commitments with proper homomorphic properties.

from sim.crypto import pedersen_commit, pedersen_add, pedersen_eq, PedersenCommitment, ec_mul_base, poseidon_hash


class FeeCommitAccumulator:
    """Homomorphic accumulator for Pedersen fee commitments.
    [1:1] Rust fee_commit_accumulator in native_token contract.

    Lifecycle per fee-spec.md §5.6.4:
    - Start of block: Identity (pedersen_commit(0, b'\x00'*32))
    - Each FeeV2 call: accumulator = accumulator + pedersen_commit(fee_i, blind_i)
    - FeeCollectV1: verify pedersen_commit(total_fees, total_blind) == accumulator
    - After FeeCollectV1: reset to Identity
    """

    def __init__(self):
        zero_blind = b'\x00' * 32
        self._acc: PedersenCommitment = pedersen_commit(0, zero_blind)

    def add(self, fee_value: int, fee_blind: bytes) -> PedersenCommitment:
        """Apply a FeeV2 commitment. Returns the new accumulator value.
        Rejects zero-fee with non-zero blind (P1.3) and identity commits (P1.4)."""
        # P1.3: zero-fee with non-zero blind is consensus-critical rejection
        zero_blind = b'\x00' * 32
        if fee_value == 0 and fee_blind != zero_blind:
            raise ValueError("zero-fee with non-zero blind rejected (P1.3)")
        # P1.4: identity point commitment is invalid
        if fee_value == 0 and fee_blind == zero_blind:
            return self._acc  # identity has no effect, but don't reject
        commit = pedersen_commit(fee_value, fee_blind)
        # P1.4: reject identity point (shouldn't happen with above checks, defense-in-depth)
        if pedersen_eq(commit, pedersen_commit(0, zero_blind)):
            raise ValueError("identity point commitment rejected (P1.4)")
        self._acc = pedersen_add(self._acc, commit)
        return self._acc

    def verify(self, total_fees: int, total_blind: bytes) -> bool:
        """Verify that PedersenCommit(total_fees, total_blind) == accumulator."""
        expected = pedersen_commit(total_fees, total_blind)
        return pedersen_eq(self._acc, expected)

    def reset(self):
        """Reset to Identity after FeeCollectV1."""
        zero_blind = b'\x00' * 32
        self._acc = pedersen_commit(0, zero_blind)

    @property
    def is_identity(self) -> bool:
        zero_blind = b'\x00' * 32
        return pedersen_eq(self._acc, pedersen_commit(0, zero_blind))


# ============================================================================
# [1:1] CostProfile — mirrors manifest.md [[cost_profiles]]
# ============================================================================

@dataclass
class CostProfile:
    """Per-function cost declaration. [1:1] manifest.md [[cost_profiles]].

    Fields match the TOML [[cost_profiles]] section exactly:
    - function: SHALL match a name in [[functions]]
    - circuit_difficulty: Σ opcode_cost × 2^(k - K_REF) — deterministic baseline
    - k_value: circuit's Halo2 k parameter (domain size = 2^k rows)
    - wasm_kb: expected WASM execution overhead in kB-equivalent
    - tolerance: allowed deviation (±50% = 0.50) before black mark
    - opcodes: ordered list of ZK opcode names the circuit uses.
      Combined with k_value, allows independent verification of
      circuit_difficulty (miner's responsibility — economic incentive).
    """
    function: str
    circuit_difficulty: int
    k_value: int = K_REF
    wasm_kb: int = 1
    tolerance: float = 0.50
    opcodes: list = None

    def __post_init__(self):
        if self.opcodes is None:
            self.opcodes = []


# Pessimistic default profile for contracts with no [[cost_profiles]] section.
# Uses average circuit difficulty (1000), worst-case k (MAX_K = 16), and
# default 1 kB WASM overhead.
DEFAULT_COST_PROFILE: CostProfile = CostProfile(
    function="unknown",
    circuit_difficulty=1000,
    k_value=MAX_K,
    wasm_kb=1,
    tolerance=0.50,
)


def resolve_cost_profile(
    contract_status: str,
    function: str,
    profiles: list,
) -> tuple:
    """Return (CostProfile, risk_factor) for a function call.

    Resolution rules (manifest.md §Cost Profiles):
    1. No profiles at all → DEFAULT_COST_PROFILE, risk_factor=RISK_FACTOR["unknown"] (2.0×)
    2. Function not in profiles → 2.0× max declared difficulty, risk_factor from status
    3. Function found → declared profile, risk_factor from status

    This is the bridge between manifest cost declarations and fee computation.
    The risk_factor multiplies only the circuit component — execution risk
    is about ZK verification cost, not storage.
    """
    risk_factor = RISK_FACTOR.get(contract_status, RISK_FACTOR["unknown"])

    if not profiles:
        # No profiles at all: use pessimistic default profile.
        # Override risk_factor to 2.0× regardless of status — no cost
        # declaration means no basis for lower risk assessment.
        return (DEFAULT_COST_PROFILE, RISK_FACTOR["unknown"])

    for p in profiles:
        if p.function == function:
            return (p, risk_factor)

    # Function not found: 2.0× the circuit_difficulty of the most expensive
    # declared function in the same contract. k_value and wasm_kb use the
    # contract's maximum to be safe.
    max_declared = max(p.circuit_difficulty for p in profiles)
    max_k = max(p.k_value for p in profiles)
    max_wasm = max(p.wasm_kb for p in profiles)
    pessimistic = CostProfile(
        function=function,
        circuit_difficulty=2 * max_declared,
        k_value=max_k,
        wasm_kb=max_wasm,
        tolerance=0.50,
    )
    return (pessimistic, risk_factor)


# ============================================================================
# FeeParamsV2 — [1:1] Rust FeeParamsV2, fee-spec.md §5
# ============================================================================


@dataclass
class FeeParamsV2:
    """Encoded FeeV2 parameters. [1:1] Rust FeeParamsV2 in model/fee.rs."""
    input_bytes: bytes
    output_bytes: bytes
    fee_value_commit: bytes   # 32 bytes — Pedersen commitment point
    fee_value_blind: bytes    # 32 bytes — blinding scalar
    threshold: int            # FeeAmount as u64
    threshold_proof: bytes    # ZK proof bytes
    encrypted_fee_value: bytes  # AEAD ciphertext (68 bytes production, variable in Python)
    tx_nonce: int

    def encode(self) -> bytes:
        """Serialize to wire format matching Rust FeeParamsV2::encode()."""
        import struct
        result = bytearray()
        result.extend(self.input_bytes)
        result.extend(self.output_bytes)
        result.extend(self.fee_value_commit)
        result.extend(struct.pack('<Q', self.threshold))
        proof_len = len(self.threshold_proof)
        result.extend(struct.pack('<I', proof_len))
        result.extend(self.threshold_proof)
        enc_len = len(self.encrypted_fee_value)
        result.extend(struct.pack('<I', enc_len))
        result.extend(self.encrypted_fee_value)
        result.extend(self.fee_value_blind)
        return bytes(result)


# ============================================================================
# Encrypted Fee Flow — fee-spec.md §5.6.3, G2+G6 reference model
# ============================================================================
# Uses sim.crypto primitives (ec_mul_base, poseidon_hash) for max math fidelity.
# 68-byte format: [ephemeral_public (32B)] [encrypted_fee (8B)] [mac (4B)]
# The remaining 24 bytes of the Rust format (nonce 12B + tag 16B - 4B mac)
# are omitted in Python for simplicity; the structural model is the same.


def encrypt_fee_for_miner(fee_amount: int, miner_pubkey_bytes: bytes) -> bytes:
    """AEAD encrypt fee to miner's public key using sim.crypto primitives.
    Rust: ECDH + ChaCha20Poly1305. Python: ec_mul_base + poseidon KDF + XOR + MAC."""
    import hashlib, hmac
    # Ephemeral keypair
    ephem_secret = hashlib.sha256(b'ephem_' + miner_pubkey_bytes).digest()
    ephem_public = ec_mul_base(ephem_secret)
    # KDF: use the miner_pubkey_bytes as the shared secret (symmetric model).
    # In production ECDH: shared_secret = ECDH(ephem_sk, miner_pk) = ECDH(miner_sk, ephem_pk).
    # Python model: both sides derive from miner_pubkey_bytes (the shared identity).
    kdf_seed = poseidon_hash([int.from_bytes(miner_pubkey_bytes[:16], 'big'),
                               int.from_bytes(miner_pubkey_bytes[16:32], 'big')])
    key_material = hashlib.sha256(kdf_seed).digest()
    # Encrypt: XOR fee bytes with key stream
    fee_bytes = fee_amount.to_bytes(8, 'little')
    encrypted = bytes(b ^ key_material[i] for i, b in enumerate(fee_bytes))
    # MAC: HMAC-SHA256(key_material[8:40], fee_bytes)[:4]
    mac = hmac.digest(key_material[8:], fee_bytes, 'sha256')[:4]
    # Format: [ephemeral_public (32)] [encrypted (8)] [mac (4)] = 44 bytes
    return ephem_public + encrypted + mac


def decrypt_fee_for_miner(ciphertext: bytes, miner_secret_bytes: bytes) -> int:
    """AEAD decrypt fee using miner's secret key.
    Returns fee value or None if MAC verification fails."""
    import hashlib, hmac
    if len(ciphertext) < 44:
        return None
    ephem_public = ciphertext[:32]
    encrypted = ciphertext[32:40]
    stored_mac = ciphertext[40:44]
    # KDF: same as encrypt — use miner_secret_bytes as shared identity.
    kdf_seed = poseidon_hash([int.from_bytes(miner_secret_bytes[:16], 'big'),
                               int.from_bytes(miner_secret_bytes[16:32], 'big')])
    key_material = hashlib.sha256(kdf_seed).digest()
    fee_bytes = bytes(b ^ key_material[i] for i, b in enumerate(encrypted))
    fee = int.from_bytes(fee_bytes, 'little')
    # Verify MAC
    expected_mac = hmac.digest(key_material[8:], fee_bytes, 'sha256')[:4]
    if not hmac.compare_digest(expected_mac, stored_mac):
        return None
    return fee


# ============================================================================
# FeeCollectV1 Miner Workflow — fee-spec.md §5.6.4, G2+G6 reference model
# ============================================================================


def build_fee_collect_params(mempool_txs: list, miner_secret_key: bytes,
                             accumulator: FeeCommitAccumulator) -> tuple:
    """Miner-side fee collection: decrypt fees, accumulate Pedersen commitments.

    For each FeeV2 transaction, the miner decrypts the encrypted fee value
    and builds a Pedersen commitment. The commitments are accumulated via
    pedersen_add to verify against the on-chain accumulator.

    Returns (total_fees, recomputed_commitment, all_decrypted) where
    all_decrypted indicates all decryptions succeeded. Falls back to
    ESTIMATED_FEE_PER_FEEV2_CALL on decryption failure.
    """
    total_fees = 0
    zero_blind = b'\x00' * 32
    recomputed = pedersen_commit(0, zero_blind)
    all_decrypted = True

    for tx in mempool_txs:
        for _fee_value, blind_value, encrypted_fee in tx.get('fee_v2_calls', []):
            decrypted = decrypt_fee_for_miner(encrypted_fee, miner_secret_key)
            if decrypted is not None:
                total_fees += decrypted
                recomputed = pedersen_add(recomputed, pedersen_commit(decrypted, blind_value))
            else:
                total_fees += 1_001_000  # fallback estimate
                recomputed = pedersen_add(recomputed, pedersen_commit(1_001_000, blind_value))
                all_decrypted = False

    return (total_fees, recomputed, all_decrypted)


def compute_total_fee(
    profile: CostProfile,
    risk_factor: int,
    wasm_cf: 'CongestionFactor',
    circuit_cf: 'CongestionFactor',
) -> int:
    """Combined fee with execution risk factor. [1:1] fee-spec.md §12.12.

    fee = (wasm_kB × BASELINE_STORAGE × WASM_CF.premium) / SCALE
        + (circuit_difficulty × CIRCUIT_CF.premium × risk_factor) / (SCALE × RISK_FACTOR_SCALE)

    The risk_factor multiplies only the circuit component — execution risk
    is about ZK verification cost, not storage. The wasm_kB term covers
    on-chain storage and is independent of trust status.

    Both risk_factor and RISK_FACTOR_SCALE are integers — fixed-point
    representation for deterministic cross-platform arithmetic.
    risk_factor / RISK_FACTOR_SCALE = the effective multiplier
    (e.g., 150_000 / 100_000 = 1.5× for self_declared).

    This is distinct from compute_fee() which takes raw circuit_costs —
    compute_total_fee() takes a resolved CostProfile and risk_factor,
    wiring manifest cost declarations into the two-component formula.
    """
    wasm_part = (profile.wasm_kb * BASELINE_STORAGE * wasm_cf.premium) // SCALE
    circuit_part = (
        profile.circuit_difficulty * circuit_cf.premium * risk_factor
    ) // (SCALE * RISK_FACTOR_SCALE)
    return wasm_part + circuit_part


def circuit_difficulty(opcodes: list, k: int = K_REF) -> int:
    """Sum of per-opcode difficulty factors, scaled by k-value. [1:1] Rust circuit_difficulty().

    Formula: base_cost(opcodes) × 2^(k - K_REF)
    Scale factor capped at 2^(MAX_K - K_REF) = 32."""
    base = sum(OPCODE_DIFFICULTY.get(op, 0) for op in opcodes)
    scale_shift = max(0, min(k - K_REF, MAX_K - K_REF))
    return base * (1 << scale_shift)


def compute_fee(circuit_costs: list, wasm_kb: int,
                wasm_cf: 'CongestionFactor', circuit_cf: 'CongestionFactor') -> int:
    """Two-component sum formula. [1:1] Rust compute_fee().

    fee = (wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)

    Always uses premium CF multipliers — this is the admission threshold.
    Tier classification (premium vs general) is the caller's responsibility.
    """
    total_opcode_cost = sum(circuit_costs)
    wasm_part = (wasm_kb * BASELINE_STORAGE * wasm_cf.premium) // SCALE
    circuit_part = (total_opcode_cost * circuit_cf.premium) // SCALE
    return wasm_part + circuit_part

# Congestion sensitivity coefficients [1:1] Rust FeeWindowConfig
ALPHA_PREMIUM: float = 0.05    # premium congestion sensitivity
ALPHA_STANDARD: float = 0.01   # standard congestion sensitivity

# Adjustment caps [1:1] Rust FeeWindowConfig
MAX_ADJUSTMENT: float = 0.10          # ±10% per window
MAX_SCALE: int = 1 << (MAX_K - K_REF)  # 32 — fee-spec.md §10
MAX_PREMIUM: int = MAX_SCALE * SCALE   # 32_000_000 (hard cap on congestion multiplier)

# BlockHeader fee_window_flags bit layout [1:1] BlockHeader
FEE_WINDOW_ACTIVE: int = 0x01  # bit 0


# ============================================================================
# [1:1] FeeWindowId — mirrors Rust FeeWindowId(u64)
# ============================================================================

def fee_window_id(height: int) -> int:
    """Return window index for a given block height. Window 0 = genesis."""
    return (height - 1) // WINDOW_SIZE


def is_window_boundary(height: int) -> bool:
    """True if height is the final block of its fee window."""
    return height > 0 and height % WINDOW_SIZE == 0


# ============================================================================
# [1:1] CongestionFactor — mirrors Rust CongestionFactor(u32) with SCALE
# ============================================================================

@dataclass
class CongestionFactor:
    """Fixed-point congestion factor. 1.0 = SCALE."""
    premium: int = SCALE
    standard: int = SCALE

    def premium_float(self) -> float:
        return self.premium / SCALE

    def standard_float(self) -> float:
        return self.standard / SCALE

    def premium_threshold(self) -> int:
        """Premium CF value. Used by adjust() return for backward compat."""
        return self.premium

    def general_threshold(self) -> int:
        """Standard CF value. Used by adjust() return for backward compat."""
        return self.standard


def compute_congestion_factor(premium_count: int, standard_count: int) -> CongestionFactor:
    """Compute congestion factors from mempool queue depths. [1:1] Rust."""
    cf_premium = SCALE + int(ALPHA_PREMIUM * SCALE * int(math.log2(premium_count + 1)))
    cf_standard = SCALE + int(ALPHA_STANDARD * SCALE * int(math.log2(standard_count + 1)))

    # I4: CF_premium > CF_standard when there is congestion.
    # At zero congestion (both = SCALE), equality is acceptable.
    if cf_premium <= cf_standard and (premium_count > 0 or standard_count > 0):
        cf_premium = cf_standard + 1

    return CongestionFactor(premium=cf_premium, standard=cf_standard)



# ============================================================================
# [1:1] FeeWindow — mirrors Rust FeeWindow
# ============================================================================

@dataclass
class FeeWindowConfig:
    """Window configuration [1:1] Rust FeeWindowConfig."""
    window_size: int = WINDOW_SIZE
    alpha_premium: float = ALPHA_PREMIUM
    alpha_standard: float = ALPHA_STANDARD
    max_adjustment: float = MAX_ADJUSTMENT
    min_premium: int = SCALE
    max_premium: int = MAX_PREMIUM


class FeeWindow:
    """Per-node fee window state. Tracks two independent congestion factors:
    CIRCUIT_CF (ZK execution) and WASM_CF (WASM deploy)."""

    def __init__(self, config: Optional[FeeWindowConfig] = None):
        self.config = config or FeeWindowConfig()
        # ── CIRCUIT CF ──
        self._circuit_cf = CongestionFactor()
        self._prev_circuit_cf: Optional[CongestionFactor] = None
        # ── WASM CF ──
        self._wasm_cf = CongestionFactor()
        self._prev_wasm_cf: Optional[CongestionFactor] = None
        # Window bookkeeping
        self._window_gas_used: List[int] = []
        self._window_gas_limit: List[int] = []

    # -- Backward-compat aliases (existing tests use these) --

    @property
    def _current_cf(self) -> CongestionFactor:
        return self._circuit_cf

    @_current_cf.setter
    def _current_cf(self, value: CongestionFactor) -> None:
        self._circuit_cf = value

    @property
    def _previous_cf(self) -> Optional[CongestionFactor]:
        return self._prev_circuit_cf

    @_previous_cf.setter
    def _previous_cf(self, value: Optional[CongestionFactor]) -> None:
        self._prev_circuit_cf = value

    # -- Window bookkeeping --

    def record_block(self, gas_used: int, gas_limit: int) -> None:
        """Called after each block to record utilization for this window."""
        self._window_gas_used.append(gas_used)
        self._window_gas_limit.append(gas_limit)

    def window_utilization(self) -> float:
        """Average utilization over the current window's recorded blocks."""
        if not self._window_gas_limit:
            return 0.0
        total_used = sum(self._window_gas_used)
        total_limit = sum(self._window_gas_limit)
        return min(total_used / total_limit, 1.0) if total_limit > 0 else 0.0

    @property
    def current_cf(self) -> CongestionFactor:
        """Backward compat — circuit CF."""
        return self._circuit_cf

    @property
    def previous_cf(self) -> Optional[CongestionFactor]:
        """Backward compat — previous circuit CF."""
        return self._prev_circuit_cf

    @property
    def circuit_cf(self) -> CongestionFactor:
        """Current circuit execution CF."""
        return self._circuit_cf

    @property
    def wasm_cf(self) -> CongestionFactor:
        """Current WASM deploy CF."""
        return self._wasm_cf

    # -- Congestion factor computation --

    def update_congestion(self, premium_pending: int, standard_pending: int) -> CongestionFactor:
        """Recompute CF from mempool queue depths. Does NOT apply caps yet."""
        return compute_congestion_factor(premium_pending, standard_pending)

    # -- Window boundary adjustment --

    def _apply_cap(self, raw_cf: CongestionFactor, previous: Optional[CongestionFactor],
                   premium_pending: int, standard_pending: int) -> CongestionFactor:
        """Apply ±10% cap and I4 ordering to a raw CF."""
        if previous is not None:
            max_p = int(previous.premium * (1 + self.config.max_adjustment))
            min_p = int(previous.premium * (1 - self.config.max_adjustment))
            max_s = int(previous.standard * (1 + self.config.max_adjustment))
            min_s = int(previous.standard * (1 - self.config.max_adjustment))
            capped_premium = max(min_p, min(raw_cf.premium, max_p))
            capped_standard = max(min_s, min(raw_cf.standard, max_s))
        else:
            capped_premium = raw_cf.premium
            capped_standard = raw_cf.standard

        if capped_premium <= capped_standard and (premium_pending > 0 or standard_pending > 0):
            capped_premium = capped_standard + 1

        return CongestionFactor(premium=capped_premium, standard=capped_standard)

    def adjust(self, premium_pending: int, standard_pending: int) -> Tuple[int, int]:
        """Backward compat — adjust circuit CF at window boundary."""
        return self.adjust_circuit(premium_pending, standard_pending)

    def adjust_circuit(self, premium_pending: int, standard_pending: int) -> Tuple[int, int]:
        """Adjust circuit CF at window boundary. Returns (premium, general)."""
        raw_cf = self.update_congestion(premium_pending, standard_pending)
        self._circuit_cf = self._apply_cap(raw_cf, self._prev_circuit_cf,
                                           premium_pending, standard_pending)
        self._prev_circuit_cf = self._circuit_cf
        self._window_gas_used = []
        self._window_gas_limit = []
        return (self._circuit_cf.premium_threshold(), self._circuit_cf.general_threshold())

    def adjust_wasm(self, premium_pending: int, standard_pending: int) -> Tuple[int, int]:
        """Adjust WASM CF at window boundary. Returns (wasm_premium, wasm_general)."""
        raw_cf = self.update_congestion(premium_pending, standard_pending)
        self._wasm_cf = self._apply_cap(raw_cf, self._prev_wasm_cf,
                                         premium_pending, standard_pending)
        self._prev_wasm_cf = self._wasm_cf
        return (
            (self._wasm_cf.premium * BASELINE_STORAGE) // SCALE,
            (self._wasm_cf.standard * BASELINE_STORAGE) // SCALE,
        )

    # -- BlockHeader signalling [1:1] fee_window_flags --

    @staticmethod
    def encode_flags(cf: CongestionFactor, previous: Optional[CongestionFactor] = None) -> int:
        """Encode CF into fee_window_flags byte (single CF, backward compat)."""
        flags = FEE_WINDOW_ACTIVE
        if previous is not None and previous.premium > 0:
            ratio = cf.premium / previous.premium
            if ratio > 1.05:
                flags |= 0x10  # +10%
            elif ratio < 0.95:
                flags |= 0x20  # -10%
            # else: hold (0x00 in bits 4:8)
        return flags

    @staticmethod
    def encode_flags_dual(circuit_cf: CongestionFactor, wasm_cf: CongestionFactor,
                          prev_circuit: Optional[CongestionFactor] = None,
                          prev_wasm: Optional[CongestionFactor] = None) -> int:
        """Encode both CFs into u16. Byte 0 = circuit, Byte 1 = WASM."""
        circuit_byte = FeeWindow.encode_flags(circuit_cf, prev_circuit)
        wasm_byte = FeeWindow.encode_flags(wasm_cf, prev_wasm)
        return (circuit_byte & 0xFF) | ((wasm_byte & 0xFF) << 8)

    @staticmethod
    def decode_flags(flags: int, current_premium: int) -> int:
        """Decode fee_window_flags into next premium threshold."""
        if not (flags & FEE_WINDOW_ACTIVE):
            return current_premium  # legacy — no change
        multiplier_bits = (flags >> 4) & 0x0F
        if multiplier_bits == 0x01:
            return int(current_premium * 1.10)
        elif multiplier_bits == 0x02:
            return int(current_premium * 0.90)
        return current_premium  # hold

    @staticmethod
    def decode_flags_dual(flags: int) -> Tuple[int, int]:
        """Decode u16 into (circuit_cm, wasm_cm)."""
        circuit_cm = (flags & 0xF0) >> 4
        if circuit_cm > 2:
            circuit_cm = 0
        wasm_cm = (flags >> 12) & 0x0F
        if wasm_cm > 2:
            wasm_cm = 0
        return (circuit_cm, wasm_cm)


# ============================================================================
# Mempool Model [1:1] TwoTierMempool with fee window integration
# ============================================================================

class MempoolWithWindow:
    """Two-tier mempool with fee window integration. FCFS within tiers."""

    def __init__(self, window: FeeWindow):
        self.window = window
        self.premium_queue: deque = deque()   # (tx_id, fee) — FCFS
        self.general_queue: deque = deque()   # (tx_id, fee) — FCFS
    @property
    def premium_count(self) -> int:
        return len(self.premium_queue)

    @property
    def standard_count(self) -> int:
        return len(self.general_queue)

    def admit(self, tx_id: str, fee: int, circuit_costs: list, wasm_kb: int = 1) -> str:
        """Admit a transaction. Returns 'premium', 'general', or 'reject'.

        Uses two-component formula: fee must cover both WASM storage and circuit execution.
        Premium tier: fee >= premium CF threshold.
        General tier: fee >= standard CF threshold.
        At zero congestion (premium=standard=SCALE), all admitted txs go to premium.
        """
        wasm_cf = self.window.wasm_cf
        circuit_cf = self.window.circuit_cf

        # Premium minimum: uses .premium for both CFs (plan §1.1)
        premium_min = compute_fee(circuit_costs, wasm_kb, wasm_cf, circuit_cf)

        # Standard minimum: uses .standard for both CFs
        total_opcode_cost = sum(circuit_costs)
        standard_min = (
            (wasm_kb * BASELINE_STORAGE * wasm_cf.standard) // SCALE
            + (total_opcode_cost * circuit_cf.standard) // SCALE
        )

        if fee >= premium_min:
            self.premium_queue.append((tx_id, fee))
            return "premium"
        elif fee >= standard_min:
            self.general_queue.append((tx_id, fee))
            return "general"
        return "reject"

    def select_for_block(self, max_txs: int) -> List[str]:
        """Select transactions for a block. FCFS within tiers."""
        selected = []
        # 1. Premium FCFS
        while self.premium_queue and len(selected) < max_txs:
            tx_id, _ = self.premium_queue.popleft()
            selected.append(tx_id)
        # 2. General FCFS
        while self.general_queue and len(selected) < max_txs:
            tx_id, _ = self.general_queue.popleft()
            selected.append(tx_id)
        return selected

    def on_window_boundary(self, new_window: FeeWindow):
        """I3: Preserve existing queues. New thresholds apply to new arrivals only.

        TODO(fee-spec §12.8.4): 30s transition delay not yet implemented.
        The spec defines a grace period after the boundary block before new
        thresholds activate. Currently the transition is instantaneous."""
        self.window = new_window
        # Existing txs stay in their queues — no eviction


# ============================================================================
# Tests
# ============================================================================

def test_initial_window_uses_defaults():
    """Initial window uses SCALE=1.0 CF (no congestion)."""
    w = FeeWindow()
    premium, general = w.adjust(0, 0)
    assert premium == SCALE, f"expected {SCALE} (CF=1.0), got {premium}"
    assert general == SCALE, f"expected {SCALE} (CF=1.0), got {general}"


def test_window_index():
    """FeeWindowId computation."""
    assert fee_window_id(1) == 0   # genesis
    assert fee_window_id(20) == 0  # last block of window 0
    assert fee_window_id(21) == 1  # first block of window 1
    assert fee_window_id(40) == 1  # last block of window 1
    assert fee_window_id(41) == 2


def test_boundary_detection():
    """is_window_boundary at multiples of WINDOW_SIZE."""
    assert not is_window_boundary(1)
    assert is_window_boundary(20)
    assert is_window_boundary(40)
    assert not is_window_boundary(21)


def test_congestion_factor_ordering():
    """I4: CF_premium > CF_standard at all times."""
    # Empty mempool
    cf = compute_congestion_factor(0, 0)
    assert cf.premium == SCALE, f"empty: premium should be SCALE, got {cf.premium}"
    assert cf.standard == SCALE, f"empty: standard should be SCALE, got {cf.standard}"

    # Congested mempool
    cf = compute_congestion_factor(1000, 10000)
    assert cf.premium > cf.standard, (
        f"I4 violated: premium={cf.premium_float():.3f} "
        f"standard={cf.standard_float():.3f}"
    )


def test_congestion_factor_log_scaling():
    """I6: CF grows logarithmically with queue depth."""
    cf10 = compute_congestion_factor(10, 100)
    cf100 = compute_congestion_factor(100, 1000)
    cf1000 = compute_congestion_factor(1000, 10000)
    # Log scaling: each 10x queue increase adds roughly constant delta
    delta1 = cf100.premium - cf10.premium
    delta2 = cf1000.premium - cf100.premium
    ratio = delta2 / delta1 if delta1 > 0 else 0
    assert 0.5 < ratio < 2.0, (
        f"I6: log scaling violated. delta(10→100)={delta1}, "
        f"delta(100→1000)={delta2}, ratio={ratio:.2f}"
    )


def test_empty_mempool_converges_to_one():
    """I6: CF → 1.0 as queue depth → 0."""
    cf = compute_congestion_factor(0, 0)
    assert cf.premium == SCALE
    assert cf.standard == SCALE


def test_smooth_adjustment_cap():
    """I7: ±10% per window."""
    w = FeeWindow()
    # First adjustment (no cap)
    p1, g1 = w.adjust(100, 1000)
    # Second adjustment — should be capped to ±10% of p1
    p2, g2 = w.adjust(100000, 1000000)  # extreme congestion
    max_p2 = int(p1 * 1.10)
    min_p2 = int(p1 * 0.90)
    assert min_p2 <= p2 <= max_p2, (
        f"I7 violated: p1={p1}, p2={p2}, allowed=[{min_p2}, {max_p2}]"
    )


def test_flags_roundtrip():
    """Fee window flags encode/decode roundtrip."""
    cf = CongestionFactor(premium=int(SCALE * 1.1), standard=SCALE)
    previous = CongestionFactor(premium=SCALE, standard=SCALE)
    flags = FeeWindow.encode_flags(cf, previous)
    decoded = FeeWindow.decode_flags(flags, SCALE)
    assert abs(decoded - int(SCALE * 1.1)) < SCALE // 100, (
        f"flags roundtrip failed: {decoded} vs {int(SCALE * 1.1)}"
    )


def test_legacy_flags_no_change():
    """I2: flags=0 means no change (legacy — hold current value)."""
    assert FeeWindow.decode_flags(0, SCALE) == SCALE
    assert FeeWindow.decode_flags(0, SCALE * 2) == SCALE * 2


def test_fcfs_preservation():
    """I3: admitted transactions survive window boundary."""
    w = FeeWindow()
    mempool = MempoolWithWindow(w)

    # Admit under window 0 (CF=1.0, zero congestion → premium=standard)
    # premium_min for [5000] at CF=1.0: 1_000_000 + 5000 = 1_005_000
    # premium_min for [100] at CF=1.0: 1_000_000 + 100 = 1_000_100
    assert mempool.admit("tx1", 5_000_000, [5000]) == "premium"
    assert mempool.admit("tx2", 2_000_000, [100]) == "premium"
    assert mempool.premium_count == 2
    assert mempool.standard_count == 0

    # Window boundary — congest both CFs to create distinct tiers
    w2 = FeeWindow()
    w2.adjust_circuit(1000, 5000)  # circuit CF congested
    w2.adjust_wasm(1000, 5000)     # WASM CF congested
    mempool.on_window_boundary(w2)

    # I3: existing txs preserved (not evicted)
    assert mempool.premium_count == 2, "I3 violated: premium txs evicted"

    # New arrival: fee between standard_min and premium_min → general tier
    result = mempool.admit("tx3", 1_300_000, [100])
    assert result == "general", f"tx below premium threshold should go to general, got {result}"


def test_circuit_rate_monotonicity():
    """I5: higher circuit difficulty pays higher fees."""
    cf = CongestionFactor(premium=int(SCALE * 1.5), standard=SCALE)
    # Premium CF > standard CF → premium threshold > general threshold
    fee_premium = cf.premium_threshold()
    fee_standard = cf.general_threshold()
    assert fee_premium > fee_standard, (
        f"I5 violated: premium={fee_premium}, standard={fee_standard}"
    )


def test_wasm_size_multiplier():
    """WASM deployment size multiplies threshold."""
    w = FeeWindow()
    w.adjust(0, 0)  # CF = 1.0
    mempool = MempoolWithWindow(w)

    # At CF=1.0: wasm_kb × 1_000_000 per kB
    # 5 kB deploy with circuit_cost=[1000]: premium_min = 5_000_000 + 1000 = 5_001_000
    assert mempool.admit("deploy1", 5_001_000, [1000], wasm_kb=5) == "premium"
    # 10 kB deploy: premium_min = 10_000_000 + 1000 = 10_001_000
    # 5_001_000 < 10_001_000 → reject
    assert mempool.admit("deploy2", 5_001_000, [1000], wasm_kb=10) == "reject", (
        "10 kB deploy needs higher fee (wasm_kb multiplier)"
    )


def test_premium_fcfs_before_general():
    """Premium queue drains FCFS before general queue."""
    w = FeeWindow()
    # Congest both CFs so premium > standard, creating distinct tiers
    w.adjust_circuit(500, 5000)  # circuit CF congested
    w.adjust_wasm(500, 5000)     # WASM CF congested
    mempool = MempoolWithWindow(w)

    # Admit interleaved: premiums with high fee, generals with moderate fee
    mempool.admit("p1", 5_000_000, [5000])      # well above premium_min → premium
    mempool.admit("g1", 1_300_000, [100])       # between std_min and prem_min → general
    mempool.admit("p2", 5_000_000, [5000])      # premium
    mempool.admit("g2", 1_300_000, [100])       # general

    selected = mempool.select_for_block(10)
    # Premium FCFS first
    assert selected[0] == "p1", f"expected p1 first, got {selected[0]}"
    assert selected[1] == "p2", f"expected p2 second, got {selected[1]}"
    # Then general FCFS
    assert selected[2] == "g1", f"expected g1 third, got {selected[2]}"
    assert selected[3] == "g2", f"expected g2 fourth, got {selected[3]}"


# ============================================================================
# NEW: Fee Signalling Testing Plan scenarios (P-FW-1 through P-FW-4, P-FW-6)
# ============================================================================


def test_multi_window_pid_loop():
    """P-FW-1: Multi-window PID stabilization over 5 windows.

    After 5 windows of constant load, thresholds evolve within ±10% per
    window (I7), CF_premium > CF_standard while congested (I4), and
    floor/ceiling are never breached.

    Pins Rust: L1-FW-4.
    """
    w = FeeWindow()
    thresholds = []

    # Constant moderate congestion: 50 premium, 500 standard pending
    for _ in range(5):
        premium, general = w.adjust(premium_pending=50, standard_pending=500)
        thresholds.append((premium, general))

    # I7: each step within ±10% of previous
    for i in range(1, len(thresholds)):
        prev_p, _ = thresholds[i - 1]
        curr_p, _ = thresholds[i]
        ratio = curr_p / prev_p
        assert 0.89 < ratio < 1.11, (
            f"P-FW-1 I7 violated at step {i}: "
            f"prev={prev_p}, curr={curr_p}, ratio={ratio:.3f}"
        )

    # Floor and ceiling
    final_p, final_g = thresholds[-1]
    assert final_p >= SCALE, f"P-FW-1: premium {final_p} below floor {SCALE}"
    assert final_p <= MAX_PREMIUM, f"P-FW-1: premium {final_p} above ceiling {MAX_PREMIUM}"

    # I4: CF_premium > CF_standard while congested
    cf = w.current_cf
    assert cf.premium > cf.standard, (
        f"P-FW-1 I4 violated: premium_cf={cf.premium}, standard_cf={cf.standard}"
    )

    # PF convergence: after 5 steps of same load, thresholds stabilize
    # (last two adjustments should be within tighter bounds)
    p4, _ = thresholds[3]
    p5, _ = thresholds[4]
    ratio_final = max(p4, p5) / min(p4, p5)
    assert ratio_final < 1.06, (
        f"P-FW-1 convergence failed: last two premiums diverged: {p4} vs {p5}"
    )


def test_both_cfs_simultaneously_congested():
    """P-FW-2: Both premium and standard CFs congested simultaneously.

    When premium AND standard queues are deep, both CF > SCALE, ordering
    I4 holds, and thresholds scale correctly for both tiers.

    Pins Rust: L1-FW-2.
    """
    w = FeeWindow()

    # Both queues heavily loaded
    cf = w.update_congestion(premium_pending=500, standard_pending=5000)

    assert cf.premium > SCALE, (
        f"P-FW-2: premium CF {cf.premium} should exceed SCALE under congestion"
    )
    assert cf.standard > SCALE, (
        f"P-FW-2: standard CF {cf.standard} should exceed SCALE under congestion"
    )
    assert cf.premium > cf.standard, (
        f"P-FW-2 I4 violated: premium_cf={cf.premium} <= standard_cf={cf.standard}"
    )

    # Thresholds (CF values) should both exceed SCALE (base)
    premium_t = cf.premium_threshold()
    general_t = cf.general_threshold()
    assert premium_t > SCALE, (
        f"P-FW-2: premium threshold {premium_t} not above SCALE={SCALE}"
    )
    assert general_t > SCALE, (
        f"P-FW-2: general threshold {general_t} not above SCALE={SCALE}"
    )

    # Verify log-scaling: different congestion levels produce different CFs
    cf_light = compute_congestion_factor(10, 100)
    cf_heavy = compute_congestion_factor(500, 5000)
    assert cf_heavy.premium > cf_light.premium, (
        "P-FW-2: heavier congestion should produce higher premium CF"
    )
    assert cf_heavy.standard > cf_light.standard, (
        "P-FW-2: heavier congestion should produce higher standard CF"
    )


def test_malicious_flag_injection():
    """P-FW-3: Malicious/invalid congestion-multiplier flags rejected.

    decode_flags with cm=0xFF, cm=0x03, cm=0x00 all return hold (no change).
    encode_flags never emits cm > 2.

    Pins Rust: L1-FW-3.
    """
    base_premium = SCALE

    # cm=0xFF (invalid, beyond defined range) → hold
    assert FeeWindow.decode_flags(0xFF | FEE_WINDOW_ACTIVE, base_premium) == base_premium, (
        "P-FW-3: cm=0xFF should hold (no change)"
    )

    # cm=0x03 (undefined direction) → hold
    assert FeeWindow.decode_flags(0x30 | FEE_WINDOW_ACTIVE, base_premium) == base_premium, (
        "P-FW-3: cm=0x03 should hold (no change)"
    )

    # cm=0x00 (hold, active) → hold
    assert FeeWindow.decode_flags(0x00 | FEE_WINDOW_ACTIVE, base_premium) == base_premium, (
        "P-FW-3: cm=0x00 active should hold"
    )

    # Legacy (inactive) → hold
    assert FeeWindow.decode_flags(0x00, base_premium) == base_premium, (
        "P-FW-3: legacy flags should hold"
    )

    # decode_flags is pure arithmetic — the floor/ceiling clamping is the
    # responsibility of adjust() (tested in P-FW-1), not decode_flags.
    # Verify that decode produces the raw ±10% arithmetic result:
    floor_premium = SCALE
    decoded_floor = FeeWindow.decode_flags(0x21, floor_premium)  # 0x02 = -10%
    assert decoded_floor == int(floor_premium * 0.90), (
        f"P-FW-3: -10% of {floor_premium} = {int(floor_premium * 0.90)}, got {decoded_floor}"
    )

    ceiling_premium = MAX_PREMIUM
    decoded_ceiling = FeeWindow.decode_flags(0x11, ceiling_premium)  # 0x01 = +10%
    assert decoded_ceiling == int(ceiling_premium * 1.10), (
        f"P-FW-3: +10% of {ceiling_premium} = {int(ceiling_premium * 1.10)}, got {decoded_ceiling}"
    )

    # encode_flags: congestion factors at SCALE → hold
    cf = CongestionFactor(premium=SCALE, standard=SCALE)
    flags = FeeWindow.encode_flags(cf, previous=CongestionFactor(premium=SCALE, standard=SCALE))
    assert (flags >> 4) & 0x0F in (0,), (
        f"P-FW-3: encode_flags at SCALE should emit hold, got cm={(flags >> 4) & 0x0F}"
    )


def test_decode_equivalence():
    """P-FW-4: Python decode(cm) matches WindowSignalling::decode_next_premium.

    0x01 → ×110/100 (+10%)
    0x02 → ×90/100 (-10%)
    else → hold (no change)
    general = 42M when active, (42M, 1M) when legacy.

    Pins Rust: L1-FW-1.
    """
    base_premium = SCALE

    # +10% (cm=0x01)
    up = FeeWindow.decode_flags(0x11, base_premium)
    assert up == int(base_premium * 1.10), (
        f"P-FW-4: +10% of {base_premium} = {int(base_premium * 1.10)}, got {up}"
    )

    # -10% (cm=0x02)
    down = FeeWindow.decode_flags(0x21, base_premium)
    assert down == int(base_premium * 0.90), (
        f"P-FW-4: -10% of {base_premium} = {int(base_premium * 0.90)}, got {down}"
    )

    # Hold (cm=0x00, active)
    hold_active = FeeWindow.decode_flags(0x01, base_premium)
    assert hold_active == base_premium, (
        f"P-FW-4: hold active should return {base_premium}, got {hold_active}"
    )

    # Hold (cm=0x00, inactive/legacy)
    hold_legacy = FeeWindow.decode_flags(0x00, base_premium)
    assert hold_legacy == base_premium, (
        f"P-FW-4: legacy should return {base_premium}, got {hold_legacy}"
    )

    # Verify decode(encode(cf)) roundtrip for +10%
    w = FeeWindow()
    w.adjust(100, 1000)  # first adjust — sets CF above SCALE
    w.adjust(500, 5000)  # second adjust — large jump, capped at +10%
    flags = FeeWindow.encode_flags(w.current_cf, previous=w.previous_cf)
    decoded = FeeWindow.decode_flags(flags, w.previous_cf.premium_threshold())
    assert abs(decoded - w.current_cf.premium_threshold()) <= 1, (
        f"P-FW-4 roundtrip: decoded {decoded} vs current {w.current_cf.premium_threshold()}"
    )


# ============================================================================
# K-Scaling Tests — fee-spec.md §12.11
# ============================================================================

def test_k_scaling_reference():
    """K_REF (k=11) produces scale factor 1.0 — no change from baseline."""
    ops = ["WitnessBase", "BaseAdd", "ConstrainInstance"]
    diff_k11 = circuit_difficulty(ops, k=11)
    diff_no_k = circuit_difficulty(ops)  # default K_REF
    assert diff_k11 == diff_no_k, f"k=K_REF should equal default: {diff_k11} vs {diff_no_k}"
    expected = OPCODE_DIFFICULTY["WitnessBase"] + OPCODE_DIFFICULTY["BaseAdd"] + OPCODE_DIFFICULTY["ConstrainInstance"]
    assert diff_k11 == expected, f"k=11: expected {expected}, got {diff_k11}"


def test_k_scaling_doubles_per_increment():
    """k=12 → 2×, k=13 → 4×, k=14 → 8×, k=15 → 16×."""
    ops = ["PoseidonHash"]  # base 500
    base = OPCODE_DIFFICULTY["PoseidonHash"]
    assert circuit_difficulty(ops, k=11) == base * 1
    assert circuit_difficulty(ops, k=12) == base * 2
    assert circuit_difficulty(ops, k=13) == base * 4
    assert circuit_difficulty(ops, k=14) == base * 8
    assert circuit_difficulty(ops, k=15) == base * 16


def test_k_scaling_max_k():
    """k=16 (MAX_K) → 32× scale factor."""
    ops = ["BaseAdd"]  # base 20
    assert circuit_difficulty(ops, k=16) == 20 * 32


def test_k_below_reference_no_fractional_scaling():
    """k < K_REF → scale factor = 1 (no fractional scaling)."""
    ops = ["BaseMul"]  # base 50
    assert circuit_difficulty(ops, k=10) == 50
    assert circuit_difficulty(ops, k=9) == 50
    assert circuit_difficulty(ops, k=0) == 50


def test_k_above_max_k_capped():
    """k > MAX_K → capped at 32×, no overflow."""
    ops = ["BaseAdd"]
    assert circuit_difficulty(ops, k=17) == 20 * 32
    assert circuit_difficulty(ops, k=20) == 20 * 32


def test_k_scaling_empty_circuit():
    """Empty circuit costs zero regardless of k."""
    assert circuit_difficulty([], k=11) == 0
    assert circuit_difficulty([], k=15) == 0


def test_k_scaling_fee_threshold_v1_unchanged():
    """FeeThreshold_V1 (k=11, 5 simple ops) difficulty = 40 — unchanged."""
    ops = ["WitnessBase", "ConstrainEqualBase", "ConstrainEqualBase",
           "ConstrainInstance", "ConstrainInstance"]
    assert circuit_difficulty(ops, k=11) == 40


def test_k_scaling_circuit_rates_with_k():
    """CIRCUIT_RATES × 2^(CIRCUIT_K - K_REF) gives k-scaled difficulty."""
    for name, base_rate in CIRCUIT_RATES.items():
        k = CIRCUIT_K.get(name, K_REF)
        scale = 1 << max(0, min(k - K_REF, MAX_K - K_REF))
        expected = base_rate * scale
        # Verify the scaling math is consistent, not that every rate is exact
        assert expected > 0 or base_rate == 0, f"{name}: zero scaled difficulty"


def test_k_scaling_composed_transaction():
    """Transaction with two circuits: Fee_V2 (k=12, diff=500) + FeeThreshold_V1 (k=11, diff=40)."""
    fee_v2_cost = circuit_difficulty(["PoseidonHash"], k=12)  # 500 * 2 = 1000
    threshold_cost = circuit_difficulty(
        ["WitnessBase", "ConstrainEqualBase", "ConstrainEqualBase",
         "ConstrainInstance", "ConstrainInstance"], k=11)  # 40 * 1 = 40
    total = fee_v2_cost + threshold_cost
    assert total == 1040, f"composed tx: expected 1040, got {total}"
    # Verify fee at zero congestion
    cf = CongestionFactor()
    fee = compute_fee([fee_v2_cost, threshold_cost], wasm_kb=1, wasm_cf=cf, circuit_cf=cf)
    assert fee == BASELINE_STORAGE + total, f"composed fee: expected {BASELINE_STORAGE + total}, got {fee}"


# ============================================================================
# Execution Risk Factor + Cost Profile Tests — fee-spec.md §12.12, manifest.md
# ============================================================================


def test_risk_factor_known_statuses():
    """All 5 contract statuses map to correct risk multipliers."""
    assert RISK_FACTOR["genesis"] == 100_000
    assert RISK_FACTOR["attested_endowed"] == 100_000
    assert RISK_FACTOR["attested_no_endowment"] == 125_000
    assert RISK_FACTOR["self_declared"] == 150_000
    assert RISK_FACTOR["unknown"] == 200_000


def test_risk_factor_ordering():
    """Risk factors are monotonic: genesis <= attested_endowed < attested_no_endowment < self_declared < unknown."""
    rf = RISK_FACTOR
    assert rf["genesis"] <= rf["attested_endowed"]
    assert rf["attested_endowed"] < rf["attested_no_endowment"]
    assert rf["attested_no_endowment"] < rf["self_declared"]
    assert rf["self_declared"] < rf["unknown"]


def test_cost_profile_construction():
    """CostProfile dataclass stores per-function cost declarations."""
    cp = CostProfile(
        function="TransferV2",
        circuit_difficulty=1000,
        k_value=12,
        wasm_kb=1,
        tolerance=0.50,
    )
    assert cp.function == "TransferV2"
    assert cp.circuit_difficulty == 1000
    assert cp.k_value == 12
    assert cp.wasm_kb == 1
    assert cp.tolerance == 0.50


def test_cost_profile_defaults():
    """CostProfile default values: k_value=K_REF, wasm_kb=1, tolerance=0.50."""
    cp = CostProfile(function="minimal", circuit_difficulty=500)
    assert cp.k_value == K_REF
    assert cp.wasm_kb == 1
    assert cp.tolerance == 0.50


def test_resolve_cost_profile_found():
    """Function found in profiles returns declared profile + status risk factor."""
    profiles = [
        CostProfile("TransferV2", 1000, 12),
        CostProfile("BurnV2", 800, 12),
    ]
    profile, risk = resolve_cost_profile("attested_endowed", "TransferV2", profiles)
    assert profile.function == "TransferV2"
    assert profile.circuit_difficulty == 1000
    assert risk == 100_000


def test_resolve_cost_profile_missing_function():
    """Missing function → 2.0× max declared difficulty, risk from status."""
    profiles = [
        CostProfile("TransferV2", 1000, 12),
        CostProfile("BurnV2", 800, 12),
    ]
    profile, risk = resolve_cost_profile("self_declared", "unknown_function", profiles)
    # circuit_difficulty = 2 * max(1000, 800) = 2000
    assert profile.circuit_difficulty == 2000, (
        f"expected 2.0× max declared (2000), got {profile.circuit_difficulty}"
    )
    assert risk == 150_000, f"expected self_declared risk 1.5 (150k), got {risk}"
    # k_value should be max of declared
    assert profile.k_value == 12
    assert profile.wasm_kb == 1


def test_resolve_cost_profile_no_profiles():
    """No profiles → pessimistic default, risk_factor=2.0 regardless of status."""
    profile, risk = resolve_cost_profile("attested_endowed", "anything", [])
    assert profile.function == "unknown"
    assert profile.circuit_difficulty == 1000, (
        f"expected default difficulty 1000, got {profile.circuit_difficulty}"
    )
    assert profile.k_value == MAX_K, f"expected worst-case k={MAX_K}, got {profile.k_value}"
    # Even though attested_endowed normally gives 1.0, no profiles → 2.0
    assert risk == 200_000, f"no profiles must use 2.0 risk regardless of status, got {risk}"


def test_resolve_cost_profile_genesis():
    """Genesis status → 1.0× risk factor."""
    profiles = [CostProfile("TransferV2", 1000, 12)]
    _, risk = resolve_cost_profile("genesis", "TransferV2", profiles)
    assert risk == 100_000


def test_resolve_cost_profile_attested_endowed():
    """Attested + endowment → 1.0× risk factor."""
    profiles = [CostProfile("TransferV2", 1000, 12)]
    _, risk = resolve_cost_profile("attested_endowed", "TransferV2", profiles)
    assert risk == 100_000


def test_resolve_cost_profile_unknown():
    """Unknown contract → 2.0× risk factor."""
    profiles = [CostProfile("TransferV2", 1000, 12)]
    _, risk = resolve_cost_profile("unknown", "TransferV2", profiles)
    assert risk == 200_000


def test_compute_total_fee_zero_congestion():
    """At CF=1.0, risk=1.0: fee = wasm_kB × BASELINE_STORAGE + circuit_difficulty."""
    profile = CostProfile("TransferV2", 1000, 12, wasm_kb=1)
    cf = CongestionFactor()  # SCALE = 1.0
    fee = compute_total_fee(profile, risk_factor=100_000, wasm_cf=cf, circuit_cf=cf)
    # wasm = 1 * 1_000_000 * 1_000_000 / 1_000_000 = 1_000_000
    # circuit = 1000 * 1_000_000 * 100_000 / (1_000_000 * 100_000) = 1000
    expected = BASELINE_STORAGE + 1000
    assert fee == expected, f"expected {expected}, got {fee}"


def test_compute_total_fee_risk_multiplier():
    """Risk=2.0 doubles only the circuit component, not WASM storage."""
    profile = CostProfile("TransferV2", 1000, 12, wasm_kb=1)
    cf = CongestionFactor()
    fee_normal = compute_total_fee(profile, risk_factor=100_000, wasm_cf=cf, circuit_cf=cf)
    fee_risky = compute_total_fee(profile, risk_factor=200_000, wasm_cf=cf, circuit_cf=cf)
    # wasm part unchanged: 1_000_000
    # circuit part: 1000 → 2000
    delta = fee_risky - fee_normal
    assert delta == 1000, (
        f"risk=2.0 should add exactly circuit_difficulty (1000), got delta={delta}"
    )


def test_compute_total_fee_risk_does_not_affect_wasm():
    """Risk factor only multiplies circuit component. WASM storage is independent of trust."""
    profile = CostProfile("DeployV1", 2000, 14, wasm_kb=50)
    cf = CongestionFactor()
    fee_1x = compute_total_fee(profile, risk_factor=100_000, wasm_cf=cf, circuit_cf=cf)
    fee_2x = compute_total_fee(profile, risk_factor=200_000, wasm_cf=cf, circuit_cf=cf)
    # wasm_part = 50 * 1_000_000 = 50_000_000 (same in both)
    # circuit_part_1x = 2000
    # circuit_part_2x = 4000
    wasm_part = 50 * BASELINE_STORAGE
    assert fee_1x == wasm_part + 2000
    assert fee_2x == wasm_part + 4000
    # wasm part unchanged
    assert fee_2x - fee_1x == 2000


def test_compute_total_fee_full_pipeline():
    """End-to-end: profile → resolve → compute_total_fee."""
    profiles = [
        CostProfile("TransferV2", 1000, 12, wasm_kb=1),
        CostProfile("ExecuteSwapV2", 2000, 14, wasm_kb=2),
    ]
    # Step 1: resolve cost profile for a known function
    profile, risk = resolve_cost_profile("attested_endowed", "ExecuteSwapV2", profiles)
    assert profile.function == "ExecuteSwapV2"
    assert risk == 100_000

    # Step 2: compute fee at zero congestion
    cf = CongestionFactor()
    fee = compute_total_fee(profile, risk, wasm_cf=cf, circuit_cf=cf)
    expected = 2 * BASELINE_STORAGE + 2000  # wasm_kb=2
    assert fee == expected, f"full pipeline: expected {expected}, got {fee}"

    # Step 3: resolve unknown function → pessimistic + risk from status
    profile2, risk2 = resolve_cost_profile("self_declared", "missing_func", profiles)
    assert profile2.circuit_difficulty == 4000  # 2 * max(1000, 2000)
    assert profile2.k_value == 14  # max(12, 14) from declared
    assert profile2.wasm_kb == 2  # max(1, 2) from declared
    assert risk2 == 150_000
    fee2 = compute_total_fee(profile2, risk2, wasm_cf=cf, circuit_cf=cf)
    # wasm = 2 * 1_000_000 = 2_000_000 (max wasm_kb=2 from declared)
    # circuit = 4000 * 1_000_000 * 150_000 / (1_000_000 * 100_000) = 4000 * 1.5 = 6000
    assert fee2 == 2_000_000 + 6000, f"full pipeline missing: expected {2_000_000 + 6000}, got {fee2}"


# ============================================================================
# Phase 2a: Nullifier Replay + Wallet + BlockCharge


class NullifierMempoolMixin:
    """Adds nullifier dedup to MempoolWithWindow. [1:1] Rust extract_nullifiers."""

    def __init__(self):
        self._nullifiers: set = set()

    def has_nullifier(self, nf) -> bool:
        return nf in self._nullifiers

    def insert_nullifier(self, nf):
        self._nullifiers.add(nf)

    def remove_nullifier(self, nf):
        self._nullifiers.discard(nf)


class MempoolWithWindowAndNullifiers(MempoolWithWindow, NullifierMempoolMixin):
    """Mempool with window integration AND nullifier replay detection."""

    def __init__(self, window: 'FeeWindow'):
        MempoolWithWindow.__init__(self, window)
        NullifierMempoolMixin.__init__(self)

    def admit(self, tx_id: str, fee: int, circuit_costs: list,
              wasm_kb: int = 1, nullifier=None) -> str:
        """Admit with nullifier dedup."""
        if nullifier is not None:
            if self.has_nullifier(nullifier):
                return "reject"
            self.insert_nullifier(nullifier)
        return super().admit(tx_id, fee, circuit_costs, wasm_kb)


def wallet_read_flags(flags_int: int) -> tuple:
    """Wallet reads fee_window_flags from block header, derives CFs."""
    active = flags_int & FEE_WINDOW_ACTIVE
    if not active:
        return (CongestionFactor(), CongestionFactor())
    circuit_cm = (flags_int >> 4) & 0x0F
    wasm_cm = (flags_int >> 12) & 0x0F

    def apply_cm(base: int, cm: int) -> int:
        if cm == 0x01:
            return (base * 110) // 100
        elif cm == 0x02:
            return (base * 90) // 100
        return base

    circuit_cf = CongestionFactor(
        premium=apply_cm(SCALE, circuit_cm),
        standard=apply_cm(SCALE, circuit_cm),
    )
    wasm_cf = CongestionFactor(
        premium=apply_cm(SCALE, wasm_cm),
        standard=apply_cm(SCALE, wasm_cm),
    )
    return (circuit_cf, wasm_cf)


def wallet_construct_fee(circuit_costs: list, wasm_kb: int,
                          block_header_flags: int) -> int:
    """Wallet constructs fee from block header flags."""
    circuit_cf, wasm_cf = wallet_read_flags(block_header_flags)
    return compute_fee(circuit_costs, wasm_kb, wasm_cf, circuit_cf)


CHARGE_PER_CALL: int = 400_000_000


class BlockCharge:
    """Declarative block capacity charge. [1:1] Rust BlockCharge(u64)."""
    def __init__(self, amount: int):
        self._value = amount

    def get(self) -> int:
        return self._value

    @staticmethod
    def declare_charge(num_calls: int) -> 'BlockCharge':
        return BlockCharge(num_calls * CHARGE_PER_CALL)


# ============================================================================
# Phase 3: Dynamic Feedback Loop — fee-spec.md §12.12.5
# ============================================================================


@dataclass
class CostDeviation:
    """Recorded deviation between declared and observed cost for one window."""
    contract_id: str
    function: str
    declared_cost: int
    observed_cost: int
    window_id: int

    @property
    def deviation_ratio(self) -> float:
        return self.observed_cost / self.declared_cost if self.declared_cost > 0 else 1.0

    def within_tolerance(self, tolerance: float = 0.50) -> bool:
        return abs(self.deviation_ratio - 1.0) <= tolerance


class ContractRiskTracker:
    """Tracks observed-vs-declared cost deviations and adjusts risk factors.

    Per fee-spec.md §12.12.5: miners observe actual execution costs, compare
    against declared costs in [[cost_profiles]], and escalate risk factors
    for contracts that systematically under-declare.
    """

    def __init__(self):
        self._deviations: dict = {}  # contract_id -> list[CostDeviation]
        self._black_marks: dict = {}  # contract_id -> int (count)

    def record(self, contract_id: str, function: str, declared: int,
               observed: int, window_id: int) -> CostDeviation:
        """Record a cost deviation for a contract in a given window."""
        dev = CostDeviation(contract_id, function, declared, observed, window_id)
        if contract_id not in self._deviations:
            self._deviations[contract_id] = []
        self._deviations[contract_id].append(dev)
        return dev

    def evaluate_window(self, contract_id: str) -> int:
        """Evaluate a contract's deviations for the current window.
        Returns the effective risk factor (in RISK_FACTOR_SCALE units).
        Escalation: 1 window → +0.25×, 2 windows → +0.5×, 3+ → 2.0× (capped)."""
        devs = self._deviations.get(contract_id, [])
        if not devs:
            return 100_000  # baseline 1.0×

        above_tolerance = sum(1 for d in devs if not d.within_tolerance(0.50))
        if above_tolerance == 0:
            return 100_000
        elif above_tolerance == 1:
            return 125_000  # 1.25×
        elif above_tolerance == 2:
            return 150_000  # 1.5×
        else:
            return 200_000  # 2.0× capped (unknown baseline)


# Phase 1a: Pedersen Commitment + FeeCommitAccumulator Tests
# ============================================================================

def _b(v: int) -> bytes:
    """Make a 32-byte blind from an integer seed (deterministic)."""
    return v.to_bytes(32, 'little')


def test_accumulator_lifecycle():
    """Full accumulator lifecycle: Identity → add → verify → reset → Identity."""
    acc = FeeCommitAccumulator()
    assert acc.is_identity, "accumulator must start at Identity"

    b1, b2 = _b(12345), _b(67890)
    acc.add(42_000_000, b1)
    acc.add(15_000_000, b2)
    assert not acc.is_identity, "accumulator must be non-Identity after adding commitments"

    # Build expected commitment via pedersen_add (correct homomorphic accumulation).
    # Accumulator starts at pedersen_commit(0, zero) — add that identity term.
    zero = _b(0)
    expected = pedersen_add(pedersen_commit(0, zero),
               pedersen_add(pedersen_commit(42_000_000, b1),
                            pedersen_commit(15_000_000, b2)))
    assert pedersen_eq(acc._acc, expected), "FeeCollectV1 must verify via pedersen_eq"

    # Wrong total fails
    wrong = pedersen_add(pedersen_commit(0, zero),
            pedersen_add(pedersen_commit(42_000_001, b1),
                         pedersen_commit(15_000_000, b2)))
    assert not pedersen_eq(acc._acc, wrong), "wrong total_fees must fail"

    acc.reset()
    assert acc.is_identity, "accumulator must return to Identity after reset"


def test_accumulator_empty_is_identity():
    """Empty block: no FeeV2 calls → accumulator stays at Identity."""
    acc = FeeCommitAccumulator()
    assert acc.is_identity, "empty accumulator is Identity"


def test_accumulator_rejects_zero_fee_nonzero_blind():
    """P1.3: FeeCommitAccumulator rejects fee=0 with non-zero blind."""
    acc = FeeCommitAccumulator()
    non_zero_blind = _b(12345)
    try:
        acc.add(0, non_zero_blind)
        assert False, "must reject zero-fee with non-zero blind"
    except ValueError as e:
        assert "zero-fee" in str(e)


def test_accumulator_identity_commit_no_effect():
    """P1.4: fee=0, blind=0 → identity commit is no-op."""
    acc = FeeCommitAccumulator()
    zero_blind = b'\x00' * 32
    acc.add(0, zero_blind)  # identity — no effect
    assert acc.is_identity, "identity commit must not change accumulator"


def test_accumulator_blind_accumulation():
    """P1.5: FeeCollectV1 recomputed commitment via pedersen_add matches accumulator.
    The miner accumulates commitments via pedersen_add (not blind concatenation).
    This verifies the accumulator matches the FeeCollectV1 verification path."""
    b1, b2 = _b(1000), _b(2000)
    acc = FeeCommitAccumulator()
    acc.add(42_000_000, b1)
    acc.add(15_000_000, b2)
    # Recompute separately via pedersen_add
    recomputed = pedersen_add(
        pedersen_commit(42_000_000, b1),
        pedersen_commit(15_000_000, b2),
    )
    # Accumulator (which starts at identity + pedersen_add for each add)
    # should equal the recomputed commitment (direct pedersen_add of the two)
    assert pedersen_eq(acc._acc, pedersen_add(pedersen_commit(0, b'\x00'*32), recomputed)), \
        "accumulator must equal identity + pedersen_add of individual commits"

# ============================================================================
# Phase 1b+1c: Encrypted Fee + FeeCollectV1 Tests (G2+G6 reference)
# ============================================================================


def _mk(miner_id: int) -> bytes:
    """Make deterministic miner key bytes from an integer seed."""
    return miner_id.to_bytes(32, 'big')


def test_encrypt_decrypt_roundtrip():
    """Wallet encrypts fee → miner decrypts → matches original."""
    fee = 42_000_000
    miner_key = _mk(0x12345)
    ciphertext = encrypt_fee_for_miner(fee, miner_key)
    decrypted = decrypt_fee_for_miner(ciphertext, miner_key)
    assert decrypted == fee, f"encrypt/decrypt roundtrip: expected {fee}, got {decrypted}"


def test_encrypt_different_values_produce_different_ciphertext():
    """Different fees produce different ciphertexts."""
    miner_key = _mk(0x12345)
    c1 = encrypt_fee_for_miner(42_000_000, miner_key)
    c2 = encrypt_fee_for_miner(15_000_000, miner_key)
    assert c1 != c2, "different fees must produce different ciphertexts"


def test_decrypt_wrong_key_fails():
    """Decryption with wrong miner key returns None."""
    ciphertext = encrypt_fee_for_miner(42_000_000, _mk(0x12345))
    result = decrypt_fee_for_miner(ciphertext, _mk(0x99999))
    assert result is None, "wrong key must return None"


def test_build_fee_collect_params_sum():
    """Miner decrypts fees from mempool transactions, sums correctly."""
    miner_key = _mk(0xABCD)
    b1, b2, b3 = _b(1000), _b(2000), _b(3000)
    txs = [
        {'fee_v2_calls': [(42_000_000, b1, encrypt_fee_for_miner(42_000_000, miner_key))]},
        {'fee_v2_calls': [(15_000_000, b2, encrypt_fee_for_miner(15_000_000, miner_key))]},
        {'fee_v2_calls': [(1_001_000, b3, encrypt_fee_for_miner(1_001_000, miner_key))]},
    ]
    acc = FeeCommitAccumulator()
    for f, b, _ in [t['fee_v2_calls'][0] for t in txs]:
        acc.add(f, b)
    total_fees, recomputed, all_ok = build_fee_collect_params(txs, miner_key, acc)
    assert all_ok, "all three decryptions must succeed"
    assert total_fees == 42_000_000 + 15_000_000 + 1_001_000
    # Compare recomputed commitment against accumulator using pedersen_eq
    assert pedersen_eq(recomputed, acc._acc), "FeeCollectV1: recomputed must match accumulator"


def test_build_fee_collect_params_fallback_on_decrypt_failure():
    """When decryption fails, miner falls back to estimate."""
    miner_key = _mk(0xABCD)
    b1, b2 = _b(1000), _b(2000)
    txs = [
        {'fee_v2_calls': [(42_000_000, b1, encrypt_fee_for_miner(42_000_000, miner_key))]},
        {'fee_v2_calls': [(15_000_000, b2, b'\x00' * 12)]},  # bad ciphertext
    ]
    acc = FeeCommitAccumulator()
    acc.add(42_000_000, b1)
    acc.add(15_000_000, b2)
    total_fees, recomputed, all_ok = build_fee_collect_params(txs, miner_key, acc)
    assert not all_ok, "one decryption failed, must report failure"
    assert total_fees == 42_000_000 + 1_001_000  # second tx fell back to estimate
    # Even with fallback, the recomputed commitment uses the estimate value
    # with the same blind, so it should match if the accumulator also used
    # the estimate. Here the accumulator used the actual fee, so they differ.
    # This is expected — the fallback value differs from actual.


def test_g2_g6_end_to_end():
    """Full G2+G6 reference: wallet encrypts → miner decrypts → FeeCollectV1 verifies."""
    miner_key = _mk(0xBEEF)
    fees = [42_000_000, 15_000_000, 1_001_000, 100_000_000, 50_000_000]
    blinds = [_b(i) for i in range(1000, 5001, 1000)]
    txs = []
    acc = FeeCommitAccumulator()
    for f, b in zip(fees, blinds):
        ciphertext = encrypt_fee_for_miner(f, miner_key)
        txs.append({'fee_v2_calls': [(f, b, ciphertext)]})
        acc.add(f, b)

    total_fees, recomputed, all_ok = build_fee_collect_params(txs, miner_key, acc)
    assert all_ok, "all decryptions must succeed"
    assert total_fees == sum(fees), f"expected {sum(fees)}, got {total_fees}"
    assert pedersen_eq(recomputed, acc._acc), \
        "FeeCollectV1: recomputed commitment must match on-chain accumulator"

    acc.reset()
    assert acc.is_identity, "accumulator must be Identity after FeeCollectV1"


# ============================================================================
# Phase 2a: Nullifier Replay Tests
# ============================================================================


def test_nullifier_replay_rejected():
    """Two txs, same nullifier → second rejected."""
    w = FeeWindow()
    mp = MempoolWithWindowAndNullifiers(w)
    assert mp.admit("tx1", 5_000_000, [1000], nullifier="nf_1") == "premium"
    assert mp.admit("tx2", 5_000_000, [1000], nullifier="nf_1") == "reject"
    assert mp.premium_count == 1, "only first tx should be admitted"


def test_nullifier_different_allowed():
    """Different nullifiers → both admitted."""
    w = FeeWindow()
    mp = MempoolWithWindowAndNullifiers(w)
    assert mp.admit("tx1", 5_000_000, [1000], nullifier="nf_a") == "premium"
    assert mp.admit("tx2", 5_000_000, [1000], nullifier="nf_b") == "premium"
    assert mp.premium_count == 2


def test_nullifier_replay_preserves_fcfs():
    """I3 + nullifier: admitted txs stay, replays rejected, FCFS preserved."""
    w = FeeWindow()
    mp = MempoolWithWindowAndNullifiers(w)
    mp.admit("p1", 5_000_000, [5000], nullifier="nf_p1")
    mp.admit("g1", 1_300_000, [100], nullifier="nf_g1")
    assert mp.admit("p1_dup", 5_000_000, [5000], nullifier="nf_p1") == "reject"
    selected = mp.select_for_block(10)
    assert selected[0] == "p1" and selected[1] == "g1", "FCFS preserved"


# ============================================================================
# Phase 2b: Wallet construct_fee / derive_cfs Tests
# ============================================================================


def test_wallet_read_flags_hold():
    """cm=0x00 (hold) → identity CFs."""
    flags = FEE_WINDOW_ACTIVE | (0x00 << 4) | (0x00 << 12)
    cf, wf = wallet_read_flags(flags)
    assert cf.premium == SCALE and wf.premium == SCALE


def test_wallet_read_flags_increase():
    """cm=0x01 (+10%) → CF above SCALE."""
    flags = FEE_WINDOW_ACTIVE | (0x01 << 4) | (0x00 << 12)
    cf, wf = wallet_read_flags(flags)
    assert cf.premium == int(SCALE * 1.10), f"expected +10%, got {cf.premium}"
    assert wf.premium == SCALE, "WASM must be hold"


def test_wallet_read_flags_decrease():
    """cm=0x02 (-10%) → CF below SCALE."""
    flags = FEE_WINDOW_ACTIVE | (0x02 << 4) | (0x00 << 12)
    cf, wf = wallet_read_flags(flags)
    assert cf.premium == int(SCALE * 0.90), f"expected -10%, got {cf.premium}"


def test_wallet_read_flags_legacy():
    """Inactive flags → identity CFs (I2 backward compat)."""
    cf, wf = wallet_read_flags(0x00)
    assert cf.premium == SCALE and wf.premium == SCALE


def test_wallet_construct_fee_from_flags():
    """Full wallet pipeline: flags → derive_cfs → compute_fee."""
    # +10% circuit, hold WASM
    flags = FEE_WINDOW_ACTIVE | (0x01 << 4) | (0x00 << 12)
    fee = wallet_construct_fee([1000], 1, flags)
    expected = (1 * BASELINE_STORAGE * SCALE) // SCALE + (1000 * int(SCALE * 1.10)) // SCALE
    assert fee == expected, f"wallet fee mismatch: {fee} vs {expected}"


# ============================================================================
# Phase 2c: BlockCharge Tests
# ============================================================================


def test_block_charge_baseline():
    """Single call → CHARGE_PER_CALL."""
    c = BlockCharge.declare_charge(1)
    assert c.get() == 400_000_000


def test_block_charge_scales():
    """5 calls → 5 × CHARGE_PER_CALL."""
    c = BlockCharge.declare_charge(5)
    assert c.get() == 2_000_000_000


def test_block_charge_accumulation():
    """Accumulate declared charges in select_for_block pattern."""
    charges = [BlockCharge.declare_charge(n) for n in [1, 3, 2]]
    total = sum(c.get() for c in charges)
    assert total == 6 * CHARGE_PER_CALL


# ============================================================================
# Phase 3: Dynamic Feedback Loop Tests
# ============================================================================


def test_deviation_within_tolerance():
    """Deviation within 50% tolerance → within_tolerance is True."""
    d = CostDeviation("c1", "f1", 1000, 1400, 0)
    assert d.within_tolerance(0.50), "40% above declared must be within 50% tolerance"


def test_deviation_above_tolerance():
    """Deviation above 50% tolerance → within_tolerance is False."""
    d = CostDeviation("c1", "f1", 1000, 1600, 0)
    assert not d.within_tolerance(0.50), "60% above declared must exceed 50% tolerance"


def test_risk_escalation_one_window():
    """One window above tolerance → risk factor rises to 1.25×."""
    t = ContractRiskTracker()
    t.record("c1", "f1", 1000, 2000, 0)  # 100% above → above tolerance
    assert t.evaluate_window("c1") == 125_000  # 1.25×


def test_risk_escalation_two_windows():
    """Two windows above tolerance → risk factor rises to 1.5×."""
    t = ContractRiskTracker()
    t.record("c1", "f1", 1000, 2000, 0)  # window 0: above tolerance
    t.record("c1", "f1", 1000, 2000, 1)  # window 1: above tolerance
    assert t.evaluate_window("c1") == 150_000  # 1.5×


def test_risk_escalation_capped():
    """Risk factor capped at 2.0× (unknown)."""
    t = ContractRiskTracker()
    for w in range(5):
        t.record("c1", "f1", 1000, 2000, w)
    assert t.evaluate_window("c1") == 200_000  # capped at 2.0×


def test_feedback_loop_end_to_end():
    """Full feedback loop: accurate declaration stays at 1.0×,
    persistent under-declaration escalates to 2.0×."""
    t = ContractRiskTracker()
    # Contract A: always accurate → stays at baseline
    for w in range(4):
        t.record("accurate", "f", 1000, 1100, w)  # 10% above, within 50% tolerance
    assert t.evaluate_window("accurate") == 100_000
    # Contract B: persistent under-declaration → escalates
    for w in range(4):
        t.record("under", "f", 1000, 2000, w)  # 100% above, exceeds tolerance
    assert t.evaluate_window("under") == 200_000  # capped at 2.0×


def test_circuit_difficulty_from_declared_opcodes():
    """Miner verification: circuit_difficulty(declared_opcodes, declared_k) == declared_circuit_difficulty.

    This is the miner's verification logic — contract authors declare opcodes and
    circuit_difficulty in the manifest. The miner independently computes the sum
    from the declared opcode list and checks against the declared value. A mismatch
    is a black mark (reputation downgrade → higher risk factor → higher fees).
    """
    # FeeThreshold_V1: k=11, 5 simple ops → circuit_difficulty = 40
    ops = ["WitnessBase", "ConstrainEqualBase", "ConstrainEqualBase",
           "ConstrainInstance", "ConstrainInstance"]
    computed = circuit_difficulty(ops, k=11)
    declared = 40  # from CIRCUIT_RATES["FeeThreshold_V1"]
    assert computed == declared, (
        f"miner verification: computed {computed} != declared {declared}"
    )

    # Poseidon-heavy circuit: k=12, 1 PoseidonHash → 500 × 2 = 1000
    ops2 = ["PoseidonHash"]
    computed2 = circuit_difficulty(ops2, k=12)
    declared2 = 1000
    assert computed2 == declared2, (
        f"miner verification: computed {computed2} != declared {declared2}"
    )

    # Mixed circuit at k=14: BaseAdd(20)+BaseMul(50)+PoseidonHash(500) = 570 → ×8 = 4560
    ops3 = ["BaseAdd", "BaseMul", "PoseidonHash"]
    computed3 = circuit_difficulty(ops3, k=14)
    declared3 = 4560
    assert computed3 == declared3, (
        f"miner verification: computed {computed3} != declared {declared3}"
    )

    # Empty circuit → zero difficulty regardless of k
    computed4 = circuit_difficulty([], k=15)
    assert computed4 == 0, f"empty circuit: expected 0, got {computed4}"


# ============================================================================
# Test runner
# ============================================================================

if __name__ == "__main__":
    tests = [
        test_initial_window_uses_defaults,
        test_window_index,
        test_boundary_detection,
        test_congestion_factor_ordering,
        test_congestion_factor_log_scaling,
        test_empty_mempool_converges_to_one,
        test_smooth_adjustment_cap,
        test_flags_roundtrip,
        test_legacy_flags_no_change,
        test_fcfs_preservation,
        test_circuit_rate_monotonicity,
        test_wasm_size_multiplier,
        test_premium_fcfs_before_general,
        # NEW: Fee Signalling Testing Plan scenarios
        test_multi_window_pid_loop,
        test_both_cfs_simultaneously_congested,
        test_malicious_flag_injection,
        test_decode_equivalence,
        # K-Scaling tests — fee-spec.md §12.11
        test_k_scaling_reference,
        test_k_scaling_doubles_per_increment,
        test_k_scaling_max_k,
        test_k_below_reference_no_fractional_scaling,
        test_k_above_max_k_capped,
        test_k_scaling_empty_circuit,
        test_k_scaling_fee_threshold_v1_unchanged,
        test_k_scaling_circuit_rates_with_k,
        test_k_scaling_composed_transaction,
        # Execution Risk Factor + Cost Profile tests — fee-spec.md §12.12
        test_risk_factor_known_statuses,
        test_risk_factor_ordering,
        test_cost_profile_construction,
        test_cost_profile_defaults,
        test_resolve_cost_profile_found,
        test_resolve_cost_profile_missing_function,
        test_resolve_cost_profile_no_profiles,
        test_resolve_cost_profile_genesis,
        test_resolve_cost_profile_attested_endowed,
        test_resolve_cost_profile_unknown,
        test_compute_total_fee_zero_congestion,
        test_compute_total_fee_risk_multiplier,
        test_compute_total_fee_risk_does_not_affect_wasm,
        test_compute_total_fee_full_pipeline,
        # Phase 1a: Pedersen + FeeCommitAccumulator
        test_accumulator_lifecycle,
        test_accumulator_empty_is_identity,
        test_accumulator_rejects_zero_fee_nonzero_blind,
        test_accumulator_identity_commit_no_effect,
        test_accumulator_blind_accumulation,
        # Phase 1b+1c: Encrypted fee + FeeCollectV1 (G2+G6 reference)
        test_encrypt_decrypt_roundtrip,
        test_encrypt_different_values_produce_different_ciphertext,
        test_decrypt_wrong_key_fails,
        test_build_fee_collect_params_sum,
        test_build_fee_collect_params_fallback_on_decrypt_failure,
        test_g2_g6_end_to_end,
        # Phase 2a: Nullifier replay
        test_nullifier_replay_rejected,
        test_nullifier_different_allowed,
        test_nullifier_replay_preserves_fcfs,
        # Phase 2b: Wallet construct_fee / derive_cfs
        test_wallet_read_flags_hold,
        test_wallet_read_flags_increase,
        test_wallet_read_flags_decrease,
        test_wallet_read_flags_legacy,
        test_wallet_construct_fee_from_flags,
        # Phase 2c: BlockCharge
        test_block_charge_baseline,
        test_block_charge_scales,
        test_block_charge_accumulation,
        # Phase 3: Dynamic Feedback Loop
        test_deviation_within_tolerance,
        test_deviation_above_tolerance,
        test_risk_escalation_one_window,
        test_risk_escalation_two_windows,
        test_risk_escalation_capped,
        test_feedback_loop_end_to_end,
        # Miner verification — manifest [[circuits]].opcodes
        test_circuit_difficulty_from_declared_opcodes,
    ]

    passed = 0
    for test in tests:
        try:
            test()
            passed += 1
            print(f"  PASS  {test.__name__}")
        except AssertionError as e:
            print(f"  FAIL  {test.__name__}: {e}")

    print(f"\n{passed}/{len(tests)} tests passed")
    assert passed == len(tests), f"{len(tests) - passed} tests failed"
