/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Bridge Test Harness
//!
//! Provides isolated testing for Bridge contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{IntentCommitment, IntentNullifier, MerkleNode, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use darkfi_serial::Encodable;

use darkfi_bridge_contract::client::{
    deposit_v1::{DepositCallData, DepositPublicInputs, create_deposit_proof},
    withdraw_v1::{WithdrawCallData, WithdrawPublicInputs, create_withdraw_proof},
};
use darkfi_bridge_contract::model::{DepositParams, ExternalChain, WithdrawParams};

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
}

impl BridgeHarness {
    /// Spawn a new Bridge harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let deposit_bin = include_bytes!("../../../bridge/proof/deposit_v1.zk.bin");
        let withdraw_bin = include_bytes!("../../../bridge/proof/withdraw_v1.zk.bin");

        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let withdraw_zkbin = ZkBinary::decode(withdraw_bin, false).unwrap();

        let deposit_pk = ProvingKey::build(
            deposit_zkbin.k,
            &ZkCircuit::new(darkfi::zk::empty_witnesses(&deposit_zkbin).unwrap(), &deposit_zkbin),
        );
        let withdraw_pk = ProvingKey::build(
            withdraw_zkbin.k,
            &ZkCircuit::new(darkfi::zk::empty_witnesses(&withdraw_zkbin).unwrap(), &withdraw_zkbin),
        );

        Self { deposit_zkbin, deposit_pk, withdraw_zkbin, withdraw_pk }
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
            recipient_pub_x: public_inputs.recipient_pub_x.to_repr(),
            recipient_pub_y: public_inputs.recipient_pub_y.to_repr(),
            bridge_nonce,
            chain,
            external_block_hash: public_inputs.external_block_hash.to_repr(),
            merkle_proof: merkle_path.iter().map(|n| n.to_bytes()).collect(),
            external_state_root: public_inputs.merkle_root_input.to_repr(),
            fee,
            proof: proof.as_ref().to_vec(),
            xmr_proof: None,
            zec_proof: None,
            azt_proof: None,
            ltc_proof: None,
        };

        let mut call_data = vec![0x01];
        params.encode(&mut call_data)?;

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
    ) -> Result<WithdrawResult, Box<dyn std::error::Error>> {
        let input = WithdrawCallData::new(
            secret,
            amount,
            recipient_hash,
            bridge_address,
            merkle_root,
            merkle_proof,
            leaf_index,
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
            amount,
            proof: proof.as_ref().to_vec(),
            fee,
        };

        let mut call_data = vec![0x02];
        params.encode(&mut call_data)?;

        Ok(WithdrawResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for BridgeHarness {
    fn name(&self) -> &str {
        "bridge"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["DepositV1", "WithdrawV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "DepositV1" => Some(&self.deposit_zkbin),
            "WithdrawV1" => Some(&self.withdraw_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "DepositV1" => Some(&self.deposit_pk),
            "WithdrawV1" => Some(&self.withdraw_pk),
            _ => None,
        }
    }
}

/// Result of deposit
pub struct DepositResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: DepositPublicInputs,
}

/// Result of withdraw
pub struct WithdrawResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: WithdrawPublicInputs,
}
