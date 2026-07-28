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

//! Promissory Note TokenMintV1 Client API
//!
//! This module provides the ability to create new token types.
//! Examples: stablecoins (USD, EUR), wrapped tokens (wBTC, wETH), etc.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64, poseidon_hash, Blind, FuncId, ScalarBlind, TokenId,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{Coin, CoinAttributes, TokenMintParamsV1};

/// Public inputs revealed after token mint proof creation
/// Order must match TokenMint_V1 circuit:
/// token_id, token_auth_parent, coin, value_commit_x, value_commit_y, spend_hook
pub struct TokenMintRevealed {
    /// Token ID (derived from auth_parent, user_data, blind)
    pub token_id: pallas::Base,
    /// Token authorization parent (public authority)
    pub token_auth_parent: pallas::Base,
    /// The initial coin commitment
    pub coin: Coin,
    /// The value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Spend hook
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TokenMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = {
            let affine = self.value_commit.to_affine();
            let coords = affine.coordinates().unwrap();
            (*coords.x(), *coords.y())
        };
        vec![
            self.token_id,
            self.token_auth_parent,
            self.coin.inner(),
            vc_x,
            vc_y,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input for building a token mint call
pub struct TokenMintCallInput {
    /// Authority parent - who has permission to create this token
    pub token_auth_parent: pallas::Base,
    /// User data for the token
    pub token_user_data: pallas::Base,
    /// Blinding factor for token ID
    pub token_blind: pallas::Base,
    /// Recipient public key (poseidon_hash of secret)
    pub recipient: pallas::Base,
    /// Initial value to mint
    pub value: u64,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
}

/// Debris produced by building a TokenMint call
pub struct TokenMintCallDebris {
    /// The contract call parameters
    pub params: TokenMintParamsV1,
    /// The ZK proofs for the token mint operation
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `PromissoryNote::TokenMintV1` contract call.
pub struct TokenMintCallBuilder {
    /// The input for the token mint
    pub input: TokenMintCallInput,
    /// `TokenMint_V1` zkas circuit ZkBinary
    pub token_mint_zkbin: ZkBinary,
    /// Proving key for the `TokenMint_V1` zk circuit
    pub token_mint_pk: ProvingKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TokenMintCallBuilder {
    /// Build the TokenMint call debris
    pub fn build(self) -> Result<TokenMintCallDebris> {
        debug!(target: "contract::promissory_note::client::token_mint", "Building PromissoryNote::TokenMintV1 contract call");

        // Generate blinds
        let value_blind = ScalarBlind::random(&mut OsRng);

        // Derive token ID from auth_parent, user_data, and blind
        let token_id = poseidon_hash([
            self.input.token_auth_parent,
            self.input.token_user_data,
            self.input.token_blind,
        ]);

        // Create coin attributes
        let attrs = CoinAttributes {
            public_key: self.input.recipient,
            value: self.input.value,
            token_id: TokenId::from_base(token_id),
            spend_hook: FuncId::from_base(self.input.spend_hook),
            user_data: self.input.user_data,
            blind: Blind(self.input.coin_blind),
        };
        let coin = attrs.to_coin();

        // Value commitment - Pedersen (additively homomorphic)
        let value_commit = pedersen_commitment_u64(self.input.value, value_blind.clone());

        // Token commitment (hides token_id)
        let token_commit = poseidon_hash([token_id, self.input.token_blind]);

        let public_inputs = TokenMintRevealed {
            token_id,
            token_auth_parent: self.input.token_auth_parent,
            coin,
            value_commit,
            spend_hook: self.input.spend_hook,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        };

        let prover_witnesses = vec![
            Witness::Base(Value::known(self.input.token_auth_parent)),
            Witness::Base(Value::known(self.input.token_user_data)),
            Witness::Base(Value::known(self.input.token_blind)),
            Witness::Base(Value::known(self.input.recipient)),
            Witness::Base(Value::known(pallas::Base::from(self.input.value))),
            Witness::Base(Value::known(token_id)),
            Witness::Base(Value::known(self.input.spend_hook)),
            Witness::Base(Value::known(self.input.user_data)),
            Witness::Base(Value::known(self.input.coin_blind)),
            Witness::Scalar(Value::known(value_blind.inner())),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
        ];

        let circuit = ZkCircuit::new(prover_witnesses, &self.token_mint_zkbin);
        let proof = Proof::create(&self.token_mint_pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

        Ok(TokenMintCallDebris {
            params: TokenMintParamsV1 {
                coin,
                value_commit,
                token_id: TokenId::from_base(token_id),
                token_auth_parent: self.input.token_auth_parent,
                token_commit,
                spend_hook: FuncId::from_base(self.input.spend_hook),
                tx_binding: pallas::Base::zero(),
                tx_nonce: self.tx_nonce,
            },
            proofs: vec![proof],
        })
    }
}
