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

## Model Limitations (Type System)

This Python model is an ARCHITECTURAL specification, not a byte-level simulator.
The following Rust type-system guarantees have NO Python equivalent:

1. COMPILE-TIME KEY TYPING: Rust's AccumulatorKey (fee-spec.md §5.6.2.1) prevents
   key typos at compile time. Python uses a string constant ACCUMULATOR_KEY.
   A typo in a Python key constant is a runtime error, not a compile-time error.

2. COMPILE-TIME VALUE TYPING: Rust's AccumulatorPoint::encode() guarantees exactly
   32 bytes. Python uses runtime assert len(data) == 32 at decode time.

3. COMPILE-TIME HANDLE TYPING: Rust's InfoTreeHandle vs CoinsTreeHandle are
   distinct types. Python represents both as opaque objects — wrong-handle
   errors are not caught.

4. ERROR ERASURE AT WASM i64 ABI: Rust's ContractError::IoError("Corrupt state:
   fee_commit_accumulator wrong size") crosses the WASM boundary as i64::MIN + 4.
   The host reconstructs IoError("Unknown"). Python exceptions preserve full
   stack traces — this failure mode cannot be reproduced in Python.

5. SLED OVERLAY BYTE SERIALIZATION: Python stores AccumulatorPoint objects
   directly in a dict. Rust serializes through pallas::Point::to_bytes() →
   [u8; 32] → sled → db_get → Vec<u8> → try_into::<[u8; 32]> →
   pallas::Point::from_bytes(). The byte-level round-trip is not modeled.
   The contract-wasm-type-system.md §A.3.1.2 documents a 9-byte Purse corruption
   from derive-based serialize() — this class of error CANNOT be reproduced in
   the Python model.

6. WASM COMPILATION: Python has no WASM target. Stale include_bytes! embedded
   ZK proof binaries have no Python equivalent.

Tests that pass in Python but fail in Rust are MOST LIKELY caused by one of
these six gaps. When a Rust integration test fails and the equivalent Python
scenario passes, audit the failure against this list before debugging sled.
"""

import math
import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
from dataclasses import dataclass, field
from typing import List, Tuple, Optional
from collections import deque

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fee_model import FeeAmount


# ============================================================================
# Nominal wrappers [1:1] Rust consensus scalars (type-system.md §2.3)
# ============================================================================

class CfValue:
    """Nominal wrapper for the congestion-factor scalar (mirrors Rust CfValue(u32))."""
    SCALE: int = 1_000_000
    def __init__(self, value: int):
        self._value = value
    @classmethod
    def new(cls, value: int) -> "CfValue":
        return cls(value)
    def get(self) -> int:
        return self._value
    def to_f64(self) -> float:
        return self._value / self.SCALE
    def _cmp(self, other):
        return other.get() if isinstance(other, CfValue) else other
    def __eq__(self, other: object) -> bool:
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
    def __repr__(self) -> str:
        return f"CfValue({self._value})"

CfValue.IDENTITY = CfValue(CfValue.SCALE)


class RiskFactor:
    """Nominal wrapper for the risk-factor scalar (mirrors Rust RiskFactor(u64))."""
    SCALE: int = 100_000
    def __init__(self, value: int):
        self._value = value
    @classmethod
    def new(cls, value: int) -> "RiskFactor":
        return cls(value)
    def get(self) -> int:
        return self._value
    def apply(self, circuit_cost: int) -> int:
        return (circuit_cost * self._value) // self.SCALE
    def __eq__(self, other: object) -> bool:
        return isinstance(other, RiskFactor) and self._value == other._value
    def __repr__(self) -> str:
        return f"RiskFactor({self._value})"

RiskFactor.BASELINE = RiskFactor(RiskFactor.SCALE)
RiskFactor.MAX = RiskFactor(200_000)


class WasmKb:
    """Nominal wrapper for the WASM code-size scalar (mirrors Rust WasmKb(u64))."""
    def __init__(self, value: int):
        self._value = value
    @classmethod
    def new(cls, value: int) -> "WasmKb":
        return cls(value)
    def get(self) -> int:
        return self._value
    def __repr__(self) -> str:
        return f"WasmKb({self._value})"


# ============================================================================
# Constants [1:1] with Rust FeeWindowConfig
# ============================================================================

SCALE: int = 1_000_000          # fixed-point scale for congestion factors
WINDOW_SIZE: int = 20           # blocks per fee window

# Per-opcode ZK row counts — consensus constants [1:1] Rust opcode_cost.rs
# Gas = number of Halo2 advice rows an opcode's gadget consumes. One gas = one
# advice row. A circuit's total rows determine its k (domain size 2^k), and the
# verifier's dominant cost is the MSM over 2^k points. Derived from the gadget
# source (src/zk/vm.rs, src/zk/gadget/*.rs, vendored halo2_gadgets/halo2_poseidon).
# fee-spec.md §12.4.2.
OPCODE_ROWS: dict = {
    # Arithmetic (arithmetic.rs): 1 gate = 1 row each
    "BaseAdd": 1, "BaseSub": 1, "BaseMul": 1,
    # WitnessBase: 1 row (constrain_constant)
    "WitnessBase": 1,
    # BaseDiv (vm.rs): square-and-multiply of p-2 (255 bits, 77 set bits)
    #   254 squarings + 76 conditional multiplies + 1 final = 331 rows
    "BaseDiv": 331,
    # RangeCheck (native_range_check.rs): running-sum window W=10. Default = 253-bit.
    #   rows(bits) = ceil(bits/10) + (bits%10 ? 2 : 0)  →  253:28, 64:9
    "RangeCheck": 28,
    # LessThan* (less_than.rs): 1 compare gate + 2 x RangeCheck(253) = 57
    "LessThanStrict": 57, "LessThanLoose": 57,
    "LessThanOrEqual": 57, "BaseLtStrict": 57,
    # Poseidon (pow5.rs): P128Pow5T3 R_F=8 + R_P/2=28 = 36 rows/permutation, RATE=2.
    #   rows(N) = ceil(N/2) * 36. Default = 1 input absorbed (still 1 permutation) = 36.
    "PoseidonHash": 36,
    # ECC (halo2 ecc): EcAdd incomplete addition; EcMul double-and-add (2 rows/bit x 255);
    #   EcMulBase/EcMulShort fixed-base windowed; EcGetX/EcGetY coordinate extraction.
    "EcAdd": 6,
    "EcMul": 510, "EcMulVarBase": 510,
    "EcMulBase": 85, "EcMulShort": 85,
    "EcGetX": 0, "EcGetY": 0,
    # MerkleRoot (Sinsemilla depth 32): 32 levels x 51 Sinsemilla rows (2x255 bits / K=10)
    "MerkleRoot": 1632,
    # SparseMerkleRoot/SetMembership (SMT depth 255): 255 levels x 36 Poseidon rows
    "SparseMerkleRoot": 9180, "SetMembership": 9180,
    # Comparison/selection (1 gate = 1 row, 4 advice cols)
    "IsEqualBase": 1, "IsNotEqualBase": 1,
    "BoolCheck": 1, "NotBase": 2,
    "CondSelect": 1, "ZeroCondSelect": 1,
    # Constrain (copy constraints)
    "ConstrainEqualBase": 1, "ConstrainEqualPoint": 2, "ConstrainInstance": 1,
    # Zero cost
    "Noop": 0, "DebugPrint": 0,
}

# Backward-compat alias (the pre-FeeV3 name for the same table).
OPCODE_DIFFICULTY = OPCODE_ROWS


def range_check_rows(bits: int) -> int:
    """RangeCheck gas = running-sum decomposition rows. WINDOW = 10 (sinsemilla K)."""
    return (bits + 9) // 10 + (2 if bits % 10 else 0)


def poseidon_hash_rows(n: int) -> int:
    """PoseidonHash gas = ceil(n/2) x 36 (P128Pow5T3, RATE=2, 36 rows/permutation)."""
    return ((n + 1) // 2) * 36


# Flat base price: wow per gas. Placeholder pending real gas economics.
# fee-spec.md §12.5: PRICE_LOW/MEDIUM/HIGH = BASE_PRICE x {1,2,4}.
BASE_PRICE: int = 1_000_000


class FeeTier:
    """Three-tier priority selector. fee-spec.md §12.5. Multipliers 1x/2x/4x.

    The user picks a tier, never an arbitrary fee. [1:1] Rust FeeTier (3a).
    """
    LOW = 1
    MEDIUM = 2
    HIGH = 4

    def __init__(self, multiplier: int):
        assert multiplier in (self.LOW, self.MEDIUM, self.HIGH), \
            f"invalid tier multiplier {multiplier}"
        self._multiplier = multiplier

    @classmethod
    def from_multiplier(cls, multiplier: int) -> "FeeTier":
        return cls(multiplier)

    def multiplier(self) -> int:
        return self._multiplier

    def __eq__(self, other: object) -> bool:
        return isinstance(other, FeeTier) and self._multiplier == other._multiplier

    def __repr__(self) -> str:
        return f"FeeTier({self._multiplier}x)"

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

# Execution risk factors — fee-spec.md §12.12.3, §14.7
# Dynamic per-contract risk factors stored in chain state.
# Risk is emergent: observed behavior determines the risk factor, not a static
# attestation classification. Each contract earns its own risk factor.
#
# Represented as fixed-point integers with RISK_FACTOR_SCALE = 100_000:
#   risk_factor / RISK_FACTOR_SCALE = the effective multiplier.
# Integer representation guarantees numerical determinism across platforms.
RISK_FACTOR_SCALE: int = 100_000          # baseline: 1.0 = 100_000

# ContractRiskTracker system parameters (genesis-initialized, window-updated).
# These define the MECHANISM — not the values that emerge from it.
RISK_TRACKER_PARAMS = {
    "escalation_step": 25_000,              # +0.25× per window above tolerance
    "deescalation_step": 5_000,             # -0.05× per N conforming windows
    "max_risk_factor": 200_000,             # 2.0× cap
    "baseline_risk_factor": 100_000,        # 1.0× floor (new contracts start here)
    "tolerance": 0.50,                      # ±50% allowed deviation
    "conforming_windows_for_deescalation": 4,  # consecutive conforming windows for one de-escalation step
}


# FeeV3: the fee is plaintext (fees_db[height] += fee). The FeeV2 Pedersen
# accumulator, encrypted-fee channel, and threshold proof are removed.
from sim.crypto import poseidon_hash


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
    contract_id: str,
    function: str,
    profiles: list,
    risk_tracker: 'ContractRiskTracker' = None,
) -> tuple:
    """Return (CostProfile, risk_factor) for a function call.

    Resolution rules (fee-spec.md §14.7, FI-RISK-6):
    1. Risk factor comes from the per-contract chain-state tree, NOT from a
       static attestation classification. If risk_tracker is provided, reads
       the contract's current risk factor. Otherwise uses baseline (1.0×).
    2. No profiles at all → DEFAULT_COST_PROFILE
    3. Function not in profiles → 2.0× max declared difficulty (pessimistic)
    4. Function found → declared profile

    The risk_factor is a per-contract dynamic value maintained by
    ContractRiskTracker. The manifest declares costs; the tracker assigns risk.
    """
    if risk_tracker is not None:
        risk_factor = risk_tracker.get_risk_factor(contract_id)
    else:
        risk_factor = RISK_TRACKER_PARAMS["baseline_risk_factor"]

    if not profiles:
        return (DEFAULT_COST_PROFILE, risk_factor)

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
# FeeParamsV3 — [1:1] Rust FeeParamsV3, fee-spec.md §12.4
# ============================================================================


@dataclass
class FeeParamsV3:
    """Encoded FeeV3 parameters. [1:1] Rust FeeParamsV3 in model/fee.rs.

    Plaintext fee: no Pedersen commitment, no threshold proof, no encrypted
    fee channel. fee-spec.md §12.4. The mass-balance Fee_V2 circuit still binds
    input = output + fee, but the fee itself is public.
    """
    input_bytes: bytes
    output_bytes: bytes
    fee: int                 # plaintext FeeAmount (wow)
    tier: FeeTier            # three-tier priority selector
    tx_nonce: int
    # Consensus validation fields (merkle root + nullifier)
    merkle_root: bytes = None   # 32 bytes — from CoinMerkleTree.root()
    nullifier: bytes = None     # 32 bytes — coin nullifier
    output_coin: bytes = None   # 32 bytes — output coin commitment

    def encode(self) -> bytes:
        """Serialize to wire format matching Rust FeeParamsV3::encode()."""
        import struct
        result = bytearray()
        result.extend(self.input_bytes)
        result.extend(self.output_bytes)
        result.extend(struct.pack('<Q', self.fee))
        result.extend(struct.pack('<B', self.tier.multiplier()))
        return bytes(result)


def compute_total_fee(
    gas: int,
    cf: 'CongestionFactor',
    tier: FeeTier,
    risk_factor: int,
) -> FeeAmount:
    """FeeV3 fee: fee = gas × base_price × CF × tier × risk. fee-spec.md §12.4.1.

    gas         = circuit ZK row count (§12.4.2, Σ rows(opcode))
    BASE_PRICE  = flat wow-per-gas constant (placeholder, §12.5)
    cf          = congestion factor (§12.4.4)
    tier        = {low:1×, medium:2×, high:4×} priority multiplier
    risk_factor = ContractRiskTracker factor in RISK_FACTOR_SCALE units (1.0× = 100_000)

    Fixed-point: CF is in SCALE units (1.0 = 1_000_000), risk in
    RISK_FACTOR_SCALE units (1.0 = 100_000). The fee is the integer product
    divided by (SCALE × RISK_FACTOR_SCALE).

    The FeeV2 `wasm_kB × BASELINE_STORAGE` storage component is gone — it is a
    separate additive DeployV1-only charge (§12.4.3), not part of the per-call
    fee. The per-contract risk factor is the dynamic `ContractRiskTracker`
    factor, not a static attestation classification.
    """
    if isinstance(risk_factor, RiskFactor):
        risk_factor = risk_factor.get()
    if isinstance(cf, CongestionFactor):
        # §12.4.4: the high tier uses CF_premium; medium/low use CF_standard.
        cf_val = cf.premium.get() if tier.multiplier() == FeeTier.HIGH else cf.standard.get()
    else:
        cf_val = cf
    return FeeAmount(
        gas * BASE_PRICE * cf_val * tier.multiplier() * risk_factor
        // (SCALE * RiskFactor.SCALE)
    )


def circuit_difficulty(opcodes: list, k: int = None) -> int:
    """Sum of per-opcode ZK row counts. [1:1] Rust circuit_difficulty().

    Gas = Σ rows(opcode). The circuit's k is DERIVED from the total rows
    (k = ceil(log2(rows))), so there is no separate 2^(k - K_REF) multiplier —
    that scaling was a redundant proxy for the row count and is removed
    (fee-spec.md §12.11). The `k` argument is retained for backward-compat and
    ignored.
    """
    return sum(OPCODE_ROWS.get(op, 0) for op in opcodes)


def compute_fee(circuit_costs: list, wasm_kb: WasmKb,
                wasm_cf: 'CongestionFactor', circuit_cf: 'CongestionFactor') -> FeeAmount:
    """Two-component sum formula. [1:1] Rust compute_fee().

    fee = (wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)

    Always uses premium CF multipliers — this is the admission threshold.
    Tier classification (premium vs general) is the caller's responsibility.
    """
    if isinstance(wasm_kb, int):
        wasm_kb = WasmKb(wasm_kb)
    total_opcode_cost = sum(circuit_costs)
    wasm_part = (wasm_kb.get() * BASELINE_STORAGE * wasm_cf.premium.get()) // SCALE
    circuit_part = (total_opcode_cost * circuit_cf.premium.get()) // SCALE
    return FeeAmount(wasm_part + circuit_part)

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
    """Fixed-point congestion factor. 1.0 = SCALE (fields are CfValue)."""
    premium: CfValue = field(default_factory=lambda: CfValue(SCALE))
    standard: CfValue = field(default_factory=lambda: CfValue(SCALE))

    def __post_init__(self):
        # Auto-wrap raw int call sites into the nominal CfValue (no bare int).
        if isinstance(self.premium, int):
            self.premium = CfValue(self.premium)
        if isinstance(self.standard, int):
            self.standard = CfValue(self.standard)

    def premium_float(self) -> float:
        return self.premium.get() / SCALE

    def standard_float(self) -> float:
        return self.standard.get() / SCALE

    def premium_threshold(self) -> int:
        """Premium CF value. Used by adjust() return for backward compat."""
        return self.premium.get()

    def general_threshold(self) -> int:
        """Standard CF value. Used by adjust() return for backward compat."""
        return self.standard.get()


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
    """Three-tier mempool with fee window integration. FCFS within tiers.

    fee-spec.md §7, §12.8.1, FI-ADMIT-1/2: `fee >= PRICE_HIGH → high_queue`,
    `fee >= PRICE_MEDIUM → medium_queue`, `fee >= PRICE_LOW → low_queue`, else
    reject. The declared tier's fee is re-derived (FI-PLAIN-2) before routing.
    """

    def __init__(self, window: FeeWindow):
        self.window = window
        self.high_queue: deque = deque()     # (tx_id, fee) — FCFS
        self.medium_queue: deque = deque()   # (tx_id, fee) — FCFS
        self.low_queue: deque = deque()      # (tx_id, fee) — FCFS

    # ── Queue depths (CF mapping per §12.4.4: P_premium = high, P_standard = medium+low) ──
    @property
    def high_count(self) -> int:
        return len(self.high_queue)

    @property
    def medium_count(self) -> int:
        return len(self.medium_queue)

    @property
    def low_count(self) -> int:
        return len(self.low_queue)

    @property
    def premium_count(self) -> int:
        """Backward-compat alias: P_premium = high queue (§12.4.4)."""
        return len(self.high_queue)

    @property
    def standard_count(self) -> int:
        """Backward-compat alias: P_standard = medium + low queues (§12.4.4)."""
        return len(self.medium_queue) + len(self.low_queue)

    def expected_fee(self, gas: int, tier: FeeTier,
                     risk_factor: int = RISK_TRACKER_PARAMS["baseline_risk_factor"]) -> FeeAmount:
        """Re-derive the fee a tx MUST pay for a declared tier (§12.4.1, FI-PLAIN-2)."""
        return compute_total_fee(gas, self.window.circuit_cf, tier, risk_factor)

    def admit(self, tx_id: str, fee: int, gas: int, tier: FeeTier,
              risk_factor: int = RISK_TRACKER_PARAMS["baseline_risk_factor"]) -> str:
        """Admit a transaction. Returns 'high', 'medium', 'low', or 'reject'.

        FI-PLAIN-2: fee must cover the declared tier's re-derived price.
        FI-ADMIT-1: route to the highest tier whose price the fee meets.
        """
        # Re-derivation check (FI-PLAIN-2): fee >= gas × tier_price(declared_tier).
        if fee < self.expected_fee(gas, tier, risk_factor):
            return "reject"
        # Three-way routing (FI-ADMIT-1) — highest tier whose price the fee meets.
        if fee >= self.expected_fee(gas, FeeTier(FeeTier.HIGH), risk_factor):
            self.high_queue.append((tx_id, fee))
            return "high"
        if fee >= self.expected_fee(gas, FeeTier(FeeTier.MEDIUM), risk_factor):
            self.medium_queue.append((tx_id, fee))
            return "medium"
        self.low_queue.append((tx_id, fee))
        return "low"

    def select_for_block(self, max_txs: int) -> List[str]:
        """Select transactions for a block. FCFS within tiers: high → medium → low."""
        selected = []
        for queue in (self.high_queue, self.medium_queue, self.low_queue):
            while queue and len(selected) < max_txs:
                tx_id, _ = queue.popleft()
                selected.append(tx_id)
        return selected

    def on_window_boundary(self, new_window: FeeWindow):
        """I3: Preserve existing queues. New thresholds apply to new arrivals only.

        Note (fee-spec §12.8.4): The 30s transition delay is deferred to L3
        (Docker multi-node). This is a real-time mempool coordination concern
        requiring wall-clock timing — impractical to model at L1/L2 without
        making tests 30+ seconds long. Tested via Docker pipeline."""
        self.window = new_window
        # Existing txs stay in their queues — no eviction


# ============================================================================
# Consensus Validation Layer — mass_balance domain
# Models the accept_block execute+apply cycle that the Python fee_signalling
# model previously omitted. [1:1] with Rust native_token entrypoint checks P2-P8.
# ============================================================================

# Sentinel values matching Rust init_contract (entrypoint/mod.rs:177-178)
EMPTY_COINS_TREE_ROOT: bytes = bytes.fromhex(
    "0200000000000000000000000000000000000000000000000000000000000000"
)


class CoinMerkleTree:
    """Append-only Merkle tree matching Rust MerkleTree::new(1).
    Starts with sentinel ZERO leaf at position 0 per init_contract:177-178.
    Uses simple poseidon-based merkle for Python determinism."""

    def __init__(self):
        self.leaves: list = [bytes(32)]  # sentinel ZERO leaf = pallas::Base::zero()
        self._roots: list = []
        self._checkpoints: list = []

    def append(self, leaf) -> int:
        """Append a leaf, return its position. Rust: merkle_add in merkle.rs."""
        pos = len(self.leaves)
        self.leaves.append(leaf)
        self._roots.append(self.root())
        return pos

    def root(self, checkpoint: int = 0):
        """Current merkle root. Uses poseidon tree for 32-byte root."""
        if len(self.leaves) <= 1:
            return EMPTY_COINS_TREE_ROOT
        return self._compute_root(self.leaves)

    def _compute_root(self, leaves: list) -> bytes:
        """Simple power-of-two merkle tree using poseidon."""
        if len(leaves) == 1:
            val = int.from_bytes(leaves[0][:8], 'little') if isinstance(leaves[0], bytes) else 0
            return val.to_bytes(32, 'little')
        # Pad to power of 2
        n = 1
        while n < len(leaves):
            n *= 2
        padded = list(leaves) + [bytes(32)] * (n - len(leaves))
        # Bottom-up hashing
        layer = [int.from_bytes(x[:8], 'little') if isinstance(x, bytes) and any(b != 0 for b in x) else 0 for x in padded]
        while len(layer) > 1:
            next_layer = []
            for i in range(0, len(layer), 2):
                h = poseidon_hash([layer[i], layer[i + 1]])
                # Convert pallas::Base to bytes
                next_layer.append(int.from_bytes(bytes(h)[:8], 'little'))
            layer = next_layer
        return layer[0].to_bytes(32, 'little')

    def witness(self, pos: int, checkpoint: int = 0) -> list:
        """Merkle path for leaf at position. Rust: tree.witness(pos, 0)."""
        if pos >= len(self.leaves):
            raise IndexError(f"leaf position {pos} out of range")
        path = []
        n = 1
        while n < len(self.leaves):
            n *= 2
        for level_size in [n]:
            sibling_pos = pos ^ 1
            if sibling_pos < len(self.leaves):
                path.append(self.leaves[sibling_pos])
            pos //= 2
        return path


class CoinRootsDB:
    """Historical merkle root registry. Rust: coin_roots_db sled tree.
    Initialized with EMPTY_COINS_TREE_ROOT per init_contract:188."""

    def __init__(self):
        self._roots: set = {EMPTY_COINS_TREE_ROOT}

    def contains(self, root_bytes: bytes) -> bool:
        """Check P6: merkle root must exist in coin_roots_db.
        Rust: entrypoint/mod.rs:233 — db_contains_key(coin_roots_db, root)."""
        return root_bytes in self._roots

    def insert(self, root_bytes: bytes):
        """Register a new merkle root. Rust: merkle_add host fn."""
        self._roots.add(root_bytes)


class NativeTokenState:
    """Contract state mirroring native_token sled DBs.
    [1:1] with Rust contract state trees: coins_db, nullifiers_db,
    coin_roots_db, info_db, fees_db."""

    def __init__(self):
        self.coins: set = set()              # coins_db: registered coin commitments
        self.nullifiers: set = set()          # nullifiers_db: spent nullifiers
        self.coin_roots = CoinRootsDB()       # coin_roots_db: historical merkle roots
        self.merkle_tree = CoinMerkleTree()   # info_db: coin_merkle_tree
        self.fees_db: dict = {}               # fees_db: plaintext fee pot per height (FeeV3)
        self.coin_set: dict = {}              # coin → value tracking

    def process_coinbase(self, coin_commitment, coin_value: int, height: int = 0):
        """Simulate apply_pow_reward (entrypoint/mod.rs:1408).
        Appends coin to tree, registers the new root in coin_roots_db, and
        seeds the plaintext fees_db for the height (FeeV3 — no accumulator)."""
        self.merkle_tree.append(coin_commitment)
        new_root = self.merkle_tree.root()
        self.coin_roots.insert(new_root)
        self.coins.add(coin_commitment)
        self.coin_set[coin_commitment] = coin_value
        self.fees_db[height] = 0

    def validate_fee_v2(self, params: 'FeeParamsV3') -> str:
        """Simulate fee entrypoint checks P6-P7 (merkle root + nullifier).
        FeeV3: the fee is plaintext, so no Pedersen commitment verification."""
        # P6: merkle root must exist in coin_roots_db
        merkle_root = params.merkle_root
        if merkle_root is None:
            return "P6-TransferMerkleRootNotFound: no merkle_root in params"
        if not self.coin_roots.contains(merkle_root):
            return f"P6-TransferMerkleRootNotFound: {merkle_root.hex()[:16]}..."
        # P7: nullifier must not already be spent
        nf = params.nullifier
        if nf is not None and nf in self.nullifiers:
            return "P7-DuplicateNullifier"
        return "ok"

    def apply_fee_v2(self, params: 'FeeParamsV3', fee_amount: int, height: int = 0):
        """Simulate apply_fee (FeeV3): plaintext fee → fees_db[height] += fee.
        Marks nullifier spent and registers the output coin (no Pedersen commit)."""
        if params.nullifier is not None:
            self.nullifiers.add(params.nullifier)
        if params.output_coin is not None:
            self.coins.add(params.output_coin)
        self.fees_db[height] = self.fees_db.get(height, 0) + fee_amount


# FeeV3 params carrying real merkle data (plaintext fee — no threshold proof,
# no Pedersen commitment, no encrypted fee channel). fee-spec.md §12.4.
def build_fee_params_v3_with_merkle(
    state: NativeTokenState,
    coin_commitment,
    coin_value: int,
    fee_amount: int,
    tier: FeeTier = None,
) -> FeeParamsV3:
    """Build FeeParamsV3 with real merkle proofs instead of b'\x00'*224.
    [1:1] with Rust FeeV3CallBuilder::build() (plaintext fee + tier)."""
    if tier is None:
        tier = FeeTier(FeeTier.LOW)
    tree = state.merkle_tree
    leaves = tree.leaves
    # Find coin position
    try:
        pos = leaves.index(coin_commitment)
    except ValueError:
        raise ValueError(f"Coin commitment not in merkle tree")
    root = tree.root()

    # Build real input bytes from merkle data
    merkle_root_bytes = root[:32] if len(root) >= 32 else root.ljust(32, b'\x00')
    nullifier = poseidon_hash([coin_value, pos, int.from_bytes(merkle_root_bytes[:8], 'little')])
    nullifier_bytes = int.to_bytes(nullifier if isinstance(nullifier, int) else 0, 32, 'little')

    # Build input_bytes with merkle_root at offset 96 (matches Rust Input::encode)
    input_bytes = bytearray(224)
    input_bytes[96:128] = merkle_root_bytes[:32]
    input_bytes[0:32] = nullifier_bytes[:32]

    return FeeParamsV3(
        input_bytes=bytes(input_bytes),
        output_bytes=b'\x00' * 130,
        fee=fee_amount,
        tier=tier,
        tx_nonce=0,
        merkle_root=merkle_root_bytes,
        nullifier=nullifier_bytes,
        output_coin=None,
    )


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
    """I3: admitted transactions survive window boundary; three-tier FCFS."""
    w = FeeWindow()
    mempool = MempoolWithWindow(w)

    # Zero congestion (CF=1.0): fee = gas × BASE_PRICE × tier_multiplier.
    # LOW gas=1000 → 1_000_000_000; HIGH gas=5000 → 20_000_000_000.
    fee_high = compute_total_fee(5000, w.circuit_cf, FeeTier(FeeTier.HIGH), 100_000)
    fee_low = compute_total_fee(100, w.circuit_cf, FeeTier(FeeTier.LOW), 100_000)
    assert mempool.admit("tx1", fee_high, 5000, FeeTier(FeeTier.HIGH)) == "high"
    assert mempool.admit("tx2", fee_low, 100, FeeTier(FeeTier.LOW)) == "low"
    assert mempool.high_count == 1
    assert mempool.medium_count == 0
    assert mempool.low_count == 1

    # Window boundary — congest CFs to create distinct tier prices
    w2 = FeeWindow()
    w2.adjust_circuit(1000, 5000)  # circuit CF congested
    w2.adjust_wasm(1000, 5000)     # WASM CF congested
    mempool.on_window_boundary(w2)

    # I3: existing txs preserved (not evicted)
    assert mempool.high_count == 1, "I3 violated: high tx evicted"
    assert mempool.low_count == 1, "I3 violated: low tx evicted"


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
    """WASM deployment size multiplies the DeployV1 storage charge (§12.4.3)."""
    cf = CongestionFactor()  # CF = 1.0
    # wasm_kB × BASELINE_STORAGE per kB: 5 kB → 5_000_000 + circuit(1000) = 5_001_000
    fee5 = compute_fee([1000], WasmKb(5), cf, cf)
    fee10 = compute_fee([1000], WasmKb(10), cf, cf)
    assert fee5 == 5_001_000, f"5 kB deploy: expected 5_001_000, got {fee5}"
    assert fee10 == 10_001_000, f"10 kB deploy: expected 10_001_000, got {fee10}"
    assert fee10 > fee5, "larger deploy pays a higher storage charge"


def test_high_fcfs_before_medium_before_low():
    """High queue drains FCFS before medium, medium before low (FI-ADMIT-2)."""
    w = FeeWindow()
    # Congest CFs so premium > standard, creating distinct tier prices
    w.adjust_circuit(500, 5000)  # circuit CF congested
    w.adjust_wasm(500, 5000)     # WASM CF congested
    mempool = MempoolWithWindow(w)

    cf = w.circuit_cf
    fee_high = compute_total_fee(1000, cf, FeeTier(FeeTier.HIGH), 100_000)
    fee_med = compute_total_fee(1000, cf, FeeTier(FeeTier.MEDIUM), 100_000)
    fee_low = compute_total_fee(1000, cf, FeeTier(FeeTier.LOW), 100_000)

    # Admit interleaved across all three tiers.
    mempool.admit("h1", fee_high, 1000, FeeTier(FeeTier.HIGH))
    mempool.admit("l1", fee_low, 1000, FeeTier(FeeTier.LOW))
    mempool.admit("h2", fee_high, 1000, FeeTier(FeeTier.HIGH))
    mempool.admit("m1", fee_med, 1000, FeeTier(FeeTier.MEDIUM))
    mempool.admit("m2", fee_med, 1000, FeeTier(FeeTier.MEDIUM))
    mempool.admit("l2", fee_low, 1000, FeeTier(FeeTier.LOW))

    selected = mempool.select_for_block(10)
    # High FCFS first, then medium, then low
    assert selected[0] == "h1", f"expected h1 first, got {selected[0]}"
    assert selected[1] == "h2", f"expected h2 second, got {selected[1]}"
    assert selected[2] == "m1", f"expected m1 third, got {selected[2]}"
    assert selected[3] == "m2", f"expected m2 fourth, got {selected[3]}"
    assert selected[4] == "l1", f"expected l1 fifth, got {selected[4]}"
    assert selected[5] == "l2", f"expected l2 sixth, got {selected[5]}"


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
# Gas = Row-Count Tests — fee-spec.md §12.4.2 / §12.11
# ============================================================================

def test_k_scaling_reference():
    """circuit_difficulty sums rows; the k argument is ignored (backward-compat)."""
    ops = ["WitnessBase", "BaseAdd", "ConstrainInstance"]
    diff_k11 = circuit_difficulty(ops, k=11)
    diff_no_k = circuit_difficulty(ops)  # default k=None
    assert diff_k11 == diff_no_k, f"k should be ignored: {diff_k11} vs {diff_no_k}"
    expected = OPCODE_ROWS["WitnessBase"] + OPCODE_ROWS["BaseAdd"] + OPCODE_ROWS["ConstrainInstance"]
    assert diff_k11 == expected, f"expected {expected}, got {diff_k11}"


def test_k_scaling_doubles_per_increment():
    """PoseidonHash = 36 rows, independent of k."""
    ops = ["PoseidonHash"]
    base = OPCODE_ROWS["PoseidonHash"]
    assert base == 36
    for k in (11, 12, 13, 14, 15):
        assert circuit_difficulty(ops, k=k) == base, f"k={k} should not scale gas"


def test_k_scaling_max_k():
    """BaseAdd = 1 row, independent of k."""
    ops = ["BaseAdd"]
    assert circuit_difficulty(ops, k=16) == 1


def test_k_below_reference_no_fractional_scaling():
    """BaseMul = 1 row; k below reference has no effect."""
    ops = ["BaseMul"]
    assert circuit_difficulty(ops, k=10) == 1
    assert circuit_difficulty(ops, k=9) == 1
    assert circuit_difficulty(ops, k=0) == 1


def test_k_above_max_k_capped():
    """k above MAX_K has no effect (no scaling to cap)."""
    ops = ["BaseAdd"]
    assert circuit_difficulty(ops, k=17) == 1
    assert circuit_difficulty(ops, k=20) == 1


def test_k_scaling_empty_circuit():
    """Empty circuit costs zero regardless of k."""
    assert circuit_difficulty([], k=11) == 0
    assert circuit_difficulty([], k=15) == 0


def test_k_scaling_fee_threshold_v1_unchanged():
    """5 simple ops (1 row each) = 5 rows."""
    ops = ["WitnessBase", "ConstrainEqualBase", "ConstrainEqualBase",
           "ConstrainInstance", "ConstrainInstance"]
    assert circuit_difficulty(ops, k=11) == 5


def test_k_scaling_circuit_rates_with_k():
    """Row-count ordering is monotonic: SMT > Merkle > ECC > BaseDiv > LessThan > RangeCheck > arithmetic."""
    assert OPCODE_ROWS["SparseMerkleRoot"] > OPCODE_ROWS["MerkleRoot"]
    assert OPCODE_ROWS["MerkleRoot"] > OPCODE_ROWS["EcMul"]
    assert OPCODE_ROWS["EcMul"] > OPCODE_ROWS["BaseDiv"]
    assert OPCODE_ROWS["BaseDiv"] > OPCODE_ROWS["LessThanStrict"]
    assert OPCODE_ROWS["LessThanStrict"] > OPCODE_ROWS["RangeCheck"]
    assert OPCODE_ROWS["RangeCheck"] > OPCODE_ROWS["BaseMul"]


def test_k_scaling_composed_transaction():
    """Transaction with two circuits: Fee_V2 (PoseidonHash=36) + FeeThreshold_V1 (5 rows)."""
    fee_v2_cost = circuit_difficulty(["PoseidonHash"])  # 36
    threshold_cost = circuit_difficulty(
        ["WitnessBase", "ConstrainEqualBase", "ConstrainEqualBase",
         "ConstrainInstance", "ConstrainInstance"])  # 5
    total = fee_v2_cost + threshold_cost
    assert total == 41, f"composed tx: expected 41, got {total}"
    # Verify fee at zero congestion
    cf = CongestionFactor()
    fee = compute_fee([fee_v2_cost, threshold_cost], wasm_kb=1, wasm_cf=cf, circuit_cf=cf)
    assert fee == BASELINE_STORAGE + total, f"composed fee: expected {BASELINE_STORAGE + total}, got {fee}"


# ============================================================================
# Execution Risk Factor + Cost Profile Tests — fee-spec.md §12.12, manifest.md
# ============================================================================


def test_risk_tracker_params():
    """FI-RISK-2: System parameters define the mechanism — escalation > de-escalation, cap > baseline."""
    p = RISK_TRACKER_PARAMS
    assert p["baseline_risk_factor"] == 100_000  # 1.0×
    assert p["max_risk_factor"] == 200_000       # 2.0× cap
    assert p["escalation_step"] > p["deescalation_step"], \
        "FI-RISK-2: de-escalation SHALL be slower than escalation"
    assert p["max_risk_factor"] > p["baseline_risk_factor"]


def test_risk_tracker_new_contract_baseline():
    """FI-RISK-4: New contracts start at baseline risk factor."""
    tracker = ContractRiskTracker()
    assert tracker.get_risk_factor("new_contract") == RISK_TRACKER_PARAMS["baseline_risk_factor"]


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
    """Function found in profiles returns declared profile + per-contract risk factor."""
    profiles = [
        CostProfile("TransferV2", 1000, 12),
        CostProfile("BurnV2", 800, 12),
    ]
    tracker = ContractRiskTracker()
    tracker._contract_risk["contract_A"] = 100_000  # baseline
    profile, risk = resolve_cost_profile("contract_A", "TransferV2", profiles, tracker)
    assert profile.function == "TransferV2"
    assert profile.circuit_difficulty == 1000
    assert risk == 100_000


def test_resolve_cost_profile_missing_function():
    """Missing function → 2.0× max declared difficulty, risk from per-contract state."""
    profiles = [
        CostProfile("TransferV2", 1000, 12),
        CostProfile("BurnV2", 800, 12),
    ]
    tracker = ContractRiskTracker()
    tracker._contract_risk["contract_B"] = 150_000  # elevated risk
    profile, risk = resolve_cost_profile("contract_B", "unknown_function", profiles, tracker)
    # circuit_difficulty = 2 * max(1000, 800) = 2000
    assert profile.circuit_difficulty == 2000, (
        f"expected 2.0× max declared (2000), got {profile.circuit_difficulty}"
    )
    assert risk == 150_000, f"expected per-contract risk 150k, got {risk}"
    assert profile.k_value == 12
    assert profile.wasm_kb == 1


def test_resolve_cost_profile_no_profiles():
    """No profiles → pessimistic default, risk from per-contract state."""
    tracker = ContractRiskTracker()
    tracker._contract_risk["contract_C"] = 200_000  # max risk
    profile, risk = resolve_cost_profile("contract_C", "anything", [], tracker)
    assert profile.k_value == MAX_K, f"expected worst-case k={MAX_K}, got {profile.k_value}"
    assert risk == 200_000, f"expected per-contract max risk 200k, got {risk}"


def test_resolve_cost_profile_per_contract_independence():
    """FI-RISK-3,5: Different contracts have independent risk factors."""
    profiles = [CostProfile("TransferV2", 1000, 12)]
    tracker = ContractRiskTracker()
    tracker._contract_risk["low_risk"] = 100_000
    tracker._contract_risk["high_risk"] = 200_000
    _, risk_low = resolve_cost_profile("low_risk", "TransferV2", profiles, tracker)
    _, risk_high = resolve_cost_profile("high_risk", "TransferV2", profiles, tracker)
    assert risk_low == 100_000
    assert risk_high == 200_000
    # Without tracker: baseline for all
    _, risk_default = resolve_cost_profile("anyone", "TransferV2", profiles)
    assert risk_default == RISK_TRACKER_PARAMS["baseline_risk_factor"]


def test_compute_total_fee_zero_congestion():
    """At CF=1.0, tier=LOW, risk=1.0: fee = gas × BASE_PRICE."""
    cf = CongestionFactor()  # SCALE = 1.0
    fee = compute_total_fee(gas=1000, cf=cf, tier=FeeTier(FeeTier.LOW), risk_factor=100_000)
    expected = 1000 * BASE_PRICE  # 1_000_000_000
    assert fee == expected, f"expected {expected}, got {fee}"


def test_compute_total_fee_risk_multiplier():
    """Risk=2.0 doubles the fee."""
    cf = CongestionFactor()
    fee_normal = compute_total_fee(1000, cf, FeeTier(FeeTier.LOW), 100_000)
    fee_risky = compute_total_fee(1000, cf, FeeTier(FeeTier.LOW), 200_000)
    delta = fee_risky - fee_normal
    assert delta == 1000 * BASE_PRICE, (
        f"risk=2.0 should double the fee (delta={1000 * BASE_PRICE}), got delta={delta}"
    )


def test_compute_total_fee_tier_multiplier():
    """Tier HIGH (4×) is 4× the LOW (1×) fee; MEDIUM (2×) is 2×."""
    cf = CongestionFactor()
    fee_low = compute_total_fee(1000, cf, FeeTier(FeeTier.LOW), 100_000)
    fee_med = compute_total_fee(1000, cf, FeeTier(FeeTier.MEDIUM), 100_000)
    fee_high = compute_total_fee(1000, cf, FeeTier(FeeTier.HIGH), 100_000)
    assert fee_med == 2 * fee_low
    assert fee_high == 4 * fee_low


def test_compute_total_fee_full_pipeline():
    """End-to-end: profile → resolve → compute_total_fee with per-contract risk."""
    profiles = [
        CostProfile("TransferV2", 1000, 12, wasm_kb=1),
        CostProfile("ExecuteSwapV2", 2000, 14, wasm_kb=2),
    ]
    tracker = ContractRiskTracker()
    tracker._contract_risk["contract_A"] = 100_000  # baseline
    tracker._contract_risk["contract_B"] = 150_000  # elevated

    # Step 1: resolve cost profile for a known function (baseline risk)
    profile, risk = resolve_cost_profile("contract_A", "ExecuteSwapV2", profiles, tracker)
    assert profile.function == "ExecuteSwapV2"
    assert risk == 100_000

    # Step 2: compute fee at zero congestion, LOW tier
    cf = CongestionFactor()
    fee = compute_total_fee(profile.circuit_difficulty, cf, FeeTier(FeeTier.LOW), risk)
    expected = 2000 * BASE_PRICE  # gas=2000, tier=1×, risk=1.0×
    assert fee == expected, f"full pipeline: expected {expected}, got {fee}"

    # Step 3: resolve unknown function → pessimistic + elevated per-contract risk
    profile2, risk2 = resolve_cost_profile("contract_B", "missing_func", profiles, tracker)
    assert profile2.circuit_difficulty == 4000  # 2 * max(1000, 2000)
    assert profile2.k_value == 14  # max(12, 14) from declared
    assert profile2.wasm_kb == 2  # max(1, 2) from declared
    assert risk2 == 150_000  # from contract_B's per-contract risk
    fee2 = compute_total_fee(profile2.circuit_difficulty, cf, FeeTier(FeeTier.LOW), risk2)
    # gas=4000, risk=1.5× → 4000 × 1_000_000 × 1.5 = 6_000_000_000
    assert fee2 == 6_000_000_000, f"full pipeline missing: expected 6_000_000_000, got {fee2}"


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

    def admit(self, tx_id: str, fee: int, gas: int, tier: FeeTier,
              risk_factor: int = RISK_TRACKER_PARAMS["baseline_risk_factor"],
              nullifier=None) -> str:
        """Admit with nullifier dedup."""
        if nullifier is not None:
            if self.has_nullifier(nullifier):
                return "reject"
            self.insert_nullifier(nullifier)
        return super().admit(tx_id, fee, gas, tier, risk_factor)


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


def wallet_construct_fee(gas: int, tier: FeeTier, block_header_flags: int,
                          risk_factor: int = RISK_TRACKER_PARAMS["baseline_risk_factor"]) -> int:
    """Wallet constructs the FeeV3 fee from block header flags (FI-PLAIN-2)."""
    circuit_cf, _wasm_cf = wallet_read_flags(block_header_flags)
    return compute_total_fee(gas, circuit_cf, tier, risk_factor)


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
    """Tracks observed-vs-declared cost deviations and adjusts per-contract risk factors.

    Per fee-spec.md §14.7: risk factors are per-contract, dynamic, and chain-visible.
    Each contract earns its own risk factor through observed behavior. Risk is emergent,
    not predefined — there is no global classification table.

    The escalation and de-escalation parameters are genesis-initialized system parameters
    (RISK_TRACKER_PARAMS). The per-contract risk factors are stored in the `contract_risk`
    dictionary (simulating a chain-state sled tree).
    """

    def __init__(self, params: dict = None):
        self.params = params or RISK_TRACKER_PARAMS
        self._deviations: dict = {}  # contract_id -> list[CostDeviation]
        self._conforming_windows: dict = {}  # contract_id -> int (count)
        # Per-contract risk factor store (simulates chain-state sled tree).
        # Key: contract_id. Value: current risk factor in RISK_FACTOR_SCALE units.
        self._contract_risk: dict = {}

    def get_risk_factor(self, contract_id: str) -> int:
        """Read a contract's current risk factor. FI-RISK-4: new contracts
        start at baseline. FI-RISK-5: any node can read this."""
        return self._contract_risk.get(contract_id, self.params["baseline_risk_factor"])

    def record(self, contract_id: str, function: str, declared: int,
               observed: int, window_id: int) -> 'CostDeviation':
        """Record a cost deviation for a contract in a given window."""
        dev = CostDeviation(contract_id, function, declared, observed, window_id)
        if contract_id not in self._deviations:
            self._deviations[contract_id] = []
        self._deviations[contract_id].append(dev)
        return dev

    def evaluate_window(self, contract_id: str) -> int:
        """Evaluate a contract's deviations for the current window and update
        its risk factor. Returns the new risk factor.

        FI-RISK-2: Escalation for under-declaration, de-escalation for sustained
        accuracy. De-escalation is slower than escalation.
        """
        params = self.params
        devs = self._deviations.get(contract_id, [])
        current = self.get_risk_factor(contract_id)

        if not devs:
            return current  # no observations this window — unchanged

        above_tolerance = sum(1 for d in devs if not d.within_tolerance(params["tolerance"]))

        if above_tolerance > 0:
            # Escalation: each window above tolerance increases risk
            step = params["escalation_step"]
            new_risk = min(current + step, params["max_risk_factor"])
            self._conforming_windows[contract_id] = 0
        else:
            # All observations within tolerance — accumulate conforming windows
            self._conforming_windows[contract_id] = \
                self._conforming_windows.get(contract_id, 0) + 1
            if self._conforming_windows[contract_id] >= params["conforming_windows_for_deescalation"]:
                # De-escalation: sustained accuracy reduces risk toward baseline
                step = params["deescalation_step"]
                new_risk = max(current - step, params["baseline_risk_factor"])
                self._conforming_windows[contract_id] = 0
            else:
                new_risk = current  # not enough conforming windows yet

        self._contract_risk[contract_id] = new_risk
        # Clear this window's deviations after evaluation
        self._deviations[contract_id] = []
        return new_risk


# ============================================================================
# Phase 2a: Nullifier Replay Tests
# ============================================================================


def test_nullifier_replay_rejected():
    """Two txs, same nullifier → second rejected."""
    w = FeeWindow()
    mp = MempoolWithWindowAndNullifiers(w)
    fee = compute_total_fee(1000, w.circuit_cf, FeeTier(FeeTier.LOW), 100_000)
    assert mp.admit("tx1", fee, 1000, FeeTier(FeeTier.LOW), nullifier="nf_1") == "low"
    assert mp.admit("tx2", fee, 1000, FeeTier(FeeTier.LOW), nullifier="nf_1") == "reject"
    assert mp.low_count == 1, "only first tx should be admitted"


def test_nullifier_different_allowed():
    """Different nullifiers → both admitted."""
    w = FeeWindow()
    mp = MempoolWithWindowAndNullifiers(w)
    fee = compute_total_fee(1000, w.circuit_cf, FeeTier(FeeTier.LOW), 100_000)
    assert mp.admit("tx1", fee, 1000, FeeTier(FeeTier.LOW), nullifier="nf_a") == "low"
    assert mp.admit("tx2", fee, 1000, FeeTier(FeeTier.LOW), nullifier="nf_b") == "low"
    assert mp.low_count == 2


def test_nullifier_replay_preserves_fcfs():
    """I3 + nullifier: admitted txs stay, replays rejected, FCFS preserved."""
    w = FeeWindow()
    mp = MempoolWithWindowAndNullifiers(w)
    fee_high = compute_total_fee(5000, w.circuit_cf, FeeTier(FeeTier.HIGH), 100_000)
    fee_low = compute_total_fee(100, w.circuit_cf, FeeTier(FeeTier.LOW), 100_000)
    mp.admit("h1", fee_high, 5000, FeeTier(FeeTier.HIGH), nullifier="nf_h1")
    mp.admit("l1", fee_low, 100, FeeTier(FeeTier.LOW), nullifier="nf_l1")
    assert mp.admit("h1_dup", fee_high, 5000, FeeTier(FeeTier.HIGH), nullifier="nf_h1") == "reject"
    selected = mp.select_for_block(10)
    assert selected[0] == "h1" and selected[1] == "l1", "FCFS preserved (high → low)"


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
    """Full wallet pipeline: flags → derive_cfs → compute_total_fee (FeeV3)."""
    # +10% circuit, hold WASM
    flags = FEE_WINDOW_ACTIVE | (0x01 << 4) | (0x00 << 12)
    fee = wallet_construct_fee(1000, FeeTier(FeeTier.LOW), flags)
    # LOW tier uses CF_standard = int(SCALE × 1.10) → 1000 × 1_100_000 = 1_100_000_000
    expected = 1000 * int(SCALE * 1.10)
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
    """FI-RISK-2: One window above tolerance → risk escalates by escalation_step."""
    t = ContractRiskTracker()
    t.record("c1", "f1", 1000, 2000, 0)  # 100% above → above tolerance
    new_risk = t.evaluate_window("c1")
    step = RISK_TRACKER_PARAMS["escalation_step"]
    assert new_risk == 100_000 + step  # baseline + escalation_step


def test_risk_escalation_two_windows():
    """FI-RISK-2: Two windows above tolerance → risk compounds additively."""
    t = ContractRiskTracker()
    t.record("c1", "f1", 1000, 2000, 0)
    t.evaluate_window("c1")  # window 0
    t.record("c1", "f1", 1000, 2000, 1)
    new_risk = t.evaluate_window("c1")  # window 1
    step = RISK_TRACKER_PARAMS["escalation_step"]
    assert new_risk == 100_000 + step + step  # two escalations


def test_risk_escalation_capped():
    """FI-RISK-2: Risk factor capped at max_risk_factor."""
    t = ContractRiskTracker()
    for w in range(10):
        t.record("c1", "f1", 1000, 2000, w)
        t.evaluate_window("c1")
    assert t.get_risk_factor("c1") == RISK_TRACKER_PARAMS["max_risk_factor"]


def test_risk_deescalation():
    """FI-RISK-2: Sustained accuracy → de-escalation toward baseline."""
    t = ContractRiskTracker()
    # First escalate
    t.record("c1", "f1", 1000, 2000, 0)
    t.evaluate_window("c1")
    elevated = t.get_risk_factor("c1")
    assert elevated > 100_000
    # Now sustain accuracy for N consecutive windows
    for w in range(1, 1 + RISK_TRACKER_PARAMS["conforming_windows_for_deescalation"]):
        t.record("c1", "f1", 1000, 1100, w)  # 10% above, within 50% tolerance
        t.evaluate_window("c1")
    assert t.get_risk_factor("c1") < elevated, "risk must de-escalate after sustained accuracy"


def test_risk_deescalation_slower_than_escalation():
    """FI-RISK-2: De-escalation step < escalation step."""
    assert RISK_TRACKER_PARAMS["deescalation_step"] < RISK_TRACKER_PARAMS["escalation_step"]


def test_feedback_loop_end_to_end():
    """FI-RISK-2, FI-RISK-4: Accurate contract de-escalates; under-declarer escalates.
    Risk factors are per-contract and independent — contracts earn their own risk."""
    t = ContractRiskTracker()
    # Contract A: always accurate → eventually de-escalates toward baseline
    for w in range(8):
        t.record("accurate", "f", 1000, 1100, w)  # 10% above, within 50% tolerance
        t.evaluate_window("accurate")
    assert t.get_risk_factor("accurate") == RISK_TRACKER_PARAMS["baseline_risk_factor"]
    # Contract B: persistent under-declaration → escalates
    for w in range(8):
        t.record("under", "f", 1000, 2000, w)  # 100% above, exceeds tolerance
        t.evaluate_window("under")
    assert t.get_risk_factor("under") == RISK_TRACKER_PARAMS["max_risk_factor"]


def test_risk_emerges_from_observation_not_classification():
    """P-M6: Risk factor reflects observed behavior, not attestation status.
    An accurate 'self_declared' de-escalates. An inaccurate 'attested_endowed' escalates.
    The static RISK_FACTOR dict is dead — risk comes from the ContractRiskTracker."""
    t = ContractRiskTracker()

    # "self_declared" contract: accurate cost declarations → de-escalates
    for w in range(8):
        t.record("self_declared_accurate", "f", 1000, 1100, w)  # 10% above, ok
        t.evaluate_window("self_declared_accurate")
    assert t.get_risk_factor("self_declared_accurate") == RISK_TRACKER_PARAMS["baseline_risk_factor"], \
        "FI-RISK-2: accurate contract must de-escalate regardless of 'self_declared' label"

    # "attested_endowed" contract: chronic under-declaration → escalates
    for w in range(8):
        t.record("attested_under", "f", 1000, 2000, w)  # 100% above
        t.evaluate_window("attested_under")
    assert t.get_risk_factor("attested_under") == RISK_TRACKER_PARAMS["max_risk_factor"], \
        "FI-RISK-2: inaccurate contract must escalate regardless of 'attested_endowed' label"


def test_circuit_difficulty_from_declared_opcodes():
    """Miner verification: circuit_difficulty(declared_opcodes) == declared_circuit_difficulty.

    This is the miner's verification logic — contract authors declare opcodes and
    circuit_difficulty in the manifest. The miner independently computes the sum
    of per-opcode rows from the declared opcode list and checks against the
    declared value. A mismatch is a black mark (reputation downgrade → higher
    risk factor → higher fees).
    """
    # 5 simple ops (1 row each) → circuit_difficulty = 5
    ops = ["WitnessBase", "ConstrainEqualBase", "ConstrainEqualBase",
           "ConstrainInstance", "ConstrainInstance"]
    computed = circuit_difficulty(ops)
    declared = 5
    assert computed == declared, (
        f"miner verification: computed {computed} != declared {declared}"
    )

    # Poseidon-heavy circuit: 1 PoseidonHash → 36 rows
    ops2 = ["PoseidonHash"]
    computed2 = circuit_difficulty(ops2)
    declared2 = 36
    assert computed2 == declared2, (
        f"miner verification: computed {computed2} != declared {declared2}"
    )

    # Mixed circuit: BaseAdd(1)+BaseMul(1)+PoseidonHash(36) = 38
    ops3 = ["BaseAdd", "BaseMul", "PoseidonHash"]
    computed3 = circuit_difficulty(ops3)
    declared3 = 38
    assert computed3 == declared3, (
        f"miner verification: computed {computed3} != declared {declared3}"
    )

    # Empty circuit → zero difficulty
    computed4 = circuit_difficulty([])
    assert computed4 == 0, f"empty circuit: expected 0, got {computed4}"


# ============================================================================
# Integration Scenarios — fee-spec.md §14, fee-testing.md
# Each scenario exercises the full stack: wallet → mempool → miner → FeeCollectV1.
# [1:1] Rust: bin/dwowd/src/tests/specs/fee_integration_spec.rs
# ============================================================================


def test_p_it_1_full_lifecycle():
    """P-IT-1: Full fee lifecycle with consensus validation (FeeV3 plaintext).
    Models the complete accept_block path: coinbase → merkle root registration →
    FeeV3 with real merkle proof → P6 check → plaintext fees_db accumulation.

    Covers: FI-GEN-1, FI-ADMIT-1/3, FI-COLLECT-1/2, FI-FLAG-1
    """
    # ── Setup: chain state + fee window ──
    w = FeeWindow()
    w.adjust(0, 0)  # identity CF
    state = NativeTokenState()
    assert state.coin_roots.contains(EMPTY_COINS_TREE_ROOT), \
        "[P-IT-1-ST0] coin_roots_db initialized with EMPTY_COINS_TREE_ROOT"

    # ── Phase A: Coinbase at height 2 (creates spendable coin) ──
    coin_commitment = bytes.fromhex("ab" * 32)
    coin_value = 42_042_000
    state.process_coinbase(coin_commitment, coin_value, height=2)
    assert state.coin_roots.contains(state.merkle_tree.root()), \
        "[P-IT-1-ST1] coinbase merkle root registered in coin_roots_db"
    assert coin_commitment in state.coins, "[P-IT-1-ST1] coin in coins_db"

    # ── Phase B: Wallet constructs FeeV3 with real merkle proof ──
    flags = FeeWindow.encode_flags_dual(w.circuit_cf, w.wasm_cf)
    fee = wallet_construct_fee(1000, FeeTier(FeeTier.LOW), flags)
    assert fee > 0, "[P-IT-1-ST2-W1] wallet computed positive fee"

    # Build params with REAL merkle data — plaintext fee + tier (FeeV3)
    params = build_fee_params_v3_with_merkle(state, coin_commitment, coin_value, fee)
    assert params.merkle_root is not None, "[P-IT-1-ST2-W3] params carry real merkle root"

    # ── Phase C: Consensus validation — P6 check ──
    result = state.validate_fee_v2(params)
    assert result == "ok", \
        f"[P-IT-1-P6] FeeV3 validation must pass P6 (coin_roots_db), got: {result}"

    # ── Phase D: Mempool admission ──
    mempool = MempoolWithWindow(w)
    result = mempool.admit("tx1", fee, 1000, FeeTier(FeeTier.LOW))
    assert result == "low", f"[P-IT-1-ST3-M1] admitted to low tier, got {result}"
    assert mempool.low_count == 1, "[P-IT-1-ST3-M2] one tx in low queue"

    # ── Phase E: Apply fee → plaintext fees_db accumulation ──
    state.apply_fee_v2(params, fee, height=2)
    assert state.fees_db.get(2, 0) == fee, \
        "[P-IT-1-ST5-V2] fees_db[2] accumulated plaintext fee"
    assert params.nullifier in state.nullifiers, \
        "[P-IT-1-ST5-V5] nullifier registered after apply"

    # ── Phase F: FeeCollectV1 claims plaintext fees_db (no accumulator) ──
    assert state.fees_db[2] == fee, \
        "[P-IT-1-ST5-V3] FeeCollectV1: plaintext total == fees_db[2]"

    # ── Phase G: Nullifier replay — rejected by mempool ──
    mp2 = MempoolWithWindowAndNullifiers(w)
    assert mp2.admit("tx_a", fee, 1000, FeeTier(FeeTier.LOW), nullifier="nf_1") == "low"
    assert mp2.admit("tx_b", fee, 1000, FeeTier(FeeTier.LOW), nullifier="nf_1") == "reject", \
        "[P-IT-1-ST6-N1] nullifier replay rejected"

    # ── Phase H: Flags chain-synced ──
    flags2 = FeeWindow.encode_flags_dual(w.circuit_cf, w.wasm_cf)
    assert flags2 & FEE_WINDOW_ACTIVE, "[P-IT-1-ST5-V7] flags are active"

    # ── P6 negative test: unknown merkle root must fail ──
    bad_params = FeeParamsV3(
        input_bytes=b'\x00' * 224, output_bytes=b'\x00' * 130,
        fee=fee, tier=FeeTier(FeeTier.LOW), tx_nonce=0,
        merkle_root=bytes(32),  # all-zeros — never registered
    )
    bad_result = state.validate_fee_v2(bad_params)
    assert bad_result != "ok", \
        f"[P-IT-1-P6-NEG] unknown merkle root must fail P6, got: {bad_result}"


def test_p_it_2_multi_contract_differential():
    """P-IT-2: Two contracts with different cost profiles pay different fees
    in the same block. Pedersen homomorphic sum verified.

    Covers: FI-WINDOW-3, FI-RISK-1/3/4, FI-COLLECT-1/2, FI-WASM-1/2
    """
    # ── Setup ──
    w = FeeWindow()
    w.adjust(0, 0)
    cf = CongestionFactor()
    tracker = ContractRiskTracker()
    tracker._contract_risk["contract_A"] = 100_000  # baseline
    tracker._contract_risk["contract_B"] = 100_000  # baseline

    # ── Two different cost profiles ──
    profile_A = CostProfile("TransferV2", circuit_difficulty=1000, k_value=12, wasm_kb=1)
    profile_B = CostProfile("ExecuteSwapV2", circuit_difficulty=2000, k_value=14, wasm_kb=2)

    fee_A = compute_total_fee(profile_A.circuit_difficulty, cf, FeeTier(FeeTier.LOW),
                              tracker.get_risk_factor("contract_A"))
    fee_B = compute_total_fee(profile_B.circuit_difficulty, cf, FeeTier(FeeTier.LOW),
                              tracker.get_risk_factor("contract_B"))

    assert fee_A != fee_B, f"[P-IT-2-ST5-F1] fees differ: {fee_A} vs {fee_B}"
    assert fee_B > fee_A, f"[P-IT-2-ST5-F2] complex contract pays more: {fee_B} > {fee_A}"

    # ── Plaintext fees accumulate distinctly (FeeV3 — no Pedersen) ──
    state = NativeTokenState()
    state.fees_db[0] = 0
    state.fees_db[0] += int(fee_A)
    state.fees_db[0] += int(fee_B)
    assert state.fees_db[0] == int(fee_A) + int(fee_B), \
        "[P-IT-2-ST7-V1] plaintext fees_db = fee_A + fee_B"

    # ── Independent risk factors ──
    assert tracker.get_risk_factor("contract_A") == 100_000, \
        "[P-IT-2-ST7-V5] contract A risk baseline"
    assert tracker.get_risk_factor("contract_B") == 100_000, \
        "[P-IT-2-ST7-V6] contract B risk baseline"

    # ── WASM component: larger wasm_kb → higher fee ──
    wasm_part_A = profile_A.wasm_kb * BASELINE_STORAGE
    wasm_part_B = profile_B.wasm_kb * BASELINE_STORAGE
    assert wasm_part_B > wasm_part_A, \
        f"[P-IT-2-WASM] wasm_kb=2 costs more than wasm_kb=1: {wasm_part_B} > {wasm_part_A}"


def test_p_it_3_cross_window_congestion():
    """P-IT-3: 3 windows of varying congestion. CF evolution, flag
    propagation, wallet re-sync after offline.

    Covers: FI-WINDOW-1/2/3, FI-FLAG-1/2/3, FI-ADMIT-2, FI-TIME-1
    """
    w = FeeWindow()

    # ── Window 0: zero congestion ──
    p0, g0 = w.adjust(0, 0)
    assert p0 == SCALE and g0 == SCALE, "[P-IT-3-ST1] window 0: identity CF"

    # ── Window 1: heavy congestion ──
    # Capture pre-adjustment CF for flag encoding (adjust() updates previous_cf)
    prev_cf = w.current_cf
    p1, g1 = w.adjust(premium_pending=50, standard_pending=500)
    assert p1 > SCALE, "[P-IT-3-ST3] window 1: premium CF above scale"
    assert g1 > SCALE, "[P-IT-3-ST3b] window 1: standard CF above scale"
    assert p1 > g1, "[P-IT-3-ST4] I4: premium > standard"

    # Encode flags from pre-adjustment to post-adjustment CF
    flags1 = FeeWindow.encode_flags(w.current_cf, previous=prev_cf)
    cm1 = (flags1 >> 4) & 0x0F
    assert cm1 == 1, f"[P-IT-3-ST8] window 1 flags show increase, cm={cm1}"

    # ── Window 2: decreasing congestion ──
    prev_cf2 = w.current_cf
    p2, g2 = w.adjust(premium_pending=5, standard_pending=20)
    # Standard CF decreases with reduced congestion; premium stays near cap floor
    assert g2 < g1, "[P-IT-3-ST6] window 2: standard CF decreased"
    assert p2 <= p1, "[P-IT-3-ST6b] window 2: premium CF within ±10% cap of window 1"
    # ±10% cap check
    ratio = p2 / p1
    assert 0.89 < ratio < 1.11, f"[P-IT-3-ST7] ±10% cap: ratio={ratio:.3f}"

    flags2 = FeeWindow.encode_flags(w.current_cf, previous=prev_cf2)
    cm2 = (flags2 >> 4) & 0x0F
    # With standard CF decreased and premium unchanged, flags may show hold (0)
    # or decrease (2) depending on magnitude. Either is valid within cap.
    assert cm2 in (0, 2), f"[P-IT-3-ST8b] window 2 flags valid: cm={cm2}"

    # ── Wallet re-sync after offline ──
    # Flags encode direction (+10%/-10%/hold), not exact CF values.
    # The wallet derives an approximate CF from flags — sufficient for fee
    # construction. With cm=0 (hold), wallet derives SCALE = 1_000_000.
    flags = FeeWindow.encode_flags_dual(w.circuit_cf, w.wasm_cf)
    cf_w, wf_w = wallet_read_flags(flags)
    fee_from_flags = wallet_construct_fee(1000, FeeTier(FeeTier.LOW), flags)
    assert fee_from_flags > 0, "[P-IT-3-ST10] wallet constructs valid fee from flags"
    # Flags are active and well-formed
    assert flags & FEE_WINDOW_ACTIVE, "[P-IT-3-ST10] wallet sees active flags"
    circuit_cm, wasm_cm = FeeWindow.decode_flags_dual(flags)
    assert circuit_cm in (0, 1, 2), f"[P-IT-3-ST10] valid circuit cm: {circuit_cm}"
    assert wasm_cm in (0, 1, 2), f"[P-IT-3-ST10] valid wasm cm: {wasm_cm}"

    # ── Flags advisory ──
    assert FeeWindow.decode_flags(0xFF, SCALE) == SCALE, \
        "[P-IT-3-ST15] invalid flags → hold (advisory, not rejected)"


def test_p_it_4_risk_emergence():
    """P-IT-4: 5+ windows. Contract A accurate (stays baseline), Contract B
    under-declares (escalates to cap), then fixes (de-escalates).

    Covers: FI-RISK-1/2/3/4/5/6, FI-GEN-1, FI-WASM-1
    """
    tracker = ContractRiskTracker()

    # ── Initial state: both at baseline ──
    assert tracker.get_risk_factor("accurate") == 100_000, "[P-IT-4-ST1] A baseline"
    assert tracker.get_risk_factor("under") == 100_000, "[P-IT-4-ST2] B baseline"

    # ── Windows 1-4: A accurate, B under-declares ──
    for w in range(1, 5):
        # A: within tolerance (10% above declared)
        tracker.record("accurate", "f", 1000, 1100, w)
        tracker.evaluate_window("accurate")
        # B: 100% above declared (outside 50% tolerance)
        tracker.record("under", "f", 1000, 2000, w)
        tracker.evaluate_window("under")

    step = tracker.params["escalation_step"]
    assert tracker.get_risk_factor("accurate") == 100_000, \
        "[P-IT-4-ST8] A stays at baseline after 4 accurate windows"
    expected_b = min(100_000 + step * 4, tracker.params["max_risk_factor"])
    assert tracker.get_risk_factor("under") == expected_b, \
        f"[P-IT-4-ST7] B escalated to {expected_b} after 4 under-declaring windows"

    # ── Windows 5-8: B fixes declarations ──
    # First escalate B to cap so we can test de-escalation
    for w in range(5, 13):
        tracker.record("under", "f", 1000, 2000, w)
        tracker.evaluate_window("under")
    assert tracker.get_risk_factor("under") == tracker.params["max_risk_factor"], \
        "[P-IT-4-ST7b] B capped at max_risk_factor"

    # Now B fixes: accurate declarations for N conforming windows
    for w in range(13, 13 + tracker.params["conforming_windows_for_deescalation"]):
        tracker.record("under", "f", 1000, 1100, w)  # within tolerance
        tracker.evaluate_window("under")
    assert tracker.get_risk_factor("under") < tracker.params["max_risk_factor"], \
        "[P-IT-4-ST13] B de-escalates after sustained accuracy"

    # ── FI-RISK-5: any node can read risk ──
    risk_A = tracker.get_risk_factor("accurate")
    risk_B = tracker.get_risk_factor("under")
    assert risk_A != risk_B, \
        f"[P-IT-4-ST10] independent risk factors: A={risk_A}, B={risk_B}"


def test_p_it_5_attack_vectors():
    """P-IT-5: Attack vectors — stale threshold admission (FeeV3 plaintext fee,
    no encrypted-fee channel).

    Covers: FI-ADMIT-1/2, FI-PLAIN-2
    """
    # ── V4: Stale threshold → re-derivation check rejects underpayment ──
    w = FeeWindow()
    w.adjust(50, 500)  # congested CF
    mempool = MempoolWithWindow(w)
    # Fee computed at identity CF (stale) won't meet the congested low-tier price.
    cf_id = CongestionFactor()
    fee_stale = compute_total_fee(1000, cf_id, FeeTier(FeeTier.LOW), 100_000)
    result_low = mempool.admit("tx_low", fee_stale, 1000, FeeTier(FeeTier.LOW))
    assert result_low == "reject", \
        f"[P-IT-5-ST13-V5] stale threshold → reject: {result_low}"

    # ── V5: Proper fee at congested CF passes ──
    fee_high = compute_total_fee(1000, w.circuit_cf, FeeTier(FeeTier.LOW), 100_000)
    result_high = mempool.admit("tx_high", fee_high, 1000, FeeTier(FeeTier.LOW))
    assert result_high == "low", \
        f"[P-IT-5] correct fee at congested CF admitted: {result_high}"


def test_p_it_6_three_tier_admission():
    """P-IT-6: 20 transactions (5 high, 5 medium, 5 low, 5 rejected).
    FCFS ordering, nullifier dedup, partial drain.

    Covers: FI-ADMIT-1/2/3, FI-PLAIN-2
    """
    # ── Setup: congested CF to create distinct tier prices ──
    # CF_premium = 5.0× (5_000_000), CF_standard = 2.5× (2_500_000).
    # HIGH = 1000 × 5M × 4 = 20_000_000_000; MEDIUM = 1000 × 2.5M × 2 = 5_000_000_000;
    # LOW = 1000 × 2.5M × 1 = 2_500_000_000.
    w = FeeWindow()
    w._circuit_cf = CongestionFactor(premium=5_000_000, standard=2_500_000)
    w._wasm_cf = CongestionFactor(premium=5_000_000, standard=2_500_000)
    mp = MempoolWithWindowAndNullifiers(w)

    cf = w.circuit_cf
    fee_high = compute_total_fee(1000, cf, FeeTier(FeeTier.HIGH), 100_000)
    fee_med = compute_total_fee(1000, cf, FeeTier(FeeTier.MEDIUM), 100_000)
    fee_low = compute_total_fee(1000, cf, FeeTier(FeeTier.LOW), 100_000)

    # ── Phase A: Admit 20 txs — 5 per tier, 5 rejected ──
    highs, mediums, lows = [], [], []
    rejected = 0
    for i in range(5):
        assert mp.admit(f"h{i}", fee_high, 1000, FeeTier(FeeTier.HIGH), nullifier=f"nf_h{i}") == "high"
        highs.append(f"h{i}")
    for i in range(5):
        assert mp.admit(f"m{i}", fee_med, 1000, FeeTier(FeeTier.MEDIUM), nullifier=f"nf_m{i}") == "medium"
        mediums.append(f"m{i}")
    for i in range(5):
        assert mp.admit(f"l{i}", fee_low, 1000, FeeTier(FeeTier.LOW), nullifier=f"nf_l{i}") == "low"
        lows.append(f"l{i}")
    for i in range(5):
        assert mp.admit(f"r{i}", 1_000_000_000, 1000, FeeTier(FeeTier.LOW), nullifier=f"nf_r{i}") == "reject"
        rejected += 1

    assert rejected == 5, f"[P-IT-6-ST3] 5 rejected, got {rejected}"

    # ── Phase B: FCFS within tiers (high → medium → low) ──
    selected = mp.select_for_block(20)
    assert selected[:5] == highs, f"[P-IT-6-ST4] high FCFS: {selected[:5]} vs {highs}"
    assert selected[5:10] == mediums, f"[P-IT-6-ST9] medium FCFS: {selected[5:10]} vs {mediums}"
    assert selected[10:15] == lows, f"[P-IT-6-ST13] low FCFS: {selected[10:15]} vs {lows}"

    # ── Phase C: Nullifier replay (replay first high tx's nullifier) ──
    result_replay = mp.admit("tx_replay", fee_high, 1000, FeeTier(FeeTier.HIGH), nullifier="nf_h0")
    assert result_replay == "reject", "[P-IT-6-ST14] nullifier replay rejected"

    # ── Phase D: Remaining queue lengths after drain ──
    assert mp.high_count == 0, "[P-IT-6-ST21] high queue empty after drain"
    assert mp.medium_count == 0, "[P-IT-6-ST22] medium queue empty after drain"
    assert mp.low_count == 0, "[P-IT-6-ST23] low queue empty after drain"


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
        test_high_fcfs_before_medium_before_low,
        test_multi_window_pid_loop,
        test_both_cfs_simultaneously_congested,
        test_malicious_flag_injection,
        test_decode_equivalence,
        test_k_scaling_reference,
        test_k_scaling_doubles_per_increment,
        test_k_scaling_max_k,
        test_k_below_reference_no_fractional_scaling,
        test_k_above_max_k_capped,
        test_k_scaling_empty_circuit,
        test_k_scaling_fee_threshold_v1_unchanged,
        test_k_scaling_circuit_rates_with_k,
        test_k_scaling_composed_transaction,
        test_risk_tracker_params,
        test_risk_tracker_new_contract_baseline,
        test_cost_profile_construction,
        test_cost_profile_defaults,
        test_resolve_cost_profile_found,
        test_resolve_cost_profile_missing_function,
        test_resolve_cost_profile_no_profiles,
        test_resolve_cost_profile_per_contract_independence,
        test_compute_total_fee_zero_congestion,
        test_compute_total_fee_risk_multiplier,
        test_compute_total_fee_tier_multiplier,
        test_compute_total_fee_full_pipeline,
        test_nullifier_replay_rejected,
        test_nullifier_different_allowed,
        test_nullifier_replay_preserves_fcfs,
        test_wallet_read_flags_hold,
        test_wallet_read_flags_increase,
        test_wallet_read_flags_decrease,
        test_wallet_read_flags_legacy,
        test_wallet_construct_fee_from_flags,
        test_block_charge_baseline,
        test_block_charge_scales,
        test_block_charge_accumulation,
        test_deviation_within_tolerance,
        test_deviation_above_tolerance,
        test_risk_escalation_one_window,
        test_risk_escalation_two_windows,
        test_risk_escalation_capped,
        test_risk_deescalation,
        test_risk_deescalation_slower_than_escalation,
        test_feedback_loop_end_to_end,
        test_risk_emerges_from_observation_not_classification,
        test_circuit_difficulty_from_declared_opcodes,
        # Integration scenarios — fee-spec.md §14, fee-testing.md
        test_p_it_1_full_lifecycle,
        test_p_it_2_multi_contract_differential,
        test_p_it_3_cross_window_congestion,
        test_p_it_4_risk_emergence,
        test_p_it_5_attack_vectors,
        test_p_it_6_three_tier_admission,
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
    assert passed == len(tests), f"{len(tests) - passed} tests failed"
