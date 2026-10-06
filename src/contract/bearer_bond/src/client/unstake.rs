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

//! Bearer Bond UnstakeV1 Client API
//!
//! Withdraw principal + unclaimed profits at maturity. Burns the stake commitment
//! (Burn_V1 proof) and creates a zero-value receipt commitment (Redeem_V1 proof).
//!
//! The receipt commitment serves as cryptographic proof that unstaking occurred —
//! non-transferable (spend_hook = issuer contract), zero monetary value.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BondInput, CommitmentAttributes, Nullifier, UnstakeParamsV1};
use super::point_coords;

// ============================================================================
// REVEALED PUBLIC INPUTS
// ============================================================================

/// Public inputs revealed after Burn_V1 proof (unstake input side).
/// Order must match Burn_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct UnstakeBurnRevealed {
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

impl UnstakeBurnRevealed {
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

/// Public inputs revealed after Redeem_V2 receipt proof.
/// Order must match Redeem_V2 circuit:
/// commitment, value_commit_x, value_commit_y, token_commit, value, spend_hook,
/// tx_binding, tx_nonce
pub struct UnstakeReceiptRevealed {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub value: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UnstakeReceiptRevealed {
    pub fn to_vec(&self) -> Result<Vec<pallas::Base>> {
        let (vc_x, vc_y) = point_coords(self.value_commit)?;
        // Redeem_V2's order, and the one the metadata pushes (`OBL-C198`): the tx pair is the last
        // two instances, *after* the hook. This was a three-way disagreement once — the circuit,
        // the metadata and the client each differed (OBL-Z15); the pair-last move re-aligned all
        // three on the circuit's order.
        Ok(vec![
            self.commitment,
            vc_x,
            vc_y,
            self.token_commit,
            self.value,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ])
    }
}

// ============================================================================
// BUILDER INPUTS
// ============================================================================

/// Input for unstaking a commitment.
pub struct UnstakeCallInput {
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
    /// Block height when stake matures (ZK-committed)
    pub maturity_block: u64,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key
    pub secret: pallas::Base,
    // Derived, not an input — see `BurnStakeDerived::signature_secret`.
    /// Current block height (for maturity verification)
    pub current_block: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output for the receipt commitment.
pub struct UnstakeCallOutput {
    /// Redeemer's address (poseidon_hash of public key)
    pub recipient: pallas::Base,
    /// Token ID (same as unstaked commitment)
    pub asset_id: pallas::Base,
    /// Spend hook (issuer contract — makes receipt non-transferable)
    pub spend_hook: pallas::Base,
    /// User data (unstaking metadata)
    pub user_data: pallas::Base,
    /// Commitment blinding factor (fresh random)
    pub commitment_blind: pallas::Base,
}

// ============================================================================
// DEBRIS
// ============================================================================

/// Debris produced by building an Unstake call.
pub struct UnstakeCallDebris {
    /// The contract call parameters
    pub params: UnstakeParamsV1,
    /// The ZK proofs (burn proof first, then receipt proof)
    pub proofs: Vec<Proof>,
}

// ============================================================================
// BUILDER
// ============================================================================

/// Builder for `BearerBond::UnstakeV1` contract call.
pub struct UnstakeCallBuilder {
    /// Stake commitment being unstaked
    pub input: UnstakeCallInput,
    /// Receipt commitment output
    pub output: UnstakeCallOutput,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for Burn_V1
    pub burn_pk: ProvingKey,
    /// `Redeem_V1` zkas circuit ZkBinary
    pub redeem_zkbin: ZkBinary,
    /// Proving key for Redeem_V1
    pub redeem_pk: ProvingKey,
}

impl UnstakeCallBuilder {
    /// `OBL-C198`: draw the blinds, derive, and assemble the call's data — stopping **before** the
    /// proofs. The commitment covers the finished call data, so the call must exist first; `prove`
    /// is the second half.
    pub fn prepare(self) -> Result<UnstakeCallPlan> {
        debug!(target: "contract::bearer_bond::client::unstake", "Preparing BearerBond::UnstakeV1 contract call");

        let value_blind = super::draw_scalar_blind();
        let asset_id_blind = super::draw_base_blind();
        let user_data_blind = super::draw_base_blind();
        let burn_derived = derive_unstake_burn(
            &self.input,
            value_blind.clone(),
            asset_id_blind.clone(),
            user_data_blind.clone(),
        );

        let bond_input = BondInput {
            value_commit: burn_derived.value_commit,
            token_commit: burn_derived.token_commit,
            nullifier: burn_derived.nullifier,
            merkle_root: burn_derived.merkle_root,
            user_data_enc: burn_derived.user_data_enc,
            spend_hook: self.input.spend_hook,
            signature_public: burn_derived.signature_public,
            commitment: burn_derived.commitment,
        };

        let receipt_value_blind = super::draw_scalar_blind();
        let receipt_asset_id_blind = super::draw_base_blind();
        let receipt_derived = derive_unstake_receipt(
            &self.output,
            receipt_value_blind.clone(),
            receipt_asset_id_blind.clone(),
        );

        let (receipt_vc_x, receipt_vc_y) = super::point_coords(receipt_derived.value_commit)?;

        let params = UnstakeParamsV1 {
            bond_input,
            current_block: self.input.current_block,
            // The receipt's note commitment, a proof-independent derivation over the output
            // (OBL-Z15) — the value the metadata pushes where `Redeem_V2` exposes `coin`.
            receipt_commitment: receipt_derived.commitment,
            // `OBL-C199`: the receipt is a note of its own, so these are its values — the arm
            // published the stake's, which are different, and the proof failed verification.
            receipt_token_commit: receipt_derived.token_commit,
            receipt_spend_hook: self.output.spend_hook,
            // The receipt's own value commitment — the arm published the stake's, which is the
            // same mistake one instance further along.
            receipt_value_commit_x: receipt_vc_x,
            receipt_value_commit_y: receipt_vc_y,
        };
        Ok(UnstakeCallPlan {
            burn_zkbin: self.burn_zkbin,
            burn_pk: self.burn_pk,
            redeem_zkbin: self.redeem_zkbin,
            redeem_pk: self.redeem_pk,
            input: self.input,
            output: self.output,
            value_blind,
            asset_id_blind,
            user_data_blind,
            receipt_value_blind,
            receipt_asset_id_blind,
            params,
        })
    }

    /// Build the call and prove it in one step, with the commitment the input carries.
    pub fn build(self) -> Result<UnstakeCallDebris> {
        let (c, n) = (self.input.tx_commitment, self.input.tx_nonce);
        self.prepare()?.prove(c, n)
    }
}

/// An unstake call whose data is assembled and whose proofs are not yet made (`OBL-C198`).
pub struct UnstakeCallPlan {
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    redeem_zkbin: ZkBinary,
    redeem_pk: ProvingKey,
    input: UnstakeCallInput,
    output: UnstakeCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
    receipt_value_blind: ScalarBlind,
    receipt_asset_id_blind: BaseBlind,
    params: UnstakeParamsV1,
}

impl UnstakeCallPlan {
    /// The contract's call parameters.
    pub fn params(&self) -> UnstakeParamsV1 { self.params.clone() }

    /// Prove the burn and the receipt, binding each to `tx_commitment`.
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<UnstakeCallDebris> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (burn_proof, _burn_revealed) = create_unstake_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &self.input,
            self.value_blind,
            self.asset_id_blind,
            self.user_data_blind,
        )?;
        let (receipt_proof, _receipt_revealed) = create_unstake_receipt_proof(
            &self.redeem_zkbin,
            &self.redeem_pk,
            &self.output,
            self.receipt_value_blind,
            self.receipt_asset_id_blind,
            tx_commitment,
            tx_nonce,
        )?;
        Ok(UnstakeCallDebris { params: self.params, proofs: vec![burn_proof, receipt_proof] })
    }
}

/// The commitment-independent values an unstake `Burn_V1` proof reveals (`OBL-C198`).
pub struct UnstakeBurnDerived {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub signature_public: pallas::Base,
    /// The note commitment — see `BondInput::commitment`.
    pub commitment: pallas::Base,
    /// Derived, not chosen — `burn.zk` binds it to (`spend_secret`, `nullifier`). See
    /// `BurnStakeDerived::signature_secret`.
    pub signature_secret: pallas::Base,
}

/// Derive, do not prove — see `UnstakeBurnDerived`.
pub fn derive_unstake_burn(
    input: &UnstakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> UnstakeBurnDerived {
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

    UnstakeBurnDerived {
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

/// The commitment-independent values an unstake `Redeem_V2` receipt proof reveals (`OBL-C198`).
pub struct UnstakeReceiptDerived {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

/// Derive, do not prove — see `UnstakeReceiptDerived`.
pub fn derive_unstake_receipt(
    output: &UnstakeCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
) -> UnstakeReceiptDerived {
    let attrs = CommitmentAttributes {
        public_key: output.recipient,
        value: 0,
        asset_id: output.asset_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        blind: output.commitment_blind,
        maturity_block: 0,
    };
    UnstakeReceiptDerived {
        commitment: attrs.to_commitment(),
        value_commit: pedersen_commitment_u64(0, value_blind),
        token_commit: poseidon_hash([pallas::Base::from(2), output.asset_id, asset_id_blind.inner()]),
    }
}

// ============================================================================
// PROOF CREATION
// ============================================================================

/// Create a Burn_V1 proof for unstaking a commitment.
///
/// Witness order must match Burn_V1 circuit:
/// secret, value, asset_id, spend_hook, user_data, commitment_blind,
/// value_blind, asset_id_blind, user_data_blind, leaf_position,
/// merkle_path, ephemeral_signature_secret
fn create_unstake_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UnstakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, UnstakeBurnRevealed)> {
    let derived = derive_unstake_burn(input, value_blind.clone(), asset_id_blind.clone(), user_data_blind.clone());

    let public_inputs = UnstakeBurnRevealed {
        commitment: derived.commitment,
        nullifier: derived.nullifier,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        merkle_root: derived.merkle_root,
        user_data_enc: derived.user_data_enc,
        spend_hook: input.spend_hook,
        signature_public: derived.signature_public,
        tx_binding: poseidon_hash([
            dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING,
            input.tx_commitment,
            input.tx_nonce,
        ]),
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
        // Derived — see `BurnStakeDerived::signature_secret`.
        Witness::Base(Value::known(derived.signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([
            dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING,
            input.tx_commitment,
            input.tx_nonce,
        ]))), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Create a Redeem_V2 proof for the zero-value receipt commitment.
///
/// Witness order must match Redeem_V2 circuit:
/// coin_public, coin_value, coin_asset_id, coin_spend_hook,
/// coin_user_data, commitment_blind, value_blind, asset_id_blind,
/// tx_commitment, tx_nonce, tx_binding
///
/// value = 0 proves the receipt has no monetary value.
fn create_unstake_receipt_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &UnstakeCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, UnstakeReceiptRevealed)> {
    let value = pallas::Base::zero();
    let derived = derive_unstake_receipt(output, value_blind.clone(), asset_id_blind.clone());

    // `OBL-C198`: the pair is bound here, from the transaction commitment the caller set.
    let tx_binding = poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, tx_nonce]);
    let public_inputs = UnstakeReceiptRevealed {
        commitment: derived.commitment,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        value,
        spend_hook: output.spend_hook,
        tx_binding,
        tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(value)),
        Witness::Base(Value::known(output.asset_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.commitment_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
        Witness::Base(Value::known(tx_commitment)), // tx_commitment
        Witness::Base(Value::known(tx_nonce)), // tx_nonce
        Witness::Base(Value::known(tx_binding)), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}
