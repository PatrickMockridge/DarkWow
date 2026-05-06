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
    crypto::{IntentCommitment, MerkleNode, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use darkfi_serial::Encodable;

use darkfi_bridge_contract::client::deposit_v1::{create_deposit_proof, DepositCallData, DepositPublicInputs};
use darkfi_bridge_contract::model::{DepositParams, ExternalChain};

/// Bridge Harness for isolated testing
pub struct BridgeHarness {
    /// Deposit_V1 ZkBinary
    deposit_zkbin: ZkBinary,
    /// Deposit_V1 ProvingKey
    deposit_pk: ProvingKey,
}

impl BridgeHarness {
    /// Spawn a new Bridge harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let deposit_bin = include_bytes!("../../../bridge/proof/deposit_v1.zk.bin");

        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();

        let deposit_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&deposit_zkbin).unwrap(),
            &deposit_zkbin,
        );

        let deposit_pk = ProvingKey::build(deposit_zkbin.k, &deposit_circuit);

        Self { deposit_zkbin, deposit_pk }
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

        let mut call_data = vec![0x01]; // DepositV1 function code
        params.encode(&mut call_data)?;

        Ok(DepositResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for BridgeHarness {
    fn name(&self) -> &str {
        "bridge"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["DepositV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "DepositV1" => Some(&self.deposit_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "DepositV1" => Some(&self.deposit_pk),
            _ => None,
        }
    }
}

/// Result of deposit
pub struct DepositResult {
    /// Encoded call data for contract execution (function code 0x01 + DepositParams)
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: darkfi::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: DepositPublicInputs,
}