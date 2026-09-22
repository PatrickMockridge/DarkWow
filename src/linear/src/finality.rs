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

//! Configurable finality layers for linear blockchain
//!
//! Nodes can independently choose their finality posture:
//! - Native: trust PoW as-is, no finality enforcement
//! - Always: enforce finality on all blocks that carry anchors (default)
//! - Signaled: only enforce when a block's header signals it requires it

use serde::{Deserialize, Serialize};

/// Finality enforcement mode for nodes
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FinalityMode {
    /// No finality — trust PoW only, ignore all anchors
    Native,
    /// Enforce finality on all blocks that carry anchors (default)
    Always,
    /// Only enforce finality when a block's header signals it requires it
    Signaled,
}

impl Default for FinalityMode {
    fn default() -> Self {
        Self::Always
    }
}

/// Finality flag bits for BlockHeader.finality_flags
pub mod flags {
    /// Block carries a Caribina (Arweave) anchor
    pub const FINALITY_CARIBNIA: u8 = 0x01;
    /// Block carries a Monero (p2pool) anchor
    pub const FINALITY_MONERO: u8 = 0x02;
    /// Block requires finality enforcement (Signaled mode)
    pub const FINALITY_SIGNALED: u8 = 0x04;
}

/// Configuration for finality layer behavior
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalityConfig {
    /// Overall finality mode
    #[serde(default)]
    pub mode: FinalityMode,
    /// Enable Caribina (Arweave) anchoring
    #[serde(default = "default_true")]
    pub caribina_enabled: bool,
    /// Enable Monero anchoring via p2pool (default: false)
    #[serde(default)]
    pub monero_enabled: bool,
    /// Monero minimum confirmations before finality
    #[serde(default = "default_monero_confirmations")]
    pub monero_min_confirmations: u32,
    /// monerod JSON-RPC URL for anchor verification (e.g. http://127.0.0.1:18081/json_rpc)
    #[serde(default)]
    pub monerod_url: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_monero_confirmations() -> u32 {
    3
}

impl Default for FinalityConfig {
    fn default() -> Self {
        Self {
            mode: FinalityMode::Always,
            caribina_enabled: true,
            monero_enabled: false,
            monero_min_confirmations: 3,
            monerod_url: None,
        }
    }
}

impl FinalityConfig {
    /// Returns true if the node should attempt anchoring blocks
    pub fn should_anchor(&self) -> bool {
        self.mode != FinalityMode::Native && self.caribina_enabled
    }

    /// Returns true if the node should attempt Monero anchoring (p2pool context)
    pub fn should_anchor_monero(&self) -> bool {
        self.mode != FinalityMode::Native && self.monero_enabled
    }

    /// Returns true if the node should enforce anchors on received blocks.
    ///
    /// This is only half the decision. Whether an anchor is *enforced* is decided at the two
    /// enforcement sites in `chain_state.rs`, and since 2026-09-22 those require
    /// `caribina::verify_anchor_proof` to succeed — enforcement **is** verification, not a second
    /// condition that could disagree with it (OBL-C65).
    ///
    /// `should_verify_anchor` and `should_verify_monero_anchor` used to sit here and were the defect:
    /// both returned `false` whenever their `*_enabled` flag was off, while this function ignored
    /// those flags entirely, so a node could enforce anchors it had decided not to check. They had no
    /// production caller in either direction, and the fix is not a corrected predicate but their
    /// removal — a dead method encoding a wrong invariant is worse than no method, and a
    /// "verifier with no caller" is the shape this whole campaign exists to remove.
    pub fn should_enforce(&self, block_flags: u8) -> bool {
        match self.mode {
            FinalityMode::Native => false,
            FinalityMode::Always => true,
            FinalityMode::Signaled => block_flags & flags::FINALITY_SIGNALED != 0,
        }
    }

    /// Returns the flags to set on a newly mined block
    pub fn mine_flags(&self) -> u8 {
        let mut f = 0u8;
        if self.caribina_enabled && self.mode != FinalityMode::Native {
            f |= flags::FINALITY_CARIBNIA;
        }
        if self.monero_enabled && self.mode != FinalityMode::Native {
            f |= flags::FINALITY_MONERO;
        }
        if self.mode == FinalityMode::Signaled {
            f |= flags::FINALITY_SIGNALED;
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let cfg = FinalityConfig::default();
        assert_eq!(cfg.mode, FinalityMode::Always);
        assert!(cfg.caribina_enabled);
        assert!(!cfg.monero_enabled);
        assert_eq!(cfg.monero_min_confirmations, 3);
        assert!(cfg.monerod_url.is_none());
    }

    #[test]
    fn test_should_anchor() {
        // Always mode + caribina enabled = true
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            caribina_enabled: true,
            ..Default::default()
        };
        assert!(cfg.should_anchor());

        // Always mode + caribina disabled = false
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            caribina_enabled: false,
            ..Default::default()
        };
        assert!(!cfg.should_anchor());

        // Native mode + caribina enabled = false
        let cfg = FinalityConfig {
            mode: FinalityMode::Native,
            caribina_enabled: true,
            ..Default::default()
        };
        assert!(!cfg.should_anchor());

        // Native mode + caribina disabled = false
        let cfg = FinalityConfig {
            mode: FinalityMode::Native,
            caribina_enabled: false,
            ..Default::default()
        };
        assert!(!cfg.should_anchor());

        // Signaled mode + caribina enabled = true
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            caribina_enabled: true,
            ..Default::default()
        };
        assert!(cfg.should_anchor());

        // Signaled mode + caribina disabled = false
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            caribina_enabled: false,
            ..Default::default()
        };
        assert!(!cfg.should_anchor());
    }

    #[test]
    fn test_should_enforce() {
        let native_cfg = FinalityConfig {
            mode: FinalityMode::Native,
            ..Default::default()
        };
        let always_cfg = FinalityConfig {
            mode: FinalityMode::Always,
            ..Default::default()
        };
        let signaled_cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            ..Default::default()
        };

        // Native: never enforces
        assert!(!native_cfg.should_enforce(0));
        assert!(!native_cfg.should_enforce(flags::FINALITY_CARIBNIA));
        assert!(!native_cfg.should_enforce(flags::FINALITY_SIGNALED));

        // Always: always enforces
        assert!(always_cfg.should_enforce(0));
        assert!(always_cfg.should_enforce(flags::FINALITY_CARIBNIA));
        assert!(always_cfg.should_enforce(flags::FINALITY_SIGNALED));

        // Signaled: only enforces when FINALITY_SIGNALED bit is set
        assert!(!signaled_cfg.should_enforce(0));
        assert!(!signaled_cfg.should_enforce(flags::FINALITY_CARIBNIA));
        assert!(signaled_cfg.should_enforce(flags::FINALITY_SIGNALED));
        assert!(signaled_cfg.should_enforce(flags::FINALITY_CARIBNIA | flags::FINALITY_SIGNALED));
    }

    #[test]
    fn test_mine_flags() {
        // Native: no flags regardless
        let cfg = FinalityConfig {
            mode: FinalityMode::Native,
            caribina_enabled: true,
            monero_enabled: true,
            ..Default::default()
        };
        assert_eq!(cfg.mine_flags(), 0);

        // Always + caribina only
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            caribina_enabled: true,
            monero_enabled: false,
            ..Default::default()
        };
        assert_eq!(cfg.mine_flags(), flags::FINALITY_CARIBNIA);

        // Always + caribina + monero
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            caribina_enabled: true,
            monero_enabled: true,
            ..Default::default()
        };
        assert_eq!(cfg.mine_flags(), flags::FINALITY_CARIBNIA | flags::FINALITY_MONERO);

        // Always + monero only (no caribina)
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            caribina_enabled: false,
            monero_enabled: true,
            ..Default::default()
        };
        assert_eq!(cfg.mine_flags(), flags::FINALITY_MONERO);

        // Signaled: adds SIGNALED bit
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            caribina_enabled: true,
            monero_enabled: false,
            ..Default::default()
        };
        assert_eq!(
            cfg.mine_flags(),
            flags::FINALITY_CARIBNIA | flags::FINALITY_SIGNALED
        );

        // Signaled + monero
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            caribina_enabled: true,
            monero_enabled: true,
            ..Default::default()
        };
        assert_eq!(
            cfg.mine_flags(),
            flags::FINALITY_CARIBNIA | flags::FINALITY_MONERO | flags::FINALITY_SIGNALED
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            caribina_enabled: false,
            monero_enabled: true,
            monero_min_confirmations: 7,
            monerod_url: Some("http://127.0.0.1:18081/json_rpc".to_string()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: FinalityConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn test_should_anchor_monero() {
        // Always mode + monero enabled = true
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            monero_enabled: true,
            ..Default::default()
        };
        assert!(cfg.should_anchor_monero());

        // Always mode + monero disabled = false (default)
        let cfg = FinalityConfig::default();
        assert!(!cfg.should_anchor_monero());

        // Native mode + monero enabled = false
        let cfg = FinalityConfig {
            mode: FinalityMode::Native,
            monero_enabled: true,
            ..Default::default()
        };
        assert!(!cfg.should_anchor_monero());

        // Signaled mode + monero enabled = true
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            monero_enabled: true,
            ..Default::default()
        };
        assert!(cfg.should_anchor_monero());

        // Signaled mode + monero disabled = false
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            monero_enabled: false,
            ..Default::default()
        };
        assert!(!cfg.should_anchor_monero());
    }

    // `test_should_verify_monero_anchor` was removed with `should_verify_monero_anchor` itself
    // (OBL-C65): it had no production caller and encoded the wrong invariant — see the method's
    // deletion note. The Monero side's real invariant is now "enforcement requires the derived
    // Monero anchor", asserted where that decision is made.

    /// OBL-C65 — enforcement never outruns verification.
    ///
    /// **This test was replaced rather than flipped, because the fix removed the thing it tested.**
    /// `should_enforce` used to be paired with `should_verify_anchor` /
    /// `should_verify_monero_anchor`: two predicates computed from unrelated conditions, so a
    /// configuration existed that enforced anchors it had decided not to check — and any peer could
    /// then drive a node into `AnchoredBlockConflict` by setting a field. The resolution was not a
    /// corrected predicate but the removal of both (see `should_enforce`'s doc), so the invariant is
    /// no longer a property of `FinalityConfig` at all.
    ///
    /// It is instead a property of the two call sites, and it holds by construction: both call
    /// `caribina::verify_anchor_proof` directly, so "enforced" and "verified" are the same call and
    /// cannot disagree. That is asserted where it lives — the positive control and the forged-key
    /// control in `chain_state::tests::test_finality_conferred_without_any_anchor_verification`, and
    /// the eight negative controls in `caribina::verify::tests`.
    ///
    /// What can still be checked here is that the *decision* to enforce is a function of the mode and
    /// the block's own flags, and nothing else — no `*_enabled` flag can suppress enforcement while
    /// leaving enforcement in place elsewhere, which was the shape of the old bug.
    #[test]
    fn test_enforce_decision_depends_only_on_mode_and_flags() {
        let flag_values = [
            0u8,
            flags::FINALITY_CARIBNIA,
            flags::FINALITY_MONERO,
            flags::FINALITY_SIGNALED,
            flags::FINALITY_CARIBNIA | flags::FINALITY_SIGNALED,
            0x07,
        ];
        for mode in [FinalityMode::Native, FinalityMode::Always, FinalityMode::Signaled] {
            for caribina_enabled in [false, true] {
                for monero_enabled in [false, true] {
                    for f in flag_values {
                        let cfg = FinalityConfig { mode, caribina_enabled, monero_enabled, ..Default::default() };
                        // The `*_enabled` flags gate *anchoring*, not enforcement; a node that enforces
                        // must decide that from the mode, and the verified proof then decides the rest.
                        let expected = match mode {
                            FinalityMode::Native => false,
                            FinalityMode::Always => true,
                            FinalityMode::Signaled => f & flags::FINALITY_SIGNALED != 0,
                        };
                        assert_eq!(
                            cfg.should_enforce(f), expected,
                            "should_enforce must depend only on mode and flags — an enablement flag \
                             leaking into it is how enforcement came to outrun verification (OBL-C65)"
                        );
                    }
                }
            }
        }
    }
}
