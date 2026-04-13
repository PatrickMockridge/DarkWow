/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or version 3
 * or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! MoneyV3 Test Harness
//!
//! Provides isolated testing for MoneyV3 contract (DeFi token contract).

use darkfi::{
    zk::{halo2::Value, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, BaseBlind, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;

use darkfi_money_v3_contract::{
    client::{
        auth_token_mint_v1::{AuthTokenMintCallBuilder, AuthTokenMintCallInput},
        mint_v1::{MintCallBuilder, MintCallInput},
        token_mint_v1::{TokenMintCallBuilder, TokenMintCallInput},
    },
    model::Coin,
};

// Re-export types for convenience
pub use darkfi_money_v3_contract::client::mint_v1::MintCallInput as MintInput;
pub use darkfi_money_v3_contract::client::auth_token_mint_v1::AuthTokenMintCallInput as AuthInput;

/// MoneyV3 Harness for isolated testing
pub struct MoneyV3Harness {
    /// TokenMint_V1 ZkBinary
    token_mint_zkbin: ZkBinary,
    /// TokenMint_V1 ProvingKey
    token_mint_pk: ProvingKey,
    /// AuthTokenMint_V1 ZkBinary
    auth_zkbin: ZkBinary,
    /// AuthTokenMint_V1 ProvingKey
    auth_pk: ProvingKey,
    /// Mint_V1 ZkBinary
    mint_zkbin: ZkBinary,
    /// Mint_V1 ProvingKey
    mint_pk: ProvingKey,
    /// Burn_V1 ZkBinary
    burn_zkbin: ZkBinary,
    /// Burn_V1 ProvingKey
    burn_pk: ProvingKey,
}

impl MoneyV3Harness {
    /// Spawn a new MoneyV3 harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let token_mint_bin = include_bytes!("../../../money_v3/proof/token_mint_v1.zk.bin");
        let auth_bin = include_bytes!("../../../money_v3/proof/auth_token_mint_v1.zk.bin");
        let mint_bin = include_bytes!("../../../money_v3/proof/mint_v1.zk.bin");
        let burn_bin = include_bytes!("../../../money_v3/proof/burn_v1.zk.bin");

        let token_mint_zkbin = ZkBinary::decode(token_mint_bin, false).unwrap();
        let auth_zkbin = ZkBinary::decode(auth_bin, false).unwrap();
        let mint_zkbin = ZkBinary::decode(mint_bin, false).unwrap();
        let burn_zkbin = ZkBinary::decode(burn_bin, false).unwrap();

        // Build proving keys
        let token_mint_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&token_mint_zkbin).unwrap(), &token_mint_zkbin);
        let auth_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&auth_zkbin).unwrap(), &auth_zkbin);
        let mint_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&mint_zkbin).unwrap(), &mint_zkbin);
        let burn_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin);

        let token_mint_pk = ProvingKey::build(token_mint_zkbin.k, &token_mint_circuit);
        let auth_pk = ProvingKey::build(auth_zkbin.k, &auth_circuit);
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);

        Self {
            token_mint_zkbin,
            token_mint_pk,
            auth_zkbin,
            auth_pk,
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
        }
    }

    /// Get the combined verifying key for all circuits
    pub fn verifying_key(&self) -> darkfi::zk::VerifyingKey {
        // Combine all circuit VKs
        darkfi::zk::VerifyingKey::build(
            self.token_mint_zkbin.k,
            &ZkCircuit::new(
                darkfi::zk::empty_witnesses(&self.token_mint_zkbin).unwrap(),
                &self.token_mint_zkbin,
            ),
        )
    }

    /// Get circuit namespaces
    pub fn circuits(&self) -> Vec<&'static str> {
        vec![
            "TokenMint_V1",
            "AuthTokenMint_V1",
            "Mint_V1",
            "Burn_V1",
        ]
    }

    /// Create a new token type
    ///
    /// Returns token creation result with auth_nullifier and auth_mint_public for subsequent minting
    pub fn create_token(
        &self,
        token_auth_parent: pallas::Base,
        token_user_data: pallas::Base,
        token_blind: pallas::Base,
        recipient: pallas::Base,
        initial_value: u64,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        coin_blind: pallas::Base,
    ) -> Result<TokenCreationResult> {
        // Derive token_id = poseidon_hash(auth_parent, user_data, blind)
        let token_id = poseidon_hash([token_auth_parent, token_user_data, token_blind]);

        // Build token mint proof using the contract's builder
        let token_input = TokenMintCallInput {
            token_auth_parent,
            token_user_data,
            token_blind,
            recipient,
            value: initial_value,
            spend_hook,
            user_data,
            coin_blind,
        };

        let token_debris = TokenMintCallBuilder {
            input: token_input,
            token_mint_zkbin: self.token_mint_zkbin.clone(),
            token_mint_pk: self.token_mint_pk.clone(),
        }
        .build()?;

        // Now authorize minting for this token
        let auth_input = AuthTokenMintCallInput {
            mint_secret: token_auth_parent, // Reuse auth parent as mint secret
            token_id,
            leaf_pos: 0,
            merkle_path: vec![MerkleNode::from(token_id)], // Simplified
        };

        let auth_debris = AuthTokenMintCallBuilder {
            input: auth_input,
            auth_zkbin: self.auth_zkbin.clone(),
            auth_pk: self.auth_pk.clone(),
        }
        .build()?;

        Ok(TokenCreationResult {
            token_id,
            coin: token_debris.params.coin,
            value_commit: token_debris.params.value_commit,
            token_commit: token_debris.params.token_commit,
            auth_nullifier: auth_debris.params.nullifier.inner(),
            auth_mint_public: auth_debris.params.mint_public,
            auth_proofs: auth_debris.proofs,
            token_proofs: token_debris.proofs,
        })
    }

    /// Mint tokens of an existing authorized type
    pub fn mint(
        &self,
        token_id: pallas::Base,
        recipient: pallas::Base,
        value: u64,
        auth_nullifier: pallas::Base,
        auth_mint_public: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        coin_blind: pallas::Base,
    ) -> Result<MintResult> {
        let mint_input = MintCallInput {
            auth_nullifier,
            auth_mint_public,
            token_leaf_pos: 0,
            token_path: vec![MerkleNode::from(token_id)], // Simplified
            recipient,
            value,
            token_id,
            spend_hook,
            user_data,
            coin_blind,
        };

        let debris = MintCallBuilder {
            input: mint_input,
            mint_zkbin: self.mint_zkbin.clone(),
            mint_pk: self.mint_pk.clone(),
        }
        .build()?;

        Ok(MintResult {
            coin: debris.params.coin,
            value_commit: debris.params.value_commit,
            proofs: debris.proofs,
        })
    }
}

/// Result of token creation
pub struct TokenCreationResult {
    pub token_id: pallas::Base,
    pub coin: Coin,
    pub value_commit: pallas::Base,
    pub token_commit: pallas::Base,
    pub auth_nullifier: pallas::Base,
    pub auth_mint_public: pallas::Base,
    pub auth_proofs: Vec<darkfi::zk::Proof>,
    pub token_proofs: Vec<darkfi::zk::Proof>,
}

/// Result of minting
pub struct MintResult {
    pub coin: Coin,
    pub value_commit: pallas::Base,
    pub proofs: Vec<darkfi::zk::Proof>,
}