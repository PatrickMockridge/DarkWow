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

//! Bearer Bond BurnStakeV1 Client API
//!
//! Issuer retires a staking pool by burning remaining stake commitments.
//! Uses Burn_V1 proofs to prove ownership and prevent double-spends.
//! No outputs are created — the commitments are destroyed.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, ScalarBlind, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BondInput, BurnStakeParamsV1, CommitmentAttributes, Nullifier};
use super::point_coords;

/// Public inputs revealed after Burn_V1 proof.
/// Order must match Burn_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct BurnStakeRevealed {
    /// `OBL-C199`: the note commitment is the **first** instance, as it is in `BlindOutput_V2`.
    pub commitment: pallas::Base,
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl BurnStakeRevealed {
    pub fn to_vec(&self) -> Result<Vec<pallas::Base>> {
        let (vc_x, vc_y) = point_coords(self.value_commit)?;
        Ok(vec![
            self.commitment,
            self.nullifier.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.spend_hook,
            self.signature_public,
            self.tx_binding,
            self.tx_nonce,
        ])
    }
}

/// Input for burning a stake commitment (retiring the staking pool).
pub struct BurnStakeCallInput {
    /// Principal value staked
    pub principal: u64,
    /// Token ID of the staking pool series
    pub asset_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Commitment blinding factor
    pub commitment_blind: pallas::Base,
    /// Block height when stake matures (ZK-committed, must match commitment)
    pub maturity_block: u64,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key
    pub secret: pallas::Base,
    // The signature secret is **derived**, not an input: see `BurnStakeDerived::signature_secret`.
    // A caller-supplied field stood here and was passed straight to the witness, which `burn.zk`
    // forbids (`constrain_equal_base(derived_signature_secret, signature_secret)`) — so the field
    // let a caller set a value the circuit would reject, and removing it is what makes the API say
    // what the circuit enforces.
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Debris produced by building a BurnStake call.
pub struct BurnStakeCallDebris {
    /// The contract call parameters
    pub params: BurnStakeParamsV1,
    /// The ZK proofs (one Burn_V1 proof per input)
    pub proofs: Vec<Proof>,
}

/// Builder for `BearerBond::BurnStakeV1` contract call.
pub struct BurnStakeCallBuilder {
    /// Stake commitments to retire
    pub inputs: Vec<BurnStakeCallInput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for Burn_V1
    pub burn_pk: ProvingKey,
}

impl BurnStakeCallBuilder {
    /// Build the BurnStake call debris.
    /// `OBL-C198`: draw each input's blinds, derive, and assemble the call's data — stopping
    /// **before** the proofs. The commitment covers the finished call data, so the call must exist
    /// first; `prove` is the second half.
    pub fn prepare(self) -> Result<BurnStakeCallPlan> {
        debug!(target: "contract::bearer_bond::client::burn_stake", "Preparing BearerBond::BurnStakeV1 contract call");

        if self.inputs.is_empty() {
            return Err(dwow_sdk::error::ContractError::Custom(
                crate::error::BearerBondError::MissingInputs.code(),
            )
            .into());
        }

        let mut parts = vec![];
        let mut inputs = vec![];

        for input in self.inputs.into_iter() {
            let value_blind = ScalarBlind::random(&mut OsRng);
            let asset_id_blind = BaseBlind::random(&mut OsRng);
            let user_data_blind = BaseBlind::random(&mut OsRng);
            let derived = derive_burn_stake(&input, value_blind.clone(), asset_id_blind.clone(), user_data_blind.clone());

            inputs.push(BondInput {
                value_commit: derived.value_commit,
                token_commit: derived.token_commit,
                nullifier: derived.nullifier,
                merkle_root: derived.merkle_root,
                user_data_enc: derived.user_data_enc,
                spend_hook: input.spend_hook,
                signature_public: derived.signature_public,
                commitment: derived.commitment,
            });
            parts.push(BurnStakePart { input, value_blind, asset_id_blind, user_data_blind });
        }

        Ok(BurnStakeCallPlan { burn_zkbin: self.burn_zkbin, burn_pk: self.burn_pk, parts, inputs })
    }

    /// Build the calls and prove them in one step, with the commitment the inputs carry.
    pub fn build(self) -> Result<BurnStakeCallDebris> {
        let (c, n) = self
            .inputs
            .first()
            .map(|i| (i.tx_commitment, i.tx_nonce))
            .unwrap_or((pallas::Base::zero(), pallas::Base::zero()));
        self.prepare()?.prove(c, n)
    }
}

/// A Burn call whose data is assembled and whose proofs are not yet made (`OBL-C198`).
pub struct BurnStakeCallPlan {
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    parts: Vec<BurnStakePart>,
    inputs: Vec<BondInput>,
}
struct BurnStakePart {
    input: BurnStakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
}

impl BurnStakeCallPlan {
    /// The contract's call parameters.
    pub fn params(&self) -> BurnStakeParamsV1 {
        BurnStakeParamsV1 { inputs: self.inputs.clone() }
    }

    /// Prove every input, binding each to `tx_commitment`.
    pub fn prove(self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<BurnStakeCallDebris> {
        let params = self.params();
        let mut proofs = vec![];
        for part in self.parts.into_iter() {
            let mut input = part.input;
            input.tx_commitment = tx_commitment;
            input.tx_nonce = tx_nonce;
            let (proof, _revealed) = create_burn_stake_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &input,
                part.value_blind,
                part.asset_id_blind,
                part.user_data_blind,
            )?;
            proofs.push(proof);
        }
        Ok(BurnStakeCallDebris { params, proofs })
    }
}

/// The commitment-independent values a `Burn_V1` proof reveals (`OBL-C198`: no tx pair — it is
/// bound at prove time from the transaction commitment). The single derivation the proof fn uses.
pub struct BurnStakeDerived {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub signature_public: pallas::Base,
    /// The note commitment — `Burn_V2`'s `coin`, recomputed in-circuit and instanced, and the
    /// commitment-set key the exec looks up. See `BondInput::commitment`.
    pub commitment: pallas::Base,
    /// The per-burn signature secret, **derived** rather than chosen.
    ///
    /// `burn.zk` constrains `poseidon_hash(DOMAIN_SIGNATURE_SECRET, spend_secret, nullifier) ==
    /// signature_secret`, so the witness is not free: a caller-supplied value makes the circuit
    /// unsatisfiable, and an unsatisfied circuit still emits proof bytes — which is why this
    /// surfaced as `invalid proof: call[0] namespace 'Burn_V2'` at the node rather than as a prover
    /// error. That constraint is deliberate (HAZOP H2/H26: without it a prover signs with an
    /// arbitrary secret unbound to the coin), so the caller's `ephemeral_signature_secret` was never
    /// the caller's to pick; the client derives it from the same secret the nullifier is built from.
    pub signature_secret: pallas::Base,
}

/// Derive, do not prove — see `BurnStakeDerived`.
pub fn derive_burn_stake(
    input: &BurnStakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> BurnStakeDerived {
    let public_key = poseidon_hash([pallas::Base::from(7), input.secret]);
    let commitment = CommitmentAttributes {
        public_key,
        value: input.principal,
        asset_id: input.asset_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.commitment_blind,
        maturity_block: input.maturity_block,
    }
    .to_commitment();
    let nullifier = Nullifier::new(SecretKey::from_base(input.secret), commitment);
    // `burn.zk` binds this to (`spend_secret`, `nullifier`) — see `BurnStakeDerived`.
    let signature_secret =
        poseidon_hash([pallas::Base::from(7), input.secret, nullifier.inner()]);
    let merkle_root = {
        let position: u64 = input.leaf_position;
        let mut current = MerkleNode::from_base(commitment);
        for (level, sibling) in input.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };
    BurnStakeDerived {
        nullifier,
        value_commit: pedersen_commitment_u64(input.principal, value_blind),
        token_commit: poseidon_hash([pallas::Base::from(2), input.asset_id, asset_id_blind.inner()]),
        merkle_root,
        user_data_enc: poseidon_hash([pallas::Base::from(6), input.user_data, user_data_blind.inner()]),
        signature_public: poseidon_hash([pallas::Base::from(7), signature_secret]),
        commitment,
        signature_secret,
    }
}

/// Create a Burn_V1 proof for retiring a stake commitment.
///
/// Witness order must match Burn_V1 circuit:
/// secret, value, asset_id, spend_hook, user_data, commitment_blind,
/// value_blind, asset_id_blind, user_data_blind, leaf_position,
/// merkle_path, ephemeral_signature_secret
fn create_burn_stake_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &BurnStakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, BurnStakeRevealed)> {
    let derived = derive_burn_stake(input, value_blind.clone(), asset_id_blind.clone(), user_data_blind.clone());

    let public_inputs = BurnStakeRevealed {
        commitment: derived.commitment,
        nullifier: derived.nullifier,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        merkle_root: derived.merkle_root,
        user_data_enc: derived.user_data_enc,
        spend_hook: input.spend_hook,
        signature_public: derived.signature_public,
        tx_binding: poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING, input.tx_commitment, input.tx_nonce]),
        tx_nonce: input.tx_nonce,
    };

    #[expect(clippy::unwrap_used, reason = "guarded by tree structure")]
    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret)),
        Witness::Base(Value::known(pallas::Base::from(input.principal))),
        Witness::Base(Value::known(input.asset_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.commitment_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(
            u64::from(input.leaf_position).try_into().unwrap(),
        )),
        Witness::MerklePath(Value::known(
            input.merkle_path.clone().try_into().unwrap(),
        )),
        // Derived, not `input.ephemeral_signature_secret` — see `BurnStakeDerived`.
        Witness::Base(Value::known(derived.signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING, input.tx_commitment, input.tx_nonce]))), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}
