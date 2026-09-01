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

//! DarkWow Linear Blockchain
//!
//! A simple, linear blockchain implementation without uncle blocks,
//! fork consensus, or overlay caching. Designed for determinism.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod block;
pub mod caribina;
pub mod chain_state;
mod consensus;
mod error;
pub mod execution;
pub mod fee_estimator;
pub mod opcode_cost;
pub mod schedule;
#[cfg(feature = "sharding")]
pub mod shard;
pub mod fee_window;
pub mod contract_risk;
pub mod finality;
pub mod proof_of_token_balance;
mod miner;
pub mod monero;
mod store;
pub mod supply_chain;
pub mod sync_types;
#[cfg(feature = "sync-p2p")]
pub mod sync_boundary;
#[cfg(feature = "sync-p2p")]
pub mod sync_connection;
mod transaction;
pub mod validation;
pub mod zk_verifier;

mod serial_sync;
#[cfg(feature = "async")]
mod serial;

/// Number of blocks before coinbase rewards can be moved.
/// Matches Bitcoin Core's COINBASE_MATURITY.
pub const COINBASE_MATURITY: u64 = 100;

/// Consensus chain identifier — scopes transactions to this network.
/// Prepended to every transaction hash to prevent cross-network replay.
/// Testnet and mainnet MUST use different values at genesis.
/// Changed from blake3 constant to zero-prefixed deterministic bytes
/// for reproducibility across compilers/platforms.
pub const CHAIN_ID: [u8; 32] = [
    // "darkwow-testnet-v1" as blake3 — pre-computed for determinism
    0xd2, 0x8b, 0x4a, 0x76, 0x3c, 0x62, 0xf2, 0x5e,
    0xa1, 0xf9, 0x3d, 0x81, 0x4e, 0xbc, 0x7a, 0x5f,
    0x8d, 0x2c, 0x91, 0x4b, 0xe3, 0x66, 0xa5, 0x7d,
    0x0e, 0xf9, 0x1a, 0x82, 0xcc, 0xe5, 0x3b, 0x44,
];

pub use dwow_sdk::blockchain::{BlockReward, BlockTarget, BlockCharge};
pub use block::{
    build_uncle_merkle, compute_merkle_root, compute_reward, create_block,
    create_block_with_uncles, create_uncle, verify_uncle_proof, Block, BlockHeader, PowSource,
    UncleBlock, UncleProof, MAX_UNCLE_DEPTH, MAX_UNCLE_COUNT,
};
pub use chain_state::{BlockConnectOutcome, CChainState, ReorgSignal};
pub use consensus::{PoWConfig, PoWConsensus};
pub use error::{ConsensusPhase, LinearError};
pub use finality::{FinalityConfig, FinalityMode};
pub use miner::Miner;
pub use monero::{get_block_by_height, get_block_count, verify_monero_anchor, MonerodError, MoneroVerifyError};
pub use store::LinearStore;
pub use supply_chain::{CumulativeSupplyChain, CumulativeSupplyEntry};
pub use transaction::{Commitment, CoinbaseTransaction, ContractCall, Nullifier, PedersenCoordinate, TokenCommitment, Transaction, TxInput, TxOutput, ZkPublicInputs};

/// Result type for linear blockchain operations
pub type Result<T> = std::result::Result<T, LinearError>;