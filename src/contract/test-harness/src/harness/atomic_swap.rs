/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or modify
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

//! AtomicSwap Test Harness
//!
//! Provides isolated testing for AtomicSwap contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_serial::Encodable;

use darkfi_atomic_swap_contract::client::{
    create_swap_v1::{CreateSwapCallData, create_swap_proof, CreateSwapPublicInputs},
    claim_swap_v1::{ClaimSwapCallData, create_claim_proof, ClaimSwapPublicInputs},
    refund_swap_v1::{RefundSwapCallData, create_refund_proof, RefundSwapPublicInputs},
};
use darkfi_atomic_swap_contract::model::{
    CreateSwapParamsV1, ClaimParamsV1, RefundParamsV1,
};

/// AtomicSwap Harness for isolated testing
pub struct AtomicSwapHarness {
    /// CreateSwap_V1 ZkBinary
    create_swap_zkbin: ZkBinary,
    /// CreateSwap_V1 ProvingKey
    create_swap_pk: ProvingKey,
    /// ClaimSwap_V1 ZkBinary
    claim_swap_zkbin: ZkBinary,
    /// ClaimSwap_V1 ProvingKey
    claim_swap_pk: ProvingKey,
    /// RefundSwap_V1 ZkBinary
    refund_swap_zkbin: ZkBinary,
    /// RefundSwap_V1 ProvingKey
    refund_swap_pk: ProvingKey,
}

impl AtomicSwapHarness {
    /// Spawn a new AtomicSwap harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../atomic_swap/proof/create_swap_v1.zk.bin");
        let claim_bin = include_bytes!("../../../atomic_swap/proof/claim_v1.zk.bin");
        let refund_bin = include_bytes!("../../../atomic_swap/proof/refund_v1.zk.bin");

        let create_swap_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let claim_swap_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let refund_swap_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_swap_zkbin).unwrap(),
            &create_swap_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_swap_zkbin).unwrap(),
            &claim_swap_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&refund_swap_zkbin).unwrap(),
            &refund_swap_zkbin,
        );

        let create_swap_pk = ProvingKey::build(create_swap_zkbin.k, &create_circuit);
        let claim_swap_pk = ProvingKey::build(claim_swap_zkbin.k, &claim_circuit);
        let refund_swap_pk = ProvingKey::build(refund_swap_zkbin.k, &refund_circuit);

        Self {
            create_swap_zkbin,
            create_swap_pk,
            claim_swap_zkbin,
            claim_swap_pk,
            refund_swap_zkbin,
            refund_swap_pk,
        }
    }

    /// Create an atomic swap with ZK proof
    pub fn create_swap(
        &self,
        hash: pallas::Base,
        timelock: u64,
        secret: pallas::Base,
        amount: u64,
        token_id: pallas::Base,
        side: u8,
        blind: pallas::Base,
        receiver_public: PublicKey,
        external_chain: u8,
        external_receiver: pallas::Base,
    ) -> Result<CreateSwapResult, Box<dyn std::error::Error>> {
        let input = CreateSwapCallData::new(
            hash, timelock, secret, amount, token_id, side, blind, receiver_public,
        );

        let (proof, public_inputs) = create_swap_proof(
            &self.create_swap_zkbin,
            &self.create_swap_pk,
            &input,
        )?;

        let params = CreateSwapParamsV1 {
            hash,
            timelock,
            side,
            external_chain,
            external_receiver,
            darkfi_receiver: receiver_public,
            amount,
            token_id,
            blind,
            commitment: public_inputs.swap_id,
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CreateSwapResult { call_data, proof, public_inputs })
    }

    /// Claim an atomic swap with ZK proof
    pub fn claim_swap(
        &self,
        swap_id: pallas::Base,
        secret: pallas::Base,
        hash: pallas::Base,
        timelock: u64,
        side: u8,
    ) -> Result<ClaimSwapResult, Box<dyn std::error::Error>> {
        let input = ClaimSwapCallData::new(swap_id, secret, hash, timelock, side);

        let (proof, public_inputs) = create_claim_proof(
            &self.claim_swap_zkbin,
            &self.claim_swap_pk,
            &input,
        )?;

        let params = ClaimParamsV1 {
            swap_id,
            secret,
            nullifier: public_inputs.nullifier,
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(ClaimSwapResult { call_data, proof, public_inputs })
    }

    /// Refund an atomic swap with ZK proof
    pub fn refund_swap(
        &self,
        swap_id: pallas::Base,
        secret: pallas::Base,
        current_block: u64,
        recipient: PublicKey,
    ) -> Result<RefundSwapResult, Box<dyn std::error::Error>> {
        let input = RefundSwapCallData::new(swap_id, secret);

        let (proof, public_inputs) = create_refund_proof(
            &self.refund_swap_zkbin,
            &self.refund_swap_pk,
            &input,
        )?;

        let params = RefundParamsV1 {
            swap_id,
            current_block,
            nullifier: public_inputs.nullifier,
            recipient,
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(RefundSwapResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for AtomicSwapHarness {
    fn name(&self) -> &str {
        "atomic_swap"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateSwap", "ClaimSwap", "RefundSwap"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwap" => Some(&self.create_swap_zkbin),
            "ClaimSwap" => Some(&self.claim_swap_zkbin),
            "RefundSwap" => Some(&self.refund_swap_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwap" => Some(&self.create_swap_pk),
            "ClaimSwap" => Some(&self.claim_swap_pk),
            "RefundSwap" => Some(&self.refund_swap_pk),
            _ => None,
        }
    }
}

// ============================================================================
// Result Structs
// ============================================================================

pub struct CreateSwapResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: CreateSwapPublicInputs,
}

pub struct ClaimSwapResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: ClaimSwapPublicInputs,
}

pub struct RefundSwapResult {
    pub call_data: Vec<u8>,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: RefundSwapPublicInputs,
}
