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

//! PromissoryNote Test Harness
//!
//! Provides isolated testing for PromissoryNote contract (DeFi token contract).

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, MerkleNode, MerkleTree},
    pasta::pallas,
};

use dwow_promissory_note_contract::{
    client::{
        mint_v1::{MintCallBuilder, MintCallInput},
        token_mint_v1::{TokenMintCallBuilder, TokenMintCallInput},
        transfer_v1::{TransferCallBuilder, TransferCallInput, TransferCallOutput},
    },
    model::Coin,
};
use dwow_serial::Encodable;

// Re-export types for convenience
pub use dwow_promissory_note_contract::client::mint_v1::MintCallInput as MintInput;

/// PromissoryNote Harness for isolated testing
pub struct PromissoryNoteHarness {
    /// TokenMint_V1 ZkBinary
    token_mint_zkbin: ZkBinary,
    /// TokenMint_V1 ProvingKey
    token_mint_pk: ProvingKey,
    /// Mint_V1 ZkBinary (standalone mint)
    mint_zkbin: ZkBinary,
    /// Mint_V1 ProvingKey (standalone mint)
    mint_pk: ProvingKey,
    /// Burn_V1 ZkBinary
    burn_zkbin: ZkBinary,
    /// Burn_V1 ProvingKey
    burn_pk: ProvingKey,
    /// BlindOutput_V1 ZkBinary (transfer/swap outputs)
    blind_output_zkbin: ZkBinary,
    /// BlindOutput_V1 ProvingKey (transfer/swap outputs)
    blind_output_pk: ProvingKey,
}

impl PromissoryNoteHarness {
    /// Spawn a new PromissoryNote harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let token_mint_bin = include_bytes!("../../../promissory_note/proof/token_mint_v1.zk.bin");
        let mint_bin = include_bytes!("../../../promissory_note/proof/mint_v1.zk.bin");
        let burn_bin = include_bytes!("../../../promissory_note/proof/burn_v1.zk.bin");
        let blind_output_bin = include_bytes!("../../../promissory_note/proof/blind_output_v1.zk.bin");

        let token_mint_zkbin = ZkBinary::decode(token_mint_bin, false).unwrap();
        let mint_zkbin = ZkBinary::decode(mint_bin, false).unwrap();
        let burn_zkbin = ZkBinary::decode(burn_bin, false).unwrap();
        let blind_output_zkbin = ZkBinary::decode(blind_output_bin, false).unwrap();

        // Build proving keys
        let token_mint_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&token_mint_zkbin).unwrap(), &token_mint_zkbin);
        let mint_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&mint_zkbin).unwrap(), &mint_zkbin);
        let burn_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin);
        let blind_output_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&blind_output_zkbin).unwrap(), &blind_output_zkbin);

        let token_mint_pk = ProvingKey::build(token_mint_zkbin.k, &token_mint_circuit).expect("ProvingKey::build failed");
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit).expect("ProvingKey::build failed");
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit).expect("ProvingKey::build failed");
        let blind_output_pk = ProvingKey::build(blind_output_zkbin.k, &blind_output_circuit).expect("ProvingKey::build failed");

        Self {
            token_mint_zkbin,
            token_mint_pk,
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
            blind_output_zkbin,
            blind_output_pk,
        }
    }

    /// Get the combined verifying key for all circuits
    pub fn verifying_key(&self) -> dwow_core::zk::VerifyingKey {
        // Combine all circuit VKs
        dwow_core::zk::VerifyingKey::build(
            self.token_mint_zkbin.k,
            &ZkCircuit::new(
                dwow_core::zk::empty_witnesses(&self.token_mint_zkbin).unwrap(),
                &self.token_mint_zkbin,
            ),
        ).expect("VerifyingKey::build failed")
    }

    /// Create a new token type
    ///
    /// Returns token creation result with mint_public for subsequent minting
    pub fn create_token(
        &self,
        mint_secret: pallas::Base,
        token_user_data: pallas::Base,
        token_blind: pallas::Base,
        recipient: pallas::Base,
        initial_value: u64,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        coin_blind: pallas::Base,
    ) -> Result<TokenCreationResult> {
        // Derive token_auth_parent = poseidon_hash(mint_secret) — backing capability commitment
        let token_auth_parent = poseidon_hash([mint_secret]);

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
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
        .build()?;

        let mut call_data = vec![];
        token_debris.params.encode(&mut call_data)?;

        Ok(TokenCreationResult {
            call_data,
            token_id,
            mint_public: token_auth_parent,
            coin: token_debris.params.coin,
            value_commit: token_debris.params.value_commit,
            token_commit: token_debris.params.token_commit,
            token_proofs: token_debris.proofs,
        })
    }

    /// Mint tokens of an existing token type
    pub fn mint(
        &self,
        mint_secret: pallas::Base,
        token_id: pallas::Base,
        recipient: pallas::Base,
        value: u64,
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
            mint_secret,
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
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
        .build()?;

        let mut call_data = vec![];
        debris.params.encode(&mut call_data)?;

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
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
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
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
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

impl super::ContractHarness for PromissoryNoteHarness {
    fn name(&self) -> &str {
        "promissory_note"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "TokenMintV1",
            "MintV1",
            "BurnV1",
            "BlindOutputV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "TokenMintV1" => Some(&self.token_mint_zkbin),
            "MintV1" => Some(&self.mint_zkbin),
            "BurnV1" => Some(&self.burn_zkbin),
            "BlindOutputV1" => Some(&self.blind_output_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "TokenMintV1" => Some(&self.token_mint_pk),
            "MintV1" => Some(&self.mint_pk),
            "BurnV1" => Some(&self.burn_pk),
            "BlindOutputV1" => Some(&self.blind_output_pk),
            _ => None,
        }
    }
}

/// Result of token creation
pub struct TokenCreationResult {
    pub call_data: Vec<u8>,
    pub token_id: pallas::Base,
    pub mint_public: pallas::Base,
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub token_proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of minting
pub struct MintResult {
    pub call_data: Vec<u8>,
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of transfer
pub struct TransferResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of OTC swap
pub struct OtcSwapResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
}
