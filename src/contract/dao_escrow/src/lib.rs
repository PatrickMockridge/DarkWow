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

//! DarkWow DAO-Escrow Contract
//!
//! ## Three Operating Modes
//!
//! DAO-Escrow supports three configuration modes:
//!
//! ### MODE_ESCROW (0x00) - Escrow-Only
//! - Pure insurance pool
//! - Members pay premiums → endowment grows
//! - No treasury (operational funds)
//! - Endowment pays out claims
//!
//! ### MODE_TREASURY (0x01) - Treasury-Only
//! - Same as DarkWow DAO
//! - Members pay fees → treasury grows
//! - DAO votes on treasury spending
//! - No endowment/insurance
//!
//! ### MODE_TREASURY_ENDOWMENT (0x02) - Treasury + Endowment
//! - Full-featured with insurance backing
//! - Fee split: treasury_share → Treasury, endowment_share → Endowment
//! - Treasury for operational costs, Endowment for insurance
//!
//! ## Composability with DrainProtection
//!
//! DAO-Escrow can integrate with the DrainProtection contract to provide
//! governance-level protections against malicious DAO actions or mass exit attacks.
//!
//! ### Integration Pattern
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    DAO-Escrow + DrainProtection                           │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                          │
//! │  ┌──────────────────────┐         ┌──────────────────────┐              │
//! │  │     DAO-Escrow       │         │  DrainProtection      │              │
//! │  │                      │         │                       │              │
//! │  │  ┌────────────────┐  │         │  ┌────────────────┐  │              │
//! │  │  │ pay_premium()  │──┼───┐     │  │  exit()        │  │              │
//! │  │  └────────────────┘  │   │     │  │  transfer()    │  │              │
//! │  │                      │   │     │  │  lock/unlock   │  │              │
//! │  │  State: Merklized    │   │     │  └───────┬────────┘  │              │
//! │  │  Membership tree     │   │     │          │            │              │
//! │  │                      │   │     │  Verifies via:        │              │
//! │  │                      │   ├────▶│  ┌────────▼────────┐ │              │
//! │  │                      │   │     │  │ Merkle proof   │ │              │
//! │  │                      │   │     │  │ from DAO-Escrow│ │              │
//! │  └──────────────────────┘   │     │  └────────────────┘ │              │
//! │                             │     │                       │              │
//! └─────────────────────────────┴─────┴───────────────────────┘              │
//!                               │                                              │
//!        Cross-Contract         │                                              │
//!        Merkle Proof          │                                              │
//!                               ▼                                              │
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    No Direct State Sharing!                              │
//! │                                                                          │
//! │  DrainProtection verifies DAO-Escrow membership via Merkle proof.        │
//! │  DAO-Escrow does NOT read DrainProtection state.                         │
//! │  Each contract maintains its own nullifier namespace.                   │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ### DrainProtection Features Enabled
//!
//! | Feature | Description |
//! |---------|-------------|
//! | Rate Limiting | Transfers exceeding base rate require 2/3 vote |
//! | Vote Thresholds | Large withdrawals need 2/3 approval + 50% quorum |
//! | Emergency Lock | Lock funds with 2/3 vote (max 7 days) |
//! | Member Exit | Any member exits with 1/3 haircut |
//!
//! ## Trust Model
//!
//! - Membership notes are time-locked (block-based expiry)
//! - Claims against endowment handled by DAO vote
//! - Built-in governance (propose/vote/exec)
//! - No external DAO dependency
//!
//! ## Use Cases
//!
//! - **Community Insurance**: Escrow mode - pure insurance pool
//! - **Protocol Treasury**: Treasury mode - same as DarkWow DAO
//! - **Full-Featured DAO**: TreasuryEndowment mode - treasury + insurance

use dwow_sdk::define_contract_function;

/// DAO-Escrow operating modes
pub mod modes {
    /// Escrow-only: Pure insurance pool
    pub const MODE_ESCROW: u8 = 0x00;
    /// Treasury-only: Same as DarkWow DAO
    pub const MODE_TREASURY: u8 = 0x01;
    /// Treasury + Endowment: Full-featured
    pub const MODE_TREASURY_ENDOWMENT: u8 = 0x02;
}

define_contract_function!(DaoEscrowFunction {
    InitializeV1 = 0x00,
    UpdateV1 = 0x01,
    PayPremiumV1 = 0x02,
    WithdrawV1 = 0x03,
    EndowmentWithdrawV1 = 0x04,
    TreasurySpendV1 = 0x05,
    EnableDrainProtectionV1 = 0x06,
    ProposeClaimV1 = 0x07,
    VoteClaimV1 = 0x08,
    ExecuteClaimV1 = 0x09,
    RegisterCapabilityRequirementV1 = 0x0a,
    VerifyMemberCapabilityV1 = 0x0b,
    ResolveDisputeV1 = 0x0c,
    CancelClaimV1 = 0x0d,
    SetGovernanceConfigV1 = 0x0e,
    SetGovernanceActiveV1 = 0x0f,
    DeactivateCapabilityRequirementV1 = 0x10,
});

/// Internal contract errors
pub mod error;

/// Per-capability key descriptors and action metadata
pub mod capability;
/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Info tree (version, config)
pub const DAO_ESCROW_CONTRACT_INFO_TREE: &str = "info";
/// Bullas tree (endowment instances)
pub const DAO_ESCROW_CONTRACT_BULLAS_TREE: &str = "bullas";
/// Membership notes tree (time-limited membership)
pub const DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE: &str = "membership";
/// Endowment pool tree (actual funds)
pub const DAO_ESCROW_CONTRACT_ENDOWMENT_TREE: &str = "endowment";
/// Proposals tree (governance proposals/claims)
pub const DAO_ESCROW_CONTRACT_PROPOSALS_TREE: &str = "proposals";
/// Votes tree (vote records per proposal)
pub const DAO_ESCROW_CONTRACT_VOTES_TREE: &str = "votes";
/// Capability requirements tree (required capability IDs per role)
pub const DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE: &str = "capability_requirements";
/// Disputes tree (dispute resolution records)
pub const DAO_ESCROW_CONTRACT_DISPUTES_TREE: &str = "disputes";
/// Nullifiers tree (prevents double-vote, double-propose)
pub const DAO_ESCROW_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Governance config tree (separate from endowment for clean separation)
pub const DAO_ESCROW_CONTRACT_GOVERNANCE_TREE: &str = "governance";

// ============================================================================
// KEYS
// ============================================================================

/// DB version key
pub const DAO_ESCROW_DB_VERSION: &[u8] = b"db_version";
/// Merkle tree key
pub const DAO_ESCROW_MERKLE_TREE: &[u8] = b"merkle_tree";
/// Latest root key
pub const DAO_ESCROW_LATEST_ROOT: &[u8] = b"last_root";

// ============================================================================
// ZKAS CIRCUIT NAMESPACES
// ============================================================================

/// ZKAS namespace for initialization
pub const DAO_ESCROW_ZKAS_INIT_NS: &str = "Init";
/// ZKAS namespace for premium payment
pub const DAO_ESCROW_ZKAS_PREMIUM_NS: &str = "PayPremium";
/// ZKAS namespace for claim proposal
pub const DAO_ESCROW_ZKAS_PROPOSE_CLAIM_NS: &str = "ProposeClaim";
/// ZKAS namespace for claim voting
pub const DAO_ESCROW_ZKAS_VOTE_CLAIM_NS: &str = "VoteClaim";
/// ZKAS namespace for member capability verification
pub const DAO_ESCROW_ZKAS_VERIFY_MEMBER_CAP_NS: &str = "VerifyMemberCapability";
/// ZKAS namespace for dispute resolution
pub const DAO_ESCROW_ZKAS_RESOLVE_DISPUTE_NS: &str = "ResolveDispute";
/// ZKAS namespace for governance config
pub const DAO_ESCROW_ZKAS_SET_GOVERNANCE_CONFIG_NS: &str = "SetGovernanceConfigV1";

// V2 circuit namespaces (HAZOP RC3: domain separation)
/// ZKAS namespace for initialization V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_INIT_NS_V2: &str = "InitV2";
/// ZKAS namespace for premium payment V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_PREMIUM_NS_V2: &str = "PayPremiumV2";
/// ZKAS namespace for claim proposal V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_PROPOSE_CLAIM_NS_V2: &str = "ProposeClaimV2";
/// ZKAS namespace for claim voting V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_VOTE_CLAIM_NS_V2: &str = "VoteClaimV2";
/// ZKAS namespace for member capability V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_VERIFY_MEMBER_CAP_NS_V2: &str = "VerifyMemberCapabilityV2";
/// ZKAS namespace for dispute resolution V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_RESOLVE_DISPUTE_NS_V2: &str = "ResolveDisputeV2";
/// ZKAS namespace for governance config V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_SET_GOVERNANCE_CONFIG_NS_V2: &str = "SetGovernanceConfigV2";

// ============================================================================
// ZK CIRCUIT BINARIES (for client-side proof generation)
// ============================================================================

// V1 ZK circuit binaries removed (rc3 Batch 4) — V1 .zk source and .zk.bin files deleted.

// ============================================================================
// DRAIN PROTECTION INTEGRATION
// ============================================================================

/// When enabled, the DAO-Escrow endowment/treasury is protected by DrainProtection
/// - Rate limiting on all fund transfers
/// - 2/3 vote required for large withdrawals
/// - Emergency lock/unlock controls
/// - Member exit with haircut
///
/// Integration: DrainProtection verifies membership via DAO-Escrow's Merkle tree.
/// The bulla is used as the fund identifier in DrainProtection.
pub const DAO_ESCROW_DRAIN_PROTECTION_KEY: &[u8] = b"drain_protection_enabled";

/// Key storing the associated DrainProtection bulla (if enabled)
pub const DAO_ESCROW_DRAIN_PROTECTION_BULLA_KEY: &[u8] = b"drain_protection_bulla";
/// Promissory Note contract ID for cross-contract routing validation
pub const PROMISSORY_NOTE_CONTRACT_ID_KEY: &[u8] = b"promissory_note_cid";
/// Identity contract ID for cross-contract routing validation (safety.md Lesson 15)
pub const IDENTITY_CONTRACT_ID_KEY: &[u8] = b"identity_cid";
/// Purse contract ID (genesis counter 8) — tracks treasury/pool/endowment balances
/// via Pedersen commitments instead of raw u64 arithmetic.
pub const PURSE_CONTRACT_ID_KEY: &[u8] = b"purse_cid";
/// Box contract ID (genesis counter 9) — replaces hand-rolled capability proofs
/// for governance roles (member_vote, board_treasury, board_endowment, dispute_arbitrator).
pub const BOX_CONTRACT_ID_KEY: &[u8] = b"box_cid";
