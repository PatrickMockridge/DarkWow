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
//! Withdraw principal + unclaimed profits at maturity. Burns the stake coin
//! (Burn_V1 proof) and creates a zero-value receipt coin (Redeem_V1 proof).
//!
//! The receipt coin serves as cryptographic proof that unstaking occurred —
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

use crate::model::{BondInput, CoinAttributes, Nullifier, UnstakeParamsV1};
use super::point_coords;

// ============================================================================
// REVEALED PUBLIC INPUTS
// ============================================================================

/// Public inputs revealed after Burn_V1 proof (unstake input side).
/// Order must match Burn_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct UnstakeBurnRevealed {
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
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![
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
        ]
    }
}

/// Public inputs revealed after Redeem_V1 receipt proof.
/// Order must match Redeem_V1 circuit:
/// coin, value_commit_x, value_commit_y, token_commit, coin_value, spend_hook
pub struct UnstakeReceiptRevealed {
    pub coin: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub coin_value: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UnstakeReceiptRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![
            self.coin,
            vc_x,
            vc_y,
            self.token_commit,
            self.coin_value,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

// ============================================================================
// BUILDER INPUTS
// ============================================================================

/// Input for unstaking a coin.
pub struct UnstakeCallInput {
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
    /// Block height when stake matures (ZK-committed)
    pub maturity_block: u64,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key
    pub secret: pallas::Base,
    /// Ephemeral signature secret — MUST be fresh per transaction
    pub ephemeral_signature_secret: pallas::Base,
    /// Current block height (for maturity verification)
    pub current_block: u64,
    /// Total payout = principal + unclaimed interest
    pub payout: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output for the receipt coin.
pub struct UnstakeCallOutput {
    /// Redeemer's address (poseidon_hash of public key)
    pub recipient: pallas::Base,
    /// Token ID (same as unstaked coin)
    pub token_id: pallas::Base,
    /// Spend hook (issuer contract — makes receipt non-transferable)
    pub spend_hook: pallas::Base,
    /// User data (unstaking metadata)
    pub user_data: pallas::Base,
    /// Coin blinding factor (fresh random)
    pub coin_blind: pallas::Base,
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
    /// Stake coin being unstaked
    pub input: UnstakeCallInput,
    /// Receipt coin output
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
    /// Build the Unstake call debris.
    pub fn build(self) -> Result<UnstakeCallDebris> {
        debug!(target: "contract::bearer_bond::client::unstake", "Building BearerBond::UnstakeV1 contract call");

        let mut proofs = vec![];

        // Build Burn_V1 proof for the input stake coin
        let value_blind = ScalarBlind::random(&mut OsRng);
        let token_id_blind = BaseBlind::random(&mut OsRng);
        let user_data_blind = BaseBlind::random(&mut OsRng);

        let (burn_proof, burn_revealed) = create_unstake_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &self.input,
            value_blind.clone(),
            token_id_blind.clone(),
            user_data_blind.clone(),
        )?;

        proofs.push(burn_proof);

        let bond_input = BondInput {
            value_commit: burn_revealed.value_commit,
            token_commit: burn_revealed.token_commit,
            nullifier: burn_revealed.nullifier,
            merkle_root: burn_revealed.merkle_root,
            user_data_enc: burn_revealed.user_data_enc,
            spend_hook: self.input.spend_hook,
            signature_public: burn_revealed.signature_public,
        };

        // Build Redeem_V1 proof for the zero-value receipt coin
        let receipt_value_blind = ScalarBlind::random(&mut OsRng);
        let receipt_token_id_blind = BaseBlind::random(&mut OsRng);

        let (receipt_proof, _receipt_revealed) = create_unstake_receipt_proof(
            &self.redeem_zkbin,
            &self.redeem_pk,
            &self.output,
            receipt_value_blind,
            receipt_token_id_blind,
        )?;

        proofs.push(receipt_proof);

        Ok(UnstakeCallDebris {
            params: UnstakeParamsV1 {
                bond_input,
                current_block: self.input.current_block,
            },
            proofs,
        })
    }
}

// ============================================================================
// PROOF CREATION
// ============================================================================

/// Create a Burn_V1 proof for unstaking a coin.
///
/// Witness order must match Burn_V1 circuit:
/// secret, value, token_id, spend_hook, user_data, coin_blind,
/// value_blind, token_id_blind, user_data_blind, leaf_position,
/// merkle_path, ephemeral_signature_secret
fn create_unstake_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UnstakeCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, UnstakeBurnRevealed)> {
    let public_key = poseidon_hash([pallas::Base::from(7), input.secret]);

    let coin = CoinAttributes {
        public_key,
        value: input.principal,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
        maturity_block: input.maturity_block,
    }
    .to_coin();

    let nullifier = Nullifier::new(SecretKey::from_base(input.secret), coin);

    let merkle_root = {
        let position: u64 = input.leaf_position;
        let mut current = MerkleNode::from_base(coin);
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

    let value_commit = pedersen_commitment_u64(input.principal, value_blind.clone());
    let token_commit = poseidon_hash([pallas::Base::from(2), input.token_id, token_id_blind.inner()]);
    let user_data_enc = poseidon_hash([pallas::Base::from(6), input.user_data, user_data_blind.inner()]);
    let signature_public = poseidon_hash([pallas::Base::from(7), input.ephemeral_signature_secret]);

    let public_inputs = UnstakeBurnRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding: pallas::Base::zero(),
        tx_nonce: input.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret)),
        Witness::Base(Value::known(pallas::Base::from(input.principal))),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(
            u64::from(input.leaf_position).try_into().unwrap(),
        )),
        Witness::MerklePath(Value::known(
            input.merkle_path.clone().try_into().unwrap(),
        )),
        Witness::Base(Value::known(input.ephemeral_signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Create a Redeem_V1 proof for the zero-value receipt coin.
///
/// Witness order must match Redeem_V1 circuit:
/// coin_public, coin_value, coin_token_id, coin_spend_hook,
/// coin_user_data, coin_blind, value_blind, token_id_blind
///
/// coin_value = 0 proves the receipt has no monetary value.
fn create_unstake_receipt_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &UnstakeCallOutput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
) -> Result<(Proof, UnstakeReceiptRevealed)> {
    let coin_value = pallas::Base::zero();
    let attrs = CoinAttributes {
        public_key: output.recipient,
        value: 0,
        token_id: output.token_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        blind: output.coin_blind,
        maturity_block: 0,
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(0, value_blind.clone());
    let token_commit = poseidon_hash([pallas::Base::from(2), output.token_id, token_id_blind.inner()]);

    let public_inputs = UnstakeReceiptRevealed {
        coin,
        value_commit,
        token_commit,
        coin_value,
        spend_hook: output.spend_hook,
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(coin_value)),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_commitment
        Witness::Base(Value::known(pallas::Base::zero())), // tx_nonce
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
