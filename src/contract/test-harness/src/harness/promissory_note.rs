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
    crypto::{poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey},
    pasta::pallas,
};

use dwow_promissory_note_contract::{
    client::{
        issue::{IssueCallBuilder, IssueCallInput},
        register_type::{RegisterTypeCallBuilder, RegisterTypeCallInput},
        transfer::{TransferCallBuilder, TransferCallInput, TransferCallOutput},
    },
    model::{CapCommitment, Nullifier},
};
use dwow_serial::Encodable;

// Re-export types for convenience
pub use dwow_promissory_note_contract::client::issue::IssueCallInput as IssueInput;

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
        let register_type_bin = include_bytes!("../../../promissory_note/proof/register_type.zk.bin");
        let issue_bin = include_bytes!("../../../promissory_note/proof/issue.zk.bin");
        let revoke_bin = include_bytes!("../../../promissory_note/proof/revoke.zk.bin");
        let transfer_bin = include_bytes!("../../../promissory_note/proof/transfer.zk.bin");
        let redeem_bin = include_bytes!("../../../promissory_note/proof/redeem.zk.bin");

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
    /// Returns token creation result with issue_public for subsequent minting
    pub fn register_type(
        &self,
        issue_secret: pallas::Base,
        token_user_data: pallas::Base,
        token_blind: pallas::Base,
        recipient: pallas::Base,
        initial_value: u64,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        commitment_blind: pallas::Base,
    ) -> Result<RegisterTypeResult> {
        // Derive token_auth_parent = poseidon_hash(DOMAIN_SIGNATURE_SECRET, issue_secret).
        // V2 circuit domain separator: DOMAIN_SIGNATURE_SECRET = 7.
        let token_auth_parent = poseidon_hash([pallas::Base::from(7), issue_secret]);

        // Derive asset_id = poseidon_hash(DOMAIN_TOK_COMMIT, auth_parent, user_data, blind).
        // V2 circuit domain separator: DOMAIN_TOK_COMMIT = 2.
        let asset_id = poseidon_hash([pallas::Base::from(2), token_auth_parent, token_user_data, token_blind]);

        // Build token mint proof using the contract's builder
        let token_input = RegisterTypeCallInput {
            token_auth_parent,
            token_user_data,
            token_blind,
            recipient,
            value: initial_value,
            spend_hook,
            user_data,
            commitment_blind,
        };

        let token_debris = RegisterTypeCallBuilder {
            input: token_input,
            register_type_zkbin: self.register_type_zkbin.clone(),
            register_type_pk: self.register_type_pk.clone(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
        .build()?;

        let mut call_data = vec![0x00u8]; // RegisterTypeV1
        call_data.extend_from_slice(&token_debris.params.encode());

        Ok(RegisterTypeResult {
            call_data,
            asset_id,
            issue_public: token_auth_parent,
            commitment:token_debris.params.commitment,
            value_commit: token_debris.params.value_commit,
            token_commit: token_debris.params.token_commit,
            token_proofs: token_debris.proofs,
        })
    }

    /// Issue capabilities of an existing type
    pub fn issue(
        &self,
        issue_secret: pallas::Base,
        asset_id: pallas::Base,
        recipient: pallas::Base,
        value: u64,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        commitment_blind: pallas::Base,
    ) -> Result<IssueResult> {
        // Build Merkle tree matching on-chain token registry structure:
        // init_contract places ZERO guard leaf at position 0, then
        // apply_token_mint (RegisterTypeV1) appends asset_id at position 1.
        // The proof must use the tree root AFTER both are committed.
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero())); // guard leaf @ pos 0
        tree.append(MerkleNode::from_base(asset_id));           // token leaf @ pos 1
        let leaf_pos_mark = tree.mark().unwrap();

        // Get Merkle path for the token leaf
        let token_path = tree.witness(leaf_pos_mark, 0).unwrap();

        let issue_input = IssueCallInput {
            issue_secret,
            token_leaf_pos: u64::from(leaf_pos_mark).try_into().unwrap(),
            token_path,
            recipient,
            value,
            asset_id,
            spend_hook,
            user_data,
            commitment_blind,
        };

        let debris = IssueCallBuilder {
            input: issue_input,
            issue_zkbin: self.issue_zkbin.clone(),
            issue_pk: self.issue_pk.clone(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
        .build()?;

        let mut call_data = vec![0x02u8]; // IssueV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(IssueResult {
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
        self.transfer_with_value_blinds(inputs, outputs, None)
    }

    /// Create a transfer proof with caller-supplied value blinds (one per
    /// input/output pair), so a child transfer's output value_commit can match
    /// a parent contract's `validate_child_value_commit`.
    pub fn transfer_with_value_blinds(
        &self,
        inputs: Vec<TransferCallInput>,
        outputs: Vec<TransferCallOutput>,
        value_blinds: Option<Vec<dwow_sdk::crypto::ScalarBlind>>,
    ) -> Result<TransferResult> {
        let debris = TransferCallBuilder {
            inputs,
            outputs,
            revoke_zkbin: self.revoke_zkbin.clone(),
            revoke_pk: self.revoke_pk.clone(),
            transfer_zkbin: self.transfer_zkbin.clone(),
            transfer_pk: self.transfer_pk.clone(),
            value_blinds,
        }
        .build()?;

        let mut call_data = vec![0x04u8]; // TransferV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(TransferResult {
            call_data,
            proofs: debris.proofs,
            nullifier: debris.params.inputs[0].nullifier,
        })
    }

    /// Perform an OTC swap between two parties
    /// Inputs are revoked, outputs are transferred - cross-token atomic swap
    /// Redeem coins (function code 0x01, ZK).
    /// Closes the bearer-instrument lifecycle: burns the coin, issues zero-value receipt.
    pub fn redeem(
        &self,
        value: u64,
        asset_id: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        commitment_blind: pallas::Base,
        secret: pallas::Base,
        recipient: pallas::Base,
        leaf_position: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Result<RedeemResult> {
        use dwow_promissory_note_contract::client::redeem::{RedeemCallBuilder, RedeemCallInput, RedeemCallOutput};
        let ephem_secret = pallas::Base::from(9u64);
        let recipient_pub = PublicKey::from_secret(SecretKey::from_base(recipient));
        let input = RedeemCallInput {
            value, asset_id, spend_hook, user_data, commitment_blind,
            leaf_position, merkle_path,
            secret,
            ephemeral_signature_secret: ephem_secret,
        };
        let output = RedeemCallOutput {
            recipient,
            recipient_pub,
            asset_id, spend_hook, user_data, commitment_blind,
        };
        let debris = RedeemCallBuilder {
            input, output,
            burn_zkbin: self.revoke_zkbin.clone(),
            burn_pk: self.revoke_pk.clone(),
            redeem_zkbin: self.redeem_zkbin.clone(),
            redeem_pk: self.redeem_pk.clone(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
        .build()
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let mut call_data = vec![0x01u8]; // RedeemV1
        call_data.extend_from_slice(&debris.params.encode());
        Ok(RedeemResult { call_data, proofs: debris.proofs, nullifier: debris.params.input.nullifier })
    }

    /// Revoke (burn) coins (function code 0x03, ZK).
    /// Constructs a RevokeCallBuilder with simplified deterministic inputs.
    pub fn revoke(
        &self,
        value: u64,
        asset_id: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        commitment_blind: pallas::Base,
        secret: pallas::Base,
        leaf_position: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Result<RevokeResult> {
        use dwow_promissory_note_contract::client::revoke::{RevokeCallBuilder, RevokeCallInput};
        let ephem_secret = pallas::Base::from(9u64);
        let input = RevokeCallInput {
            value, asset_id, spend_hook, user_data, commitment_blind,
            leaf_position,
            merkle_path,
            secret,
            ephemeral_signature_secret: ephem_secret,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let debris = RevokeCallBuilder {
            inputs: vec![input],
            revoke_zkbin: self.revoke_zkbin.clone(),
            revoke_pk: self.revoke_pk.clone(),
        }
        .build()
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let mut call_data = vec![0x03u8]; // RevokeV1
        call_data.extend_from_slice(&debris.params.encode());
        Ok(RevokeResult { call_data, proofs: debris.proofs, nullifier: debris.params.inputs[0].nullifier })
    }

    pub fn otc_swap(
        &self,
        inputs: Vec<TransferCallInput>,
        outputs: Vec<TransferCallOutput>,
    ) -> Result<OtcSwapResult> {
        let debris = TransferCallBuilder {
            inputs,
            outputs,
            revoke_zkbin: self.revoke_zkbin.clone(),
            revoke_pk: self.revoke_pk.clone(),
            transfer_zkbin: self.transfer_zkbin.clone(),
            transfer_pk: self.transfer_pk.clone(),
            value_blinds: None,
        }
        .build()?;

        let mut call_data = vec![0x05u8]; // OtcSwapV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(OtcSwapResult {
            call_data,
            proofs: debris.proofs,
            nullifier: debris.params.inputs[0].nullifier,
        })
    }
}

impl super::ContractHarness for PromissoryNoteHarness {
    fn name(&self) -> &str {
        "promissory_note"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "RegisterTypeV2",
            "IssueV2",
            "RevokeV2",
            "TransferV2",
            "RedeemV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "RegisterTypeV2" => Some(&self.register_type_zkbin),
            "IssueV2" => Some(&self.issue_zkbin),
            "RevokeV2" => Some(&self.revoke_zkbin),
            "TransferV2" => Some(&self.transfer_zkbin),
            "RedeemV2" => Some(&self.redeem_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "RegisterTypeV2" => Some(&self.register_type_pk),
            "IssueV2" => Some(&self.issue_pk),
            "RevokeV2" => Some(&self.revoke_pk),
            "TransferV2" => Some(&self.transfer_pk),
            "RedeemV2" => Some(&self.redeem_pk),
            _ => None,
        }
    }
}

/// Result of token creation
pub struct RegisterTypeResult {
    pub call_data: Vec<u8>,
    pub asset_id: pallas::Base,
    pub issue_public: pallas::Base,
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub token_proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of minting
pub struct IssueResult {
    pub call_data: Vec<u8>,
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of transfer
pub struct TransferResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
    pub nullifier: Nullifier,
}

/// Result of OTC swap
pub struct OtcSwapResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
    pub nullifier: Nullifier,
}

pub struct RedeemResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
    pub nullifier: Nullifier,
}

pub struct RevokeResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
    pub nullifier: Nullifier,
}
