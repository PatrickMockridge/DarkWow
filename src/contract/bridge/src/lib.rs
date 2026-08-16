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

//! DarkWow Bridge Contract
//!
//! Smart contract implementing anonymous bridging between DarkWow and external
//! blockchains using Object Capability Security. Unlike VSS-based bridges,
//! this design uses deterministic address derivation - users control their
//! own funds via secrets, no threshold signing required.
//!
//! ## Architecture
//!
//! The bridge uses a modular, plugin-based architecture:
//!
//! - `chain_handler/`: Chain-specific handlers implementing `ChainHandler` trait
//! - `light_client/`: Light client verification implementing `LightClient` trait
//! - `capability/`: Object Capability derivation for authorization
//!
//! ## Security Model
//!
//! Object Capability model:
//! - Capabilities are derived, never assigned
//! - No VSS / threshold signing required
//! - User alone authorizes via secret knowledge
//! - Light client verification (no oracles)

/// Chain handler module - plugin architecture for external chains
pub mod chain_handler;
/// Light client module - trustless external chain verification
pub mod light_client;
/// Object Capability module - capability derivation and verification
pub mod capability;
/// Cross-chain cryptographic verification (gated behind bridge-verify feature)
#[cfg(feature = "bridge-verify")]
pub mod verify;

use dwow_sdk::define_contract_function;

// Functions available in the contract
define_contract_function!(BridgeFunction {
    InitializeV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
    UpdateConfigV1 = 0x03,
    CancelWithdrawV1 = 0x04,  // Cancel timed-out withdrawal
    ExecuteGuaranteedWithdrawV1 = 0x05,  // Execute guaranteed withdrawal with pool stake
    // HTLC operations for cross-chain atomic swaps
    CreateHtlcV1 = 0x06,
    ClaimHtlcV1 = 0x07,
    RefundHtlcV1 = 0x08,
    ReassignWithdrawalV1 = 0x09,  // Reassign stuck withdrawal to a new relayer
    RegisterRelayerV1 = 0x0a,   // Register a relayer pubkey with the bridge
    AcceptWithdrawalV1 = 0x0b,  // Accept a pending withdrawal as a relayer
    VerifyRelayerReputationV1 = 0x0c, // Query relayer reputation on-chain
    RegisterFeeScheduleV1 = 0x0d, // Register a fee schedule commitment
    // Cold/precise governance operations
    GovernanceReportV1 = 0x0e,  // Per-chain accounting report proving no fractional reserve
});

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// These are the different sled trees that will be created
pub const BRIDGE_CONTRACT_INFO_TREE: &str = "info";
pub const BRIDGE_CONTRACT_DEPOSITS_TREE: &str = "deposits";
pub const BRIDGE_CONTRACT_WITHDRAWALS_TREE: &str = "withdrawals";
pub const BRIDGE_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const BRIDGE_CONTRACT_KEYS_TREE: &str = "keys";
pub const BRIDGE_CONTRACT_PENDING_WITHDRAWALS_TREE: &str = "pending_withdrawals";
// HTLC trees for cross-chain atomic swaps
pub const BRIDGE_CONTRACT_HTLCS_TREE: &str = "htlcs";
pub const BRIDGE_CONTRACT_HTLC_NULLIFIERS_TREE: &str = "htlc_nullifiers";
pub const BRIDGE_CONTRACT_RELAYERS_TREE: &str = "relayers";
pub const BRIDGE_CONTRACT_GOVERNANCE_REPORTS_TREE: &str = "governance_reports";
// Contract Merkle tree (matches Box/Purse pattern — Sinsemilla, depth 32)
pub const BRIDGE_CONTRACT_BRIDGE_ROOTS_TREE: &str = "bridge_roots";

// These are keys inside the info tree
pub const BRIDGE_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const BRIDGE_CONTRACT_GOVERNANCE_PUBKEY_KEY: &[u8] = b"governance_pubkey";
/// Promissory Note contract ID for cross-contract routing validation
pub const PROMISSORY_NOTE_CONTRACT_ID_KEY: &[u8] = b"promissory_note_cid";
pub const BRIDGE_CONTRACT_PURSE_CONTRACT_ID: &[u8] = b"purse_cid";
pub const BRIDGE_CONTRACT_STATE: &[u8] = b"state";
pub const BRIDGE_CONTRACT_EXTERNAL_CHAIN: &[u8] = b"external_chain";
/// Latest Merkle root of all bridge deposits (Sinsemilla tree, depth 32)
pub const BRIDGE_CONTRACT_LATEST_BRIDGE_ROOT: &[u8] = b"latest_bridge_root";
/// Serialized MerkleTree state (BridgeTree<MerkleNode, usize, 32>)
pub const BRIDGE_CONTRACT_BRIDGE_MERKLE_TREE: &[u8] = b"bridge_merkle_tree";

// zkas circuit namespaces
/// zkas deposit circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V1: &str = "DepositV1";
/// zkas withdrawal circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V1: &str = "WithdrawV1";
/// zkas update config circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V1: &str = "UpdateConfigV1";

// V2 circuit namespaces (HAZOP RC3: domain separation)
/// zkas deposit circuit namespace V2 (domain-separated)
pub const BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V2: &str = "DepositV2";
/// zkas withdrawal circuit namespace V2 (domain-separated)
pub const BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V2: &str = "WithdrawV2";
/// zkas update config circuit namespace V2 (domain-separated)
pub const BRIDGE_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V2: &str = "UpdateConfigV2";
/// zkas claim HTLC circuit namespace V1
pub const BRIDGE_CONTRACT_ZKAS_CLAIM_HTLC_NS_V1: &str = "ClaimHtlcV1";
/// zkas cancel withdraw circuit namespace V1
pub const BRIDGE_CONTRACT_ZKAS_CANCEL_WITHDRAW_NS_V1: &str = "CancelWithdrawV1";
pub const BRIDGE_CONTRACT_ZKAS_EXECUTE_GW_NS_V1: &str = "ExecuteGuaranteedWithdrawV1";
pub const BRIDGE_CONTRACT_ZKAS_REFUND_HTLC_NS_V1: &str = "RefundHtlcV1";
pub const BRIDGE_CONTRACT_ZKAS_ACCEPT_WITHDRAWAL_NS_V1: &str = "AcceptWithdrawalV1";
pub const BRIDGE_CONTRACT_ZKAS_ACCEPT_WITHDRAWAL_NS_V2: &str = "AcceptWithdrawalV2";
pub const BRIDGE_CONTRACT_ZKAS_CANCEL_WITHDRAW_NS_V2: &str = "CancelWithdrawV2";
pub const BRIDGE_CONTRACT_ZKAS_CLAIM_HTLC_NS_V2: &str = "ClaimHtlcV2";
pub const BRIDGE_CONTRACT_ZKAS_EXECUTE_GW_NS_V2: &str = "ExecuteGuaranteedWithdrawV2";
pub const BRIDGE_CONTRACT_ZKAS_REFUND_HTLC_NS_V2: &str = "RefundHtlcV2";

// XMR (Monero) specific constants
/// Number of block confirmations required for XMR deposits
pub const BRIDGE_CONTRACT_XMR_CONFIRMATIONS: u64 = 10;
/// Hash function identifier for XMR (keccak256 = cn_fast_hash)
pub const BRIDGE_CONTRACT_XMR_HASH_FUNCTION: u8 = 3;

// ZEC (Zcash) specific constants
/// Number of block confirmations required for ZEC deposits (Sapling)
pub const BRIDGE_CONTRACT_ZEC_CONFIRMATIONS: u64 = 10;
/// Hash function identifier for ZEC (blake2s for Sapling)
pub const BRIDGE_CONTRACT_ZEC_HASH_FUNCTION: u8 = 4;

// AZT (Aztec) specific constants
/// Number of rollup confirmations required for Aztec deposits
pub const BRIDGE_CONTRACT_AZT_CONFIRMATIONS: u64 = 5;
/// Hash function identifier for Aztec (keccak256 for rollup)
pub const BRIDGE_CONTRACT_AZT_HASH_FUNCTION: u8 = 3;

/// Aztec asset IDs
pub const BRIDGE_CONTRACT_AZT_ASSET_ETH: u32 = 0;
pub const BRIDGE_CONTRACT_AZT_ASSET_DAI: u32 = 1;

// LTC (Litecoin) specific constants
/// Number of block confirmations required for LTC deposits
/// Litecoin faster block time (2.5 min vs Bitcoin 10 min) means faster confirmations
pub const BRIDGE_CONTRACT_LTC_CONFIRMATIONS: u64 = 6;
/// Hash function identifier for LTC (scrypt for PoW, same family as sha256)
pub const BRIDGE_CONTRACT_LTC_HASH_FUNCTION: u8 = 5;

/// Withdrawal timeout constants
/// Default number of blocks before a withdrawal can be cancelled
pub const BRIDGE_CONTRACT_WITHDRAWAL_TIMEOUT_BLOCKS: u64 = 100;

/// Basis point precision (10000 = 100%)
pub const BRIDGE_CONTRACT_BP_PRECISION: u64 = 10000;

/// Slash parameters — proportional to withdrawal amount
/// Minimum slash floor (in smallest unit) — 0.001 XMR equivalent
pub const BRIDGE_CONTRACT_MIN_SLASH: u64 = 1_000_000;
/// Slash rate in basis points (1000 = 10% of withdrawal amount)
pub const BRIDGE_CONTRACT_SLASH_BP: u64 = 1000;

/// Maximum relayer fee in basis points (1000 = 10% of withdrawal amount)
pub const BRIDGE_CONTRACT_MAX_FEE_BP: u64 = 1000;

/// Minimum guaranteed coverage ratio (15000 = 150%)
/// Relayer must have 1.5x the withdrawal amount in available stake
pub const BRIDGE_CONTRACT_MIN_GUARANTEED_COVERAGE_RATIO: u64 = 15000;
/// Info tree key: max guaranteed withdrawal total (circuit breaker)
pub const BRIDGE_CONTRACT_MAX_GUARANTEED_TOTAL: &[u8] = b"max_guaranteed_total";
/// Info tree key: current pending guaranteed amount
pub const BRIDGE_CONTRACT_GUARANTEED_PENDING: &[u8] = b"guaranteed_pending";

/// Config DB keys for per-chain balance sheet tracking (governance reports)
pub const BRIDGE_CONTRACT_TOTAL_DEPOSITED_PREFIX: &[u8] = b"total_deposited_";
pub const BRIDGE_CONTRACT_TOTAL_WITHDRAWN_PREFIX: &[u8] = b"total_withdrawn_";

/// Thread-safe flag for deterministic ZK proof generation.
/// Set by tests before endpoint exercise to eliminate OsRng from collateral/debt
/// blinds, note encryption, and proof generation, so a chain-replay determinism
/// check (PI-7) produces identical bytes on both chains.
/// Must be set BEFORE any ZK proof is created.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation for testing.
/// Replaces OsRng with StdRng::seed_from_u64(0).
#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled. Always `false` unless the
/// `deterministic-zk` feature is enabled (test builds only — heavyweight-spec.md §7.4 DZ-4).
pub fn deterministic_zk_enabled() -> bool {
    #[cfg(feature = "deterministic-zk")]
    {
        DETERMINISTIC_ZK.load(Ordering::SeqCst)
    }
    #[cfg(not(feature = "deterministic-zk"))]
    {
        false
    }
}
