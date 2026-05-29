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

//! Bearer Bond ClaimProfitsV1 Client API
//!
//! Holder claims their pro-rata share of declared but unclaimed profits.
//! The stake coin is NOT consumed — only `last_claim_block` is updated.
//! A BlindOutput_V1 proof creates the profit payout coin.
//!
//! ## Profit Share Formula
//!
//! ```text
//! share = staked_principal x declared_profit / total_staked_in_series
//! ```
//!
//! The profit share is computed off-chain by scanning profit declarations
//! and summing the holder's pro-rata share. The result is passed to the
//! contract as `profit_share` in the params.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, BaseBlind, ScalarBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use dwow_sdk::crypto::ContractId;

use crate::model::{BondInput, ClaimProfitsParamsV1, CoinAttributes};
use super::point_coords;

/// Public inputs revealed after BlindOutput_V1 proof for the profit payout coin.
/// Order must match BlindOutput_V1 circuit:
/// coin, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct ClaimProfitsRevealed {
    pub coin: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
}

impl ClaimProfitsRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![self.coin, vc_x, vc_y, self.token_commit, self.spend_hook]
    }
}

/// Input for building a ClaimProfits call.
pub struct ClaimProfitsCallInput {
    /// The existing stake coin's on-chain input fields
    pub bond_input: BondInput,
    /// Current block height
    pub claim_block: u64,
    /// Minimum claim threshold (dust protection)
    pub min_claim: u64,
    /// Computed profit share (off-chain calculation from declarations)
    pub profit_share: u64,
    /// Token ID of the staking pool series
    pub token_id: pallas::Base,
    /// Holder's address (to receive the profit coin)
    pub holder: pallas::Base,
    /// Spend hook for the profit coin
    pub spend_hook: pallas::Base,
    /// User data for the profit coin
    pub user_data: pallas::Base,
    /// Coin blinding factor for the profit coin
    pub coin_blind: pallas::Base,
}

/// Debris produced by building a ClaimProfits call.
pub struct ClaimProfitsCallDebris {
    /// The contract call parameters
    pub params: ClaimProfitsParamsV1,
    /// The ZK proof for the profit payout coin
    pub proofs: Vec<Proof>,
    /// Private note data for the profit coin (holder needs this to spend)
    pub profit_note: super::BearerBondNote,
}

/// Builder for `BearerBond::ClaimProfitsV1` contract call.
pub struct ClaimProfitsCallBuilder {
    /// Claim input
    pub input: ClaimProfitsCallInput,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub blind_output_zkbin: ZkBinary,
    /// Proving key for BlindOutput_V1
    pub blind_output_pk: ProvingKey,
}

impl ClaimProfitsCallBuilder {
    /// Build the ClaimProfits call debris.
    pub fn build(self) -> Result<ClaimProfitsCallDebris> {
        debug!(target: "contract::bearer_bond::client::claim_profits", "Building BearerBond::ClaimProfitsV1 contract call");

        let value_blind = ScalarBlind::random(&mut OsRng);
        let token_id_blind = BaseBlind::random(&mut OsRng);

        let (proof, _revealed) = create_claim_profits_proof(
            &self.blind_output_zkbin,
            &self.blind_output_pk,
            &self.input,
            value_blind,
            token_id_blind,
        )?;

        let profit_note = super::BearerBondNote {
            principal: self.input.profit_share,
            token_id: self.input.token_id,
            spend_hook: self.input.spend_hook,
            user_data: self.input.user_data,
            coin_blind: self.input.coin_blind,
            value_blind: value_blind.inner(),
            token_blind: token_id_blind.inner(),
            last_claim_block: self.input.claim_block,
            maturity_block: 0,
            issuer_contract: ContractId::from(pallas::Base::zero()),
        };

        Ok(ClaimProfitsCallDebris {
            params: ClaimProfitsParamsV1 {
                bond_input: self.input.bond_input,
                claim_block: self.input.claim_block,
                min_claim: self.input.min_claim,
                profit_share: self.input.profit_share,
            },
            proofs: vec![proof],
            profit_note,
        })
    }
}

/// Create a BlindOutput_V1 proof for the profit payout coin.
///
/// Witness order must match BlindOutput_V1 circuit:
/// coin_public, coin_value, coin_token_id, coin_spend_hook,
/// coin_user_data, coin_blind, value_blind, token_id_blind
fn create_claim_profits_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ClaimProfitsCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
) -> Result<(Proof, ClaimProfitsRevealed)> {
    let attrs = CoinAttributes {
        public_key: input.holder,
        value: input.profit_share,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(input.profit_share, value_blind);
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    let public_inputs = ClaimProfitsRevealed {
        coin,
        value_commit,
        token_commit,
        spend_hook: input.spend_hook,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.holder)),
        Witness::Base(Value::known(pallas::Base::from(input.profit_share))),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
