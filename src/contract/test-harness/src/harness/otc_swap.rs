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

//! OTC Swap Test Harness

use dwow_core::{
    zk::{empty_witnesses, Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{pedersen_commitment_u64, Blind, MerkleNode, PublicKey},
    crypto::pasta_prelude::Group,
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_otc_swap_contract::client::{
    cancel_swap::{cancel_swap_proof, CancelSwapCallData, CancelSwapPublicInputs},
    create_swap::{create_swap_proof, CreateSwapCallData, CreateSwapPublicInputs},
    execute_swap::{execute_swap_proof, ExecuteSwapCallData, ExecuteSwapPublicInputs},
    fund_swap::{fund_swap_proof, FundSwapCallData, FundSwapPublicInputs},
};
use dwow_otc_swap_contract::model::{
    CancelSwapParamsV1, CreateSwapParamsV1, ExecuteSwapParamsV1, FundSwapParamsV1,
};

/// OTC Swap Harness for isolated testing
pub struct OtcSwapHarness {
    create_zkbin: ZkBinary,
    create_pk: ProvingKey,
    fund_zkbin: ZkBinary,
    fund_pk: ProvingKey,
    execute_zkbin: ZkBinary,
    execute_pk: ProvingKey,
    cancel_zkbin: ZkBinary,
    cancel_pk: ProvingKey,
}

impl OtcSwapHarness {
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../otc_swap/proof/create_swap.zk.bin");
        let fund_bin = include_bytes!("../../../otc_swap/proof/fund_swap.zk.bin");
        let execute_bin = include_bytes!("../../../otc_swap/proof/execute_swap.zk.bin");
        let cancel_bin = include_bytes!("../../../otc_swap/proof/cancel_swap.zk.bin");

        let create_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let create_circuit = ZkCircuit::new(empty_witnesses(&create_zkbin).unwrap(), &create_zkbin);
        let create_pk = ProvingKey::build(create_zkbin.k, &create_circuit).expect("ProvingKey::build failed");
        let fund_zkbin = ZkBinary::decode(fund_bin, false).unwrap();
        let fund_circuit = ZkCircuit::new(empty_witnesses(&fund_zkbin).unwrap(), &fund_zkbin);
        let fund_pk = ProvingKey::build(fund_zkbin.k, &fund_circuit).expect("ProvingKey::build failed");
        let execute_zkbin = ZkBinary::decode(execute_bin, false).unwrap();
        let execute_circuit = ZkCircuit::new(empty_witnesses(&execute_zkbin).unwrap(), &execute_zkbin);
        let execute_pk = ProvingKey::build(execute_zkbin.k, &execute_circuit).expect("ProvingKey::build failed");
        let cancel_zkbin = ZkBinary::decode(cancel_bin, false).unwrap();
        let cancel_circuit = ZkCircuit::new(empty_witnesses(&cancel_zkbin).unwrap(), &cancel_zkbin);
        let cancel_pk = ProvingKey::build(cancel_zkbin.k, &cancel_circuit).expect("ProvingKey::build failed");

        Self { create_zkbin, create_pk, fund_zkbin, fund_pk, execute_zkbin, execute_pk, cancel_zkbin, cancel_pk }
    }

    /// Create an OTC swap (function code 0x01)
    #[allow(clippy::too_many_arguments)]
    pub fn create_swap(
        &self,
        alice_secret: pallas::Base,
        alice_pubkey: PublicKey,
        bob_pubkey: PublicKey,
        send_value: u64,
        send_asset_id: pallas::Base,
        recv_value: u64,
        recv_asset_id: pallas::Base,
        timeout: u64,
    ) -> Result<CreateSwapResult, Box<dyn std::error::Error>> {
        let input = CreateSwapCallData::new(
            alice_secret, alice_pubkey, bob_pubkey,
            send_value, send_asset_id, recv_value, recv_asset_id, timeout,
        );
        let (proof, public_inputs) =
            create_swap_proof(&self.create_zkbin, &self.create_pk, &input)?;

        let params = CreateSwapParamsV1 {
            alice_pubkey,
            bob_pubkey,
            send_value,
            send_asset_id,
            recv_value,
            recv_asset_id,
            timeout,
            commitment: public_inputs.commitment,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateSwapResult { call_data, proof, swap_id: public_inputs.commitment })
    }

    /// Fund an OTC swap (function code 0x02)
    pub fn fund_swap(
        &self,
        value: u64,
        value_blind: pallas::Scalar,
        swap_id: pallas::Base,
        merkle_leaf_pos: u32,
        merkle_path: Vec<MerkleNode>,
    ) -> Result<FundSwapResult, Box<dyn std::error::Error>> {
        let input = FundSwapCallData::new(
            value, value_blind, swap_id, merkle_leaf_pos, merkle_path.clone(),
        );
        let (proof, public_inputs) =
            fund_swap_proof(&self.fund_zkbin, &self.fund_pk, &input)?;

        let merkle_root = MerkleNode::new(public_inputs.merkle_root);
        let merkle_proof: Vec<pallas::Base> = merkle_path.iter().map(|n| n.inner()).collect();

        let params = FundSwapParamsV1 {
            swap_id: public_inputs.swap_id,
            value_commit: pedersen_commitment_u64(value, Blind(value_blind)),
            merkle_proof,
            merkle_root,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        Ok(FundSwapResult { call_data, proof })
    }

    /// Execute an OTC swap (function code 0x03)
    pub fn execute_swap(
        &self,
        swap_id: pallas::Base,
        bob_secret: pallas::Base,
        bob_pubkey: PublicKey,
        alice_recipient: PublicKey,
        bob_recipient: PublicKey,
    ) -> Result<ExecuteSwapResult, Box<dyn std::error::Error>> {
        let input = ExecuteSwapCallData::new(
            swap_id, bob_secret, bob_pubkey, alice_recipient, bob_recipient,
        );
        let (proof, public_inputs) =
            execute_swap_proof(&self.execute_zkbin, &self.execute_pk, &input)?;

        let params = ExecuteSwapParamsV1 {
            swap_id: public_inputs.swap_id,
            bob_secret,
            spent_nullifier: public_inputs.spent_nullifier,
            alice_recipient,
            bob_recipient,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(ExecuteSwapResult { call_data, proof })
    }

    /// Cancel an OTC swap (function code 0x04)
    pub fn cancel_swap(
        &self,
        swap_id: pallas::Base,
        alice_secret: pallas::Base,
        alice_pubkey: PublicKey,
        timeout: u64,
        current_block: u64,
        recipient_pubkey: PublicKey,
    ) -> Result<CancelSwapResult, Box<dyn std::error::Error>> {
        let input = CancelSwapCallData::new(
            swap_id, alice_secret, alice_pubkey, timeout, current_block, recipient_pubkey,
        );
        let (proof, public_inputs) =
            cancel_swap_proof(&self.cancel_zkbin, &self.cancel_pk, &input)?;

        let params = CancelSwapParamsV1 {
            swap_id: public_inputs.swap_id,
            alice_secret,
            spent_nullifier: public_inputs.spent_nullifier,
            current_block,
            timeout,
            recipient_pubkey,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        Ok(CancelSwapResult { call_data, proof })
    }
}

impl super::ContractHarness for OtcSwapHarness {
    fn name(&self) -> &str {
        "otc_swap"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateSwapV2", "FundSwapV2", "ExecuteSwapV2", "CancelSwapV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwapV2" => Some(&self.create_zkbin),
            "FundSwapV2" => Some(&self.fund_zkbin),
            "ExecuteSwapV2" => Some(&self.execute_zkbin),
            "CancelSwapV2" => Some(&self.cancel_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwapV2" => Some(&self.create_pk),
            "FundSwapV2" => Some(&self.fund_pk),
            "ExecuteSwapV2" => Some(&self.execute_pk),
            "CancelSwapV2" => Some(&self.cancel_pk),
            _ => None,
        }
    }
}

/// Result of create_swap
pub struct CreateSwapResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
    pub swap_id: pallas::Base,
}

/// Result of fund_swap
pub struct FundSwapResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

/// Result of execute_swap
pub struct ExecuteSwapResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

/// Result of cancel_swap
pub struct CancelSwapResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}
