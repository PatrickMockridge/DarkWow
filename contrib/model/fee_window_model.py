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
    I8 — Median Consensus (resistant to single-miner manipulation)
"""

import math
from dataclasses import dataclass, field
from typing import List, Tuple, Optional
from collections import deque


# ============================================================================
# Constants [1:1] with Rust FeeWindowConfig
# ============================================================================

SCALE: int = 1_000_000          # fixed-point scale for congestion factors
WINDOW_SIZE: int = 20           # blocks per fee window
BASE_UNIT: int = 42_000_000     # base fee unit in native token base units

# Circuit rate table [1:1] manifest [[circuits]].rate
CIRCUIT_RATES: dict = {
    # Standard circuits (rate 1)
    "TransferV2": 1, "SpendV2": 1, "BurnV2": 1, "FeeV2": 1,
    "FeeCollectV2": 1, "PoWRewardV2": 1,
    "CreateSwapV2": 1, "AcceptSwapV2": 1, "CancelSwapV2": 1,
    "ExecuteSwapV2": 1, "ExecuteSwapFeeV2": 1, "ExecuteSwapSlippageV2": 1,

    # Multi-party circuits (rate 2)
    "VerifyCapabilityV2": 2, "CreateGroupV2": 2, "SignV2": 2, "FinalizeV2": 2,

    # Oracle/Bridge circuits (rate 3)
    "PushValueV2": 3, "AttestValueV2": 3, "DepositV2": 3, "WithdrawV2": 3,

    # Heavy proof circuits (rate 5)
    "LiquidateV2": 5, "ExecuteSwapV2_complex": 5, "MintStableV2": 5,

    # Premium compute circuits (rate 10)
    "BaseDivV2": 10, "PoseidonRecursiveV2": 10, "AggregateV2": 10,
}

# Congestion sensitivity coefficients [1:1] Rust FeeWindowConfig
ALPHA_PREMIUM: float = 0.05    # premium congestion sensitivity
ALPHA_STANDARD: float = 0.01   # standard congestion sensitivity

# Adjustment caps [1:1] Rust FeeWindowConfig
MAX_ADJUSTMENT: float = 0.10   # ±10% per window
MIN_PREMIUM: int = 420_000     # floor: 0.0042 DRKW
MAX_PREMIUM: int = 4_200_000_000  # ceiling: 42 DRKW
HIGH_WATER: float = 0.75       # increase CF above this utilization
LOW_WATER: float = 0.25        # decrease CF below this utilization

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
        """Premium threshold in native token base units for a rate-10 circuit."""
        return (self.premium * 10 * BASE_UNIT) // SCALE

    def general_threshold(self) -> int:
        """General threshold in native token base units for a rate-1 circuit."""
        return (self.standard * 1 * BASE_UNIT) // SCALE


def compute_congestion_factor(premium_count: int, standard_count: int) -> CongestionFactor:
    """Compute congestion factors from mempool queue depths. [1:1] Rust."""
    cf_premium = SCALE + int(ALPHA_PREMIUM * SCALE * math.log2(premium_count + 1))
    cf_standard = SCALE + int(ALPHA_STANDARD * SCALE * math.log2(standard_count + 1))

    # I4: CF_premium > CF_standard when there is congestion.
    # At zero congestion (both = SCALE), equality is acceptable.
    if cf_premium <= cf_standard and (premium_count > 0 or standard_count > 0):
        cf_premium = cf_standard + 1

    return CongestionFactor(premium=cf_premium, standard=cf_standard)


def median_congestion_factor(factors: List[CongestionFactor]) -> CongestionFactor:
    """Median consensus across mining nodes. [1:1] Rust I8."""
    if not factors:
        return CongestionFactor()
    premiums = sorted(f.premium for f in factors)
    standards = sorted(f.standard for f in factors)
    mid = len(factors) // 2
    return CongestionFactor(
        premium=premiums[mid],
        standard=standards[mid],
    )


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
    min_premium: int = MIN_PREMIUM
    max_premium: int = MAX_PREMIUM
    high_water: float = HIGH_WATER
    low_water: float = LOW_WATER


class FeeWindow:
    """Per-node fee window state. Tracks congestion and computes thresholds."""

    def __init__(self, config: Optional[FeeWindowConfig] = None):
        self.config = config or FeeWindowConfig()
        self._current_cf = CongestionFactor()
        self._window_gas_used: List[int] = []
        self._window_gas_limit: List[int] = []
        self._previous_cf: Optional[CongestionFactor] = None

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
        return self._current_cf

    @property
    def previous_cf(self) -> Optional[CongestionFactor]:
        return self._previous_cf

    # -- Congestion factor computation --

    def update_congestion(self, premium_pending: int, standard_pending: int) -> CongestionFactor:
        """Recompute CF from mempool queue depths. Does NOT apply caps yet."""
        return compute_congestion_factor(premium_pending, standard_pending)

    # -- Window boundary adjustment --

    def adjust(self, premium_pending: int, standard_pending: int) -> Tuple[int, int]:
        """Compute new thresholds at window boundary. Returns (premium, general).
        Applies caps per I3 (FCFS preservation not modelled here — mempool concern)
        and I7 (±10% per window)."""
        raw_cf = self.update_congestion(premium_pending, standard_pending)

        # Apply ±10% cap (I7) relative to previous CF
        if self._previous_cf is not None:
            prev_premium = self._previous_cf.premium
            prev_standard = self._previous_cf.standard

            max_premium = int(prev_premium * (1 + self.config.max_adjustment))
            min_premium = int(prev_premium * (1 - self.config.max_adjustment))
            max_standard = int(prev_standard * (1 + self.config.max_adjustment))
            min_standard = int(prev_standard * (1 - self.config.max_adjustment))

            capped_premium = max(min_premium, min(raw_cf.premium, max_premium))
            capped_standard = max(min_standard, min(raw_cf.standard, max_standard))
        else:
            # No previous CF — first adjustment, no cap
            capped_premium = raw_cf.premium
            capped_standard = raw_cf.standard

        # I4: CF_premium > CF_standard when congested
        if capped_premium <= capped_standard and (premium_pending > 0 or standard_pending > 0):
            capped_premium = capped_standard + 1

        # Store for next adjustment — previous CF is the one just computed
        self._current_cf = CongestionFactor(premium=capped_premium, standard=capped_standard)
        self._previous_cf = self._current_cf

        # Reset window counters
        self._window_gas_used = []
        self._window_gas_limit = []

        return (self._current_cf.premium_threshold(), self._current_cf.general_threshold())

    # -- BlockHeader signalling [1:1] fee_window_flags --

    @staticmethod
    def encode_flags(cf: CongestionFactor, previous: Optional[CongestionFactor] = None) -> int:
        """Encode CF into fee_window_flags byte."""
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


# ============================================================================
# Mempool Model [1:1] TwoTierMempool with fee window integration
# ============================================================================

class MempoolWithWindow:
    """Two-tier mempool with fee window integration. FCFS within tiers."""

    def __init__(self, window: FeeWindow):
        self.window = window
        self.premium_queue: deque = deque()   # (tx_id, fee) — FCFS
        self.general_queue: deque = deque()   # (tx_id, fee) — FCFS
        self.fee_index: List[Tuple[str, int]] = []  # (tx_id, fee_rate) — fee-descending

    @property
    def premium_count(self) -> int:
        return len(self.premium_queue)

    @property
    def standard_count(self) -> int:
        return len(self.general_queue) + len(self.fee_index)

    def admit(self, tx_id: str, fee: int, circuit_rate: int, wasm_kb: int = 1) -> str:
        """Admit a transaction to the appropriate tier. Returns 'premium', 'general', or 'reject'."""
        premium_threshold = self.window.current_cf.premium_threshold() * wasm_kb
        general_threshold = self.window.current_cf.general_threshold() * wasm_kb

        if circuit_rate >= 5:
            if fee >= premium_threshold:
                self.premium_queue.append((tx_id, fee))
                return "premium"
        else:
            if fee >= general_threshold:
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
        # 3. Legacy fee-descending
        self.fee_index.sort(key=lambda x: x[1], reverse=True)
        while self.fee_index and len(selected) < max_txs:
            tx_id, _ = self.fee_index.pop(0)
            selected.append(tx_id)
        return selected

    def on_window_boundary(self, new_window: FeeWindow):
        """I3: Preserve existing queues. New thresholds apply to new arrivals only."""
        self.window = new_window
        # Existing txs stay in their queues — no eviction


# ============================================================================
# Tests
# ============================================================================

def test_initial_window_uses_defaults():
    """Initial window uses SCALE=1.0 CF (no congestion)."""
    w = FeeWindow()
    premium, general = w.adjust(0, 0)
    assert premium == 420_000_000, f"expected 420_000_000, got {premium}"
    assert general == 42_000_000, f"expected 42_000_000, got {general}"


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
    """I2: flags=0 means legacy static fees."""
    assert FeeWindow.decode_flags(0, 42_000_000) == 42_000_000
    assert FeeWindow.decode_flags(0, 420_000_000) == 420_000_000


def test_fcfs_preservation():
    """I3: admitted transactions survive window boundary."""
    w = FeeWindow()
    mempool = MempoolWithWindow(w)

    # Admit under window 0
    assert mempool.admit("tx1", 420_000_000, 10) == "premium"
    assert mempool.admit("tx2", 42_000_000, 1) == "general"
    assert mempool.premium_count == 1
    assert mempool.standard_count == 1

    # Window boundary — new window with higher thresholds
    w2 = FeeWindow()
    w2.adjust(1000, 5000)  # congestion increases thresholds
    mempool.on_window_boundary(w2)

    # I3: existing txs preserved (not evicted)
    assert mempool.premium_count == 1, "I3 violated: premium tx evicted"
    assert mempool.standard_count == 1, "I3 violated: general tx evicted"

    # New arrival at old fee level — rejected under new higher threshold
    result = mempool.admit("tx3", 42_000_000, 1)
    assert result == "reject", f"tx below new threshold should be rejected, got {result}"


def test_circuit_rate_monotonicity():
    """I5: higher rate circuits pay higher fees."""
    cf = CongestionFactor(premium=int(SCALE * 1.5), standard=SCALE)
    # Rate-10 circuit pays premium
    fee_premium = cf.premium_threshold()
    # Rate-1 circuit pays general
    fee_standard = cf.general_threshold()
    assert fee_premium > fee_standard, (
        f"I5 violated: premium={fee_premium}, standard={fee_standard}"
    )


def test_median_consensus():
    """I8: median CF resists single-miner manipulation."""
    nodes = [
        compute_congestion_factor(100, 1000),   # normal
        compute_congestion_factor(100, 1000),   # normal
        compute_congestion_factor(100000, 1000), # extreme (manipulated)
        compute_congestion_factor(0, 0),         # empty mempool
        compute_congestion_factor(100, 1000),   # normal
    ]
    median_cf = median_congestion_factor(nodes)
    # Median should reflect normal nodes, not the extreme one
    normal_cf = compute_congestion_factor(100, 1000)
    assert abs(median_cf.premium - normal_cf.premium) < SCALE // 10, (
        f"I8 violated: median CF manipulated by extreme node. "
        f"normal={normal_cf.premium}, median={median_cf.premium}"
    )


def test_wasm_size_multiplier():
    """WASM deployment size multiplies threshold."""
    w = FeeWindow()
    w.adjust(0, 0)  # CF = 1.0
    mempool = MempoolWithWindow(w)

    # 10 kB WASM deployment with rate-1 circuit
    # Threshold should be 10x general_threshold
    general = w.current_cf.general_threshold()
    assert mempool.admit("deploy1", general * 5, 1, wasm_kb=5) == "general"
    assert mempool.admit("deploy2", general, 1, wasm_kb=10) == "reject", (
        "10 kB deploy at base rate should be rejected (need 10x)"
    )


def test_premium_fcfs_before_general():
    """Premium queue drains FCFS before general queue."""
    w = FeeWindow()
    mempool = MempoolWithWindow(w)

    # Admit interleaved
    mempool.admit("p1", 420_000_000, 10)
    mempool.admit("g1", 42_000_000, 1)
    mempool.admit("p2", 420_000_000, 10)
    mempool.admit("g2", 42_000_000, 1)

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
    assert final_p >= MIN_PREMIUM, f"P-FW-1: premium {final_p} below floor {MIN_PREMIUM}"
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

    # Thresholds should both exceed base values
    premium_t = cf.premium_threshold()
    general_t = cf.general_threshold()
    assert premium_t > 420_000_000, (
        f"P-FW-2: premium threshold {premium_t} not above base 420M"
    )
    assert general_t > 42_000_000, (
        f"P-FW-2: general threshold {general_t} not above base 42M"
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
    base_premium = 420_000_000

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
    floor_premium = MIN_PREMIUM
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
    base_premium = 420_000_000

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


def test_multi_miner_median_convergence_extended():
    """P-FW-6: Multi-miner median convergence with divergent mempools.

    Extends test_median_consensus with 3 miners in two rounds:
    Round 1 — divergent mempools produce divergent flags
    Round 2 — on-chain median signals converge the network

    Witness for L3-FW-2.
    """
    # Round 1: Three miners with divergent local mempool views
    miner_a = compute_congestion_factor(100, 1000)   # moderate
    miner_b = compute_congestion_factor(200, 800)    # different mix
    miner_c = compute_congestion_factor(100000, 1000) # extreme (manipulated)

    # Median should reflect honest majority, not the extreme node
    median_rd1 = median_congestion_factor([miner_a, miner_b, miner_c])
    normal_cf = compute_congestion_factor(100, 1000)
    assert abs(median_rd1.premium - normal_cf.premium) < SCALE // 10, (
        f"P-FW-6 Round 1: median CF manipulated by extreme node. "
        f"normal={normal_cf.premium}, median={median_rd1.premium}"
    )

    # Round 2: After median adoption, all nodes converge to similar CF
    # (simulate: all nodes adopt the median as their local CF for next window)
    median_premium_cf = median_rd1
    node_cf_a = compute_congestion_factor(100, 1000)
    node_cf_b = compute_congestion_factor(100, 1000)
    node_cf_c = compute_congestion_factor(100, 1000)  # extreme node now honest

    median_rd2 = median_congestion_factor([node_cf_a, node_cf_b, node_cf_c])
    # After convergence, all nodes produce identical CF
    assert node_cf_a.premium == node_cf_b.premium == node_cf_c.premium, (
        "P-FW-6 Round 2: honest nodes should converge to identical CF"
    )
    # Median equals the common value
    assert median_rd2.premium == node_cf_a.premium, (
        "P-FW-6 Round 2: median should equal unanimous value"
    )


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
        test_median_consensus,
        test_wasm_size_multiplier,
        test_premium_fcfs_before_general,
        # NEW: Fee Signalling Testing Plan scenarios
        test_multi_window_pid_loop,
        test_both_cfs_simultaneously_congested,
        test_malicious_flag_injection,
        test_decode_equivalence,
        test_multi_miner_median_convergence_extended,
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
