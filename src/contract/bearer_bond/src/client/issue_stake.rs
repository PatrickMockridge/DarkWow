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
//! Creates a new staking pool and mints the initial stake commitment. The issuer
//! provides capital and sets terms (maturity, asset_id). The initial stake
//! commitment is minted to the staker via a BlindOutput_V1 proof.

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

use crate::model::{BondCommitment, CommitmentAttributes, IssueStakeParamsV1};
use super::point_coords;

/// Public inputs revealed after BlindOutput_V1 proof for initial stake commitment.
/// Order must match BlindOutput_V1 circuit:
/// commitment, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct IssueStakeRevealed {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl IssueStakeRevealed {
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
    pub asset_id: pallas::Base,
    /// Staker's address (poseidon_hash of public key)
    pub staker: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Commitment blinding factor
    pub commitment_blind: pallas::Base,
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
    /// Input for the initial stake commitment
    pub input: IssueStakeCallInput,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub blind_output_zkbin: ZkBinary,
    /// Proving key for BlindOutput_V1
    pub blind_output_pk: ProvingKey,
}

impl IssueStakeCallBuilder {
    /// `OBL-C198`: draw the blinds, derive, and assemble the call's data — stopping **before** the
    /// proof. The commitment covers the finished call data, so the call must exist first; `prove` is
    /// the second half.
    pub fn prepare(self) -> Result<IssueStakeCallPlan> {
        debug!(target: "contract::bearer_bond::client::issue_stake", "Preparing BearerBond::IssueStakeV1 contract call");

        let value_blind = super::draw_scalar_blind();
        let asset_id_blind = super::draw_base_blind();
        let derived = derive_issue_stake(&self.input, value_blind.clone(), asset_id_blind.clone());

        let params = IssueStakeParamsV1 {
            min_claim: self.input.min_claim,
            issuer_contract: self.input.issuer_contract,
            asset_id: self.input.asset_id,
            commitment: BondCommitment {
                value_commit: derived.value_commit,
                commitment: derived.commitment,
                token_commit: derived.token_commit,
                series_asset_id: self.input.asset_id,
                nullifier: crate::model::Nullifier::ZERO,
                merkle_root: MerkleNode::from_base(pallas::Base::zero()),
                user_data_enc: pallas::Base::zero(),
                spend_hook: self.input.spend_hook,
                signature_public: self.input.staker,
                last_claim_block: 0,
                maturity_block: self.input.maturity_block,
                issuer_contract: self.input.issuer_contract,
            },
        };
        Ok(IssueStakeCallPlan {
            blind_output_zkbin: self.blind_output_zkbin,
            blind_output_pk: self.blind_output_pk,
            part: IssueStakePart { input: self.input, value_blind, asset_id_blind },
            params,
        })
    }

    /// Build the call and prove it in one step, with the commitment the input carries.
    pub fn build(self) -> Result<IssueStakeCallDebris> {
        let (c, n) = (self.input.tx_commitment, self.input.tx_nonce);
        self.prepare()?.prove(c, n)
    }
}

/// A `BlindOutput_V1` issue-stake call whose data is assembled and whose proof is not yet made.
pub struct IssueStakeCallPlan {
    blind_output_zkbin: ZkBinary,
    blind_output_pk: ProvingKey,
    part: IssueStakePart,
    params: IssueStakeParamsV1,
}
struct IssueStakePart {
    input: IssueStakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
}

impl IssueStakeCallPlan {
    /// The contract's call parameters.
    pub fn params(&self) -> IssueStakeParamsV1 { self.params.clone() }

    /// Prove the call, binding to `tx_commitment`.
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<IssueStakeCallDebris> {
        self.part.input.tx_commitment = tx_commitment;
        self.part.input.tx_nonce = tx_nonce;
        let (proof, _revealed) = create_issue_stake_proof(
            &self.blind_output_zkbin,
            &self.blind_output_pk,
            &self.part.input,
            self.part.value_blind,
            self.part.asset_id_blind,
        )?;
        Ok(IssueStakeCallDebris { params: self.params, proofs: vec![proof] })
    }
}

/// Create a BlindOutput_V1 proof for the initial stake commitment.
///
/// Witness order must match BlindOutput_V1 circuit:
/// coin_public, coin_value, coin_asset_id, coin_spend_hook,
/// coin_user_data, commitment_blind, value_blind, asset_id_blind
/// The commitment-independent values a `BlindOutput_V1` proof reveals. `OBL-C198`: the tx pair is
/// **not** here — it is bound at prove time from the transaction commitment, so `prepare` can build
/// the call data without it. This is the single derivation `create_issue_stake_proof` also uses
/// (`safety.md` RC5).
pub struct IssueStakeDerived {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

/// Derive, do not prove — see `IssueStakeDerived`.
pub fn derive_issue_stake(
    input: &IssueStakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
) -> IssueStakeDerived {
    let attrs = CommitmentAttributes {
        public_key: input.staker,
        value: input.principal,
        asset_id: input.asset_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.commitment_blind,
        maturity_block: input.maturity_block,
    };
    IssueStakeDerived {
        commitment: attrs.to_commitment(),
        value_commit: pedersen_commitment_u64(input.principal, value_blind),
        token_commit: poseidon_hash([pallas::Base::from(2), input.asset_id, asset_id_blind.inner()]),
    }
}

pub fn create_issue_stake_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &IssueStakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
) -> Result<(Proof, IssueStakeRevealed)> {
    let derived = derive_issue_stake(input, value_blind.clone(), asset_id_blind.clone());
    let commitment = derived.commitment;
    let value_commit = derived.value_commit;
    let token_commit = derived.token_commit;

    let public_inputs = IssueStakeRevealed {
        commitment,
        value_commit,
        token_commit,
        spend_hook: input.spend_hook,
        tx_binding: poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING, input.tx_commitment, input.tx_nonce]),
        tx_nonce: input.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.staker)),
        Witness::Base(Value::known(pallas::Base::from(input.principal))),
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
