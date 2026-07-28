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

//! Bridge Test Harness
//!
//! Provides isolated testing for Bridge contract.

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{IntentCommitment, IntentNullifier, MerkleNode, PublicKey, pasta_prelude::PrimeField, smt::SMT_FP_DEPTH},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_bridge_contract::client::{
    azt_deposit_v1::{AztDepositCallData, AztDepositPublicInputs, create_azt_deposit_proof},
    deposit_v1::{DepositCallData, DepositPublicInputs, create_deposit_proof},
    ltc_deposit_v1::{LtcDepositCallData, LtcDepositPublicInputs, create_ltc_deposit_proof},
    withdraw_v1::{WithdrawCallData, WithdrawPublicInputs, create_withdraw_proof},
    xmr_deposit_v1::{XmrDepositCallData, XmrDepositPublicInputs, create_xmr_deposit_proof},
    zec_deposit_v1::{ZecDepositCallData, ZecDepositPublicInputs, create_zec_deposit_proof},
};
use dwow_bridge_contract::model::{DepositParams, ExternalChain, ExternalChainProof, UpdateConfigParams, WithdrawParams};

/// Bridge Harness for isolated testing
pub struct BridgeHarness {
    /// Deposit_V1 ZkBinary
    deposit_zkbin: ZkBinary,
    /// Deposit_V1 ProvingKey
    deposit_pk: ProvingKey,
    /// Withdraw_V1 ZkBinary
    withdraw_zkbin: ZkBinary,
    /// Withdraw_V1 ProvingKey
    withdraw_pk: ProvingKey,
    /// AztDepositV1 ZkBinary
    azt_deposit_zkbin: ZkBinary,
    /// AztDepositV1 ProvingKey
    azt_deposit_pk: ProvingKey,
    /// LtcDepositV1 ZkBinary
    ltc_deposit_zkbin: ZkBinary,
    /// LtcDepositV1 ProvingKey
    ltc_deposit_pk: ProvingKey,
    /// UpdateConfigV1 ZkBinary
    update_config_zkbin: ZkBinary,
    /// UpdateConfigV1 ProvingKey
    update_config_pk: ProvingKey,
    /// XmrDepositV1 ZkBinary
    xmr_deposit_zkbin: ZkBinary,
    /// XmrDepositV1 ProvingKey
    xmr_deposit_pk: ProvingKey,
    /// ZecDepositV1 ZkBinary
    zec_deposit_zkbin: ZkBinary,
    /// ZecDepositV1 ProvingKey
    zec_deposit_pk: ProvingKey,
}

impl BridgeHarness {
    /// Spawn a new Bridge harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let deposit_bin = include_bytes!("../../../bridge/proof/deposit_v1.zk.bin");
        let withdraw_bin = include_bytes!("../../../bridge/proof/withdraw_v1.zk.bin");
        let azt_deposit_bin = include_bytes!("../../../bridge/proof/azt_deposit_v1.zk.bin");
        let ltc_deposit_bin = include_bytes!("../../../bridge/proof/ltc_deposit_v1.zk.bin");
        let update_config_bin = include_bytes!("../../../bridge/proof/update_config_v1.zk.bin");
        let xmr_deposit_bin = include_bytes!("../../../bridge/proof/xmr_deposit_v1.zk.bin");
        let zec_deposit_bin = include_bytes!("../../../bridge/proof/zec_deposit_v1.zk.bin");

        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let withdraw_zkbin = ZkBinary::decode(withdraw_bin, false).unwrap();
        let azt_deposit_zkbin = ZkBinary::decode(azt_deposit_bin, false).unwrap();
        let ltc_deposit_zkbin = ZkBinary::decode(ltc_deposit_bin, false).unwrap();
        let update_config_zkbin = ZkBinary::decode(update_config_bin, false).unwrap();
        let xmr_deposit_zkbin = ZkBinary::decode(xmr_deposit_bin, false).unwrap();
        let zec_deposit_zkbin = ZkBinary::decode(zec_deposit_bin, false).unwrap();

        let deposit_pk = ProvingKey::build(
            deposit_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&deposit_zkbin).unwrap(), &deposit_zkbin),
        ).expect("ProvingKey::build failed");
        let withdraw_pk = ProvingKey::build(
            withdraw_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&withdraw_zkbin).unwrap(), &withdraw_zkbin),
        ).expect("ProvingKey::build failed");
        let azt_deposit_pk = ProvingKey::build(
            azt_deposit_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&azt_deposit_zkbin).unwrap(), &azt_deposit_zkbin),
        ).expect("ProvingKey::build failed");
        let ltc_deposit_pk = ProvingKey::build(
            ltc_deposit_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&ltc_deposit_zkbin).unwrap(), &ltc_deposit_zkbin),
        ).expect("ProvingKey::build failed");
        let update_config_pk = ProvingKey::build(
            update_config_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&update_config_zkbin).unwrap(), &update_config_zkbin),
        ).expect("ProvingKey::build failed");
        let xmr_deposit_pk = ProvingKey::build(
            xmr_deposit_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&xmr_deposit_zkbin).unwrap(), &xmr_deposit_zkbin),
        ).expect("ProvingKey::build failed");
        let zec_deposit_pk = ProvingKey::build(
            zec_deposit_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&zec_deposit_zkbin).unwrap(), &zec_deposit_zkbin),
        ).expect("ProvingKey::build failed");

        Self {
            deposit_zkbin, deposit_pk,
            withdraw_zkbin, withdraw_pk,
            azt_deposit_zkbin, azt_deposit_pk,
            ltc_deposit_zkbin, ltc_deposit_pk,
            update_config_zkbin, update_config_pk,
            xmr_deposit_zkbin, xmr_deposit_pk,
            zec_deposit_zkbin, zec_deposit_pk,
        }
    }

    /// Create a deposit with ZK proof
    pub fn deposit(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        external_block_hash: pallas::Base,
        merkle_root: pallas::Base,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
        chain: ExternalChain,
        fee: u64,
    ) -> Result<DepositResult, Box<dyn std::error::Error>> {
        let input = DepositCallData::new(
            secret,
            amount,
            recipient_public,
            bridge_nonce,
            external_block_hash,
            merkle_root,
            leaf_pos,
            merkle_path.clone(),
        );

        let (proof, public_inputs) = create_deposit_proof(
            &self.deposit_zkbin,
            &self.deposit_pk,
            &input,
        )?;

        let params = DepositParams {
            commitment: IntentCommitment::from_bytes(public_inputs.commitment.to_repr())
                .map_err(|e| format!("Invalid commitment: {e}"))?,
            recipient_pub: recipient_public,
            bridge_nonce,
            chain,
            external_block_hash: public_inputs.external_block_hash.to_repr(),
            merkle_proof: merkle_path.iter().map(|n| n.to_bytes()).collect(),
            external_state_root: public_inputs.merkle_root_input.to_repr(),
            fee,
            proof: proof.as_ref().to_vec(),
            chain_proof: ExternalChainProof::Ethereum,
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(DepositResult { call_data, proof, public_inputs })
    }

    /// Create a withdrawal with ZK proof (function code 0x02)
    pub fn withdraw(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_hash: pallas::Base,
        bridge_address: pallas::Base,
        merkle_root: pallas::Base,
        merkle_proof: [pallas::Base; 4],
        leaf_index: u64,
        fee: u64,
        token_minimum: u64,
    ) -> Result<WithdrawResult, Box<dyn std::error::Error>> {
        let mut padded_proof = [pallas::Base::zero(); SMT_FP_DEPTH];
        for (i, elem) in merkle_proof.iter().enumerate() {
            padded_proof[i] = *elem;
        }

        let input = WithdrawCallData::new(
            secret,
            amount,
            recipient_hash,
            bridge_address,
            merkle_root,
            padded_proof,
            leaf_index,
            token_minimum,
        );

        let (proof, public_inputs) = create_withdraw_proof(
            &self.withdraw_zkbin,
            &self.withdraw_pk,
            &input,
        )?;

        let nullifier = IntentNullifier::from_bytes(public_inputs.nullifier.to_repr())
            .map_err(|e| format!("Invalid nullifier: {e}"))?;

        let params = WithdrawParams {
            nullifier,
            recipient_hash: public_inputs.recipient_hash.to_repr(),
            deposit_leaf: pallas::Base::zero(),
            amount,
            proof: proof.as_ref().to_vec(),
            fee,
            timeout_height: 0,
            feed_mode: 0,
            max_fee_bp: None,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        Ok(WithdrawResult { call_data, proof, public_inputs })
    }

    /// Create an Aztec deposit with ZK proof (function code 0x01, chain Aztec)
    #[allow(clippy::too_many_arguments)]
    pub fn azt_deposit(
        &self,
        secret: pallas::Base,
        note_secret: pallas::Base,
        blinding_factor: pallas::Base,
        value: u64,
        asset_id: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        nullifier: pallas::Base,
        commitment: pallas::Base,
        anchor: pallas::Base,
        rollup_height: u64,
        eth_block_height: u64,
        confirmations: u64,
        rollup_tx_hash_0: pallas::Base,
        rollup_tx_hash_1: pallas::Base,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Result<AztDepositResult, Box<dyn std::error::Error>> {
        let input = AztDepositCallData::new(
            secret, note_secret, blinding_factor, value, asset_id,
            recipient_public, bridge_nonce, nullifier, commitment, anchor,
            rollup_height, eth_block_height, confirmations,
            rollup_tx_hash_0, rollup_tx_hash_1, leaf_pos, merkle_path,
        );
        let (proof, public_inputs) = create_azt_deposit_proof(
            &self.azt_deposit_zkbin, &self.azt_deposit_pk, &input,
        )?;
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&proof.as_ref());
        Ok(AztDepositResult { call_data, proof, public_inputs })
    }

    /// Create a Litecoin deposit with ZK proof (function code 0x01, chain Litecoin)
    #[allow(clippy::too_many_arguments)]
    pub fn ltc_deposit(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        tx_hash_0: pallas::Base,
        tx_hash_1: pallas::Base,
        output_index: u64,
        block_merkle_root: pallas::Base,
        block_height: u64,
        confirmations: u64,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Result<LtcDepositResult, Box<dyn std::error::Error>> {
        let input = LtcDepositCallData::new(
            secret, amount, recipient_public, bridge_nonce,
            tx_hash_0, tx_hash_1, output_index, block_merkle_root,
            block_height, confirmations, leaf_pos, merkle_path,
        );
        let (proof, public_inputs) = create_ltc_deposit_proof(
            &self.ltc_deposit_zkbin, &self.ltc_deposit_pk, &input,
        )?;
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&proof.as_ref());
        Ok(LtcDepositResult { call_data, proof, public_inputs })
    }

    /// Create a Monero deposit with ZK proof (function code 0x01, chain Monero)
    #[allow(clippy::too_many_arguments)]
    pub fn xmr_deposit(
        &self,
        secret: pallas::Base,
        one_time_addr_secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        tx_hash: pallas::Base,
        block_height: u64,
        output_index: u64,
        ephemeral_pub_x: pallas::Base,
        ephemeral_pub_y: pallas::Base,
        confirmations: u64,
        merkle_root: pallas::Base,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Result<XmrDepositResult, Box<dyn std::error::Error>> {
        let input = XmrDepositCallData::new(
            secret, one_time_addr_secret, amount, recipient_public, bridge_nonce,
            tx_hash, block_height, output_index, ephemeral_pub_x, ephemeral_pub_y,
            confirmations, merkle_root, leaf_pos, merkle_path,
        );
        let (proof, public_inputs) = create_xmr_deposit_proof(
            &self.xmr_deposit_zkbin, &self.xmr_deposit_pk, &input,
        )?;
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&proof.as_ref());
        Ok(XmrDepositResult { call_data, proof, public_inputs })
    }

    /// Create a Zcash deposit with ZK proof (function code 0x01, chain Zcash)
    #[allow(clippy::too_many_arguments)]
    pub fn zec_deposit(
        &self,
        secret: pallas::Base,
        position: pallas::Base,
        note_encryption: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        nullifier: pallas::Base,
        commitment: pallas::Base,
        anchor: pallas::Base,
        block_height: u64,
        randomized_pub_key_x: pallas::Base,
        randomized_pub_key_y: pallas::Base,
        randomness: pallas::Base,
        confirmations: u64,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Result<ZecDepositResult, Box<dyn std::error::Error>> {
        let input = ZecDepositCallData::new(
            secret, position, note_encryption, amount, recipient_public,
            bridge_nonce, nullifier, commitment, anchor, block_height,
            randomized_pub_key_x, randomized_pub_key_y, randomness,
            confirmations, leaf_pos, merkle_path,
        );
        let (proof, public_inputs) = create_zec_deposit_proof(
            &self.zec_deposit_zkbin, &self.zec_deposit_pk, &input,
        )?;
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&proof.as_ref());
        Ok(ZecDepositResult { call_data, proof, public_inputs })
    }

    /// Update bridge configuration (function code 0x03)
    pub fn update_config(
        &self,
        deposit_fee: u64,
        withdrawal_fee: u64,
        min_confirmations: u32,
        max_deposit: u64,
        max_withdrawal: u64,
        gov_pub_x: pallas::Base,
        gov_pub_y: pallas::Base,
        config_nullifier: pallas::Base,
    ) -> Result<UpdateConfigResult, Box<dyn std::error::Error>> {
        let witnesses = dwow_core::zk::empty_witnesses(&self.update_config_zkbin)?;
        let circuit = ZkCircuit::new(witnesses, &self.update_config_zkbin);
        let proof = Proof::create(&self.update_config_pk, &[circuit], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        let params = UpdateConfigParams {
            deposit_fee,
            withdrawal_fee,
            min_confirmations,
            max_deposit,
            max_withdrawal,
            gov_pub_x,
            gov_pub_y,
            config_nullifier,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(UpdateConfigResult { call_data, proof })
    }
}

impl super::ContractHarness for BridgeHarness {
    fn name(&self) -> &str {
        "bridge"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["DepositV1", "WithdrawV1", "AztDepositV1", "LtcDepositV1", "UpdateConfigV1", "XmrDepositV1", "ZecDepositV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "DepositV1" => Some(&self.deposit_zkbin),
            "WithdrawV1" => Some(&self.withdraw_zkbin),
            "AztDepositV1" => Some(&self.azt_deposit_zkbin),
            "LtcDepositV1" => Some(&self.ltc_deposit_zkbin),
            "UpdateConfigV1" => Some(&self.update_config_zkbin),
            "XmrDepositV1" => Some(&self.xmr_deposit_zkbin),
            "ZecDepositV1" => Some(&self.zec_deposit_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "DepositV1" => Some(&self.deposit_pk),
            "WithdrawV1" => Some(&self.withdraw_pk),
            "AztDepositV1" => Some(&self.azt_deposit_pk),
            "LtcDepositV1" => Some(&self.ltc_deposit_pk),
            "UpdateConfigV1" => Some(&self.update_config_pk),
            "XmrDepositV1" => Some(&self.xmr_deposit_pk),
            "ZecDepositV1" => Some(&self.zec_deposit_pk),
            _ => None,
        }
    }
}

/// Result of deposit
pub struct DepositResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: DepositPublicInputs,
}

/// Result of withdraw
pub struct WithdrawResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: WithdrawPublicInputs,
}

/// Result of azt_deposit
pub struct AztDepositResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AztDepositPublicInputs,
}

/// Result of ltc_deposit
pub struct LtcDepositResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: LtcDepositPublicInputs,
}

/// Result of xmr_deposit
pub struct XmrDepositResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: XmrDepositPublicInputs,
}

/// Result of zec_deposit
pub struct ZecDepositResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ZecDepositPublicInputs,
}

/// Result of update_config
pub struct UpdateConfigResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}
