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

//! Bearer Bond Client API — Fixed-Interest Staking Model
//!
//! This module provides client-side proof builders for all Bearer Bond
//! contract functions. The ZK circuits (Burn_V1, BlindOutput_V1, Redeem_V1)
//! are reused from PromissoryNote — bond metadata (principal, last_claim_block,
//! issuer_contract) is plaintext, validated at the entrypoint, while maturity
//! is ZK-committed in the coin hash.
//!
//! ## Plugin Architecture
//!
//! Any parent contract (promissory_note, betting contract, auction) that needs
//! capital formation can embed Bearer Bond calls as child calls. The
//! `issuer_contract` field on `BondCoin` identifies the parent contract.
//! Builders return `ContractCallImport`-compatible data so parent contracts
//! can compose them without reimplementing proof logic.
//!
//! ## Builders
//!
//! | Builder | Contract Function | ZK Circuits |
//! |---------|-------------------|-------------|
//! | `IssueStakeCallBuilder` | IssueStakeV1 | BlindOutput_V1 |
//! | `TransferStakeCallBuilder` | TransferStakeV1 | Burn_V1 + BlindOutput_V1 |
//! | `RequestInterestCallBuilder` | RequestInterestV1 | Burn_V1 |
//! | `EmergencyUnstakeCallBuilder` | EmergencyUnstakeV1 | Burn_V1 + Redeem_V1 |
//! | `UnstakeCallBuilder` | UnstakeV1 | Burn_V1 + Redeem_V1 |
//! | `BurnStakeCallBuilder` | BurnStakeV1 | Burn_V1 |
//! | `ProveCoverageCallBuilder` | ProveCoverageV1 | ProveCoverage_V1 |
//! | `PayInterestCallBuilder` | PayInterestV1 | BlindOutput_V1 |

use dwow_sdk::{
    crypto::ContractId,
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// ZK circuit binary constants
pub mod zkbins;

/// `BearerBond::IssueStakeV1` API — create staking pool, mint initial stake coin
pub mod issue_stake_v1;

/// `BearerBond::TransferStakeV1` API — transfer stake position
pub mod transfer_stake_v1;

/// `BearerBond::RequestInterestV1` API — request interest payment (prove ownership)
pub mod request_interest_v1;

/// `BearerBond::EmergencyUnstakeV1` API — exit before maturity on coverage failure
pub mod emergency_unstake_v1;

/// `BearerBond::UnstakeV1` API — withdraw principal at maturity
pub mod unstake_v1;

/// `BearerBond::BurnStakeV1` API — retire staking pool
pub mod burn_stake_v1;

/// `BearerBond::ProveCoverageV1` API — governance: prove solvency
pub mod prove_coverage_v1;

/// `BearerBond::PayInterestV1` API — issuer pays a pending interest claim
pub mod pay_interest_v1;

/// BearerBondNote holds all the attributes of a received stake coin.
///
/// After a TransferStakeV1, the recipient uses their `SecretKey` to derive
/// their public key and verify the coin commitment. The note contains both
/// the ZK-committed attributes (value, token_id, spend_hook, user_data,
/// maturity_block, blinds) and the bond-specific metadata (principal,
/// last_claim_block, issuer_contract) that travels as plaintext on the BondCoin.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct BearerBondNote {
    /// Principal value staked
    pub principal: u64,
    /// Token ID of the staking pool series
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blinding factor
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value (Pedersen commitment)
    pub value_blind: pallas::Scalar,
    /// Blinding factor for the token ID
    pub token_blind: pallas::Base,
    /// Block height of last interest claim (inherited from previous coin)
    pub last_claim_block: u64,
    /// Block height when stake matures (ZK-committed in CoinAttributes)
    pub maturity_block: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// Annual interest rate in basis points for the series
    pub interest_rate_bps: u64,
}

/// Extract (x, y) base-field coordinates from a pallas::Point for ZK public inputs.
pub fn point_coords(pt: pallas::Point) -> (pallas::Base, pallas::Base) {
    use dwow_sdk::crypto::pasta_prelude::{Curve, CurveAffine};
    let affine = pt.to_affine();
    let coords = affine.coordinates().unwrap();
    (*coords.x(), *coords.y())
}
