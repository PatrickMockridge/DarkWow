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

//! Bearer Bond DeclareProfitsV1 Client API
//!
//! Issuer declares a profit distribution for a staking pool series.
//! No ZK proofs are required — the issuer self-reports. The entrypoint
//! validates the issuer's identity via signature.
//!
//! ## Plugin Architecture
//!
//! Any parent contract (promissory_note, betting contract, auction) calls
//! this as a child call. The caller must be the `issuer_contract` on the
//! BondCoin. This builder produces the `DeclareProfitsParamsV1` that can
//! be embedded in a parent contract's `ContractCall`.

use dwow_sdk::pasta::pallas;
use tracing::debug;

use crate::model::DeclareProfitsParamsV1;

/// Input for building a DeclareProfits call.
pub struct DeclareProfitsCallInput {
    /// Token ID of the staking pool series
    pub series_token_id: pallas::Base,
    /// Total profit amount being declared
    pub profit_amount: u64,
    /// Start block of the earning period
    pub start_block: u64,
    /// End block of the earning period
    pub end_block: u64,
}

/// Debris produced by building a DeclareProfits call.
pub struct DeclareProfitsCallDebris {
    /// The contract call parameters
    pub params: DeclareProfitsParamsV1,
}

/// Builder for `BearerBond::DeclareProfitsV1` contract call.
///
/// No ZK proofs are required — the issuer self-reports profits.
/// Trust model: if the issuer lies, holders sell and the stake coin
/// price goes to zero. Future phases add profit verification via
/// cross-contract calls or attestations.
pub struct DeclareProfitsCallBuilder {
    /// Profit declaration input
    pub input: DeclareProfitsCallInput,
}

impl DeclareProfitsCallBuilder {
    /// Build the DeclareProfits call debris.
    pub fn build(self) -> DeclareProfitsCallDebris {
        debug!(target: "contract::bearer_bond::client::declare_profits", "Building BearerBond::DeclareProfitsV1 contract call");

        DeclareProfitsCallDebris {
            params: DeclareProfitsParamsV1 {
                series_token_id: self.input.series_token_id,
                profit_amount: self.input.profit_amount,
                start_block: self.input.start_block,
                end_block: self.input.end_block,
            },
        }
    }
}
