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

//! DEX Test Harness
//!
//! Provides isolated testing for DEX atomic swap contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{SecretKey, PublicKey, IntentCommitment, IntentNullifier, pasta_prelude::PrimeField},
    pasta::pallas,
};
use darkfi_serial::{Encodable, Decodable};

/// Helper to convert pallas::Base to IntentCommitment
fn to_intent_commitment(base: pallas::Base) -> IntentCommitment {
    IntentCommitment::from_bytes(base.to_repr()).unwrap()
}

/// Helper to convert pallas::Base to IntentNullifier
fn to_intent_nullifier(base: pallas::Base) -> IntentNullifier {
    IntentNullifier::from_bytes(base.to_repr()).unwrap()
}

/// Helper to convert pallas::Base to [u8; 32]
fn base_to_bytes(base: pallas::Base) -> [u8; 32] {
    base.to_repr()
}

use darkfi_dex_contract::client::{
    create_swap_v1::{create_create_swap_proof, CreateSwapCallData, CreateSwapPublicInputs},
    accept_swap_v1::{create_accept_swap_proof, AcceptSwapCallData, AcceptSwapPublicInputs},
    execute_swap_v1::{create_execute_swap_proof, ExecuteSwapCallData, ExecuteSwapPublicInputs},
    cancel_swap_v1::{create_cancel_swap_proof, CancelSwapCallData, CancelSwapPublicInputs},
};
use darkfi_dex_contract::model::{
    CreateSwapParams, AcceptSwapParams, ExecuteSwapParams, CancelSwapParams,
};
use darkfi_dex_contract::DexFunction;

/// DEX Harness for atomic swap testing
pub struct DexHarness {
    /// CreateSwap_V1 ZkBinary
    create_swap_zkbin: ZkBinary,
    /// CreateSwap_V1 ProvingKey
    create_swap_pk: ProvingKey,
    /// AcceptSwap_V1 ZkBinary
    accept_swap_zkbin: ZkBinary,
    /// AcceptSwap_V1 ProvingKey
    accept_swap_pk: ProvingKey,
    /// ExecuteSwap_V1 ZkBinary
    execute_swap_zkbin: ZkBinary,
    /// ExecuteSwap_V1 ProvingKey
    execute_swap_pk: ProvingKey,
    /// CancelSwap_V1 ZkBinary
    cancel_swap_zkbin: ZkBinary,
    /// CancelSwap_V1 ProvingKey
    cancel_swap_pk: ProvingKey,
}

impl DexHarness {
    /// Create a new DEX harness with pre-loaded circuits
    pub fn new() -> Self {
        // Load circuit binaries
        let create_bin = include_bytes!("../../../dex/proof/create_swap_v1.zk.bin");
        let accept_bin = include_bytes!("../../../dex/proof/accept_swap_v1.zk.bin");
        let execute_bin = include_bytes!("../../../dex/proof/execute_swap_v1.zk.bin");
        let cancel_bin = include_bytes!("../../../dex/proof/cancel_swap_v1.zk.bin");

        let create_swap_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let accept_swap_zkbin = ZkBinary::decode(accept_bin, false).unwrap();
        let execute_swap_zkbin = ZkBinary::decode(execute_bin, false).unwrap();
        let cancel_swap_zkbin = ZkBinary::decode(cancel_bin, false).unwrap();

        // Build proving keys
        let create_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_swap_zkbin).unwrap(),
            &create_swap_zkbin,
        );
        let accept_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&accept_swap_zkbin).unwrap(),
            &accept_swap_zkbin,
        );
        let execute_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&execute_swap_zkbin).unwrap(),
            &execute_swap_zkbin,
        );
        let cancel_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&cancel_swap_zkbin).unwrap(),
            &cancel_swap_zkbin,
        );

        let create_swap_pk = ProvingKey::build(create_swap_zkbin.k, &create_swap_circuit);
        let accept_swap_pk = ProvingKey::build(accept_swap_zkbin.k, &accept_swap_circuit);
        let execute_swap_pk = ProvingKey::build(execute_swap_zkbin.k, &execute_swap_circuit);
        let cancel_swap_pk = ProvingKey::build(cancel_swap_zkbin.k, &cancel_swap_circuit);

        Self {
            create_swap_zkbin,
            create_swap_pk,
            accept_swap_zkbin,
            accept_swap_pk,
            execute_swap_zkbin,
            execute_swap_pk,
            cancel_swap_zkbin,
            cancel_swap_pk,
        }
    }

    /// Get circuit namespaces
    pub fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateSwap_V1",
            "AcceptSwap_V1",
            "ExecuteSwap_V1",
            "CancelSwap_V1",
        ]
    }

    /// Create a swap proposal with ZK proof and return encoded call data
    pub fn create_swap(
        &self,
        secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        request_token: pallas::Base,
        request_amount: u64,
        signature_secret: SecretKey,
    ) -> Result<CreateSwapResult, Box<dyn std::error::Error>> {
        let input = CreateSwapCallData::new(
            secret,
            offer_token,
            offer_amount,
            request_token,
            request_amount,
            signature_secret,
        );

        let (proof, public_inputs) = create_create_swap_proof(
            &self.create_swap_zkbin,
            &self.create_swap_pk,
            &input,
        )?;

        // Build CreateSwapParams
        let params = CreateSwapParams {
            swap_id: base_to_bytes(public_inputs.swap_id),
            offer_token: base_to_bytes(offer_token),
            offer_amount,
            request_token: base_to_bytes(request_token),
            request_amount,
            lock_commitment: to_intent_commitment(public_inputs.lock_commitment),
            nullifier: to_intent_nullifier(public_inputs.nullifier),
            lock_proof: vec![[0u8; 32]; 32], // Placeholder Merkle proof
            signature_public: input.signature_public,
            fee: 0,
            open_execution: false,
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CreateSwapResult {
            call_data,
            proof,
            public_inputs,
        })
    }

    /// Accept a swap with ZK proof
    pub fn accept_swap(
        &self,
        swap_id: pallas::Base,
        proposer_lock_commitment: pallas::Base,
        secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        signature_secret: SecretKey,
    ) -> Result<AcceptSwapResult, Box<dyn std::error::Error>> {
        let input = AcceptSwapCallData::new(
            swap_id,
            proposer_lock_commitment,
            secret,
            offer_token,
            offer_amount,
            signature_secret,
        );

        let (proof, public_inputs) = create_accept_swap_proof(
            &self.accept_swap_zkbin,
            &self.accept_swap_pk,
            &input,
        )?;

        // Build AcceptSwapParams
        let params = AcceptSwapParams {
            swap_id: base_to_bytes(swap_id),
            lock_commitment: to_intent_commitment(public_inputs.acceptor_lock_commitment),
            nullifier: to_intent_nullifier(public_inputs.acceptor_nullifier),
            lock_proof: vec![[0u8; 32]; 32], // Placeholder Merkle proof
            signature_public: input.signature_public,
            fee: 0,
            immediate_execute: false,
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(AcceptSwapResult {
            call_data,
            swap_id,
            proposer_lock_commitment,
            secret,
            proof,
            public_inputs,
        })
    }

    /// Execute a swap with ZK proof
    pub fn execute_swap(
        &self,
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: u64,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: u64,
        bob_lock: pallas::Base,
        fill_amount: u64,
        alice_otc_func_id: pallas::Base,
        bob_otc_func_id: pallas::Base,
    ) -> Result<ExecuteSwapResult, Box<dyn std::error::Error>> {
        let input = ExecuteSwapCallData::new(
            alice_secret,
            alice_token,
            alice_amount,
            alice_lock,
            bob_secret,
            bob_token,
            bob_amount,
            bob_lock,
            fill_amount,
            alice_otc_func_id,
            bob_otc_func_id,
        );

        let (proof, public_inputs) = create_execute_swap_proof(
            &self.execute_swap_zkbin,
            &self.execute_swap_pk,
            &input,
        )?;

        // Build ExecuteSwapParams
        let params = ExecuteSwapParams {
            swap_id: base_to_bytes(public_inputs.swap_id),
            alice_secret: base_to_bytes(alice_secret),
            bob_secret: base_to_bytes(bob_secret),
            alice_lock: to_intent_commitment(public_inputs.alice_lock),
            bob_lock: to_intent_commitment(public_inputs.bob_lock),
            alice_nullifier: to_intent_nullifier(public_inputs.alice_nullifier),
            bob_nullifier: to_intent_nullifier(public_inputs.bob_nullifier),
            proof: vec![], // Placeholder
            fee: 0,
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(ExecuteSwapResult {
            call_data,
            proof,
            public_inputs,
        })
    }

    /// Cancel a swap with ZK proof
    pub fn cancel_swap(
        &self,
        swap_id: pallas::Base,
        lock_commitment: pallas::Base,
        secret: pallas::Base,
        token: pallas::Base,
        amount: u64,
    ) -> Result<CancelSwapResult, Box<dyn std::error::Error>> {
        let input = CancelSwapCallData::new(
            swap_id,
            lock_commitment,
            secret,
            token,
            amount,
        );

        let (proof, public_inputs) = create_cancel_swap_proof(
            &self.cancel_swap_zkbin,
            &self.cancel_swap_pk,
            &input,
        )?;

        // Build CancelSwapParams
        let params = CancelSwapParams {
            swap_id: base_to_bytes(swap_id),
            secret: base_to_bytes(secret),
            nullifier: to_intent_nullifier(public_inputs.nullifier),
            proof: vec![], // Placeholder
            fee: 0,
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CancelSwapResult {
            call_data,
            proof,
            public_inputs,
        })
    }
}

impl Default for DexHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl super::ContractHarness for DexHarness {
    fn name(&self) -> &str {
        "dex"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateSwapV1",
            "AcceptSwapV1",
            "ExecuteSwapV1",
            "CancelSwapV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwapV1" => Some(&self.create_swap_zkbin),
            "AcceptSwapV1" => Some(&self.accept_swap_zkbin),
            "ExecuteSwapV1" => Some(&self.execute_swap_zkbin),
            "CancelSwapV1" => Some(&self.cancel_swap_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwapV1" => Some(&self.create_swap_pk),
            "AcceptSwapV1" => Some(&self.accept_swap_pk),
            "ExecuteSwapV1" => Some(&self.execute_swap_pk),
            "CancelSwapV1" => Some(&self.cancel_swap_pk),
            _ => None,
        }
    }
}

/// Result of create_swap
pub struct CreateSwapResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: CreateSwapPublicInputs,
}

/// Result of accept_swap
pub struct AcceptSwapResult {
    pub call_data: Vec<u8>,
    pub swap_id: pallas::Base,
    pub proposer_lock_commitment: pallas::Base,
    pub secret: pallas::Base,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: AcceptSwapPublicInputs,
}

/// Result of execute_swap
pub struct ExecuteSwapResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: ExecuteSwapPublicInputs,
}

/// Result of cancel_swap
pub struct CancelSwapResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: CancelSwapPublicInputs,
}
