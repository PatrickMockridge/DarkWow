/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! NativeToken PoWRewardV1 Client API
//!
//! This module provides the ability to build PoW reward calls for block rewards.

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    blockchain::expected_reward,
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, Blind, Keypair, PublicKey,
    },
    pasta::pallas,
};
use darkfi_serial::serialize;
use rand::rngs::OsRng;
use tracing::debug;

use super::{transfer_v1::proof::create_transfer_mint_proof, NativeNote};
use crate::model::{ClearInput, Coin, CoinAttributes, DARK_TOKEN_ID, Output, PoWRewardParamsV1};

/// Debris produced by building a PoWReward call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct PoWRewardCallDebris {
    /// The contract call parameters
    pub params: PoWRewardParamsV1,
    /// The ZK proofs for the mint operation
    pub proofs: Vec<Proof>,
}

/// Public inputs revealed after proof creation
pub struct PoWRewardRevealed {
    /// The coin created
    pub coin: Coin,
    /// Pedersen commitment of the value
    pub value_commit: pallas::Point,
    /// Token commitment
    pub token_commit: pallas::Base,
}

impl PoWRewardRevealed {
    /// Convert to vector of base field elements (public inputs for ZK circuit)
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![self.coin.inner(), *valcom_coords.x(), *valcom_coords.y(), self.token_commit]
    }
}

/// Builder for creating PoWRewardV1 contract calls.
///
/// This is used to claim block rewards after successfully mining a block.
pub struct PoWRewardCallBuilder {
    /// Caller's keypair, corresponding to the one used in the signature
    pub signature_keypair: Keypair,
    /// Rewarded block height
    pub block_height: u32,
    /// Rewarded block transactions paid fees
    pub fees: u64,
    /// Optional recipient's public key, in case we want to mint to a different address
    pub recipient: Option<PublicKey>,
    /// Optional contract spend hook to use in the output (as pallas::Base)
    pub spend_hook: Option<pallas::Base>,
    /// Optional user data to use in the output
    pub user_data: Option<pallas::Base>,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl PoWRewardCallBuilder {
    /// Build the PoWReward call debris
    fn _build(&self, value: u64) -> Result<PoWRewardCallDebris> {
        debug!(target: "contract::native_token::client::pow_reward", "Building NativeToken::PoWRewardV1 contract call");

        // In this call, we will build one clear input and one anonymous output.
        // Only DARK_TOKEN_ID can be minted as PoW reward.
        let token_id = DARK_TOKEN_ID;

        // Building the clear input using random blinds
        let value_blind = Blind::random(&mut OsRng);
        let token_blind = Blind::random(&mut OsRng);
        let coin_blind = Blind::random(&mut OsRng);
        let c_input = ClearInput {
            value,
            token_id,
            value_blind,
            token_blind: token_blind.inner(),
            signature_public: self.signature_keypair.public,
        };

        // Grab the spend hook and user data to use in the output
        let spend_hook = self.spend_hook.unwrap_or(pallas::Base::ZERO);
        let user_data = self.user_data.unwrap_or(pallas::Base::ZERO);

        // Building the anonymous output using CoinAttributes (TransferCallOutput)
        let output = CoinAttributes {
            public_key: self.recipient.unwrap_or(self.signature_keypair.public),
            value,
            token_id,
            spend_hook,
            user_data,
            blind: coin_blind.inner(),
        };

        debug!(target: "contract::native_token::client::pow_reward", "Creating token mint proof for output");
        let (proof, public_inputs) = create_transfer_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            value_blind,
            token_blind,
            spend_hook,
            user_data,
            coin_blind,
        )?;

        let note = NativeNote {
            value: output.value,
            token_id: output.token_id,
            spend_hook,
            user_data,
            coin_blind: coin_blind.inner(),
            value_blind: value_blind.inner(),
            token_blind: token_blind.inner(),
            memo: serialize(&self.signature_keypair.secret),
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let c_output = Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        let params = PoWRewardParamsV1 { input: c_input, output: c_output };
        let debris = PoWRewardCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }

    /// Build the PoWReward call with the standard block reward plus fees.
    pub fn build(&self) -> Result<PoWRewardCallDebris> {
        let reward = expected_reward(self.block_height) + self.fees;
        self._build(reward)
    }

    /// Build with a custom reward value (for testing purposes only).
    /// In production, the reward should come from expected_reward().
    pub fn build_with_custom_reward(&self, reward: u64) -> Result<PoWRewardCallDebris> {
        self._build(reward + self.fees)
    }
}