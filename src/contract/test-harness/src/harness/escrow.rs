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

//! Escrow Test Harness
//!
//! Provides isolated testing for Escrow contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::PrimeField, MerkleNode, PublicKey},
    pasta::pallas,
};
use darkfi_serial::Encodable;

use darkfi_escrow_contract::client::{
    claim_v1::{ClaimEscrowCallData, create_claim_escrow_proof, ClaimEscrowPublicInputs},
    create_escrow_v1::{CreateEscrowCallData, create_escrow_proof, CreateEscrowPublicInputs},
    fund_v1::{FundEscrowCallData, create_fund_escrow_proof, FundEscrowPublicInputs},
    refund_v1::{RefundEscrowCallData, create_refund_escrow_proof, RefundEscrowPublicInputs},
};
use darkfi_escrow_contract::model::{
    CreateEscrowParamsV1, FundEscrowParamsV1, ClaimEscrowParamsV1, RefundEscrowParamsV1,
};

/// Escrow Harness for isolated testing
pub struct EscrowHarness {
    /// CreateEscrow_V1 ZkBinary
    create_escrow_zkbin: ZkBinary,
    /// CreateEscrow_V1 ProvingKey
    create_escrow_pk: ProvingKey,
    /// Fund_V1 ZkBinary
    fund_zkbin: ZkBinary,
    /// Fund_V1 ProvingKey
    fund_pk: ProvingKey,
    /// Claim_V1 ZkBinary
    claim_zkbin: ZkBinary,
    /// Claim_V1 ProvingKey
    claim_pk: ProvingKey,
    /// Refund_V1 ZkBinary
    refund_zkbin: ZkBinary,
    /// Refund_V1 ProvingKey
    refund_pk: ProvingKey,
}

impl EscrowHarness {
    /// Spawn a new Escrow harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../escrow/proof/create_escrow_v1.zk.bin");
        let fund_bin = include_bytes!("../../../escrow/proof/fund_v1.zk.bin");
        let claim_bin = include_bytes!("../../../escrow/proof/claim_v1.zk.bin");
        let refund_bin = include_bytes!("../../../escrow/proof/refund_v1.zk.bin");

        let create_escrow_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let fund_zkbin = ZkBinary::decode(fund_bin, false).unwrap();
        let claim_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let refund_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_escrow_zkbin).unwrap(),
            &create_escrow_zkbin,
        );
        let fund_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&fund_zkbin).unwrap(),
            &fund_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_zkbin).unwrap(),
            &claim_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&refund_zkbin).unwrap(),
            &refund_zkbin,
        );

        let create_escrow_pk = ProvingKey::build(create_escrow_zkbin.k, &create_circuit);
        let fund_pk = ProvingKey::build(fund_zkbin.k, &fund_circuit);
        let claim_pk = ProvingKey::build(claim_zkbin.k, &claim_circuit);
        let refund_pk = ProvingKey::build(refund_zkbin.k, &refund_circuit);

        Self {
            create_escrow_zkbin,
            create_escrow_pk,
            fund_zkbin,
            fund_pk,
            claim_zkbin,
            claim_pk,
            refund_zkbin,
            refund_pk,
        }
    }
}

impl EscrowHarness {
    /// Create an escrow with ZK proof and return encoded call data
    pub fn create_escrow(
        &self,
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        seller_pubkey: PublicKey,
        value: u64,
        token_id: pallas::Base,
        timeout: u64,
    ) -> Result<CreateEscrowResult, Box<dyn std::error::Error>> {
        let input = CreateEscrowCallData::new(
            buyer_secret,
            buyer_pubkey,
            seller_pubkey,
            value,
            token_id,
            timeout,
        );

        let (proof, public_inputs) = create_escrow_proof(
            &self.create_escrow_zkbin,
            &self.create_escrow_pk,
            &input,
        )?;

        // Build CreateEscrowParamsV1
        let params = CreateEscrowParamsV1 {
            buyer_pubkey,
            seller_pubkey,
            value,
            token_id,
            timeout,
            commitment: public_inputs.commitment,
            merkle_root: MerkleNode::new(pallas::Base::zero()),
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CreateEscrowResult { call_data, proof, public_inputs })
    }

    /// Fund an escrow (no ZK proof needed)
    pub fn fund_escrow(
        &self,
        escrow_id: pallas::Base,
        value_commit: pallas::Point,
    ) -> Result<FundEscrowResult, Box<dyn std::error::Error>> {
        // Build FundEscrowParamsV1
        let params = FundEscrowParamsV1 {
            escrow_id,
            value_commit,
            merkle_proof: vec![],
            merkle_root: MerkleNode::new(pallas::Base::zero()),
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(FundEscrowResult { call_data, escrow_id })
    }

    /// Claim an escrow with ZK proof
    pub fn claim_escrow(
        &self,
        escrow_id: pallas::Base,
        seller_secret: pallas::Base,
        seller_pubkey: PublicKey,
        escrow_seller_commitment: pallas::Base,
        recipient_pubkey: PublicKey,
    ) -> Result<ClaimEscrowResult, Box<dyn std::error::Error>> {
        let input = ClaimEscrowCallData::new(
            escrow_id,
            seller_secret,
            seller_pubkey,
            escrow_seller_commitment,
        );

        let (proof, public_inputs) = create_claim_escrow_proof(
            &self.claim_zkbin,
            &self.claim_pk,
            &input,
        )?;

        // Build ClaimEscrowParamsV1
        let params = ClaimEscrowParamsV1 {
            escrow_id,
            seller_secret,
            spent_nullifier: public_inputs.spent_nullifier,
            recipient_pubkey,
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(ClaimEscrowResult { call_data, proof, public_inputs })
    }

    /// Refund an escrow with ZK proof (after timeout)
    pub fn refund_escrow(
        &self,
        escrow_id: pallas::Base,
        timeout: u64,
        current_block: u64,
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        escrow_buyer_pub_x: pallas::Base,
        escrow_buyer_pub_y: pallas::Base,
        recipient_pubkey: PublicKey,
    ) -> Result<RefundEscrowResult, Box<dyn std::error::Error>> {
        let input = RefundEscrowCallData::new(
            escrow_id,
            timeout,
            current_block,
            buyer_secret,
            buyer_pubkey,
            escrow_buyer_pub_x,
            escrow_buyer_pub_y,
        );

        let (proof, public_inputs) = create_refund_escrow_proof(
            &self.refund_zkbin,
            &self.refund_pk,
            &input,
        )?;

        // Build RefundEscrowParamsV1
        let params = RefundEscrowParamsV1 {
            escrow_id,
            buyer_secret,
            spent_nullifier: public_inputs.spent_nullifier,
            current_block,
            recipient_pubkey,
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(RefundEscrowResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for EscrowHarness {
    fn name(&self) -> &str {
        "escrow"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateEscrowV1", "FundV1", "ClaimV1", "RefundV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateEscrowV1" => Some(&self.create_escrow_zkbin),
            "FundV1" => Some(&self.fund_zkbin),
            "ClaimV1" => Some(&self.claim_zkbin),
            "RefundV1" => Some(&self.refund_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateEscrowV1" => Some(&self.create_escrow_pk),
            "FundV1" => Some(&self.fund_pk),
            "ClaimV1" => Some(&self.claim_pk),
            "RefundV1" => Some(&self.refund_pk),
            _ => None,
        }
    }
}

// ============================================================================
// Result Structs
// ============================================================================

/// Result of create_escrow
pub struct CreateEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: darkfi::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: CreateEscrowPublicInputs,
}

/// Result of fund_escrow
pub struct FundEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// Escrow ID
    pub escrow_id: pallas::Base,
}

/// Result of claim_escrow
pub struct ClaimEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: darkfi::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: ClaimEscrowPublicInputs,
}

/// Result of refund_escrow
pub struct RefundEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: darkfi::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: RefundEscrowPublicInputs,
}