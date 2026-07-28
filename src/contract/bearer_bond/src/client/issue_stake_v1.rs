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

//! Bearer Bond IssueStakeV1 Client API
//!
//! Creates a new staking pool and mints the initial stake coin. The issuer
//! provides capital and sets terms (maturity, token_id). The initial stake
//! coin is minted to the staker via a BlindOutput_V1 proof.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        pedersen_commitment_u64, poseidon_hash, BaseBlind, ContractId, MerkleNode, ScalarBlind,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BondCoin, CoinAttributes, IssueStakeParamsV1};
use super::point_coords;

/// Public inputs revealed after BlindOutput_V1 proof for initial stake coin.
/// Order must match BlindOutput_V1 circuit:
/// coin, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct IssueStakeRevealed {
    pub coin: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl IssueStakeRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![
            self.coin,
            vc_x,
            vc_y,
            self.token_commit,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input for building an IssueStake call.
pub struct IssueStakeCallInput {
    /// Principal value staked
    pub principal: u64,
    /// Block height when stake matures
    pub maturity_block: u64,
    /// Minimum claim threshold (dust protection)
    pub min_claim: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// Token ID for the staking pool series
    pub token_id: pallas::Base,
    /// Staker's address (poseidon_hash of public key)
    pub staker: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blinding factor
    pub coin_blind: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Debris produced by building an IssueStake call.
pub struct IssueStakeCallDebris {
    /// The contract call parameters
    pub params: IssueStakeParamsV1,
    /// The ZK proof
    pub proofs: Vec<Proof>,
}

/// Builder for `BearerBond::IssueStakeV1` contract call.
pub struct IssueStakeCallBuilder {
    /// Input for the initial stake coin
    pub input: IssueStakeCallInput,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub blind_output_zkbin: ZkBinary,
    /// Proving key for BlindOutput_V1
    pub blind_output_pk: ProvingKey,
}

impl IssueStakeCallBuilder {
    /// Build the IssueStake call debris.
    pub fn build(self) -> Result<IssueStakeCallDebris> {
        debug!(target: "contract::bearer_bond::client::issue_stake", "Building BearerBond::IssueStakeV1 contract call");

        let value_blind = ScalarBlind::random(&mut OsRng);
        let token_id_blind = BaseBlind::random(&mut OsRng);

        let (proof, revealed) = create_issue_stake_proof(
            &self.blind_output_zkbin,
            &self.blind_output_pk,
            &self.input,
            value_blind.clone(),
            token_id_blind.clone(),
        )?;

        let coin = BondCoin {
            value_commit: revealed.value_commit,
            token_commit: revealed.token_commit,
            nullifier: crate::model::Nullifier::from_base(pallas::Base::zero()),
            merkle_root: MerkleNode::from_base(pallas::Base::zero()),
            user_data_enc: pallas::Base::zero(),
            spend_hook: self.input.spend_hook,
            signature_public: self.input.staker,
            last_claim_block: 0,
            maturity_block: self.input.maturity_block,
            issuer_contract: self.input.issuer_contract,
        };

        Ok(IssueStakeCallDebris {
            params: IssueStakeParamsV1 {
                min_claim: self.input.min_claim,
                issuer_contract: self.input.issuer_contract,
                token_id: self.input.token_id,
                coin,
            },
            proofs: vec![proof],
        })
    }
}

/// Create a BlindOutput_V1 proof for the initial stake coin.
///
/// Witness order must match BlindOutput_V1 circuit:
/// coin_public, coin_value, coin_token_id, coin_spend_hook,
/// coin_user_data, coin_blind, value_blind, token_id_blind
pub fn create_issue_stake_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &IssueStakeCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
) -> Result<(Proof, IssueStakeRevealed)> {
    let attrs = CoinAttributes {
        public_key: input.staker,
        value: input.principal,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
        maturity_block: input.maturity_block,
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(input.principal, value_blind);
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    let public_inputs = IssueStakeRevealed {
        coin,
        value_commit,
        token_commit,
        spend_hook: input.spend_hook,
        tx_binding: pallas::Base::zero(),
        tx_nonce: input.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.staker)),
        Witness::Base(Value::known(pallas::Base::from(input.principal))),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
