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

mod block;
mod blockchain;
mod consensus;
mod error;
mod miner;
mod store;
mod transaction;

#[cfg(feature = "async")]
mod serial;

pub use block::{
    build_uncle_merkle, compute_reward, create_block, create_block_with_uncles, create_uncle,
    verify_uncle_proof, UncleBlock, UncleProof, Block, BlockHeader, MAX_UNCLE_DEPTH,
};
pub use blockchain::LinearBlockchain;
pub use consensus::PoWConsensus;
pub use error::LinearError;
pub use miner::Miner;
pub use store::LinearStore;
pub use transaction::{CoinbaseTransaction, Input, Output, Transaction, ContractCall};

/// Result type for linear blockchain operations
pub type Result<T> = std::result::Result<T, LinearError>;