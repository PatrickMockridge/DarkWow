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
    model::CapCommitment,
};
use dwow_serial::Encodable;

// Re-export types for convenience
pub use dwow_promissory_note_contract::client::mint_v1::MintCallInput as MintInput;

/// PromissoryNote Harness for isolated testing
pub struct PromissoryNoteHarness {
    /// RegisterType_V1 ZkBinary
    register_type_zkbin: ZkBinary,
    /// RegisterType_V1 ProvingKey
    register_type_pk: ProvingKey,
    /// Issue_V1 ZkBinary (standalone mint)
    issue_zkbin: ZkBinary,
    /// Issue_V1 ProvingKey (standalone mint)
    issue_pk: ProvingKey,
    /// Revoke_V1 ZkBinary
    revoke_zkbin: ZkBinary,
    /// Revoke_V1 ProvingKey
    revoke_pk: ProvingKey,
    /// Transfer_V1 ZkBinary (transfer/swap outputs)
    transfer_zkbin: ZkBinary,
    /// Transfer_V1 ProvingKey (transfer/swap outputs)
    transfer_pk: ProvingKey,
    /// Redeem_V1 ZkBinary
    redeem_zkbin: ZkBinary,
    /// Redeem_V1 ProvingKey
    redeem_pk: ProvingKey,
}

impl PromissoryNoteHarness {
    /// Spawn a new PromissoryNote harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let register_type_bin = include_bytes!("../../../promissory_note/proof/token_mint_v2.zk.bin");
        let issue_bin = include_bytes!("../../../promissory_note/proof/mint_v2.zk.bin");
        let revoke_bin = include_bytes!("../../../promissory_note/proof/burn_v2.zk.bin");
        let transfer_bin = include_bytes!("../../../promissory_note/proof/blind_output_v2.zk.bin");
        let redeem_bin = include_bytes!("../../../promissory_note/proof/redeem_v2.zk.bin");

        let register_type_zkbin = ZkBinary::decode(register_type_bin, false).unwrap();
        let issue_zkbin = ZkBinary::decode(issue_bin, false).unwrap();
        let revoke_zkbin = ZkBinary::decode(revoke_bin, false).unwrap();
        let transfer_zkbin = ZkBinary::decode(transfer_bin, false).unwrap();
        let redeem_zkbin = ZkBinary::decode(redeem_bin, false).unwrap();

        // Build proving keys
        let register_type_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&register_type_zkbin).unwrap(), &register_type_zkbin);
        let issue_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&issue_zkbin).unwrap(), &issue_zkbin);
        let revoke_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&revoke_zkbin).unwrap(), &revoke_zkbin);
        let transfer_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&transfer_zkbin).unwrap(), &transfer_zkbin);
        let redeem_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&redeem_zkbin).unwrap(), &redeem_zkbin);

        let register_type_pk = ProvingKey::build(register_type_zkbin.k, &register_type_circuit).expect("ProvingKey::build failed");
        let issue_pk = ProvingKey::build(issue_zkbin.k, &issue_circuit).expect("ProvingKey::build failed");
        let revoke_pk = ProvingKey::build(revoke_zkbin.k, &revoke_circuit).expect("ProvingKey::build failed");
        let transfer_pk = ProvingKey::build(transfer_zkbin.k, &transfer_circuit).expect("ProvingKey::build failed");
        let redeem_pk = ProvingKey::build(redeem_zkbin.k, &redeem_circuit).expect("ProvingKey::build failed");

        Self {
            register_type_zkbin,
            register_type_pk,
            issue_zkbin,
            issue_pk,
            revoke_zkbin,
            revoke_pk,
            transfer_zkbin,
            transfer_pk,
            redeem_zkbin,
            redeem_pk,
        }
    }

    /// Get the combined verifying key for all circuits
    pub fn verifying_key(&self) -> dwow_core::zk::VerifyingKey {
        // Combine all circuit VKs
        dwow_core::zk::VerifyingKey::build(
            self.register_type_zkbin.k,
            &ZkCircuit::new(
                dwow_core::zk::empty_witnesses(&self.register_type_zkbin).unwrap(),
                &self.register_type_zkbin,
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
            token_mint_zkbin: self.register_type_zkbin.clone(),
            token_mint_pk: self.register_type_pk.clone(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
        .build()?;

        let mut call_data = vec![0x00u8]; // RegisterTypeV1
        call_data.extend_from_slice(&token_debris.params.encode());

        Ok(TokenCreationResult {
            call_data,
            token_id,
            mint_public: token_auth_parent,
            commitment:token_debris.params.commitment,
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
        // Build Merkle tree matching on-chain token registry structure:
        // init_contract places ZERO guard leaf at position 0, then
        // apply_token_mint (RegisterTypeV1) appends token_id at position 1.
        // The proof must use the tree root AFTER both are committed.
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero())); // guard leaf @ pos 0
        tree.append(MerkleNode::from_base(token_id));           // token leaf @ pos 1
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
            mint_zkbin: self.issue_zkbin.clone(),
            mint_pk: self.issue_pk.clone(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
        .build()?;

        let mut call_data = vec![0x02u8]; // IssueV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(MintResult {
            call_data,
            commitment:debris.params.commitment,
            value_commit: debris.params.value_commit,
            proofs: debris.proofs,
        })
    }

    /// Create a transfer proof (revoke + issue)
    pub fn transfer(
        &self,
        inputs: Vec<TransferCallInput>,
        outputs: Vec<TransferCallOutput>,
    ) -> Result<TransferResult> {
        let debris = TransferCallBuilder {
            inputs,
            outputs,
            burn_zkbin: self.revoke_zkbin.clone(),
            burn_pk: self.revoke_pk.clone(),
            blind_output_zkbin: self.transfer_zkbin.clone(),
            blind_output_pk: self.transfer_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x04u8]; // TransferV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(TransferResult {
            call_data,
            proofs: debris.proofs,
        })
    }

    /// Perform an OTC swap between two parties
    /// Inputs are revoked, outputs are transferred - cross-token atomic swap
    pub fn otc_swap(
        &self,
        inputs: Vec<TransferCallInput>,
        outputs: Vec<TransferCallOutput>,
    ) -> Result<OtcSwapResult> {
        let debris = TransferCallBuilder {
            inputs,
            outputs,
            burn_zkbin: self.revoke_zkbin.clone(),
            burn_pk: self.revoke_pk.clone(),
            blind_output_zkbin: self.transfer_zkbin.clone(),
            blind_output_pk: self.transfer_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x05u8]; // OtcSwapV1
        call_data.extend_from_slice(&debris.params.encode());

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
            "RegisterType_V1",
            "Issue_V1",
            "Revoke_V1",
            "Transfer_V1",
            "Redeem_V1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "RegisterType_V1" => Some(&self.register_type_zkbin),
            "Issue_V1" => Some(&self.issue_zkbin),
            "Revoke_V1" => Some(&self.revoke_zkbin),
            "Transfer_V1" => Some(&self.transfer_zkbin),
            "Redeem_V1" => Some(&self.redeem_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "RegisterType_V1" => Some(&self.register_type_pk),
            "Issue_V1" => Some(&self.issue_pk),
            "Revoke_V1" => Some(&self.revoke_pk),
            "Transfer_V1" => Some(&self.transfer_pk),
            "Redeem_V1" => Some(&self.redeem_pk),
            _ => None,
        }
    }
}

/// Result of token creation
pub struct TokenCreationResult {
    pub call_data: Vec<u8>,
    pub token_id: pallas::Base,
    pub mint_public: pallas::Base,
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub token_proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of minting
pub struct MintResult {
    pub call_data: Vec<u8>,
    pub commitment: CapCommitment,
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
