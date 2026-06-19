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

//! Bearer Bond PayInterestV1 Client API
//!
//! Issuer pays a pending interest claim. Reads the claim record from the
//! `bonds_info` tree, verifies reserves are sufficient, and creates a
//! fresh payment coin (BlindOutput_V1) addressed to the holder's one-time
//! `payment_key` from the claim.
//!
//! Fresh `coin_blind` and `value_blind` per payment ensure unlinkable
//! payment addresses — the issuer cannot track the holder across payments.
//!
//! ## Flow
//!
//! 1. Holder calls RequestInterestV1 → claim record stored on-chain
//! 2. Issuer scans bonds_info tree for Pending claims
//! 3. Issuer calls PayInterestV1 with a BlindOutput_V1 coin to the holder's payment_key

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

use crate::model::{CoinAttributes, PayInterestParamsV1};
use super::point_coords;

/// Public inputs revealed after BlindOutput_V1 proof for the payment coin.
/// Order must match BlindOutput_V1 circuit:
/// coin, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct PayInterestRevealed {
    pub coin: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl PayInterestRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![
            self.coin,
            vc_x,
            vc_y,
            self.token_commit,
            self.spend_hook,
            self.tx_commitment,
        ]
    }
}

/// Input for building a PayInterest call (issuer-side).
pub struct PayInterestCallInput {
    /// Token commit of the bond being paid against
    pub bond_token_commit: pallas::Base,
    /// Block height of the claim being paid
    pub claim_block: u64,
    /// Interest amount to pay (must match the claim's interest_amount)
    pub interest_amount: u64,
    /// Token ID of the staking pool series
    pub token_id: pallas::Base,
    /// Holder's one-time payment key (from the claim record)
    pub payment_key: pallas::Base,
    /// Spend hook for the payment coin
    pub spend_hook: pallas::Base,
    /// User data for the payment coin
    pub user_data: pallas::Base,
    /// Fresh coin blinding factor for the payment coin (unlinkable address)
    pub coin_blind: pallas::Base,
    pub tx_commitment: pallas::Base,
}

/// Debris produced by building a PayInterest call.
pub struct PayInterestCallDebris {
    /// The contract call parameters
    pub params: PayInterestParamsV1,
    /// The ZK proof (BlindOutput_V1 for the payment coin)
    pub proofs: Vec<Proof>,
}

/// Builder for `BearerBond::PayInterestV1` contract call.
pub struct PayInterestCallBuilder {
    /// Payment input
    pub input: PayInterestCallInput,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub blind_output_zkbin: ZkBinary,
    /// Proving key for BlindOutput_V1
    pub blind_output_pk: ProvingKey,
}

impl PayInterestCallBuilder {
    /// Build the PayInterest call debris.
    pub fn build(self) -> Result<PayInterestCallDebris> {
        debug!(target: "contract::bearer_bond::client::pay_interest", "Building BearerBond::PayInterestV1 contract call");

        let value_blind = ScalarBlind::random(&mut OsRng);
        let token_id_blind = BaseBlind::random(&mut OsRng);

        let (proof, revealed) = create_pay_interest_proof(
            &self.blind_output_zkbin,
            &self.blind_output_pk,
            &self.input,
            value_blind,
            token_id_blind,
        )?;

        let interest_coin = crate::model::BondCoin {
            value_commit: revealed.value_commit,
            token_commit: revealed.token_commit,
            nullifier: crate::model::Nullifier::from_base(pallas::Base::zero()),
            merkle_root: dwow_sdk::crypto::MerkleNode::from(pallas::Base::zero()),
            user_data_enc: pallas::Base::zero(),
            spend_hook: self.input.spend_hook,
            signature_public: self.input.payment_key,
            last_claim_block: 0,
            maturity_block: 0,
            issuer_contract: dwow_sdk::crypto::ContractId::from(pallas::Base::zero()),
        };

        Ok(PayInterestCallDebris {
            params: PayInterestParamsV1 {
                bond_token_commit: self.input.bond_token_commit,
                claim_block: self.input.claim_block,
                interest_coin,
            },
            proofs: vec![proof],
        })
    }
}

/// Create a BlindOutput_V1 proof for the payment coin.
///
/// The issuer creates this proof — NOT the holder. Each payment uses a
/// fresh random `coin_blind` and `value_blind`, making payment addresses
/// unlinkable across claims.
///
/// Witness order must match BlindOutput_V1 circuit:
/// coin_public, coin_value, coin_token_id, coin_spend_hook,
/// coin_user_data, coin_blind, value_blind, token_id_blind
fn create_pay_interest_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PayInterestCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
) -> Result<(Proof, PayInterestRevealed)> {
    let attrs = CoinAttributes {
        public_key: input.payment_key,
        value: input.interest_amount,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
        maturity_block: 0, // Payment coins don't have maturity
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(input.interest_amount, value_blind);
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    let public_inputs = PayInterestRevealed {
        coin,
        value_commit,
        token_commit,
        spend_hook: input.spend_hook,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.payment_key)),
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
