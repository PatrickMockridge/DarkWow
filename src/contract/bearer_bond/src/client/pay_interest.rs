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
//! fresh payment commitment (BlindOutput_V1) addressed to the holder's one-time
//! `payment_key` from the claim.
//!
//! Fresh `commitment_blind` and `value_blind` per payment ensure unlinkable
//! payment addresses — the issuer cannot track the holder across payments.
//!
//! ## Flow
//!
//! 1. Holder calls RequestInterestV1 → claim record stored on-chain
//! 2. Issuer scans bonds_info tree for Pending claims
//! 3. Issuer calls PayInterestV1 with a BlindOutput_V1 commitment to the holder's payment_key

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

use crate::model::{CommitmentAttributes, PayInterestParamsV1};
use super::point_coords;

/// Public inputs revealed after BlindOutput_V1 proof for the payment commitment.
/// Order must match BlindOutput_V1 circuit:
/// commitment, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct PayInterestRevealed {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PayInterestRevealed {
    pub fn to_vec(&self) -> Result<Vec<pallas::Base>> {
        let (vc_x, vc_y) = point_coords(self.value_commit)?;
        Ok(vec![
            self.commitment,
            vc_x,
            vc_y,
            self.token_commit,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ])
    }
}

/// Input for building a PayInterest call (issuer-side).
pub struct PayInterestCallInput {
    /// Token commit of the bond being paid against
    pub bond_commitment: pallas::Base,
    /// Block height of the claim being paid
    pub claim_block: u64,
    /// Interest amount to pay (must match the claim's interest_amount)
    pub interest_amount: u64,
    /// Token ID of the staking pool series
    pub asset_id: pallas::Base,
    /// Holder's one-time payment key (from the claim record)
    pub payment_key: pallas::Base,
    /// Spend hook for the payment commitment
    pub spend_hook: pallas::Base,
    /// User data for the payment commitment
    pub user_data: pallas::Base,
    /// Fresh commitment blinding factor for the payment commitment (unlinkable address)
    pub commitment_blind: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Debris produced by building a PayInterest call.
pub struct PayInterestCallDebris {
    /// The contract call parameters
    pub params: PayInterestParamsV1,
    /// The ZK proof (BlindOutput_V1 for the payment commitment)
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
    /// `OBL-C198`: draw the blinds, derive, and assemble the call's data — stopping **before** the
    /// proof. The commitment covers the finished call data, so the call must exist first.
    pub fn prepare(self) -> Result<PayInterestCallPlan> {
        debug!(target: "contract::bearer_bond::client::pay_interest", "Preparing BearerBond::PayInterestV1 contract call");

        let value_blind = super::draw_scalar_blind();
        let asset_id_blind = super::draw_base_blind();
        let derived = derive_pay_interest(&self.input, value_blind.clone(), asset_id_blind.clone());

        let params = PayInterestParamsV1 {
            bond_commitment: self.input.bond_commitment,
            claim_block: self.input.claim_block,
            interest_commitment: crate::model::BondCommitment {
                value_commit: derived.value_commit,
                commitment: derived.commitment,
                token_commit: derived.token_commit,
                series_asset_id: self.input.asset_id,
                nullifier: crate::model::Nullifier::ZERO,
                merkle_root: dwow_sdk::crypto::MerkleNode::from_base(pallas::Base::zero()),
                user_data_enc: pallas::Base::zero(),
                spend_hook: self.input.spend_hook,
                signature_public: self.input.payment_key,
                last_claim_block: 0,
                maturity_block: 0,
                issuer_contract: dwow_sdk::crypto::ContractId::from_base(pallas::Base::zero()),
            },
        };
        Ok(PayInterestCallPlan {
            blind_output_zkbin: self.blind_output_zkbin,
            blind_output_pk: self.blind_output_pk,
            part: PayInterestPart { input: self.input, value_blind, asset_id_blind },
            params,
        })
    }

    /// Build the call and prove it in one step, with the commitment the input carries.
    pub fn build(self) -> Result<PayInterestCallDebris> {
        let (c, n) = (self.input.tx_commitment, self.input.tx_nonce);
        self.prepare()?.prove(c, n)
    }
}

/// A pay-interest call whose data is assembled and whose proof is not yet made (`OBL-C198`).
pub struct PayInterestCallPlan {
    blind_output_zkbin: ZkBinary,
    blind_output_pk: ProvingKey,
    part: PayInterestPart,
    params: PayInterestParamsV1,
}
struct PayInterestPart {
    input: PayInterestCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
}

impl PayInterestCallPlan {
    /// The contract's call parameters.
    pub fn params(&self) -> PayInterestParamsV1 { self.params.clone() }

    /// Prove the call, binding to `tx_commitment`.
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<PayInterestCallDebris> {
        self.part.input.tx_commitment = tx_commitment;
        self.part.input.tx_nonce = tx_nonce;
        let (proof, _revealed) = create_pay_interest_proof(
            &self.blind_output_zkbin,
            &self.blind_output_pk,
            &self.part.input,
            self.part.value_blind,
            self.part.asset_id_blind,
        )?;
        Ok(PayInterestCallDebris { params: self.params, proofs: vec![proof] })
    }
}

/// The commitment-independent values a `BlindOutput_V1` pay-interest proof reveals (`OBL-C198`).
pub struct PayInterestDerived {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

/// Derive, do not prove — see `PayInterestDerived`.
pub fn derive_pay_interest(
    input: &PayInterestCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
) -> PayInterestDerived {
    let attrs = CommitmentAttributes {
        public_key: input.payment_key,
        value: input.interest_amount,
        asset_id: input.asset_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.commitment_blind,
        maturity_block: 0, // Payment commitments don't have maturity
    };
    PayInterestDerived {
        commitment: attrs.to_commitment(),
        value_commit: pedersen_commitment_u64(input.interest_amount, value_blind),
        token_commit: poseidon_hash([pallas::Base::from(2), input.asset_id, asset_id_blind.inner()]),
    }
}

/// Create a BlindOutput_V1 proof for the payment commitment.
///
/// The issuer creates this proof — NOT the holder. Each payment uses a
/// fresh random `commitment_blind` and `value_blind`, making payment addresses
/// unlinkable across claims.
///
/// Witness order must match BlindOutput_V1 circuit:
/// coin_public, coin_value, coin_asset_id, coin_spend_hook,
/// coin_user_data, commitment_blind, value_blind, asset_id_blind
fn create_pay_interest_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PayInterestCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
) -> Result<(Proof, PayInterestRevealed)> {
    let derived = derive_pay_interest(input, value_blind.clone(), asset_id_blind.clone());

    let public_inputs = PayInterestRevealed {
        commitment: derived.commitment,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        spend_hook: input.spend_hook,
        tx_binding: poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING, input.tx_commitment, input.tx_nonce]),
        tx_nonce: input.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.payment_key)),
        Witness::Base(Value::known(pallas::Base::from(input.interest_amount))),
        Witness::Base(Value::known(input.asset_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.commitment_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING, input.tx_commitment, input.tx_nonce]))), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}
