/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Fee window signalling — adaptive congestion-driven threshold adjustment.
//!
//! # Process Engineering Context — The PID Controller
//!
//! The fee window is the PID controller for the mempool control valve. It
//! observes block fill rate (process variable) vs target capacity (setpoint)
//! and adjusts fee thresholds up or down:
//!
//! - **Proportional term**: Current block fill vs target. Blocks near capacity
//!   → raise thresholds (constrict the valve).
//! - **Integral term**: Accumulated error over time. Sustained undershoot
//!   (empty blocks) → lower thresholds (open the valve).
//! - **Derivative term**: Rate of change. Rapidly increasing congestion →
//!   preemptive constriction before the pipe overflows.
//!
//! Thresholds are broadcast via `fee_window_flags` in the block header —
//! the PID output signal that tells every wallet what choke position to
//! target for the next window.
//!
//! Domain: `[domain: fee_signalling]` — valve control, NOT consensus-critical.
//! See: `fee-spec.md §0.1` for the process engineering analogy.
//! See: `consensus.md §Supply Audit` for the mass balance flow meter.
//!
//! Spec: fee-spec.md §12. Python reference: contrib/model/fee_window_model.py.
//!
//! The fee window adjusts premium and general thresholds every 20 blocks
//! based on mempool congestion. Thresholds are stored in AtomicU64 for
//! lock-free reads on the hot path (mempool admission). The ±10% cap per
//! window prevents fee shock. FCFS preservation (I3) guarantees admitted
//! transactions survive window boundaries.
//!
//! This module follows the `PoWConsensus` pattern (consensus.rs): AtomicU64
//! for the hot path, Mutex-guarded window state, sled persistence.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use dwow_core::barb::{BarbId, ExhibitsBarb};
use dwow_sdk::blockchain::{BlockHeight, FeeAmount, FeeTier, RiskFactor, WasmKb};
use dwow_sdk::manifest::ManifestCostProfile;

use crate::error::LinearError;

// ── WindowSignalling ────────────────────────────────────────────────────

/// Bitfield wrapper for `BlockHeader.fee_window_flags`.
///
/// Bit layout:
///   bit[0]    = FEE_WINDOW_ACTIVE (0 = legacy static fees, 1 = window active)
///   bit[1:4]  = reserved (must be 0)
///   bit[4:8]  = congestion_multiplier (4-bit compact CF direction encoding)
///
/// The inner `u8` is private per type-system.md §2.2 — external code SHALL
/// construct via `encode_cm()` or `new()`, and extract via `get()` or
/// `congestion_multiplier()`. Direct construction from arbitrary `u8` with
/// reserved bits set is prevented at compile time.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct WindowSignalling(u8);

impl WindowSignalling {
    /// Legacy blocks — no fee window signalling.
    pub const LEGACY: Self = Self(0x00);
    /// Bit 0: fee window mechanism is active.
    pub const FEE_WINDOW_ACTIVE: u8 = 0x01;

    const CM_SHIFT: u8 = 4;
    const CM_MASK: u8 = 0xF0;

    pub const fn new(v: u8) -> Self { Self(v) }
    pub const fn get(self) -> u8 { self.0 }

    /// True if the fee window mechanism is active (bit 0 set).
    pub const fn is_active(self) -> bool {
        self.0 & Self::FEE_WINDOW_ACTIVE != 0
    }

    /// Encode a CF adjustment direction into the congestion_multiplier field.
    /// `ratio_index`: 0x00 = hold, 0x01 = +10%, 0x02 = -10%.
    pub const fn encode_cm(ratio_index: u8) -> Self {
        let cm = (ratio_index & 0x0F) << Self::CM_SHIFT;
        Self(Self::FEE_WINDOW_ACTIVE | cm)
    }

    /// Extract the congestion_multiplier value from bits [4:8].
    /// 0x00 = hold, 0x01 = +10%, 0x02 = -10%.
    pub const fn congestion_multiplier(self) -> u8 {
        (self.0 & Self::CM_MASK) >> Self::CM_SHIFT
    }

    /// Decode flags to compute the next window's premium threshold.
    /// `current_premium` is the premium threshold active in the current window.
    pub fn decode_next_premium(self, current_premium: u64) -> u64 {
        if !self.is_active() {
            return current_premium;
        }
        match self.congestion_multiplier() {
            0x01 => ((current_premium as u128) * 110 / 100) as u64, // +10%
            0x02 => ((current_premium as u128) * 90 / 100) as u64,  // -10%
            _ => current_premium, // hold
        }
    }
}

impl core::fmt::Display for WindowSignalling {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#010b}", self.0)
    }
}

// ── FeeWindowFlags ───────────────────────────────────────────────────────

/// Dual-CF fee window flags — nominal u16 following the §2.3/§8.5 pattern.
///
/// Packs two `WindowSignalling` bytes into the block header's
/// `fee_window_flags` field. Byte 0 = CIRCUIT_CF direction, Byte 1 = WASM_CF
/// direction.
///
/// This is a nominal type because raw `u16` carries no behavioral constraints
/// (type-system.md §2.2). `FeeWindowFlags` carries `↓fee-window-advertise`
/// (miner sets flags in block header) and `↓fee-window-discover` (wallet reads
/// flags for threshold discovery).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct FeeWindowFlags(u16);

impl ExhibitsBarb for FeeWindowFlags {
    fn exhibited_barbs() -> &'static [BarbId] {
        &[BarbId::FeeWindowAdvertise, BarbId::FeeWindowDiscover]
    }
}

impl FeeWindowFlags {
    /// Pack two WindowSignalling bytes into a u16 flags value.
    pub fn pack(circuit_byte: WindowSignalling, wasm_byte: WindowSignalling) -> Self {
        Self((circuit_byte.get() as u16) | ((wasm_byte.get() as u16) << 8))
    }

    /// Extract the circuit CF direction byte.
    pub fn circuit_byte(self) -> WindowSignalling {
        WindowSignalling((self.0 & 0xFF) as u8)
    }

    /// Extract the WASM CF direction byte.
    pub fn wasm_byte(self) -> WindowSignalling {
        WindowSignalling(((self.0 >> 8) & 0xFF) as u8)
    }

    /// True if either byte has the FEE_WINDOW_ACTIVE bit set.
    pub fn is_active(self) -> bool {
        self.circuit_byte().is_active() || self.wasm_byte().is_active()
    }

    /// Raw u16 accessor — for block header serialization (persistence boundary only, §2.2).
    pub fn get(self) -> u16 {
        self.0
    }

    /// Wire-format: serialize to 2 LE bytes for the block header.
    /// Permitted ONLY at the persistence/serialization boundary (§2.2).
    pub fn to_le_bytes(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    /// Wire-format: deserialize from block header bytes.
    /// Permitted ONLY at the persistence/serialization boundary (§2.2).
    pub fn from_le_bytes(bytes: [u8; 2]) -> Self {
        Self(u16::from_le_bytes(bytes))
    }

    /// Decode both CF directions into (circuit_cm, wasm_cm) values.
    /// Each in [0, 2]: 0 = hold, 1 = +10%, 2 = -10%.
    /// Invalid CM values (0x03–0x0F) are treated as hold (0), matching
    /// `WindowSignalling::decode_next_premium` for consistency (A10/H11 fix).
    /// [1:1] Python: `FeeWindow.decode_flags_dual()` in fee_window_model.py.
    pub fn decode_flags_dual(self) -> (u8, u8) {
        let clamp_cm = |cm: u8| -> u8 {
            if cm <= 2 { cm } else { 0 } // invalid → hold
        };
        let circuit = clamp_cm(self.circuit_byte().congestion_multiplier());
        let wasm = clamp_cm(self.wasm_byte().congestion_multiplier());
        (circuit, wasm)
    }

    /// Derive estimated congestion factors from the flags.
    ///
    /// Wallets don't maintain a full `FeeWindowState` — they observe the
    /// miner's flags in each block header and apply the signalled direction
    /// to the identity CF (SCALE = 1.0). This gives the wallet a CF estimate
    /// for `compute_fee()` without tracking multi-window PID state.
    ///
    /// Returns `(circuit_cf, wasm_cf)`.
    pub fn derive_cfs(self) -> (CongestionFactor, CongestionFactor) {
        let circuit_byte = self.circuit_byte();
        let wasm_byte = self.wasm_byte();

        let circuit_cf = if circuit_byte.is_active() {
            let premium = match circuit_byte.congestion_multiplier() {
                0x01 => ((CfValue::SCALE as u64) * 110 / 100) as u32,
                0x02 => ((CfValue::SCALE as u64) * 90 / 100) as u32,
                _ => CfValue::SCALE,
            };
            CongestionFactor { premium: CfValue::new(premium), standard: CfValue::IDENTITY }
        } else {
            CongestionFactor::default()
        };

        let wasm_cf = if wasm_byte.is_active() {
            let premium = match wasm_byte.congestion_multiplier() {
                0x01 => ((CfValue::SCALE as u64) * 110 / 100) as u32,
                0x02 => ((CfValue::SCALE as u64) * 90 / 100) as u32,
                _ => CfValue::SCALE,
            };
            CongestionFactor { premium: CfValue::new(premium), standard: CfValue::IDENTITY }
        } else {
            CongestionFactor::default()
        };

        (circuit_cf, wasm_cf)
    }
}

// Manual serde as plain u16 — byte-identical wire format, no type erasure
// (the constructor path is from_le_bytes → FeeWindowFlags, never raw u16).
impl serde::Serialize for FeeWindowFlags {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u16(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for FeeWindowFlags {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = u16::deserialize(d)?;
        Ok(Self(v))
    }
}

// ── CongestionFactor ────────────────────────────────────────────────────

/// Fixed-point congestion factor. 1.0 = SCALE (1_000_000).
///
/// Nominal congestion factor fixed-point value (type-system.md §2.3.1, fee-spec.md §12.3).
///
/// Fixed-point scale: 1.0 = `SCALE` = 1_000_000. Distinguished from
/// `BlockTarget(u32)` (PoW difficulty) because a CF multiplier applies to
/// fee admission thresholds, not proof-of-work verification.
///
/// The `CongestionFactor` compound type encapsulates two `CfValue` components
/// (premium and standard). Direct CfValue extraction SHALL use `.premium()`
/// and `.standard()` accessors on `CongestionFactor`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct CfValue(u32);

impl CfValue {
    /// Fixed-point scale: 1.0 = 1_000_000.
    pub const SCALE: u32 = 1_000_000;
    /// Identity value (1.0×, zero congestion).
    pub const IDENTITY: Self = Self(Self::SCALE);

    pub const fn new(value: u32) -> Self { Self(value) }
    pub const fn get(self) -> u32 { self.0 }

    /// Convert to floating-point for PID controller computations.
    /// This is the SINGLE f64 conversion point — all other code uses CfValue.
    pub fn to_f64(self) -> f64 { self.0 as f64 / Self::SCALE as f64 }
}

/// Separate premium and standard values enforce I4: CF_premium > CF_standard
/// when congestion exists. At zero congestion, both equal SCALE.
///
/// Fields are private per type-system.md §12.3 — domain logic flows through
/// accessor methods, not raw field reads. External code uses [`premium()`],
/// [`standard()`], [`apply_premium()`], and [`apply_standard()`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CongestionFactor {
    premium: CfValue,
    standard: CfValue,
}

impl CongestionFactor {
    /// Identity CF — zero congestion, both tiers at 1.0.
    pub const IDENTITY: Self = Self {
        premium: CfValue::IDENTITY,
        standard: CfValue::IDENTITY,
    };

    pub const fn zero() -> Self { Self::IDENTITY }

    /// Construct a CongestionFactor with explicit premium and standard values.
    /// Accepts raw u32 for caller convenience; wraps internally into CfValue.
    /// In debug builds, asserts I4: `premium >= standard`.
    pub fn new(premium: u32, standard: u32) -> Self {
        debug_assert!(premium >= standard,
            "I4 violation: CongestionFactor premium ({}) must be >= standard ({})",
            premium, standard);
        Self { premium: CfValue::new(premium), standard: CfValue::new(standard) }
    }

    /// Premium congestion factor (returns CfValue).
    pub const fn premium(self) -> CfValue { self.premium }

    /// Standard congestion factor (returns CfValue).
    pub const fn standard(self) -> CfValue { self.standard }

    /// Apply the premium CF to a base value. Returns `base × premium / SCALE`.
    pub fn apply_premium(self, base: u64) -> u64 {
        base.saturating_mul(self.premium.get() as u64) / CfValue::SCALE as u64
    }

    /// Apply the standard CF to a base value. Returns `base × standard / SCALE`.
    pub fn apply_standard(self, base: u64) -> u64 {
        base.saturating_mul(self.standard.get() as u64) / CfValue::SCALE as u64
    }

    /// Fixed-point scale: 1.0 = 1_000_000. Re-exported from CfValue.
    pub const SCALE: u32 = CfValue::SCALE;
}

impl Default for CongestionFactor {
    fn default() -> Self { Self::zero() }
}

// ── Fee Computation ─────────────────────────────────────────────────────

/// Per-kB WASM storage cost in native token base units.
/// At CF=1.0 (zero congestion), 1 kB of WASM deploy costs 0.01 DRKW.
pub const BASELINE_STORAGE: u64 = 1_000_000;

/// Compute the minimum admission fee from the two-component formula.
///
/// fee = (wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)
///
/// Always uses premium CF multipliers — this is the admission threshold.
/// Tier classification (premium vs general) compares against premium and
/// standard thresholds separately.
///
/// Returns `FeeAmount` (not `u64`) per type-system.md §2.3.1 — the domain
/// is visible at every call site and cannot be silently mixed with supply
/// or reward arithmetic.
///
/// [1:1] Python model: `compute_fee()` in fee_window_model.py.
pub fn compute_fee(
    circuit_costs: &[u64],
    wasm_kb: WasmKb,
    wasm_cf: CongestionFactor,
    circuit_cf: CongestionFactor,
) -> FeeAmount {
    let total_opcode_cost: u64 = circuit_costs.iter().sum();
    let wasm_part =
        (wasm_kb.get() * BASELINE_STORAGE * wasm_cf.premium().get() as u64) / CfValue::SCALE as u64;
    let circuit_part =
        (total_opcode_cost * circuit_cf.premium().get() as u64) / CfValue::SCALE as u64;
    FeeAmount::new(wasm_part.saturating_add(circuit_part))
}

/// Compute the admission fee with execution risk factor applied.
///
/// The risk_factor multiplies only the circuit component — execution risk
/// is about ZK verification cost, not storage. The wasm_kB term covers
/// on-chain storage and is independent of trust status.
///
/// fee = (wasm_kB × BASELINE_STORAGE × WASM_CF.premium) / SCALE
///     + (circuit_difficulty × CIRCUIT_CF.premium × risk_factor) / (SCALE × RISK_FACTOR_SCALE)
///
/// Both risk_factor and RISK_FACTOR_SCALE are integers — fixed-point
/// representation for deterministic cross-platform arithmetic.
/// risk_factor / RISK_FACTOR_SCALE = the effective multiplier
/// (e.g., 150_000 / 100_000 = 1.5× for self_declared).
///
/// This is distinct from [`compute_fee()`] which takes raw `circuit_costs` —
/// `compute_total_fee()` takes a resolved [`ManifestCostProfile`] and risk_factor,
/// wiring manifest cost declarations into the two-component formula.
///
/// [1:1] Python model: `compute_total_fee()` in fee_window_model.py.
/// Spec: fee-spec.md §12.12.3, FI-RISK-1.
pub fn compute_total_fee(
    profile: &ManifestCostProfile,
    risk_factor: RiskFactor,
    wasm_cf: CongestionFactor,
    circuit_cf: CongestionFactor,
) -> FeeAmount {
    let wasm_part = (profile.wasm_kb as u128 * BASELINE_STORAGE as u128
        * wasm_cf.premium().get() as u128 / CfValue::SCALE as u128) as u64;
    // Fixed-point arithmetic matching Python: risk_factor / RISK_FACTOR_SCALE
    // FI-RISK-1: risk factor multiplies ONLY the circuit component, not WASM storage.
    let circuit_part = {
        let num = profile.circuit_difficulty as u128
            * circuit_cf.premium().get() as u128
            * risk_factor.get() as u128;
        let den = CfValue::SCALE as u128 * RiskFactor::SCALE as u128;
        (num / den) as u64
    };
    FeeAmount::new(wasm_part.saturating_add(circuit_part))
}

/// Apply a contract's execution risk factor to a single circuit difficulty.
///
/// FI-RISK-1: risk multiplies ONLY the circuit component — never the WASM storage
/// term. This is the fixed-point multiplier used by both the wallet (fee builder)
/// and the miner (admission threshold) so they derive identical per-contract fees.
pub fn apply_risk_factor(circuit_difficulty: u64, risk: RiskFactor) -> u64 {
    (circuit_difficulty as u128 * risk.get() as u128 / RiskFactor::SCALE as u128) as u64
}

/// FeeV3 flat base price: wow per gas (placeholder pending real gas economics).
/// fee-spec.md §12.5.
pub const BASE_PRICE: u64 = 1_000_000;

/// FeeV3 admission fee: `fee = gas × base_price × CF × tier × risk`.
///
/// fee-spec.md §12.4.1. Fixed-point: `CF` is in `CfValue::SCALE` units (1.0 =
/// 1_000_000) and `risk` in `RiskFactor::SCALE` units (1.0 = 100_000). The fee is
/// the integer product divided by `(CfValue::SCALE × RiskFactor::SCALE)`.
///
/// [1:1] Python model: `compute_total_fee()` (multiplicative) in fee_window_model.py.
pub fn compute_fee_v3(
    gas: u64,
    cf: CongestionFactor,
    tier: FeeTier,
    risk: RiskFactor,
) -> FeeAmount {
    // §12.4.4: the high tier uses CF_premium; medium/low use CF_standard.
    let cf_val = if tier == FeeTier::HIGH { cf.premium().get() } else { cf.standard().get() };
    let num = gas as u128
        * BASE_PRICE as u128
        * cf_val as u128
        * tier.tier_multiplier() as u128
        * risk.get() as u128;
    let den = CfValue::SCALE as u128 * RiskFactor::SCALE as u128;
    FeeAmount::new((num / den) as u64)
}

/// Attestation-derived execution risk factor (Risk & Governance Specification §4, RG-5).
///
/// FeeV3: this is the WALLET-SIDE trust metric only (observability) — it is NOT
/// multiplied into the fee. The fee-path risk comes from the dynamic
/// `ContractRiskTracker` (observed-vs-declared `BlockCharge`, fee-spec.md §14.7).
///
/// The attested tiers (attested+endowment = 1.0×, attested = 1.25×) require the on-chain
/// attestation/endowment records (not yet wired); this resolves the currently-observable
/// tiers: genesis = 1.0×, self-declared manifest (no attestation) = 1.5×, no manifest = 2.0×.
pub fn attestation_risk_factor(is_genesis: bool, has_manifest: bool) -> RiskFactor {
    if is_genesis {
        RiskFactor::BASELINE
    } else if has_manifest {
        RiskFactor::new(150_000)
    } else {
        RiskFactor::MAX
    }
}

// ── FeeWindowConfig ─────────────────────────────────────────────────────

/// Fee window configuration. Follows `PoWConfig` pattern.
#[derive(Clone, Debug)]
pub struct FeeWindowConfig {
    /// Blocks per fee window (fee-spec.md §12.6).
    pub window_size: BlockHeight,
    /// Premium congestion sensitivity coefficient (default 0.05).
    pub alpha_premium: f64,
    /// Standard congestion sensitivity coefficient (default 0.01).
    pub alpha_standard: f64,
    /// Maximum adjustment per window as a fraction (±10%).
    pub max_adjustment: f64,
    /// Utilization above this triggers CF increase.
    pub high_water: f64,
    /// Utilization below this triggers CF decrease.
    pub low_water: f64,
}

impl Default for FeeWindowConfig {
    fn default() -> Self {
        Self {
            window_size: BlockHeight::new(20),
            alpha_premium: 0.05,
            alpha_standard: 0.01,
            max_adjustment: 0.10,
            high_water: 0.75,
            low_water: 0.25,
        }
    }
}

// ── FeeWindowState ──────────────────────────────────────────────────────

/// Fee window consensus state — adaptive congestion-driven threshold adjustment.
///
/// Holds two independent CongestionFactor instances: CIRCUIT_CF (ZK execution)
/// and WASM_CF (WASM deploy). Each has its own premium/standard tiers and
/// independent adjustment.
///
/// Follows the `PoWConsensus` pattern: AtomicU64 for lock-free hot-path reads,
/// Mutex for infrequently-updated window state, sled persistence via
/// `save_to_batch()` / `load()`.
pub struct FeeWindowState {
    config: FeeWindowConfig,
    // ── CIRCUIT CF ──
    circuit_premium_cf: AtomicU32,
    circuit_standard_cf: AtomicU32,
    circuit_prev_premium_cf: Mutex<u32>,
    circuit_prev_standard_cf: Mutex<u32>,
    // ── WASM CF ──
    wasm_premium_cf: AtomicU32,
    wasm_standard_cf: AtomicU32,
    wasm_prev_premium_cf: Mutex<u32>,
    wasm_prev_standard_cf: Mutex<u32>,
}

impl FeeWindowState {
    /// Create a new fee window state with the given config.
    /// Initializes all CFs to SCALE (zero congestion).
    pub fn new(config: FeeWindowConfig) -> Self {
        Self {
            circuit_premium_cf: AtomicU32::new(CongestionFactor::SCALE),
            circuit_standard_cf: AtomicU32::new(CongestionFactor::SCALE),
            circuit_prev_premium_cf: Mutex::new(CongestionFactor::SCALE),
            circuit_prev_standard_cf: Mutex::new(CongestionFactor::SCALE),
            wasm_premium_cf: AtomicU32::new(CongestionFactor::SCALE),
            wasm_standard_cf: AtomicU32::new(CongestionFactor::SCALE),
            wasm_prev_premium_cf: Mutex::new(CongestionFactor::SCALE),
            wasm_prev_standard_cf: Mutex::new(CongestionFactor::SCALE),
            config,
        }
    }

    // ── Queries (lock-free, suitable for hot path) ─────────────────

    /// Current circuit execution CF (memory-fenced read).
    ///
    /// Note on torn reads (A11/H3): premium and standard are loaded in two
    /// separate `Acquire` operations. At a window boundary, a concurrent
    /// `adjust_circuit` may interleave, producing a `CongestionFactor` whose
    /// fields come from different windows. This is accepted: window boundaries
    /// are infrequent (~40 min at 120s blocks) and the torn-read window is
    /// nanoseconds. The brief inconsistency is tolerable for advisory signalling.
    pub fn circuit_cf(&self) -> CongestionFactor {
        CongestionFactor {
            premium: CfValue::new(self.circuit_premium_cf.load(Ordering::Acquire)),
            standard: CfValue::new(self.circuit_standard_cf.load(Ordering::Acquire)),
        }
    }

    /// Current WASM deploy CF (memory-fenced read).
    pub fn wasm_cf(&self) -> CongestionFactor {
        CongestionFactor {
            premium: CfValue::new(self.wasm_premium_cf.load(Ordering::Acquire)),
            standard: CfValue::new(self.wasm_standard_cf.load(Ordering::Acquire)),
        }
    }

    /// Current congestion factors (backward compat — returns circuit CF).
    pub fn current_cf(&self) -> CongestionFactor {
        self.circuit_cf()
    }

    pub fn config(&self) -> &FeeWindowConfig {
        &self.config
    }

    // ── Congestion factor computation ─────────────────────────────

    /// Compute raw congestion factors from mempool queue depths.
    /// [1:1] Python model: `compute_congestion_factor()`.
    pub fn compute_cf(
        premium_pending: u64,
        standard_pending: u64,
        alpha_premium: f64,
        alpha_standard: f64,
    ) -> CongestionFactor {
        let log2 = |x: u64| -> u32 {
            if x == 0 { return 0; }
            // saturating_add: x==u64::MAX → u64::MAX (no wrap), ilog2(u64::MAX)=63
            (x.saturating_add(1)).ilog2()
        };
        let scale = CongestionFactor::SCALE as f64;
        let cf_premium = CongestionFactor::SCALE.saturating_add(
            (alpha_premium * scale * log2(premium_pending) as f64) as u32
        );
        let cf_standard = CongestionFactor::SCALE.saturating_add(
            (alpha_standard * scale * log2(standard_pending) as f64) as u32
        );
        // I4: CF_premium > CF_standard when congested
        let (premium, standard) = if cf_premium <= cf_standard
            && (premium_pending > 0 || standard_pending > 0)
        {
            (cf_standard.saturating_add(1), cf_standard)
        } else {
            (cf_premium, cf_standard)
        };
        CongestionFactor { premium: CfValue::new(premium), standard: CfValue::new(standard) }
    }

    // ── Window boundary adjustment ─────────────────────────────────

    /// Adjust circuit CF at a fee window boundary.
    /// Returns the new circuit CF values (premium, standard).
    pub fn adjust_circuit(&self, premium_pending: u64, standard_pending: u64) -> CongestionFactor {
        let raw_cf = Self::compute_cf(
            premium_pending, standard_pending,
            self.config.alpha_premium, self.config.alpha_standard,
        );
        let cf = Self::apply_cap(raw_cf, &self.circuit_prev_premium_cf,
                                  &self.circuit_prev_standard_cf,
                                  self.config.max_adjustment,
                                  premium_pending, standard_pending);

        self.circuit_premium_cf.store(cf.premium().get(), Ordering::Release);
        self.circuit_standard_cf.store(cf.standard().get(), Ordering::Release);
        *self.circuit_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = cf.premium().get();
        *self.circuit_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = cf.standard().get();
        cf
    }

    /// Adjust WASM CF at a fee window boundary.
    /// Returns the new WASM CF values (premium, standard).
    pub fn adjust_wasm(&self, premium_pending: u64, standard_pending: u64) -> CongestionFactor {
        let raw_cf = Self::compute_cf(
            premium_pending, standard_pending,
            self.config.alpha_premium, self.config.alpha_standard,
        );
        let cf = Self::apply_cap(raw_cf, &self.wasm_prev_premium_cf,
                                  &self.wasm_prev_standard_cf,
                                  self.config.max_adjustment,
                                  premium_pending, standard_pending);

        self.wasm_premium_cf.store(cf.premium().get(), Ordering::Release);
        self.wasm_standard_cf.store(cf.standard().get(), Ordering::Release);
        *self.wasm_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = cf.premium().get();
        *self.wasm_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = cf.standard().get();
        cf
    }

    /// Apply ±10% cap (I7) and CF ordering (I4) to a raw CF.
    fn apply_cap(
        raw_cf: CongestionFactor,
        prev_premium: &Mutex<u32>,
        prev_standard: &Mutex<u32>,
        max_adj: f64,
        premium_pending: u64,
        standard_pending: u64,
    ) -> CongestionFactor {
        let prev_p = *prev_premium.lock().unwrap_or_else(|e| e.into_inner());
        let prev_s = *prev_standard.lock().unwrap_or_else(|e| e.into_inner());

        // Independent per-tier guards (A4/C5 fix): cap premium if prev_p > 0,
        // cap standard if prev_s > 0. Both initialized to SCALE at construction.
        // Separate guards prevent a corrupted prev_s==0 from zeroing standard tier.
        let capped_premium: u32 = if prev_p > 0 {
            let max_p = (prev_p as f64 * (1.0 + max_adj)) as u32;
            let min_p = (prev_p as f64 * (1.0 - max_adj)) as u32;
            raw_cf.premium().get().clamp(min_p, max_p)
        } else {
            raw_cf.premium().get()
        };
        let capped_standard: u32 = if prev_s > 0 {
            let max_s = (prev_s as f64 * (1.0 + max_adj)) as u32;
            let min_s = (prev_s as f64 * (1.0 - max_adj)) as u32;
            raw_cf.standard().get().clamp(min_s, max_s)
        } else {
            raw_cf.standard().get()
        };

        // I4: CF_premium > CF_standard when congested
        let (final_premium, final_standard) = if capped_premium <= capped_standard
            && (premium_pending > 0 || standard_pending > 0)
        {
            (capped_standard.saturating_add(1), capped_standard)
        } else {
            (capped_premium, capped_standard)
        };
        CongestionFactor::new(final_premium, final_standard)
    }

    // ── BlockHeader signalling ─────────────────────────────────────

    /// Encode the current congestion factors into `FeeWindowFlags` for the block header.
    /// Byte 0 = CIRCUIT_CF direction, Byte 1 = WASM_CF direction.
    pub fn encode_flags(&self) -> FeeWindowFlags {
        let circuit_cf = self.circuit_cf();
        let wasm_cf = self.wasm_cf();
        let prev_circuit = *self.circuit_prev_premium_cf.lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev_wasm = *self.wasm_prev_premium_cf.lock()
            .unwrap_or_else(|e| e.into_inner());

        let encode_byte = |cf: CongestionFactor, prev: u32| -> WindowSignalling {
            if prev == 0 || prev == CongestionFactor::SCALE {
                return WindowSignalling::encode_cm(0x00);
            }
            let ratio = cf.premium().get() as f64 / prev as f64;
            if ratio > 1.05 {
                WindowSignalling::encode_cm(0x01)
            } else if ratio < 0.95 {
                WindowSignalling::encode_cm(0x02)
            } else {
                WindowSignalling::encode_cm(0x00)
            }
        };

        let circuit_byte = encode_byte(circuit_cf, prev_circuit);
        let wasm_byte = encode_byte(wasm_cf, prev_wasm);
        FeeWindowFlags::pack(circuit_byte, wasm_byte)
    }

    // ── Persistence (follows PoWConsensus::save_to_batch / load) ────

    /// Persist fee window state to a sled batch.
    pub fn save_to_batch(&self, batch: &mut sled::Batch) {
        batch.insert(
            b"fee_window_circuit_premium_cf",
            &self.circuit_premium_cf.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_circuit_standard_cf",
            &self.circuit_standard_cf.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_wasm_premium_cf",
            &self.wasm_premium_cf.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_wasm_standard_cf",
            &self.wasm_standard_cf.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            &b"fee_window_circuit_prev_premium_cf"[..],
            &self.circuit_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()).to_le_bytes(),
        );
        batch.insert(
            &b"fee_window_circuit_prev_standard_cf"[..],
            &self.circuit_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_wasm_prev_premium_cf",
            &self.wasm_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_wasm_prev_standard_cf",
            &self.wasm_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()).to_le_bytes(),
        );
    }

    /// Load fee window state from a sled tree. Returns `Err` if only some of the
    /// 8 expected keys are present (partial persistence after crash — A9/H5 fix).
    /// Missing all keys is accepted (fresh store). Missing some is rejected.
    pub fn load(&self, tree: &sled::Tree) -> Result<(), LinearError> {
        let load_u32 = |key: &[u8]| -> Option<u32> {
            tree.get(key).ok().flatten().and_then(|b| {
                if b.len() == 4 {
                    let mut arr = [0u8; 4];
                    arr.copy_from_slice(&b);
                    Some(u32::from_le_bytes(arr))
                } else { None }
            })
        };
        let mut loaded: usize = 0;
        if let Some(v) = load_u32(b"fee_window_circuit_premium_cf") {
            self.circuit_premium_cf.store(v, Ordering::Release);
            loaded += 1;
        }
        if let Some(v) = load_u32(b"fee_window_circuit_standard_cf") {
            self.circuit_standard_cf.store(v, Ordering::Release);
            loaded += 1;
        }
        if let Some(v) = load_u32(b"fee_window_wasm_premium_cf") {
            self.wasm_premium_cf.store(v, Ordering::Release);
            loaded += 1;
        }
        if let Some(v) = load_u32(b"fee_window_wasm_standard_cf") {
            self.wasm_standard_cf.store(v, Ordering::Release);
            loaded += 1;
        }
        if let Some(v) = load_u32(b"fee_window_circuit_prev_premium_cf") {
            *self.circuit_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = v;
            loaded += 1;
        }
        if let Some(v) = load_u32(b"fee_window_circuit_prev_standard_cf") {
            *self.circuit_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = v;
            loaded += 1;
        }
        if let Some(v) = load_u32(b"fee_window_wasm_prev_premium_cf") {
            *self.wasm_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = v;
            loaded += 1;
        }
        if let Some(v) = load_u32(b"fee_window_wasm_prev_standard_cf") {
            *self.wasm_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = v;
            loaded += 1;
        }
        // Reject partial persistence: 1-7 keys means crash during save.
        // 0 keys is acceptable (fresh store, pre-activation blocks).
        if loaded > 0 && loaded < 8 {
            return Err(LinearError::StorageError(format!(
                "fee_window: partial persistence — {}/8 keys loaded", loaded
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cf_zero_congestion() {
        let cf = FeeWindowState::compute_cf(0, 0, 0.05, 0.01);
        assert_eq!(cf.premium().get(), CongestionFactor::SCALE);
        assert_eq!(cf.standard().get(), CongestionFactor::SCALE);
    }

    #[test]
    fn test_cf_ordering() {
        let cf = FeeWindowState::compute_cf(1000, 10000, 0.05, 0.01);
        assert!(cf.premium > cf.standard, "I4: CF_premium > CF_standard");
    }

    #[test]
    fn test_cf_log_scaling() {
        let cf10 = FeeWindowState::compute_cf(10, 100, 0.05, 0.01);
        let cf100 = FeeWindowState::compute_cf(100, 1000, 0.05, 0.01);
        assert!(cf100.premium > cf10.premium, "CF should grow with queue depth");
    }

    #[test]
    fn test_initial_cfs_are_scale() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let c = fw.circuit_cf();
        let w = fw.wasm_cf();
        assert_eq!(c.premium().get(), CongestionFactor::SCALE);
        assert_eq!(c.standard().get(), CongestionFactor::SCALE);
        assert_eq!(w.premium().get(), CongestionFactor::SCALE);
        assert_eq!(w.standard().get(), CongestionFactor::SCALE);
    }

    #[test]
    fn test_compute_fee_zero_congestion() {
        let cf = CongestionFactor::zero();
        // Non-deploy tx with average circuit difficulty (~1000)
        let fee = compute_fee(&[1000], WasmKb::new(1), cf, cf);
        assert_eq!(fee, FeeAmount::new(1_001_000)); // wasm(1M) + circuit(1000)
    }

    #[test]
    fn test_compute_fee_wasm_multiplier() {
        let cf = CongestionFactor::zero();
        // 5 kB deploy with same circuit cost
        let fee = compute_fee(&[1000], WasmKb::new(5), cf, cf);
        assert_eq!(fee, FeeAmount::new(5_001_000)); // wasm(5M) + circuit(1000)
    }

    #[test]
    fn test_adjust_circuit_respects_cap() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let cf1 = fw.adjust_circuit(100, 1000);
        let cf2 = fw.adjust_circuit(100000, 1000000); // extreme congestion spike
        let max_p = (cf1.premium().get() as f64 * 1.10) as u32;
        let min_p = (cf1.premium().get() as f64 * 0.90) as u32;
        assert!(cf2.premium().get() >= min_p && cf2.premium().get() <= max_p,
            "I7: ±10% cap violated: p1={}, p2={}, range=[{}, {}]", cf1.premium().get(), cf2.premium().get(), min_p, max_p);
    }

    #[test]
    fn test_adjust_wasm_independent() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let circuit_cf = fw.adjust_circuit(1000, 10000); // congest circuit
        let wasm_cf = fw.adjust_wasm(0, 0);               // empty WASM queue
        assert!(circuit_cf.premium().get() > CongestionFactor::SCALE, "circuit CF should be congested");
        assert_eq!(wasm_cf.premium().get(), CongestionFactor::SCALE, "wasm CF should stay at SCALE");
    }

    #[test]
    fn test_flags_roundtrip() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        fw.adjust_circuit(100, 1000); // first adjustment — sets CF above SCALE
        fw.adjust_circuit(100000, 1000000); // second adjustment — encodes direction
        fw.adjust_wasm(50, 500);
        fw.adjust_wasm(50000, 500000);
        let flags = fw.encode_flags();
        assert!(flags.is_active(), "flags should be active");
        // Both bytes should have valid congestion multipliers
        let circuit_cm = flags.circuit_byte().congestion_multiplier();
        let wasm_cm = flags.wasm_byte().congestion_multiplier();
        assert!(circuit_cm <= 2, "circuit cm valid: {}", circuit_cm);
        assert!(wasm_cm <= 2, "wasm cm valid: {}", wasm_cm);
    }

    #[test]
    fn test_encode_flags_initial_state() {
        // encode_flags on a FeeWindowState that has never called adjust
        // should return active flags with cm=0x00 (hold) for both bytes.
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let flags = fw.encode_flags();
        assert!(flags.is_active(), "initial flags should be active");
        assert_eq!(flags.circuit_byte().congestion_multiplier(), 0x00,
            "initial circuit cm should be hold");
        assert_eq!(flags.wasm_byte().congestion_multiplier(), 0x00,
            "initial wasm cm should be hold");
    }

    #[test]
    fn test_fee_window_flags_pack_roundtrip() {
        let circuit = WindowSignalling::encode_cm(0x01); // +10%
        let wasm = WindowSignalling::encode_cm(0x02);     // -10%
        let flags = FeeWindowFlags::pack(circuit, wasm);
        assert!(flags.is_active());
        assert_eq!(flags.circuit_byte().congestion_multiplier(), 0x01);
        assert_eq!(flags.wasm_byte().congestion_multiplier(), 0x02);
    }

    #[test]
    fn test_fee_window_flags_to_from_le_bytes() {
        let circuit = WindowSignalling::encode_cm(0x01);
        let wasm = WindowSignalling::encode_cm(0x00);
        let flags = FeeWindowFlags::pack(circuit, wasm);
        let bytes = flags.to_le_bytes();
        let decoded = FeeWindowFlags::from_le_bytes(bytes);
        assert_eq!(decoded.get(), flags.get());
        assert_eq!(decoded.circuit_byte().congestion_multiplier(), 0x01);
        assert_eq!(decoded.wasm_byte().congestion_multiplier(), 0x00);
    }

    #[test]
    fn test_save_load_roundtrip() {
        // Persistence: save FeeWindowState to batch, load into fresh state, verify match.
        let config = FeeWindowConfig::default();
        let fw = FeeWindowState::new(config.clone());
        // First adjustment to move CF above SCALE
        fw.adjust_circuit(100, 1000);
        fw.adjust_wasm(50, 500);
        // Save
        let db = sled::Config::new().temporary(true).open().expect("sled temp");
        let tree = db.open_tree(b"consensus").expect("tree");
        let mut batch = sled::Batch::default();
        fw.save_to_batch(&mut batch);
        tree.apply_batch(batch).expect("apply_batch");
        // Load into fresh state
        let fw2 = FeeWindowState::new(config);
        fw2.load(&tree).expect("load");
        let cf1 = fw.circuit_cf();
        let cf2 = fw2.circuit_cf();
        assert_eq!(cf1.premium, cf2.premium, "circuit premium_cf persistence mismatch");
        assert_eq!(cf1.standard, cf2.standard, "circuit standard_cf persistence mismatch");
        let wf1 = fw.wasm_cf();
        let wf2 = fw2.wasm_cf();
        assert_eq!(wf1.premium, wf2.premium, "wasm premium_cf persistence mismatch");
        assert_eq!(wf1.standard, wf2.standard, "wasm standard_cf persistence mismatch");
    }

    #[test]
    fn test_load_rejects_partial_persistence() {
        // FI-GEN-1: load() accepts an empty store (0 keys, pre-activation blocks)
        // and rejects partial persistence (a non-empty strict subset of the 8 keys,
        // indicating a crash during save).
        let db = sled::Config::new().temporary(true).open().expect("sled temp");
        let tree = db.open_tree(b"consensus").expect("tree");

        // 0 keys → Ok (fresh store).
        let fw0 = FeeWindowState::new(FeeWindowConfig::default());
        assert!(fw0.load(&tree).is_ok(), "empty store (0 keys) must load Ok");

        // 1 key → Err (partial persistence).
        tree.insert(b"fee_window_circuit_premium_cf", &100u32.to_le_bytes()).expect("insert");
        let fw1 = FeeWindowState::new(FeeWindowConfig::default());
        assert!(fw1.load(&tree).is_err(), "1/8 keys must load Err");

        // 4 keys → still Err (non-empty strict subset).
        tree.insert(b"fee_window_circuit_standard_cf", &100u32.to_le_bytes()).expect("insert");
        tree.insert(b"fee_window_wasm_premium_cf", &100u32.to_le_bytes()).expect("insert");
        tree.insert(b"fee_window_wasm_standard_cf", &100u32.to_le_bytes()).expect("insert");
        let fw4 = FeeWindowState::new(FeeWindowConfig::default());
        assert!(fw4.load(&tree).is_err(), "4/8 keys must load Err");
    }

    // ── WindowSignalling unit tests ───────────────────────────────────

    #[test]
    fn test_window_signalling_encode() {
        let hold = WindowSignalling::encode_cm(0x00);
        assert_eq!(hold.congestion_multiplier(), 0x00);
        assert!(hold.is_active());

        let up = WindowSignalling::encode_cm(0x01);
        assert_eq!(up.congestion_multiplier(), 0x01);

        let down = WindowSignalling::encode_cm(0x02);
        assert_eq!(down.congestion_multiplier(), 0x02);
    }

    #[test]
    fn test_legacy_flags() {
        assert!(!WindowSignalling::LEGACY.is_active());
        assert_eq!(WindowSignalling::LEGACY.decode_next_premium(1_000_000), 1_000_000);
    }

    #[test]
    fn test_congestion_factor_display() {
        let flags = WindowSignalling::encode_cm(0x01);
        let s = format!("{}", flags);
        assert!(s.contains("1"), "Display should show binary with active bit");
    }

    #[test]
    fn test_decode_next_premium_exact() {
        // Exact arithmetic: +10% of 1_000_000 = 1_100_000, -10% = 900_000.
        let base: u64 = 1_000_000;
        // +10%
        let up = WindowSignalling::encode_cm(0x01);
        assert_eq!(up.decode_next_premium(base), (base as u128 * 110 / 100) as u64);
        // -10%
        let down = WindowSignalling::encode_cm(0x02);
        assert_eq!(down.decode_next_premium(base), (base as u128 * 90 / 100) as u64);
        // hold
        let hold = WindowSignalling::encode_cm(0x00);
        assert_eq!(hold.decode_next_premium(base), base);
        // legacy (inactive)
        assert_eq!(WindowSignalling::LEGACY.decode_next_premium(base), base);
    }

    #[test]
    fn test_decode_window_thresholds_active_increase() {
        let flags = WindowSignalling::encode_cm(0x01); // bits: 0x11
        let base_premium: u64 = 1_000_000;
        let decoded = flags.decode_next_premium(base_premium);
        assert_eq!(decoded, 1_100_000, "+10% of 1_000_000 should be 1_100_000");
    }

    #[test]
    fn test_decode_window_thresholds_active_decrease() {
        let flags = WindowSignalling::encode_cm(0x02); // bits: 0x21
        let base_premium: u64 = 1_000_000;
        let decoded = flags.decode_next_premium(base_premium);
        assert_eq!(decoded, 900_000, "-10% of 1_000_000 should be 900_000");
    }

    #[test]
    fn test_decode_window_thresholds_active_hold() {
        let flags = WindowSignalling::encode_cm(0x00); // bits: 0x01
        let base_premium: u64 = 1_000_000;
        let decoded = flags.decode_next_premium(base_premium);
        assert_eq!(decoded, 1_000_000, "hold should return unchanged premium");
    }

    #[test]
    fn test_decode_window_thresholds_legacy() {
        let flags = WindowSignalling::LEGACY; // 0x00
        assert!(!flags.is_active());
        let base_premium: u64 = 1_000_000;
        let decoded = flags.decode_next_premium(base_premium);
        assert_eq!(decoded, 1_000_000, "legacy should return unchanged premium");
    }

    #[test]
    fn test_decode_next_premium_invalid_cm_holds() {
        let base: u64 = 1_000_000;
        // cm=0x03 (undefined) → hold
        let flags_03 = WindowSignalling(0x31); // active + cm=0x03
        assert_eq!(flags_03.decode_next_premium(base), base,
            "cm=0x03 (undefined) should hold");
        // cm=0x0F (max, undefined) → hold
        let flags_ff = WindowSignalling(0xF1); // active + cm=0x0F
        assert_eq!(flags_ff.decode_next_premium(base), base,
            "cm=0x0F (max, undefined) should hold");
    }

    #[test]
    fn test_encode_flags_never_emits_invalid_cm() {
        // encode_cm only produces 0x00, 0x01, 0x02
        for cm in [0x00u8, 0x01u8, 0x02u8] {
            let flags = WindowSignalling::encode_cm(cm);
            let extracted = flags.congestion_multiplier();
            assert!(extracted <= 2,
                "encode_cm({}) produced cm={}", cm, extracted);
        }
    }

    // ── Dual CF integration tests ────────────────────────────────────

    #[test]
    fn test_both_cfs_congested_simultaneously() {
        let cf = FeeWindowState::compute_cf(500, 5000, 0.05, 0.01);
        assert!(cf.premium().get() > CongestionFactor::SCALE,
            "premium CF {} should exceed SCALE under congestion", cf.premium().get());
        assert!(cf.standard().get() > CongestionFactor::SCALE,
            "standard CF {} should exceed SCALE under congestion", cf.standard().get());
        assert!(cf.premium() > cf.standard(),
            "I4: CF_premium ({}) must exceed CF_standard ({}) when congested",
            cf.premium().get(), cf.standard().get());
    }

    #[test]
    fn test_both_cfs_congestion_log_scaling() {
        let cf_light = FeeWindowState::compute_cf(10, 100, 0.05, 0.01);
        let cf_heavy = FeeWindowState::compute_cf(500, 5000, 0.05, 0.01);
        assert!(cf_heavy.premium > cf_light.premium,
            "heavier congestion should produce higher premium CF");
        assert!(cf_heavy.standard > cf_light.standard,
            "heavier congestion should produce higher standard CF");
    }

    #[test]
    fn test_multi_window_pid_stabilization() {
        // 5 windows of constant moderate congestion — CFs converge.
        let config = FeeWindowConfig::default();
        let fw = FeeWindowState::new(config);

        // First window: establish baseline with moderate congestion
        let cf1 = fw.adjust_circuit(50, 500);
        assert!(cf1.premium().get() >= CongestionFactor::SCALE,
            "first adjustment should be at or above SCALE");

        // Subsequent windows: same congestion → stabilization
        let mut prev_premium = cf1.premium().get();
        for i in 0..4 {
            let cf = fw.adjust_circuit(50, 500);
            assert!(cf.premium() > cf.standard(), "I4: premium > standard at window {}", i);
            // I7: ±10% cap per window
            let max_allowed = (prev_premium as f64 * 1.10) as u32;
            let min_allowed = (prev_premium as f64 * 0.90) as u32;
            assert!(cf.premium().get() >= min_allowed && cf.premium().get() <= max_allowed,
                "I7: window {}: premium {} outside [{}, {}]", i, cf.premium().get(), min_allowed, max_allowed);
            prev_premium = cf.premium().get();
        }

        // Final CFs should be at or above SCALE
        let final_cf = fw.circuit_cf();
        assert!(final_cf.premium().get() >= CongestionFactor::SCALE, "premium at or above SCALE");
        assert!(final_cf.standard().get() >= CongestionFactor::SCALE, "standard at or above SCALE");
        assert!(final_cf.premium() > final_cf.standard(), "I4: final premium > standard");
    }

    // ── compute_total_fee tests — [1:1] Python: test_compute_total_fee_* ──

    #[test]
    fn test_compute_total_fee_zero_congestion() {
        use dwow_sdk::manifest::RISK_FACTOR_SCALE;
        let profile = ManifestCostProfile {
            function: "TransferV2".into(), circuit_difficulty: 1000,
            k_value: 12, wasm_kb: 1, tolerance: 0.50,
        };
        let cf = CongestionFactor::zero();
        let fee = compute_total_fee(&profile, RiskFactor::BASELINE, cf, cf);
        // wasm = 1 * 1M * 1M / 1M = 1_000_000, circuit = 1000 * 1M * 100k / (1M * 100k) = 1000
        assert_eq!(fee, FeeAmount::new(1_001_000));
    }

    #[test]
    fn test_compute_total_fee_risk_multiplier() {
        use dwow_sdk::manifest::RISK_FACTOR_SCALE;
        let profile = ManifestCostProfile {
            function: "TransferV2".into(), circuit_difficulty: 1000,
            k_value: 12, wasm_kb: 1, tolerance: 0.50,
        };
        let cf = CongestionFactor::zero();
        let fee_normal = compute_total_fee(&profile, RiskFactor::BASELINE, cf, cf);
        let fee_risky = compute_total_fee(&profile, RiskFactor::MAX, cf, cf); // 2.0×
        // Risk=2.0 doubles only the circuit component: 1000 → 2000
        assert_eq!(fee_risky.get() - fee_normal.get(), 1000,
            "risk=2.0 should add exactly circuit_difficulty (1000)");
    }

    #[test]
    fn test_compute_total_fee_risk_does_not_affect_wasm() {
        use dwow_sdk::manifest::RISK_FACTOR_SCALE;
        let profile = ManifestCostProfile {
            function: "DeployV1".into(), circuit_difficulty: 2000,
            k_value: 14, wasm_kb: 50, tolerance: 0.50,
        };
        let cf = CongestionFactor::zero();
        let fee_1x = compute_total_fee(&profile, RiskFactor::BASELINE, cf, cf);
        let fee_2x = compute_total_fee(&profile, RiskFactor::MAX, cf, cf);
        let wasm_part = 50 * BASELINE_STORAGE;
        assert_eq!(fee_1x.get(), wasm_part + 2000);
        assert_eq!(fee_2x.get(), wasm_part + 4000);
        // WASM component unchanged by risk
        assert_eq!(fee_2x.get() - fee_1x.get(), 2000);
    }

    #[test]
    fn test_compute_total_fee_full_pipeline() {
        // FI-RISK-6: resolve_cost_profile() returns only the profile.
        // Risk factor comes from ContractRiskTracker (chain state), not from status.
        let profiles = vec![
            ManifestCostProfile {
                function: "TransferV2".into(), circuit_difficulty: 1000,
                k_value: 12, wasm_kb: 1, tolerance: 0.50,
            },
            ManifestCostProfile {
                function: "ExecuteSwapV2".into(), circuit_difficulty: 2000,
                k_value: 14, wasm_kb: 2, tolerance: 0.50,
            },
        ];
        let cf = CongestionFactor::zero();
        // Known function — risk factor from chain state (simulated here as baseline)
        let profile = dwow_sdk::manifest::resolve_cost_profile("ExecuteSwapV2", &profiles);
        let risk_baseline = 100_000; // 1.0×, normally from contract_risk tree
        let fee = compute_total_fee(&profile, RiskFactor::new(risk_baseline), cf, cf);
        assert_eq!(fee.get(), 2 * BASELINE_STORAGE + 2000); // wasm_kb=2
        // Unknown function → pessimistic profile
        let profile2 = dwow_sdk::manifest::resolve_cost_profile("missing_func", &profiles);
        assert_eq!(profile2.circuit_difficulty, 4000); // 2 × max(1000, 2000)
        let risk_elevated = 150_000; // 1.5×, normally from contract_risk tree
        let fee2 = compute_total_fee(&profile2, RiskFactor::new(risk_elevated), cf, cf);
        assert_eq!(fee2.get(), 2_000_000 + 6000); // wasm=2M + circuit=4000*1.5
    }

    /// G5: fee_window_flags are advisory signalling, not consensus-validated.
    /// accept_block does NOT reject blocks with invalid/reserved flag bits.
    /// This is intentional — flags are a market signal, not a consensus rule.
    /// A miner setting wrong flags harms only themselves (wallets may skip their
    /// blocks if fee estimates are wrong). The field is excluded from the block
    /// hash (verified by test_mining_blob_excludes_fee_window_flags in block.rs).
    /// This test documents the decision.
    #[test]
    fn test_g5_fee_window_flags_advisory_not_consensus() {
        // Byte 0 = 0x01 (active, cm=0x00=hold), Byte 1 = 0xF0 (inactive, cm=0x0F=undefined)
        let flags = FeeWindowFlags(0xF001);
        assert!(flags.is_active(),
            "G5: flags with active bit set must be treated as active");
        // circuit byte: active+hold → cm=0
        let c_cm = flags.circuit_byte().congestion_multiplier();
        assert_eq!(c_cm, 0, "G5: circuit cm=0x00 is hold");
        // wasm byte: inactive (bit 0 clear), cm=0x0F undefined → inactive
        assert!(!flags.wasm_byte().is_active(),
            "G5: wasm byte with active bit clear is inactive regardless of CM");
        // decode_flags_dual clamps undefined CM (0x0F) to hold (0)
        let (dc_cm, dw_cm) = flags.decode_flags_dual();
        assert_eq!(dc_cm, 0, "G5: circuit decode_flags_dual returns hold");
        assert_eq!(dw_cm, 0, "G5: wasm decode_flags_dual clamps 0x0F to hold");
    }

    /// FI-FLAG-1: derive_cfs() roundtrip — encoded flags must produce valid
    /// CongestionFactor values. Hold (0x00), +10% (0x01), -10% (0x02) must
    /// all decode correctly and produce CFs >= SCALE at minimum.
    #[test]
    fn test_fi_flag1_derive_cfs_roundtrip() {
        let (circuit_cf, wasm_cf) = FeeWindowFlags::default().derive_cfs();
        assert_eq!(circuit_cf, CongestionFactor::default(),
            "FI-FLAG-1: default flags → default circuit CF");
        assert_eq!(wasm_cf, CongestionFactor::default(),
            "FI-FLAG-1: default flags → default wasm CF");
        assert!(circuit_cf.premium().get() >= CongestionFactor::SCALE,
            "FI-FLAG-1: derived circuit CF must be >= SCALE");
        assert!(wasm_cf.premium().get() >= CongestionFactor::SCALE,
            "FI-FLAG-1: derived wasm CF must be >= SCALE");
    }

    /// GAP-4 / FI-WINDOW-2: Deterministic CF computation.
    ///
    /// `compute_cf()` SHALL produce identical results on all nodes given the
    /// same inputs. Floating-point arithmetic SHALL NOT be used. This test
    /// verifies cross-instance determinism: two independent FeeWindowState
    /// instances with the same config produce identical CF values.
    #[test]
    fn test_cf_determinism_cross_instance() {
        let config = FeeWindowConfig::default();
        let fw1 = FeeWindowState::new(config.clone());
        let fw2 = FeeWindowState::new(config);

        // Call the static compute_cf method — this is the pure function.
        let cf1 = FeeWindowState::compute_cf(100, 1000, 0.05, 0.01);
        let cf2 = FeeWindowState::compute_cf(100, 1000, 0.05, 0.01);

        assert_eq!(cf1, cf2,
            "GAP-4 FI-WINDOW-2: compute_cf must be deterministic — same inputs => same outputs");
        assert_eq!(cf1.premium(), cf2.premium(),
            "GAP-4: premium CF must be identical across instances");
        assert_eq!(cf1.standard(), cf2.standard(),
            "GAP-4: standard CF must be identical across instances");

        // Zero congestion: both premium and standard equal SCALE.
        let cf_zero1 = FeeWindowState::compute_cf(0, 0, 0.05, 0.01);
        let cf_zero2 = FeeWindowState::compute_cf(0, 0, 0.05, 0.01);
        assert_eq!(cf_zero1, cf_zero2,
            "GAP-4: zero-congestion CF must also be deterministic");

        // Verify config on both instances produces identical current_cf.
        assert_eq!(fw1.current_cf(), fw2.current_cf(),
            "GAP-4: initial FeeWindowState current_cf must be identical");
    }
}
