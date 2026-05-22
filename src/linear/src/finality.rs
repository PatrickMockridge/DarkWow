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

    /// Returns true if the node should enforce anchors on received blocks
    pub fn should_enforce(&self, block_flags: u8) -> bool {
        match self.mode {
            FinalityMode::Native => false,
            FinalityMode::Always => true,
            FinalityMode::Signaled => block_flags & flags::FINALITY_SIGNALED != 0,
        }
    }

    /// Returns true if the node should verify anchor proofs on received blocks
    pub fn should_verify_anchor(&self, block_flags: u8) -> bool {
        if !self.caribina_enabled {
            return false;
        }
        if self.mode == FinalityMode::Native {
            return false;
        }
        if self.mode == FinalityMode::Signaled {
            return block_flags & flags::FINALITY_SIGNALED != 0;
        }
        true
    }

    /// Returns true if the node should verify Monero anchors on received blocks
    pub fn should_verify_monero_anchor(&self, block_flags: u8) -> bool {
        if !self.monero_enabled {
            return false;
        }
        if self.mode == FinalityMode::Native {
            return false;
        }
        if self.mode == FinalityMode::Signaled {
            return block_flags & flags::FINALITY_SIGNALED != 0;
        }
        true
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
    fn test_should_verify_anchor() {
        // caribina disabled → always false regardless of mode
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            caribina_enabled: false,
            ..Default::default()
        };
        assert!(!cfg.should_verify_anchor(flags::FINALITY_CARIBNIA));

        // Native mode → always false
        let cfg = FinalityConfig {
            mode: FinalityMode::Native,
            caribina_enabled: true,
            ..Default::default()
        };
        assert!(!cfg.should_verify_anchor(flags::FINALITY_CARIBNIA));

        // Always mode + caribina enabled → true
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            caribina_enabled: true,
            ..Default::default()
        };
        assert!(cfg.should_verify_anchor(flags::FINALITY_CARIBNIA));
        assert!(cfg.should_verify_anchor(0)); // Always ignores flags

        // Signaled mode → only when SIGNALED bit set
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            caribina_enabled: true,
            ..Default::default()
        };
        assert!(!cfg.should_verify_anchor(0));
        assert!(!cfg.should_verify_anchor(flags::FINALITY_CARIBNIA));
        assert!(cfg.should_verify_anchor(flags::FINALITY_SIGNALED));
        assert!(cfg.should_verify_anchor(flags::FINALITY_CARIBNIA | flags::FINALITY_SIGNALED));
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

    #[test]
    fn test_should_verify_monero_anchor() {
        // monero disabled -> always false regardless of mode
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            monero_enabled: false,
            ..Default::default()
        };
        assert!(!cfg.should_verify_monero_anchor(flags::FINALITY_MONERO));

        // Native mode -> always false
        let cfg = FinalityConfig {
            mode: FinalityMode::Native,
            monero_enabled: true,
            ..Default::default()
        };
        assert!(!cfg.should_verify_monero_anchor(flags::FINALITY_MONERO));

        // Always mode + monero enabled -> true
        let cfg = FinalityConfig {
            mode: FinalityMode::Always,
            monero_enabled: true,
            ..Default::default()
        };
        assert!(cfg.should_verify_monero_anchor(flags::FINALITY_MONERO));
        assert!(cfg.should_verify_monero_anchor(0)); // Always ignores flags

        // Signaled mode -> only when SIGNALED bit set
        let cfg = FinalityConfig {
            mode: FinalityMode::Signaled,
            monero_enabled: true,
            ..Default::default()
        };
        assert!(!cfg.should_verify_monero_anchor(0));
        assert!(!cfg.should_verify_monero_anchor(flags::FINALITY_MONERO));
        assert!(cfg.should_verify_monero_anchor(flags::FINALITY_SIGNALED));
        assert!(cfg.should_verify_monero_anchor(flags::FINALITY_MONERO | flags::FINALITY_SIGNALED));
    }
}
