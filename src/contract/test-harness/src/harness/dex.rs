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

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{ContractId, SecretKey, Nullifier, pasta_prelude::PrimeField},
    pasta::pallas,
};
use dwow_promissory_note_contract::model::CapCommitment;
use dwow_serial::Encodable;

/// The commitment a call set's proofs must bind to (`OBL-C198`) — over the **whole ordered set**
/// the node will hash, children before parents, each call serialized with its contract id. One
/// helper because every builder needs the same derivation and a second copy is a second value
/// (`safety.md` RC5).
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

/// Helper to convert pallas::Base to CapCommitment
fn to_cap_commitment(base: pallas::Base) -> CapCommitment {
    CapCommitment::decode(&base.to_repr()).unwrap()
}

/// Helper to convert pallas::Base to Nullifier
fn to_nullifier(base: pallas::Base) -> Nullifier {
    Nullifier::from_bytes(base.to_repr()).unwrap()
}

/// Helper to convert pallas::Base to [u8; 32]
fn base_to_bytes(base: pallas::Base) -> [u8; 32] {
    base.to_repr()
}

use dwow_dex_contract::client::{
    accept_swap::{create_accept_swap_proof, AcceptSwapCallData, AcceptSwapPublicInputs},
    cancel_swap::{create_cancel_swap_proof, CancelSwapCallData, CancelSwapPublicInputs},
    create_swap::{create_create_swap_proof, CreateSwapCallData, CreateSwapPublicInputs},
    execute_swap_fee::{
        create_execute_swap_fee_proof, ExecuteSwapFeeCallData, ExecuteSwapFeePublicInputs,
    },
    execute_swap_slippage::{
        create_execute_swap_slippage_proof, ExecuteSwapSlippageCallData,
        ExecuteSwapSlippagePublicInputs,
    },
    execute_swap::{create_execute_swap_proof, ExecuteSwapCallData, ExecuteSwapPublicInputs},
};
use dwow_dex_contract::model::{
    AcceptSwapParams, CancelSwapParams, CreateSwapParams, ExecuteSwapParams,
};

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
    /// ExecuteSwapFee_V1 ZkBinary
    execute_swap_fee_zkbin: ZkBinary,
    /// ExecuteSwapFee_V1 ProvingKey
    execute_swap_fee_pk: ProvingKey,
    /// ExecuteSwapSlippage_V1 ZkBinary
    execute_swap_slippage_zkbin: ZkBinary,
    /// ExecuteSwapSlippage_V1 ProvingKey
    execute_swap_slippage_pk: ProvingKey,
    /// SetTransparencyLevel_V1 ZkBinary
    set_transparency_level_zkbin: ZkBinary,
    /// SetTransparencyLevel_V1 ProvingKey
    set_transparency_level_pk: ProvingKey,
    /// UpdateConfig_V1 ZkBinary
    update_config_zkbin: ZkBinary,
    /// UpdateConfig_V1 ProvingKey
    update_config_pk: ProvingKey,
    /// The deployed id of the dex contract this harness builds calls for — the commitment covers the
    /// call *including* the contract id, so a harness that proves must be told it (`OBL-C198`).
    contract_id: ContractId,
}

impl DexHarness {
    /// Spawn a new DEX harness for the contract deployed at `contract_id` (alias for `new`) — see the
    /// field's note for why the id cannot be defaulted.
    pub fn spawn(contract_id: ContractId) -> Self {
        Self::new(contract_id)
    }

    /// Create a new DEX harness with pre-loaded circuits
    pub fn new(contract_id: ContractId) -> Self {
        // Load circuit binaries
        let create_bin = include_bytes!("../../../dex/proof/create_swap.zk.bin");
        let accept_bin = include_bytes!("../../../dex/proof/accept_swap.zk.bin");
        let execute_bin = include_bytes!("../../../dex/proof/execute_swap.zk.bin");
        let cancel_bin = include_bytes!("../../../dex/proof/cancel_swap.zk.bin");
        let execute_swap_fee_bin = include_bytes!("../../../dex/proof/execute_swap_fee.zk.bin");
        let execute_swap_slippage_bin = include_bytes!("../../../dex/proof/execute_swap_slippage.zk.bin");
        let set_transparency_level_bin = include_bytes!("../../../dex/proof/set_transparency_level.zk.bin");
        let update_config_bin = include_bytes!("../../../dex/proof/update_config.zk.bin");

        let create_swap_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let accept_swap_zkbin = ZkBinary::decode(accept_bin, false).unwrap();
        let execute_swap_zkbin = ZkBinary::decode(execute_bin, false).unwrap();
        let cancel_swap_zkbin = ZkBinary::decode(cancel_bin, false).unwrap();
        let execute_swap_fee_zkbin = ZkBinary::decode(execute_swap_fee_bin, false).unwrap();
        let execute_swap_slippage_zkbin = ZkBinary::decode(execute_swap_slippage_bin, false).unwrap();
        let set_transparency_level_zkbin = ZkBinary::decode(set_transparency_level_bin, false).unwrap();
        let update_config_zkbin = ZkBinary::decode(update_config_bin, false).unwrap();

        // Build proving keys
        let create_swap_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_swap_zkbin).unwrap(),
            &create_swap_zkbin,
        );
        let accept_swap_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&accept_swap_zkbin).unwrap(),
            &accept_swap_zkbin,
        );
        let execute_swap_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&execute_swap_zkbin).unwrap(),
            &execute_swap_zkbin,
        );
        let cancel_swap_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&cancel_swap_zkbin).unwrap(),
            &cancel_swap_zkbin,
        );
        let execute_swap_fee_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&execute_swap_fee_zkbin).unwrap(),
            &execute_swap_fee_zkbin,
        );
        let execute_swap_slippage_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&execute_swap_slippage_zkbin).unwrap(),
            &execute_swap_slippage_zkbin,
        );
        let set_transparency_level_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&set_transparency_level_zkbin).unwrap(),
            &set_transparency_level_zkbin,
        );
        let update_config_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&update_config_zkbin).unwrap(),
            &update_config_zkbin,
        );

        let create_swap_pk = ProvingKey::build(create_swap_zkbin.k, &create_swap_circuit).expect("ProvingKey::build failed");
        let accept_swap_pk = ProvingKey::build(accept_swap_zkbin.k, &accept_swap_circuit).expect("ProvingKey::build failed");
        let execute_swap_pk = ProvingKey::build(execute_swap_zkbin.k, &execute_swap_circuit).expect("ProvingKey::build failed");
        let cancel_swap_pk = ProvingKey::build(cancel_swap_zkbin.k, &cancel_swap_circuit).expect("ProvingKey::build failed");
        let execute_swap_fee_pk = ProvingKey::build(execute_swap_fee_zkbin.k, &execute_swap_fee_circuit).expect("ProvingKey::build failed");
        let execute_swap_slippage_pk = ProvingKey::build(execute_swap_slippage_zkbin.k, &execute_swap_slippage_circuit).expect("ProvingKey::build failed");
        let set_transparency_level_pk = ProvingKey::build(set_transparency_level_zkbin.k, &set_transparency_level_circuit).expect("ProvingKey::build failed");
        let update_config_pk = ProvingKey::build(update_config_zkbin.k, &update_config_circuit).expect("ProvingKey::build failed");

        Self {
            create_swap_zkbin,
            create_swap_pk,
            accept_swap_zkbin,
            accept_swap_pk,
            execute_swap_zkbin,
            execute_swap_pk,
            cancel_swap_zkbin,
            cancel_swap_pk,
            execute_swap_fee_zkbin,
            execute_swap_fee_pk,
            execute_swap_slippage_zkbin,
            execute_swap_slippage_pk,
            set_transparency_level_zkbin,
            set_transparency_level_pk,
            update_config_zkbin,
            update_config_pk,
            contract_id,
        }
    }

    /// Create a swap proposal with ZK proof and return encoded call data
    pub fn create_swap(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        request_token: pallas::Base,
        request_amount: u64,
        ephemeral_signature_secret: SecretKey,
    ) -> Result<CreateSwapResult, Box<dyn std::error::Error>> {
        let mut input = CreateSwapCallData::new_deterministic(
            secret,
            offer_token,
            offer_amount,
            request_token,
            request_amount,
            ephemeral_signature_secret,
        );

        // `OBL-C198`: the call data first, the commitment over the whole ordered set next, the proof
        // last — the reverse order cannot bind to a real transaction. The public inputs are a pure
        // function of the call data's own inputs, so they need no proof.
        let public_inputs = input.compute_public_inputs();

        // Build CreateSwapParams
        let params = CreateSwapParams {
            swap_id: base_to_bytes(public_inputs.swap_id),
            offer_token: base_to_bytes(offer_token),
            offer_amount,
            request_token: base_to_bytes(request_token),
            request_amount,
            lock_commitment: to_cap_commitment(public_inputs.lock_commitment),
            nullifier: to_nullifier(public_inputs.nullifier),
            signature_public: input.signature_public,
            fee: 0,
            open_execution: false,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x01]; // CreateSwapV1
        call_data.extend_from_slice(&params.encode());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_create_swap_proof(
            &self.create_swap_zkbin,
            &self.create_swap_pk,
            &input,
        )?;

        Ok(CreateSwapResult {
            call_data,
            proof,
            public_inputs,
            commitment,
        })
    }

    /// Accept a swap with ZK proof
    pub fn accept_swap(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        swap_id: pallas::Base,
        proposer_lock_commitment: pallas::Base,
        secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        ephemeral_signature_secret: SecretKey,
    ) -> Result<AcceptSwapResult, Box<dyn std::error::Error>> {
        let mut input = AcceptSwapCallData::new_deterministic(
            swap_id,
            proposer_lock_commitment,
            secret,
            offer_token,
            offer_amount,
            ephemeral_signature_secret,
        );

        // `OBL-C198`: the call data first, the commitment over the whole ordered set next, the proof
        // last — see `create_swap`.
        let public_inputs = input.compute_public_inputs();

        // Build AcceptSwapParams
        let params = AcceptSwapParams {
            swap_id: base_to_bytes(swap_id),
            lock_commitment: to_cap_commitment(public_inputs.acceptor_lock_commitment),
            nullifier: to_nullifier(public_inputs.acceptor_nullifier),
            signature_public: input.signature_public,
            fee: 0,
            immediate_execute: false,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x02]; // AcceptSwapV1
        call_data.extend_from_slice(&params.encode());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_accept_swap_proof(
            &self.accept_swap_zkbin,
            &self.accept_swap_pk,
            &input,
        )?;

        Ok(AcceptSwapResult {
            call_data,
            swap_id,
            proposer_lock_commitment,
            secret,
            proof,
            public_inputs,
            commitment,
        })
    }

    /// Execute a swap with ZK proof
    pub fn execute_swap(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
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
        let mut input = ExecuteSwapCallData::new_deterministic(
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

        // `OBL-C198`: the call data first, the commitment over the whole ordered set next, the proof
        // last — see `create_swap`.
        let public_inputs = input.compute_public_inputs();

        // Build ExecuteSwapParams
        let params = ExecuteSwapParams {
            swap_id: base_to_bytes(public_inputs.swap_id),
            alice_secret: base_to_bytes(alice_secret),
            bob_secret: base_to_bytes(bob_secret),
            alice_lock: to_cap_commitment(public_inputs.alice_lock),
            bob_lock: to_cap_commitment(public_inputs.bob_lock),
            alice_nullifier: to_nullifier(public_inputs.alice_nullifier),
            bob_nullifier: to_nullifier(public_inputs.bob_nullifier),
            proof: vec![], // Placeholder
            fee: 0,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x03]; // ExecuteSwapV1
        call_data.extend_from_slice(&params.encode());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_execute_swap_proof(
            &self.execute_swap_zkbin,
            &self.execute_swap_pk,
            &input,
        )?;

        Ok(ExecuteSwapResult {
            call_data,
            proof,
            public_inputs,
            commitment,
        })
    }

    /// Cancel a swap with ZK proof
    pub fn cancel_swap(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        swap_id: pallas::Base,
        lock_commitment: pallas::Base,
        secret: pallas::Base,
        token: pallas::Base,
        amount: u64,
        request_token: pallas::Base,
        request_amount: u64,
    ) -> Result<CancelSwapResult, Box<dyn std::error::Error>> {
        let mut input = CancelSwapCallData::new(
            swap_id,
            lock_commitment,
            secret,
            token,
            amount,
            request_token,
            request_amount,
        );

        // `OBL-C198`: the call data first, the commitment over the whole ordered set next, the proof
        // last — see `create_swap`.
        let public_inputs = input.compute_public_inputs();

        // Build CancelSwapParams
        let params = CancelSwapParams {
            swap_id: base_to_bytes(swap_id),
            secret: base_to_bytes(secret),
            nullifier: to_nullifier(public_inputs.nullifier),
            proof: vec![], // Placeholder
            fee: 0,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x04]; // CancelSwapV1
        call_data.extend_from_slice(&params.encode());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_cancel_swap_proof(
            &self.cancel_swap_zkbin,
            &self.cancel_swap_pk,
            &input,
        )?;

        Ok(CancelSwapResult {
            call_data,
            proof,
            public_inputs,
            commitment,
        })
    }

    /// Execute swap with fee (function code 0x07)
    #[allow(clippy::too_many_arguments)]
    pub fn execute_swap_fee(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: pallas::Base,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: pallas::Base,
        bob_lock: pallas::Base,
        fill_amount: pallas::Base,
        fee_bps: pallas::Base,
    ) -> Result<ExecuteSwapFeeResult, Box<dyn std::error::Error>> {
        let mut input = ExecuteSwapFeeCallData::new(
            alice_secret, alice_token, alice_amount, alice_lock,
            bob_secret, bob_token, bob_amount, bob_lock, fill_amount, fee_bps,
        );
        // `OBL-C198`: the public inputs first (this fixture writes them as the call data — see the
        // note in the harness's history), then the commitment over the whole ordered set, then the
        // proof. The pair the client derives is the stale `H(3, 0, 0)` in those bytes; the arm
        // publishes the derived one, which is what the proof is bound to.
        let public_inputs = input.compute_public_inputs();

        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&public_inputs.to_vec().iter()
            .flat_map(|x| x.to_repr().to_vec()).collect::<Vec<u8>>());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_execute_swap_fee_proof(
            &self.execute_swap_fee_zkbin, &self.execute_swap_fee_pk, &input,
        )?;

        Ok(ExecuteSwapFeeResult { call_data, proof, public_inputs, commitment })
    }

    /// Execute swap with slippage protection (function code 0x08)
    #[allow(clippy::too_many_arguments)]
    pub fn execute_swap_slippage(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: pallas::Base,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: pallas::Base,
        bob_lock: pallas::Base,
        fill_amount: pallas::Base,
        slippage_bps: pallas::Base,
    ) -> Result<ExecuteSwapSlippageResult, Box<dyn std::error::Error>> {
        let mut input = ExecuteSwapSlippageCallData::new(
            alice_secret, alice_token, alice_amount, alice_lock,
            bob_secret, bob_token, bob_amount, bob_lock, fill_amount, slippage_bps,
        );
        // `OBL-C198`: as `execute_swap_fee` above.
        let public_inputs = input.compute_public_inputs();

        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&public_inputs.to_vec().iter()
            .flat_map(|x| x.to_repr().to_vec()).collect::<Vec<u8>>());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_execute_swap_slippage_proof(
            &self.execute_swap_slippage_zkbin, &self.execute_swap_slippage_pk, &input,
        )?;

        Ok(ExecuteSwapSlippageResult { call_data, proof, public_inputs, commitment })
    }

    /// Set transparency level (function code 0x06)
    pub fn set_transparency_level(
        &self, level: u8,
    ) -> Result<SetTransparencyLevelResult, Box<dyn std::error::Error>> {
        let w = dwow_core::zk::empty_witnesses(&self.set_transparency_level_zkbin)?;
        let c = ZkCircuit::new(w, &self.set_transparency_level_zkbin);
        let proof = Proof::create(&self.set_transparency_level_pk, &[c], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;
        Ok(SetTransparencyLevelResult { call_data: vec![0x06], proof })
    }

    /// Update DEX configuration (function code 0x05)
    pub fn update_config(
        &self, timeout: u64, fee: u64,
    ) -> Result<UpdateConfigDexResult, Box<dyn std::error::Error>> {
        let w = dwow_core::zk::empty_witnesses(&self.update_config_zkbin)?;
        let c = ZkCircuit::new(w, &self.update_config_zkbin);
        let proof = Proof::create(&self.update_config_pk, &[c], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;
        Ok(UpdateConfigDexResult { call_data: vec![0x05], proof })
    }
}

// `impl Default for DexHarness` stood here, delegating to `new()`. `OBL-C198` gave `new` the
// contract id it must be told — the commitment covers the call including the id — and a `Default`
// has no honest id to supply, so the impl is removed rather than given a zero placeholder that
// would prove against the wrong contract.

impl super::ContractHarness for DexHarness {
    fn name(&self) -> &str {
        "dex"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateSwapV2",
            "AcceptSwapV2",
            "ExecuteSwapV2",
            "CancelSwapV2",
            "ExecuteSwapFeeV2",
            "ExecuteSwapSlippageV2",
            "SetTransparencyLevelV2",
            "UpdateConfigV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwapV2" => Some(&self.create_swap_zkbin),
            "AcceptSwapV2" => Some(&self.accept_swap_zkbin),
            "ExecuteSwapV2" => Some(&self.execute_swap_zkbin),
            "CancelSwapV2" => Some(&self.cancel_swap_zkbin),
            "ExecuteSwapFeeV2" => Some(&self.execute_swap_fee_zkbin),
            "ExecuteSwapSlippageV2" => Some(&self.execute_swap_slippage_zkbin),
            "SetTransparencyLevelV2" => Some(&self.set_transparency_level_zkbin),
            "UpdateConfigV2" => Some(&self.update_config_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwapV2" => Some(&self.create_swap_pk),
            "AcceptSwapV2" => Some(&self.accept_swap_pk),
            "ExecuteSwapV2" => Some(&self.execute_swap_pk),
            "CancelSwapV2" => Some(&self.cancel_swap_pk),
            "ExecuteSwapFeeV2" => Some(&self.execute_swap_fee_pk),
            "ExecuteSwapSlippageV2" => Some(&self.execute_swap_slippage_pk),
            "SetTransparencyLevelV2" => Some(&self.set_transparency_level_pk),
            "UpdateConfigV2" => Some(&self.update_config_pk),
            _ => None,
        }
    }
}

/// Result of create_swap
pub struct CreateSwapResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CreateSwapPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so a caller can prove
    /// a child against the same value.
    pub commitment: pallas::Base,
}

/// Result of accept_swap
pub struct AcceptSwapResult {
    pub call_data: Vec<u8>,
    pub swap_id: pallas::Base,
    pub proposer_lock_commitment: pallas::Base,
    pub secret: pallas::Base,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AcceptSwapPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of execute_swap
pub struct ExecuteSwapResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ExecuteSwapPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of cancel_swap
pub struct CancelSwapResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CancelSwapPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of execute_swap_fee
pub struct ExecuteSwapFeeResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ExecuteSwapFeePublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of execute_swap_slippage
pub struct ExecuteSwapSlippageResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ExecuteSwapSlippagePublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

pub struct SetTransparencyLevelResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct UpdateConfigDexResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
