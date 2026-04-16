/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or version 3
 * or any later version.
 *
 * This program is distributed in the hope that it is useful, but WITHOUT
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

use atomic_swap_contract::client::{
    claim_swap_v1::{ClaimSwapCallData, create_claim_proof as create_claim_swap_proof},
    create_swap_v1::{CreateSwapCallData, create_swap_proof},
    refund_swap_v1::{RefundSwapCallData, create_refund_proof as create_refund_swap_proof},
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
        // Load circuit binaries
        // Note: proof files are named differently than modules
        // claim_v1.zk.bin -> claim_swap_v1 module
        // refund_v1.zk.bin -> refund_swap_v1 module
        let create_swap_bin = include_bytes!("../../../atomic_swap/proof/create_swap_v1.zk.bin");
        let claim_swap_bin = include_bytes!("../../../atomic_swap/proof/claim_v1.zk.bin");
        let refund_swap_bin = include_bytes!("../../../atomic_swap/proof/refund_v1.zk.bin");

        let create_swap_zkbin = ZkBinary::decode(create_swap_bin, false).unwrap();
        let claim_swap_zkbin = ZkBinary::decode(claim_swap_bin, false).unwrap();
        let refund_swap_zkbin = ZkBinary::decode(refund_swap_bin, false).unwrap();

        // Build proving keys
        let create_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_swap_zkbin).unwrap(),
            &create_swap_zkbin,
        );
        let claim_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_swap_zkbin).unwrap(),
            &claim_swap_zkbin,
        );
        let refund_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&refund_swap_zkbin).unwrap(),
            &refund_swap_zkbin,
        );

        let create_swap_pk = ProvingKey::build(create_swap_zkbin.k, &create_swap_circuit);
        let claim_swap_pk = ProvingKey::build(claim_swap_zkbin.k, &claim_swap_circuit);
        let refund_swap_pk = ProvingKey::build(refund_swap_zkbin.k, &refund_swap_circuit);

        Self {
            create_swap_zkbin,
            create_swap_pk,
            claim_swap_zkbin,
            claim_swap_pk,
            refund_swap_zkbin,
            refund_swap_pk,
        }
    }

    /// Create a swap
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
    ) -> Result<CreateSwapResult, Box<dyn std::error::Error>> {
        let input = CreateSwapCallData::new(
            hash,
            timelock,
            secret,
            amount,
            token_id,
            side,
            blind,
            receiver_public,
        );

        let (proof, public_inputs) = create_swap_proof(
            &self.create_swap_zkbin,
            &self.create_swap_pk,
            &input,
        )?;

        Ok(CreateSwapResult { swap_id: public_inputs.swap_id, proof })
    }

    /// Claim a swap
    pub fn claim_swap(
        &self,
        swap_id: pallas::Base,
        secret: pallas::Base,
        hash: pallas::Base,
        timelock: u64,
        side: u8,
    ) -> Result<ClaimSwapResult, Box<dyn std::error::Error>> {
        let input = ClaimSwapCallData::new(swap_id, secret, hash, timelock, side);

        let (proof, public_inputs) = create_claim_swap_proof(
            &self.claim_swap_zkbin,
            &self.claim_swap_pk,
            &input,
        )?;

        Ok(ClaimSwapResult { nullifier: public_inputs.nullifier, proof })
    }

    /// Refund a swap
    pub fn refund_swap(
        &self,
        swap_id: pallas::Base,
        secret: pallas::Base,
    ) -> Result<RefundSwapResult, Box<dyn std::error::Error>> {
        let input = RefundSwapCallData::new(swap_id, secret);

        let (proof, public_inputs) = create_refund_swap_proof(
            &self.refund_swap_zkbin,
            &self.refund_swap_pk,
            &input,
        )?;

        Ok(RefundSwapResult { nullifier: public_inputs.nullifier, proof })
    }
}

impl super::ContractHarness for AtomicSwapHarness {
    fn name(&self) -> &str {
        "atomic_swap"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateSwap_V1", "ClaimSwap_V1", "RefundSwap_V1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwap_V1" => Some(&self.create_swap_zkbin),
            "ClaimSwap_V1" => Some(&self.claim_swap_zkbin),
            "RefundSwap_V1" => Some(&self.refund_swap_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwap_V1" => Some(&self.create_swap_pk),
            "ClaimSwap_V1" => Some(&self.claim_swap_pk),
            "RefundSwap_V1" => Some(&self.refund_swap_pk),
            _ => None,
        }
    }
}

/// Result of create_swap
pub struct CreateSwapResult {
    pub swap_id: pallas::Base,
    pub proof: darkfi::zk::Proof,
}

/// Result of claim_swap
pub struct ClaimSwapResult {
    pub nullifier: pallas::Base,
    pub proof: darkfi::zk::Proof,
}

/// Result of refund_swap
pub struct RefundSwapResult {
    pub nullifier: pallas::Base,
    pub proof: darkfi::zk::Proof,
}
