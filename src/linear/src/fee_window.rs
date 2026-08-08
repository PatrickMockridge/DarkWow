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

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use dwow_sdk::blockchain::BlockHeight;

use crate::error::LinearError;

// ── WindowSignalling ────────────────────────────────────────────────────

/// Bitfield wrapper for `BlockHeader.fee_window_flags`.
///
/// Bit layout:
///   bit[0]    = FEE_WINDOW_ACTIVE (0 = legacy static fees, 1 = window active)
///   bit[1:4]  = reserved (must be 0)
///   bit[4:8]  = congestion_multiplier (4-bit compact CF direction encoding)
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct WindowSignalling(pub u8);

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

// ── CongestionFactor ────────────────────────────────────────────────────

/// Fixed-point congestion factor. 1.0 = SCALE (1_000_000).
///
/// Separate premium and standard values enforce I4: CF_premium > CF_standard
/// when congestion exists. At zero congestion, both equal SCALE.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CongestionFactor {
    pub premium: u32,
    pub standard: u32,
}

impl CongestionFactor {
    /// Fixed-point scale: 1.0 = 1_000_000.
    pub const SCALE: u32 = 1_000_000;

    /// Base premium circuit rate (rate 10 per fee-spec.md §12.4.2).
    pub const BASE_PREMIUM_RATE: u16 = 10;
    /// Base standard circuit rate (rate 1).
    pub const BASE_STANDARD_RATE: u16 = 1;
    /// Base fee unit in native token base units (0.42 DRKW).
    pub const BASE_UNIT: u64 = 42_000_000;

    /// Identity CF — zero congestion, both tiers at 1.0.
    pub const IDENTITY: Self = Self::zero();

    pub const fn zero() -> Self {
        Self { premium: Self::SCALE, standard: Self::SCALE }
    }

    /// Premium threshold in native token base units for a rate-10 circuit.
    pub fn premium_threshold(self) -> u64 {
        (self.premium as u64 * Self::BASE_PREMIUM_RATE as u64 * Self::BASE_UNIT)
            / Self::SCALE as u64
    }

    /// General threshold in native token base units for a rate-1 circuit.
    pub fn general_threshold(self) -> u64 {
        (self.standard as u64 * Self::BASE_STANDARD_RATE as u64 * Self::BASE_UNIT)
            / Self::SCALE as u64
    }
}

impl Default for CongestionFactor {
    fn default() -> Self { Self::zero() }
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
    /// Premium threshold floor (0.0042 DRKW).
    pub min_premium: u64,
    /// Premium threshold ceiling (42 DRKW).
    pub max_premium: u64,
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
            min_premium: 420_000,
            max_premium: 4_200_000_000,
            high_water: 0.75,
            low_water: 0.25,
        }
    }
}

// ── FeeWindowState ──────────────────────────────────────────────────────

/// Fee window consensus state — adaptive congestion-driven threshold adjustment.
///
/// Follows the `PoWConsensus` pattern: AtomicU64 for lock-free hot-path reads,
/// Mutex for infrequently-updated window state, sled persistence via
/// `save_to_batch()` / `load()`.
pub struct FeeWindowState {
    config: FeeWindowConfig,
    /// Current premium threshold (AtomicU64 — lock-free read on mempool hot path).
    premium_threshold: AtomicU64,
    /// Current general threshold.
    general_threshold: AtomicU64,
    /// Current premium congestion factor (u32 fixed-point, SCALE = 1_000_000).
    premium_cf: AtomicU32,
    /// Current standard congestion factor.
    standard_cf: AtomicU32,
    /// Previous premium CF (for ±10% cap on next adjustment).
    prev_premium_cf: Mutex<u32>,
    /// Previous standard CF.
    prev_standard_cf: Mutex<u32>,
}

impl FeeWindowState {
    /// Create a new fee window state with the given config.
    /// Initializes thresholds to the zero-congestion values.
    pub fn new(config: FeeWindowConfig) -> Self {
        let initial = CongestionFactor::zero();
        Self {
            premium_threshold: AtomicU64::new(initial.premium_threshold()),
            general_threshold: AtomicU64::new(initial.general_threshold()),
            premium_cf: AtomicU32::new(CongestionFactor::SCALE),
            standard_cf: AtomicU32::new(CongestionFactor::SCALE),
            prev_premium_cf: Mutex::new(CongestionFactor::SCALE),
            prev_standard_cf: Mutex::new(CongestionFactor::SCALE),
            config,
        }
    }

    // ── Queries (lock-free, suitable for hot path) ─────────────────

    /// Current premium threshold (memory-fenced read).
    pub fn premium_threshold(&self) -> u64 {
        self.premium_threshold.load(Ordering::Acquire)
    }

    /// Current general threshold (memory-fenced read).
    pub fn general_threshold(&self) -> u64 {
        self.general_threshold.load(Ordering::Acquire)
    }

    /// Current congestion factors (memory-fenced read).
    pub fn current_cf(&self) -> CongestionFactor {
        CongestionFactor {
            premium: self.premium_cf.load(Ordering::Acquire),
            standard: self.standard_cf.load(Ordering::Acquire),
        }
    }

    pub fn config(&self) -> &FeeWindowConfig {
        &self.config
    }

    // ── Congestion factor computation ─────────────────────────────

    /// Compute raw congestion factors from mempool queue depths.
    /// [1:1] Python model: `compute_congestion_factor()`.
    ///
    /// Formula (fee-spec.md §12.4.4):
    ///   CF = SCALE + floor(α × SCALE × log₂(P + 1))
    pub fn compute_cf(
        premium_pending: u64,
        standard_pending: u64,
        alpha_premium: f64,
        alpha_standard: f64,
    ) -> CongestionFactor {
        let log2 = |x: u64| -> u32 {
            if x == 0 { return 0; }
            (x + 1).ilog2()
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
        CongestionFactor { premium, standard }
    }

    // ── Window boundary adjustment ─────────────────────────────────

    /// Adjust thresholds at a fee window boundary.
    /// Called by the miner when `height % 20 == 0`.
    ///
    /// Returns (premium_threshold, general_threshold) for the new window.
    /// Applies ±10% cap (I7), CF ordering (I4), and floor/ceiling bounds.
    pub fn adjust(&self, premium_pending: u64, standard_pending: u64) -> (u64, u64) {
        let raw_cf = Self::compute_cf(
            premium_pending, standard_pending,
            self.config.alpha_premium, self.config.alpha_standard,
        );

        // Apply ±10% cap relative to previous CF (I7)
        let prev_premium = self.prev_premium_cf.lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev_standard = self.prev_standard_cf.lock()
            .unwrap_or_else(|e| e.into_inner());
        let max_adj = self.config.max_adjustment;

        let (capped_premium, capped_standard) = if *prev_premium > CongestionFactor::SCALE {
            let max_p = (*prev_premium as f64 * (1.0 + max_adj)) as u32;
            let min_p = (*prev_premium as f64 * (1.0 - max_adj)) as u32;
            let max_s = (*prev_standard as f64 * (1.0 + max_adj)) as u32;
            let min_s = (*prev_standard as f64 * (1.0 - max_adj)) as u32;
            (
                raw_cf.premium.clamp(min_p, max_p),
                raw_cf.standard.clamp(min_s, max_s),
            )
        } else {
            // First adjustment after genesis — no cap
            (raw_cf.premium, raw_cf.standard)
        };

        // I4: CF_premium > CF_standard when congested
        let (final_premium, final_standard) = if capped_premium <= capped_standard
            && (premium_pending > 0 || standard_pending > 0)
        {
            (capped_standard.saturating_add(1), capped_standard)
        } else {
            (capped_premium, capped_standard)
        };

        let cf = CongestionFactor { premium: final_premium, standard: final_standard };
        let premium_thresh = cf.premium_threshold().clamp(self.config.min_premium, self.config.max_premium);
        let general_thresh = cf.general_threshold();

        // Publish atomically
        self.premium_cf.store(final_premium, Ordering::Release);
        self.standard_cf.store(final_standard, Ordering::Release);
        self.premium_threshold.store(premium_thresh, Ordering::Release);
        self.general_threshold.store(general_thresh, Ordering::Release);

        // Store previous for next window's cap
        *self.prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = final_premium;
        *self.prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = final_standard;

        (premium_thresh, general_thresh)
    }

    // ── BlockHeader signalling ─────────────────────────────────────

    /// Encode the current congestion factors into a `WindowSignalling` byte
    /// for the block header. Compares CF against previous to determine
    /// direction (hold, +10%, -10%).
    pub fn encode_flags(&self) -> WindowSignalling {
        let cf = self.current_cf();
        let prev = *self.prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner());
        if prev == 0 || prev == CongestionFactor::SCALE {
            return WindowSignalling(WindowSignalling::FEE_WINDOW_ACTIVE);
        }
        let ratio = cf.premium as f64 / prev as f64;
        if ratio > 1.05 {
            WindowSignalling::encode_cm(0x01) // +10%
        } else if ratio < 0.95 {
            WindowSignalling::encode_cm(0x02) // -10%
        } else {
            WindowSignalling::encode_cm(0x00) // hold
        }
    }

    // ── Persistence (follows PoWConsensus::save_to_batch / load) ────

    /// Persist fee window state to a sled batch.
    pub fn save_to_batch(&self, batch: &mut sled::Batch) {
        batch.insert(
            b"fee_window_premium_threshold",
            &self.premium_threshold.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_general_threshold",
            &self.general_threshold.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_premium_cf",
            &self.premium_cf.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_standard_cf",
            &self.standard_cf.load(Ordering::Acquire).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_prev_premium_cf",
            &self.prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()).to_le_bytes(),
        );
        batch.insert(
            b"fee_window_prev_standard_cf",
            &self.prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()).to_le_bytes(),
        );
    }

    /// Load fee window state from a sled tree. Missing keys are silently
    /// skipped (fresh store, pre-activation blocks).
    pub fn load(&self, tree: &sled::Tree) -> Result<(), LinearError> {
        let load_u64 = |key: &[u8]| -> Option<u64> {
            tree.get(key).ok().flatten().and_then(|b| {
                if b.len() == 8 {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(&b);
                    Some(u64::from_le_bytes(arr))
                } else { None }
            })
        };
        let load_u32 = |key: &[u8]| -> Option<u32> {
            tree.get(key).ok().flatten().and_then(|b| {
                if b.len() == 4 {
                    let mut arr = [0u8; 4];
                    arr.copy_from_slice(&b);
                    Some(u32::from_le_bytes(arr))
                } else { None }
            })
        };
        if let Some(v) = load_u64(b"fee_window_premium_threshold") {
            self.premium_threshold.store(v, Ordering::Release);
        }
        if let Some(v) = load_u64(b"fee_window_general_threshold") {
            self.general_threshold.store(v, Ordering::Release);
        }
        if let Some(v) = load_u32(b"fee_window_premium_cf") {
            self.premium_cf.store(v, Ordering::Release);
        }
        if let Some(v) = load_u32(b"fee_window_standard_cf") {
            self.standard_cf.store(v, Ordering::Release);
        }
        if let Some(v) = load_u32(b"fee_window_prev_premium_cf") {
            *self.prev_premium_cf.lock().unwrap_or_else(|e| e.into_inner()) = v;
        }
        if let Some(v) = load_u32(b"fee_window_prev_standard_cf") {
            *self.prev_standard_cf.lock().unwrap_or_else(|e| e.into_inner()) = v;
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
        assert_eq!(cf.premium, CongestionFactor::SCALE);
        assert_eq!(cf.standard, CongestionFactor::SCALE);
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
    fn test_initial_thresholds() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        assert_eq!(fw.premium_threshold(), 420_000_000);
        assert_eq!(fw.general_threshold(), 42_000_000);
    }

    #[test]
    fn test_adjust_respects_cap() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let (p1, _) = fw.adjust(100, 1000);
        let (p2, _) = fw.adjust(100000, 1000000); // extreme congestion spike
        let max_p2 = (p1 as f64 * 1.10) as u64;
        let min_p2 = (p1 as f64 * 0.90) as u64;
        assert!(p2 >= min_p2 && p2 <= max_p2,
            "I7: ±10% cap violated: p1={}, p2={}, range=[{}, {}]", p1, p2, min_p2, max_p2);
    }

    #[test]
    fn test_adjust_respects_bounds() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let (p, _) = fw.adjust(0, 0);
        assert!(p >= 420_000, "below min_premium: {}", p);
        assert!(p <= 4_200_000_000, "above max_premium: {}", p);
    }

    #[test]
    fn test_flags_roundtrip() {
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        fw.adjust(100, 1000); // first adjustment — sets CF above SCALE
        fw.adjust(100000, 1000000); // second adjustment — encodes direction
        let flags = fw.encode_flags();
        assert!(flags.is_active(), "flags should be active");
        let decoded = flags.decode_next_premium(fw.premium_threshold());
        // Decoded should be within ±10% of current
        let current = fw.premium_threshold();
        let ratio = decoded as f64 / current as f64;
        assert!(ratio > 0.89 && ratio < 1.11,
            "flags roundtrip drift: decoded={}, current={}, ratio={:.3}", decoded, current, ratio);
    }

    #[test]
    fn test_legacy_flags() {
        assert!(!WindowSignalling::LEGACY.is_active());
        assert_eq!(WindowSignalling::LEGACY.decode_next_premium(42_000_000), 42_000_000);
    }

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
    fn test_save_load_roundtrip() {
        // Persistence: save FeeWindowState to batch, load into fresh state, verify match.
        let config = FeeWindowConfig::default();
        let fw = FeeWindowState::new(config.clone());
        // First adjustment to move CF above SCALE
        fw.adjust(100, 1000);
        // Save
        let db = sled::Config::new().temporary(true).open().expect("sled temp");
        let tree = db.open_tree(b"consensus").expect("tree");
        let mut batch = sled::Batch::default();
        fw.save_to_batch(&mut batch);
        tree.apply_batch(batch).expect("apply_batch");
        // Load into fresh state
        let fw2 = FeeWindowState::new(config);
        fw2.load(&tree).expect("load");
        assert_eq!(fw.premium_threshold(), fw2.premium_threshold(),
            "premium_threshold persistence mismatch");
        assert_eq!(fw.general_threshold(), fw2.general_threshold(),
            "general_threshold persistence mismatch");
        let cf1 = fw.current_cf();
        let cf2 = fw2.current_cf();
        assert_eq!(cf1.premium, cf2.premium, "premium_cf persistence mismatch");
        assert_eq!(cf1.standard, cf2.standard, "standard_cf persistence mismatch");
    }

    #[test]
    fn test_encode_flags_initial_state() {
        // encode_flags on a FeeWindowState that has never called adjust
        // should return FEE_WINDOW_ACTIVE (0x01) without congestion multiplier.
        let fw = FeeWindowState::new(FeeWindowConfig::default());
        let flags = fw.encode_flags();
        assert!(flags.is_active(), "initial flags should be active");
        assert_eq!(flags.congestion_multiplier(), 0x00,
            "initial flags should have no CM (hold)");
        assert_eq!(flags.get(), WindowSignalling::FEE_WINDOW_ACTIVE);
    }

    #[test]
    fn test_decode_next_premium_exact() {
        // Exact arithmetic: +10% of 100_000_000 = 110_000_000, -10% = 90_000_000.
        let base: u64 = 420_000_000;
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
    fn test_congestion_factor_display() {
        let flags = WindowSignalling::encode_cm(0x01);
        let s = format!("{}", flags);
        assert!(s.contains("1"), "Display should show binary with active bit");
    }
}
