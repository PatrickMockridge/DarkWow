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

//! Bearer Bond ClaimInterestV1 Client API
//!
//! Holder claims deterministic interest accrued on their stake position.
//! The stake coin is NOT consumed — only `last_claim_block` is updated.
//! A BlindOutput_V1 proof creates the interest payout coin.
//!
//! ## Interest Formula
//!
//! ```text
//! interest = principal × interest_rate_bps × blocks_elapsed / (10000 × BLOCKS_PER_YEAR)
//! ```
//!
//! Interest is computed deterministically from on-chain state — no issuer
//! reporting is needed.

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

use crate::model::{BondInput, ClaimInterestParamsV1, CoinAttributes};
use super::point_coords;

/// Public inputs revealed after BlindOutput_V1 proof for the interest payout coin.
pub struct ClaimInterestRevealed {
    pub coin: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
}

impl ClaimInterestRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![self.coin, vc_x, vc_y, self.token_commit, self.spend_hook]
    }
}

/// Input for building a ClaimInterest call.
pub struct ClaimInterestCallInput {
    /// The existing stake coin's on-chain input fields
    pub bond_input: BondInput,
    /// Current block height
    pub claim_block: u64,
    /// Minimum claim threshold (dust protection)
    pub min_claim: u64,
    /// Token ID of the staking pool series
    pub token_id: pallas::Base,
    /// Interest amount (computed off-chain via calculate_interest)
    pub interest_amount: u64,
    /// Holder's address (to receive the interest coin)
    pub holder: pallas::Base,
    /// Spend hook for the interest coin
    pub spend_hook: pallas::Base,
    /// User data for the interest coin
    pub user_data: pallas::Base,
    /// Coin blinding factor for the interest coin
    pub coin_blind: pallas::Base,
}

/// Debris produced by building a ClaimInterest call.
pub struct ClaimInterestCallDebris {
    /// The contract call parameters
    pub params: ClaimInterestParamsV1,
    /// The ZK proof for the interest payout coin
    pub proofs: Vec<Proof>,
    /// Private note data for the interest coin (holder needs this to spend)
    pub interest_note: super::BearerBondNote,
}

/// Builder for `BearerBond::ClaimInterestV1` contract call.
pub struct ClaimInterestCallBuilder {
    /// Claim input
    pub input: ClaimInterestCallInput,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub blind_output_zkbin: ZkBinary,
    /// Proving key for BlindOutput_V1
    pub blind_output_pk: ProvingKey,
}

impl ClaimInterestCallBuilder {
    /// Build the ClaimInterest call debris.
    pub fn build(self) -> Result<ClaimInterestCallDebris> {
        debug!(target: "contract::bearer_bond::client::claim_interest", "Building BearerBond::ClaimInterestV1 contract call");

        let value_blind = ScalarBlind::random(&mut OsRng);
        let token_id_blind = BaseBlind::random(&mut OsRng);

        let (proof, _revealed) = create_claim_interest_proof(
            &self.blind_output_zkbin,
            &self.blind_output_pk,
            &self.input,
            value_blind,
            token_id_blind,
        )?;

        let interest_note = super::BearerBondNote {
            principal: self.input.interest_amount,
            token_id: self.input.token_id,
            spend_hook: self.input.spend_hook,
            user_data: self.input.user_data,
            coin_blind: self.input.coin_blind,
            value_blind: value_blind.inner(),
            token_blind: token_id_blind.inner(),
            last_claim_block: self.input.claim_block,
            maturity_block: 0,
            issuer_contract: ContractId::from(pallas::Base::zero()),
            interest_rate_bps: 0,
        };

        Ok(ClaimInterestCallDebris {
            params: ClaimInterestParamsV1 {
                bond_input: self.input.bond_input,
                claim_block: self.input.claim_block,
                min_claim: self.input.min_claim,
            },
            proofs: vec![proof],
            interest_note,
        })
    }
}

/// Create a BlindOutput_V1 proof for the interest payout coin.
fn create_claim_interest_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ClaimInterestCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
) -> Result<(Proof, ClaimInterestRevealed)> {
    let attrs = CoinAttributes {
        public_key: input.holder,
        value: input.interest_amount,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
        maturity_block: 0,
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(input.interest_amount, value_blind);
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    let public_inputs = ClaimInterestRevealed {
        coin,
        value_commit,
        token_commit,
        spend_hook: input.spend_hook,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.holder)),
        Witness::Base(Value::known(pallas::Base::from(input.interest_amount))),
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
