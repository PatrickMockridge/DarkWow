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

//! Transfer mint proof generation for NativeToken
//!
//! This module provides ZK proof creation for mint operations (creating new coins).

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, BaseBlind, ScalarBlind,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use super::TransferCallOutput;
use crate::model::{Coin, CoinAttributes};

/// Public inputs revealed after proof creation
pub struct TransferMintRevealed {
    /// The coin commitment
    pub coin: Coin,
    /// Pedersen commitment of the value
    pub value_commit: pallas::Point,
    /// Token commitment
    pub token_commit: pallas::Base,
}

impl TransferMintRevealed {
    /// Convert to vector of base field elements (public inputs for ZK circuit)
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![self.coin.inner(), *valcom_coords.x(), *valcom_coords.y(), self.token_commit]
    }
}

/// Create a ZK proof for minting (creating) a new coin.
///
/// This is used by both PoWRewardV1 and TransferV1 for creating output coins.
#[allow(clippy::too_many_arguments)]
pub fn create_transfer_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    coin_blind: BaseBlind,
) -> Result<(Proof, TransferMintRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let token_commit = poseidon_hash([output.token_id, token_blind.inner()]);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin_attrs = CoinAttributes {
        public_key: output.public_key,
        value: output.value,
        token_id: output.token_id,
        spend_hook,
        user_data,
        blind: coin_blind.inner(),
    };
    debug!(target: "contract::native_token::client::transfer::proof", "Created coin: {coin_attrs:?}");
    let coin = coin_attrs.to_coin();

    let public_inputs = TransferMintRevealed { coin, value_commit, token_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}