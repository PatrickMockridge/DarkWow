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

//! MoneyV3 Test Harness
//!
//! Provides isolated testing for MoneyV3 contract (DeFi token contract).

use dwow::{
    zk::{halo2::Value, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, BaseBlind, MerkleNode, MerkleTree},
    pasta::pallas,
};
use rand::rngs::OsRng;

use dwow_money_v3_contract::{
    client::{
        auth_token_mint_v1::{AuthTokenMintCallBuilder, AuthTokenMintCallInput},
        mint_v1::{MintCallBuilder, MintCallInput},
        token_mint_v1::{TokenMintCallBuilder, TokenMintCallInput},
        transfer_v1::{TransferCallBuilder, TransferCallDebris, TransferCallInput, TransferCallOutput},
    },
    model::{Coin, MintParamsV1, TransferParamsV1},
};
use dwow_serial::Encodable;

// Re-export types for convenience
pub use dwow_money_v3_contract::client::mint_v1::MintCallInput as MintInput;
pub use dwow_money_v3_contract::client::auth_token_mint_v1::AuthTokenMintCallInput as AuthInput;

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
            ZkCircuit::new(dwow::zk::empty_witnesses(&token_mint_zkbin).unwrap(), &token_mint_zkbin);
        let auth_circuit =
            ZkCircuit::new(dwow::zk::empty_witnesses(&auth_zkbin).unwrap(), &auth_zkbin);
        let mint_circuit =
            ZkCircuit::new(dwow::zk::empty_witnesses(&mint_zkbin).unwrap(), &mint_zkbin);
        let burn_circuit =
            ZkCircuit::new(dwow::zk::empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin);

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
    pub fn verifying_key(&self) -> dwow::zk::VerifyingKey {
        // Combine all circuit VKs
        dwow::zk::VerifyingKey::build(
            self.token_mint_zkbin.k,
            &ZkCircuit::new(
                dwow::zk::empty_witnesses(&self.token_mint_zkbin).unwrap(),
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
        // Build a proper Merkle tree with the token as the first leaf
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from(token_id));
        let leaf_pos_mark = tree.mark().unwrap();

        // Get Merkle path for the token leaf
        let merkle_path = tree.witness(leaf_pos_mark, 0).unwrap();

        let auth_input = AuthTokenMintCallInput {
            mint_secret: token_auth_parent, // Reuse auth parent as mint secret
            token_id,
            leaf_pos: leaf_pos_mark.into(),
            merkle_path,
        };

        let auth_debris = AuthTokenMintCallBuilder {
            input: auth_input,
            auth_zkbin: self.auth_zkbin.clone(),
            auth_pk: self.auth_pk.clone(),
        }
        .build()?;

        // Capture the token_registry_root for use in mint()
        let token_registry_root = auth_debris.params.token_registry_root.inner();

        // Encode TokenMintParamsV1 + AuthTokenMintParamsV1 for call_data
        let auth_params = dwow_money_v3_contract::model::AuthTokenMintParamsV1 {
            nullifier: auth_debris.params.nullifier,
            mint_public: auth_debris.params.mint_public,
            token_id,
            token_registry_root: auth_debris.params.token_registry_root,
        };
        let mut call_data = vec![];
        token_debris.params.encode(&mut call_data)?;
        auth_params.encode(&mut call_data)?;

        Ok(TokenCreationResult {
            call_data,
            token_id,
            coin: token_debris.params.coin,
            value_commit: token_debris.params.value_commit,
            token_commit: token_debris.params.token_commit,
            auth_nullifier: auth_debris.params.nullifier.inner(),
            auth_mint_public: auth_debris.params.mint_public,
            token_registry_root,
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
        token_registry_root: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        coin_blind: pallas::Base,
    ) -> Result<MintResult> {
        // Build same Merkle tree structure as used in create_token
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from(token_id));
        let leaf_pos_mark = tree.mark().unwrap();

        // Get Merkle path for the token leaf
        let token_path = tree.witness(leaf_pos_mark, 0).unwrap();

        let mint_input = MintCallInput {
            auth_nullifier,
            auth_mint_public,
            token_leaf_pos: u64::from(leaf_pos_mark).try_into().unwrap(),
            token_path,
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

        // Build MintParamsV1 with auth_proof including token_registry_root
        let mint_params = MintParamsV1 {
            auth_proof: dwow_money_v3_contract::model::AuthProof {
                nullifier: dwow_money_v3_contract::model::Nullifier::from_base(auth_nullifier),
                mint_public: auth_mint_public,
                token_registry_root: dwow_sdk::crypto::MerkleNode::from(token_registry_root),
            },
            coin: debris.params.coin,
            value_commit: debris.params.value_commit,
            token_id,
        };

        let mut call_data = vec![];
        mint_params.encode(&mut call_data)?;

        Ok(MintResult {
            call_data,
            coin: debris.params.coin,
            value_commit: debris.params.value_commit,
            proofs: debris.proofs,
        })
    }

    /// Create a transfer proof (burn + mint)
    pub fn transfer(
        &self,
        inputs: Vec<TransferCallInput>,
        outputs: Vec<TransferCallOutput>,
    ) -> Result<TransferResult> {
        let debris = TransferCallBuilder {
            inputs,
            outputs,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            mint_zkbin: self.mint_zkbin.clone(),
            mint_pk: self.mint_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![];
        debris.params.encode(&mut call_data)?;

        Ok(TransferResult {
            call_data,
            proofs: debris.proofs,
        })
    }

    /// Perform an OTC swap between two parties
    /// Inputs are burned, outputs are minted - cross-token atomic swap
    pub fn otc_swap(
        &self,
        inputs: Vec<TransferCallInput>,
        outputs: Vec<TransferCallOutput>,
    ) -> Result<OtcSwapResult> {
        let debris = TransferCallBuilder {
            inputs,
            outputs,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            mint_zkbin: self.mint_zkbin.clone(),
            mint_pk: self.mint_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![];
        debris.params.encode(&mut call_data)?;

        Ok(OtcSwapResult {
            call_data,
            proofs: debris.proofs,
        })
    }
}

impl super::ContractHarness for MoneyV3Harness {
    fn name(&self) -> &str {
        "money_v3"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "TokenMintV1",
            "AuthTokenMintV1",
            "MintV1",
            "BurnV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "TokenMintV1" => Some(&self.token_mint_zkbin),
            "AuthTokenMintV1" => Some(&self.auth_zkbin),
            "MintV1" => Some(&self.mint_zkbin),
            "BurnV1" => Some(&self.burn_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "TokenMintV1" => Some(&self.token_mint_pk),
            "AuthTokenMintV1" => Some(&self.auth_pk),
            "MintV1" => Some(&self.mint_pk),
            "BurnV1" => Some(&self.burn_pk),
            _ => None,
        }
    }
}

/// Result of token creation
pub struct TokenCreationResult {
    pub call_data: Vec<u8>,
    pub token_id: pallas::Base,
    pub coin: Coin,
    pub value_commit: pallas::Base,
    pub token_commit: pallas::Base,
    pub auth_nullifier: pallas::Base,
    pub auth_mint_public: pallas::Base,
    pub token_registry_root: pallas::Base,
    pub auth_proofs: Vec<dwow::zk::Proof>,
    pub token_proofs: Vec<dwow::zk::Proof>,
}

/// Result of minting
pub struct MintResult {
    pub call_data: Vec<u8>,
    pub coin: Coin,
    pub value_commit: pallas::Base,
    pub proofs: Vec<dwow::zk::Proof>,
}

/// Result of transfer
pub struct TransferResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow::zk::Proof>,
}

/// Result of OTC swap
pub struct OtcSwapResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow::zk::Proof>,
}