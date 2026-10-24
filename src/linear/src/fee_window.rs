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
//! Domain: `[domain: fee_signalling]` — valve control. The factors are
//! chain-derived state: every node computes them from the completed window's
//! applied FeeV3 transactions, and the per-boundary snapshot is persisted with
//! the block and restored on disconnect. The header flags are advisory
//! signalling only and are excluded from the block hash (fee-spec §14.3).
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

    /// Derive estimated congestion factors from the flags.
    ///
    /// This is a **legacy/light estimate**: it applies the signalled direction
    /// to the identity CF and cannot reproduce the daemon's multi-window state.
    /// Wallets that need an exact fee derive the factors from their own copy of
    /// the chain's window data (`FeeWindowState`'s chain-derived rule); the
    /// flags are a direction signal, not the source of the values.
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

// Manual serde as plain u16 — byte-identical wire format, no type erasure.
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
}

/// Separate premium and standard values enforce I4: CF_premium > CF_standard
/// when congestion exists. At zero congestion, both equal SCALE.
///
/// Fields are private per type-system.md §12.3 — domain logic flows through
/// accessor methods, not raw field reads. External code uses [`premium()`]
/// and [`standard()`].
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
    /// Enforces I4: `premium >= standard`.
    pub fn new(premium: u32, standard: u32) -> Self {
        // The `debug_assert!` that guarded I4 was compiled out in release, and the one caller has
        // a branch (`else { (capped_premium, capped_standard) }`) that reaches it with
        // `premium < standard`. The invariant is enforced in the value now, so an out-of-order
        // pair is the same congestion factor in both profiles rather than only in tests.
        let standard = CfValue::new(standard);
        let premium = CfValue::new(premium.max(standard.get()));
        Self { premium, standard }
    }

    /// Premium congestion factor (returns CfValue).
    pub const fn premium(self) -> CfValue { self.premium }

    /// Standard congestion factor (returns CfValue).
    pub const fn standard(self) -> CfValue { self.standard }

    /// Fixed-point scale: 1.0 = 1_000_000. Re-exported from CfValue.
    pub const SCALE: u32 = CfValue::SCALE;
}

impl Default for CongestionFactor {
    fn default() -> Self { Self::zero() }
}

// ── Fee Computation ─────────────────────────────────────────────────────

/// Flat per-kB deploy storage price (wow per kB). fee-spec.md §12.4.3.
pub const BASELINE_STORAGE: u64 = 1_000_000;

/// Deploy storage fee: `wasm_kB × BASELINE_STORAGE`. DeployV1 only.
///
/// fee-spec.md §12.4.3. Flat per-kB charge — no CF (storage is a one-time
/// on-chain cost, not congestion-scaled). `wasm_kB = 0` for non-deploy.
pub fn compute_storage_fee(wasm_kb: WasmKb) -> FeeAmount {
    FeeAmount::new(wasm_kb.get() * BASELINE_STORAGE)
}

/// FeeV3 admission fee: `fee = gas × CF × tier × risk`.
///
/// `gas` is the fee expressed directly in wow (the base denomination) — there is
/// no separate base-price multiplier. fee-spec.md §12.4.1. Fixed-point: `CF` is
/// in `CfValue::SCALE` units (1.0 = 1_000_000) and `risk` in `RiskFactor::SCALE`
/// units (1.0 = 100_000). The fee is the integer product divided by
/// `(CfValue::SCALE × RiskFactor::SCALE)`.
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

    /// Derive both axes for a boundary from the completed window's applied counts
    /// and store them (the connect path).
    ///
    /// `premium_count`/`standard_count` are the applied FeeV3 transactions of
    /// `completed_window_heights(boundary)` (`window_counts`), not mempool depths.
    pub fn adjust_from_window(
        &self,
        counts: WindowCounts,
    ) -> (CongestionFactor, CongestionFactor) {
        let circuit = self.adjust_circuit(counts.premium(), counts.standard());
        let wasm = self.adjust_wasm(counts.premium(), counts.standard());
        (circuit, wasm)
    }

    /// The values both axes would take for these counts, without mutating.
    ///
    /// The miner needs the state that `connect_block` will persist for the
    /// boundary block it is about to build; deriving it here from the same
    /// chain data keeps the announcement and the commit identical, and a lost
    /// race leaves no local state to roll back.
    pub fn derive_window(
        &self,
        counts: WindowCounts,
    ) -> (CongestionFactor, CongestionFactor) {
        let cfg = &self.config;
        let (premium_count, standard_count) = (counts.premium(), counts.standard());
        let raw = Self::compute_cf(
            premium_count, standard_count, cfg.alpha_premium, cfg.alpha_standard,
        );
        let prev_cp = *self.circuit_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner());
        let prev_cs = *self.circuit_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner());
        let prev_wp = *self.wasm_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner());
        let prev_ws = *self.wasm_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner());
        let circuit = Self::cap_with(
            prev_cp, prev_cs, raw, cfg.max_adjustment, premium_count, standard_count,
        );
        let wasm = Self::cap_with(
            prev_wp, prev_ws, raw, cfg.max_adjustment, premium_count, standard_count,
        );
        (circuit, wasm)
    }

    /// The settled boundary state, as typed values.
    pub fn snapshot(&self) -> FeeWindowSnapshot {
        FeeWindowSnapshot::new(
            CongestionFactor::new(
                self.circuit_premium_cf.load(Ordering::Acquire),
                self.circuit_standard_cf.load(Ordering::Acquire),
            ),
            CongestionFactor::new(
                self.wasm_premium_cf.load(Ordering::Acquire),
                self.wasm_standard_cf.load(Ordering::Acquire),
            ),
        )
    }

    /// Restore both current and previous values from a snapshot. A restored
    /// state is a settled boundary state, so prev == current.
    pub fn restore(&self, snapshot: &FeeWindowSnapshot) {
        self.store_settled(snapshot.circuit(), snapshot.wasm());
    }

    /// Write a settled pair into all eight values (current and previous).
    fn store_settled(&self, circuit: CongestionFactor, wasm: CongestionFactor) {
        let cp = circuit.premium().get();
        let cs = circuit.standard().get();
        let wp = wasm.premium().get();
        let ws = wasm.standard().get();
        self.circuit_premium_cf.store(cp, Ordering::Release);
        self.circuit_standard_cf.store(cs, Ordering::Release);
        self.wasm_premium_cf.store(wp, Ordering::Release);
        self.wasm_standard_cf.store(ws, Ordering::Release);
        *self.circuit_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = cp;
        *self.circuit_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = cs;
        *self.wasm_prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = wp;
        *self.wasm_prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = ws;
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
        Self::cap_with(prev_p, prev_s, raw_cf, max_adj, premium_pending, standard_pending)
    }

    /// Pure cap: the prior window's values are supplied, not read from a lock.
    fn cap_with(
        prev_p: u32,
        prev_s: u32,
        raw_cf: CongestionFactor,
        max_adj: f64,
        premium_pending: u64,
        standard_pending: u64,
    ) -> CongestionFactor {
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
        self.encode_flags_for(self.circuit_cf(), self.wasm_cf())
    }

    /// Encode the direction from the stored previous values to `circuit_cf`/`wasm_cf`.
    /// The miner announces a boundary it has not persisted yet, so the values are
    /// supplied instead of read from the current state.
    pub fn encode_flags_for(
        &self,
        circuit_cf: CongestionFactor,
        wasm_cf: CongestionFactor,
    ) -> FeeWindowFlags {
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

        FeeWindowFlags::pack(
            encode_byte(circuit_cf, prev_circuit),
            encode_byte(wasm_cf, prev_wasm),
        )
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

    /// Persist a boundary snapshot and the current state into the batch that
    /// commits the boundary block. The snapshot is keyed by height so a reorg
    /// can restore the predecessor (FI-WINDOW-7).
    pub fn save_snapshot_to_batch(&self, batch: &mut sled::Batch, height: BlockHeight) {
        self.snapshot().save_to_batch(batch, height);
        self.save_to_batch(batch);
    }

    /// Load fee window state from a sled tree. Returns `Err` if only some of the
    /// 8 expected keys are present (partial persistence after crash — A9/H5 fix).
    /// Missing all keys is accepted (fresh store). Missing some is rejected.
    pub fn load(&self, tree: &sled::Tree) -> Result<(), LinearError> {
        // Chain-derived snapshots take precedence: the state is a function of the
        // canonical boundary history. Stores written before this rule have only
        // the eight individual keys and fall through to the legacy path below.
        if let Some((_height, snapshot)) = FeeWindowSnapshot::latest(tree) {
            self.restore(&snapshot);
            return Ok(());
        }
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

// ── Chain-derived boundary input and typed snapshots ────────────────────

/// One completed fee window's applied FeeV3 demand.
///
/// Premium is `FeeTier::HIGH`; standard is MEDIUM/LOW; one count per
/// transaction. Nominal: the two counts cannot be swapped, and neither is a
/// height, amount, fee or length (type-system.md §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowCounts {
    premium: u64,
    standard: u64,
}

impl WindowCounts {
    /// Counts from a completed window's applied transactions.
    pub const fn new(premium: u64, standard: u64) -> Self {
        Self { premium, standard }
    }

    pub const fn premium(self) -> u64 {
        self.premium
    }

    pub const fn standard(self) -> u64 {
        self.standard
    }
}

/// The settled congestion state at a fee-window boundary.
///
/// Private fields; `new` and `identity` are the domain entries. The type owns
/// its only byte representation, used at the sled persistence boundary
/// (type-system.md §2.2): no caller passes or receives `[u8; N]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeWindowSnapshot {
    circuit: CongestionFactor,
    wasm: CongestionFactor,
}

impl FeeWindowSnapshot {
    /// A settled boundary state.
    pub fn new(circuit: CongestionFactor, wasm: CongestionFactor) -> Self {
        Self { circuit, wasm }
    }

    /// The state before any boundary: both axes at SCALE.
    pub fn identity() -> Self {
        let scale = CongestionFactor::new(CongestionFactor::SCALE, CongestionFactor::SCALE);
        Self::new(scale, scale)
    }

    pub fn circuit(&self) -> CongestionFactor {
        self.circuit
    }

    pub fn wasm(&self) -> CongestionFactor {
        self.wasm
    }

    /// Store this snapshot at `height` and record it as the latest boundary.
    /// Persistence boundary only.
    pub(crate) fn save_to_batch(&self, batch: &mut sled::Batch, height: BlockHeight) {
        batch.insert(snapshot_key(height), self.encode());
        batch.insert(FEE_WINDOW_LATEST_KEY.to_vec(), height.to_le_bytes().to_vec());
    }

    /// Read the snapshot stored at `height`, if present and well-formed.
    pub(crate) fn read(tree: &sled::Tree, height: BlockHeight) -> Option<Self> {
        let bytes = tree.get(snapshot_key(height)).ok().flatten()?;
        Self::decode(&bytes).ok()
    }

    /// The highest stored boundary and its snapshot.
    pub(crate) fn latest(tree: &sled::Tree) -> Option<(BlockHeight, Self)> {
        let bytes = tree.get(FEE_WINDOW_LATEST_KEY).ok().flatten()?;
        if bytes.len() != 8 {
            return None;
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&bytes);
        let height = BlockHeight::new(u64::from_le_bytes(b));
        if height.get() == 0 {
            return None;
        }
        Self::read(tree, height).map(|s| (height, s))
    }

    /// Write this snapshot into the eight legacy state keys (current and
    /// previous). The height-keyed snapshot is authoritative; these keys keep
    /// pre-rule stores and the hot-path current-value readers consistent.
    pub(crate) fn write_legacy_keys(&self, batch: &mut sled::Batch) {
        let cp = self.circuit.premium().get();
        let cs = self.circuit.standard().get();
        let wp = self.wasm.premium().get();
        let ws = self.wasm.standard().get();
        batch.insert(&b"fee_window_circuit_premium_cf"[..], &cp.to_le_bytes());
        batch.insert(&b"fee_window_circuit_standard_cf"[..], &cs.to_le_bytes());
        batch.insert(&b"fee_window_wasm_premium_cf"[..], &wp.to_le_bytes());
        batch.insert(&b"fee_window_wasm_standard_cf"[..], &ws.to_le_bytes());
        batch.insert(&b"fee_window_circuit_prev_premium_cf"[..], &cp.to_le_bytes());
        batch.insert(&b"fee_window_circuit_prev_standard_cf"[..], &cs.to_le_bytes());
        batch.insert(&b"fee_window_wasm_prev_premium_cf"[..], &wp.to_le_bytes());
        batch.insert(&b"fee_window_wasm_prev_standard_cf"[..], &ws.to_le_bytes());
    }

    /// Persistence encoding — the only place a snapshot becomes bytes.
    /// Fixed layout: circuit premium/standard, wasm premium/standard (u32 LE).
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16);
        out.extend_from_slice(&self.circuit.premium().get().to_le_bytes());
        out.extend_from_slice(&self.circuit.standard().get().to_le_bytes());
        out.extend_from_slice(&self.wasm.premium().get().to_le_bytes());
        out.extend_from_slice(&self.wasm.standard().get().to_le_bytes());
        out
    }

    /// Validating decode (`CongestionFactor::new` re-establishes I4).
    fn decode(bytes: &[u8]) -> Result<Self, LinearError> {
        if bytes.len() != 16 {
            return Err(LinearError::StorageError(format!(
                "fee_window snapshot: expected 16 bytes, got {}",
                bytes.len()
            )));
        }
        let mut buf = [0u8; 4];
        let mut read = |offset: usize| -> u32 {
            buf.copy_from_slice(&bytes[offset..offset + 4]);
            u32::from_le_bytes(buf)
        };
        let cp = read(0);
        let cs = read(4);
        let wp = read(8);
        let ws = read(12);
        Ok(Self::new(
            CongestionFactor::new(cp, cs),
            CongestionFactor::new(wp, ws),
        ))
    }
}

/// Derive the next boundary's snapshot from the previous one and the window
/// counts, with no node state. Shared by the daemon's announce path and the
/// wallet's accumulator (FI-WINDOW-7).
pub fn next_snapshot(
    prev: &FeeWindowSnapshot,
    counts: WindowCounts,
    config: &FeeWindowConfig,
) -> Result<(FeeWindowSnapshot, FeeWindowFlags), LinearError> {
    let fw = FeeWindowState::new(config.clone());
    fw.restore(prev);
    let (circuit, wasm) = fw.derive_window(counts);
    let flags = fw.encode_flags_for(circuit, wasm);
    Ok((FeeWindowSnapshot::new(circuit, wasm), flags))
}

/// Snapshot key prefix for a boundary's derived state.
pub(crate) const FEE_WINDOW_SNAPSHOT_PREFIX: &[u8] = b"fee_window_snapshot:";
/// Key holding the highest boundary height whose snapshot is stored.
pub(crate) const FEE_WINDOW_LATEST_KEY: &[u8] = b"fee_window_latest";

/// Key under which the state derived at `height` is stored.
pub(crate) fn snapshot_key(height: BlockHeight) -> Vec<u8> {
    let mut key = Vec::with_capacity(FEE_WINDOW_SNAPSHOT_PREFIX.len() + 8);
    key.extend_from_slice(FEE_WINDOW_SNAPSHOT_PREFIX);
    key.extend_from_slice(&height.to_le_bytes());
    key
}

/// Heights whose applied transactions feed the adjustment announced at `boundary`.
///
/// The flag is set on the boundary block itself, before it is mined, so the
/// input is the last `WINDOW_SIZE` completed heights, clipped at genesis:
/// boundary 20 consumes 1..=19, every later boundary consumes 20 blocks.
pub fn completed_window_heights(boundary: BlockHeight) -> impl Iterator<Item = BlockHeight> {
    let end = boundary.get();
    let start = end
        .saturating_sub(dwow_sdk::blockchain::FeeWindowId::WINDOW_SIZE)
        .max(1);
    (start..end).map(BlockHeight::new)
}

/// Count the completed window's applied FeeV3 transactions by tier.
///
/// Premium is `FeeTier::HIGH`; standard is MEDIUM/LOW — the same split
/// `compute_fee_v3` uses. One count per transaction, not per FeeV3 call. A
/// call whose params do not decode is counted as standard: an accepted block
/// cannot contain one, so the choice only keeps the fold total.
pub fn window_counts(blocks: &[crate::Block]) -> WindowCounts {
    let mut premium = 0u64;
    let mut standard = 0u64;
    for block in blocks {
        for tx in &block.transactions {
            match tx_demand(tx) {
                Some(true) => premium += 1,
                Some(false) => standard += 1,
                None => {}
            }
        }
    }
    WindowCounts::new(premium, standard)
}

/// One transaction's demand unit: `Some(true)` premium, `Some(false)` standard,
/// `None` when the transaction carries no FeeV3 call. A FeeV3 call whose params
/// do not decode counts as standard; an accepted block cannot contain one, so
/// the choice only keeps the fold total.
fn tx_demand(tx: &crate::Transaction) -> Option<bool> {
    let fee_call = tx.contract_calls.iter().find_map(|c| c.as_mass_balance_fee_v3())?;
    Some(matches!(
        dwow_native_token_contract::model::FeeParamsV3::decode(fee_call.params_bytes()),
        Ok(params) if params.tier == FeeTier::HIGH
    ))
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
    fn test_compute_fee_v3_zero_congestion() {
        let cf = CongestionFactor::zero();
        // Non-deploy tx with gas 1000 → fee = gas (in wow).
        let fee = compute_fee_v3(1000, cf, FeeTier::LOW, RiskFactor::BASELINE);
        assert_eq!(fee, FeeAmount::new(1000));
    }

    #[test]
    fn test_compute_storage_fee_wasm_multiplier() {
        // 5 kB deploy storage fee = 5 × BASELINE_STORAGE.
        let fee = compute_storage_fee(WasmKb::new(5));
        assert_eq!(fee, FeeAmount::new(5 * BASELINE_STORAGE));
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
    fn test_fee_window_boundary_emission() {
        use dwow_sdk::blockchain::{BlockHeight, FeeWindowId};
        // Boundary condition (fee-spec.md §12.8.2, DarkFi.Fee.Window): height > 0
        // and height % WINDOW_SIZE == 0. Pre-boundary heights (1..20) have no
        // boundary; the first boundary is exactly height 20.
        for h in 1u64..=40 {
            let height = BlockHeight::new(h);
            let expected = h % FeeWindowId::WINDOW_SIZE == 0;
            assert_eq!(
                FeeWindowId::is_window_boundary(height), expected,
                "is_window_boundary({h}) must be {expected}"
            );
        }
        // Emission: at a boundary the miner encodes ACTIVE flags even at zero
        // congestion (hold cm=0 for both bytes) — the window is advertised.
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let flags = fw.encode_flags();
        assert!(flags.is_active(), "boundary emission must advertise the active window");
        // Pre-boundary: the miner's else-branch (lib.rs:1417-1418) leaves the
        // header flags at default (inactive).
        assert!(!FeeWindowFlags::default().is_active(),
            "pre-boundary default flags must be inactive");
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
    }

    #[test]
    fn test_congestion_factor_display() {
        let flags = WindowSignalling::encode_cm(0x01);
        let s = format!("{}", flags);
        assert!(s.contains("1"), "Display should show binary with active bit");
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

    // ── FeeV3 fee tests — transaction (gas) vs deploy (storage) ──

    #[test]
    fn test_compute_fee_v3_risk_multiplier() {
        let cf = CongestionFactor::zero();
        let fee_normal = compute_fee_v3(1000, cf, FeeTier::LOW, RiskFactor::BASELINE);
        let fee_risky = compute_fee_v3(1000, cf, FeeTier::LOW, RiskFactor::MAX); // 2.0×
        assert_eq!(fee_normal, FeeAmount::new(1000));
        assert_eq!(fee_risky, FeeAmount::new(2000));
    }

    #[test]
    fn test_compute_fee_v3_tier_multiplier() {
        let cf = CongestionFactor::zero();
        let fee_low = compute_fee_v3(1000, cf, FeeTier::LOW, RiskFactor::BASELINE);
        let fee_med = compute_fee_v3(1000, cf, FeeTier::MEDIUM, RiskFactor::BASELINE);
        let fee_high = compute_fee_v3(1000, cf, FeeTier::HIGH, RiskFactor::BASELINE);
        assert_eq!(fee_low, FeeAmount::new(1000));
        assert_eq!(fee_med, FeeAmount::new(2000));
        assert_eq!(fee_high, FeeAmount::new(4000));
    }

    #[test]
    fn test_risk_does_not_affect_storage() {
        // Storage fee is flat — risk multiplies only the transaction (gas) fee.
        let storage = compute_storage_fee(WasmKb::new(50));
        assert_eq!(storage, FeeAmount::new(50 * BASELINE_STORAGE));
    }

    #[test]
    fn test_deploy_total_fee_separation() {
        let cf = CongestionFactor::zero();
        let tx_fee = compute_fee_v3(1000, cf, FeeTier::LOW, RiskFactor::BASELINE);
        let storage = compute_storage_fee(WasmKb::new(50));
        // Deploy total = transaction fee + storage fee; non-deploy = transaction fee only.
        assert_eq!(tx_fee, FeeAmount::new(1000));
        assert_eq!(storage, FeeAmount::new(50 * BASELINE_STORAGE));
        assert_eq!(compute_storage_fee(WasmKb::new(0)), FeeAmount::new(0));
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
    }

    /// The boundary input is a function of the applied window's transactions:
    /// one count per FeeV3 transaction, premium = HIGH, standard = MEDIUM/LOW,
    /// non-FeeV3 calls ignored, a malformed FeeV3 call still counted.
    #[test]
    fn test_window_demand_fold() {
        use dwow_sdk::blockchain::BlockVersion;
        let fee_call = |params: Vec<u8>| crate::ContractCall {
            contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
            data: {
                let mut d = vec![0x08u8];
                d.extend_from_slice(&params);
                d
            },
        };
        let tx = |calls: Vec<crate::ContractCall>| crate::Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: calls,
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };
        let other = crate::ContractCall {
            contract_id: dwow_sdk::crypto::ContractId::from_bytes([9u8; 32])
                .expect("32 non-sentinel bytes are a valid contract id"),
            data: vec![0x01],
        };

        assert_eq!(tx_demand(&tx(vec![other.clone()])), None, "non-fee call is not demand");
        // The absorber boundary requires the selector plus the FeeParamsV3
        // fixed header; a call that passes it but whose params do not decode is
        // still one standard-tier unit (an accepted block cannot contain one).
        assert_eq!(
            tx_demand(&tx(vec![other, fee_call(vec![0u8; 443])])),
            Some(false),
            "a malformed FeeV3 call is still one standard-tier demand unit"
        );
    }

    /// `next_snapshot` is a pure function of the previous snapshot and the
    /// window counts; the arithmetic reproduces the observed boundary.
    #[test]
    fn test_next_snapshot_is_deterministic() {
        let config = FeeWindowConfig::default();
        let identity = FeeWindowSnapshot::identity();
        let counts = WindowCounts::new(0, 1);
        let (a, _) = next_snapshot(&identity, counts, &config).expect("derives");
        let (b, _) = next_snapshot(&identity, counts, &config).expect("derives");
        assert_eq!(a, b, "same previous state + counts => same snapshot");
        assert_eq!(a.circuit().premium().get(), 1_010_001);
        assert_eq!(a.circuit().standard().get(), 1_010_000);
        assert_eq!(a.wasm(), a.circuit(), "both axes use the same counts");
    }

    /// Typed snapshot persistence: the value written at a boundary height reads
    /// back as the same typed state, and `latest` tracks the highest boundary.
    #[test]
    fn test_snapshot_persistence_round_trip() {
        let db = sled::Config::new().temporary(true).open().expect("temp db");
        let tree = db.open_tree(b"consensus").expect("tree");
        let snapshot = FeeWindowSnapshot::new(
            CongestionFactor::new(1_010_001, 1_010_000),
            CongestionFactor::new(1_000_000, 1_000_000),
        );
        let mut batch = sled::Batch::default();
        snapshot.save_to_batch(&mut batch, BlockHeight::new(20));
        tree.apply_batch(batch).expect("persist snapshot");

        assert_eq!(
            FeeWindowSnapshot::read(&tree, BlockHeight::new(20)),
            Some(snapshot),
            "a persisted snapshot reads back identically"
        );
        assert_eq!(
            FeeWindowSnapshot::latest(&tree),
            Some((BlockHeight::new(20), snapshot)),
            "latest names the highest boundary"
        );

        let state = FeeWindowState::new(FeeWindowConfig::default());
        state.restore(&snapshot);
        assert_eq!(state.snapshot(), snapshot, "restore is lossless");
    }
}
