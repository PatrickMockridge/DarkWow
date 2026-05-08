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

//! RelayerEndowment Test Harness
//!
//! Provides isolated testing for RelayerEndowment contract.

use dwow::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_relayer_endowment_contract::client::{
    initialize_v1::{InitializeV1CallData, initialize_v1_proof, InitializeV1PublicInputs},
    deploy_capital_v1::{DeployCapitalV1CallData, deploy_capital_v1_proof, DeployCapitalV1PublicInputs},
    claim_fees_v1::{ClaimFeesV1CallData, claim_fees_v1_proof, ClaimFeesV1PublicInputs},
};
use dwow_relayer_endowment_contract::model::{
    InitializeParamsV1, DeployCapitalParamsV1, ClaimFeesParamsV1,
};

/// RelayerEndowment Harness for isolated testing
pub struct RelayerEndowmentHarness {
    /// Initialize_V1 ZkBinary
    initialize_zkbin: ZkBinary,
    /// Initialize_V1 ProvingKey
    initialize_pk: ProvingKey,
    /// DeployCapital_V1 ZkBinary
    deploy_capital_zkbin: ZkBinary,
    /// DeployCapital_V1 ProvingKey
    deploy_capital_pk: ProvingKey,
    /// ClaimFees_V1 ZkBinary
    claim_fees_zkbin: ZkBinary,
    /// ClaimFees_V1 ProvingKey
    claim_fees_pk: ProvingKey,
}

impl RelayerEndowmentHarness {
    /// Spawn a new RelayerEndowment harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let init_bin = include_bytes!("../../../relayer_endowment/proof/initialize_v1.zk.bin");
        let deploy_bin = include_bytes!("../../../relayer_endowment/proof/deploy_capital_v1.zk.bin");
        let claim_bin = include_bytes!("../../../relayer_endowment/proof/claim_fees_v1.zk.bin");

        let initialize_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let deploy_capital_zkbin = ZkBinary::decode(deploy_bin, false).unwrap();
        let claim_fees_zkbin = ZkBinary::decode(claim_bin, false).unwrap();

        let init_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&initialize_zkbin).unwrap(),
            &initialize_zkbin,
        );
        let deploy_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&deploy_capital_zkbin).unwrap(),
            &deploy_capital_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&claim_fees_zkbin).unwrap(),
            &claim_fees_zkbin,
        );

        let initialize_pk = ProvingKey::build(initialize_zkbin.k, &init_circuit);
        let deploy_capital_pk = ProvingKey::build(deploy_capital_zkbin.k, &deploy_circuit);
        let claim_fees_pk = ProvingKey::build(claim_fees_zkbin.k, &claim_circuit);

        Self {
            initialize_zkbin,
            initialize_pk,
            deploy_capital_zkbin,
            deploy_capital_pk,
            claim_fees_zkbin,
            claim_fees_pk,
        }
    }

    /// Initialize a relayer endowment account with ZK proof
    pub fn initialize(
        &self,
        relayer_public: PublicKey,
        default_backer_cut_bp: u32,
        nonce: u64,
    ) -> Result<InitializeResult, Box<dyn std::error::Error>> {
        let input = InitializeV1CallData::new(relayer_public, default_backer_cut_bp, nonce);

        let (proof, public_inputs) = initialize_v1_proof(
            &self.initialize_zkbin,
            &self.initialize_pk,
            &input,
        )?;

        let params = InitializeParamsV1 {
            default_backer_cut_bp,
            signature_public: relayer_public,
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(InitializeResult { call_data, proof, public_inputs })
    }

    /// Deploy capital to a relayer's endowment with ZK proof
    pub fn deploy_capital(
        &self,
        endowment_id: pallas::Base,
        backer_public: PublicKey,
        deploy_amount: u64,
        token_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
        relayer_pub: PublicKey,
        backer_cut_bp: u32,
    ) -> Result<DeployCapitalResult, Box<dyn std::error::Error>> {
        let input = DeployCapitalV1CallData::new(
            endowment_id,
            backer_public,
            deploy_amount,
            token_id,
            nonce,
            value_blind,
        );

        let (proof, public_inputs) = deploy_capital_v1_proof(
            &self.deploy_capital_zkbin,
            &self.deploy_capital_pk,
            &input,
        )?;

        let params = DeployCapitalParamsV1 {
            relayer_pub,
            amount: deploy_amount,
            backer_cut_bp,
            signature_public: backer_public,
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(DeployCapitalResult { call_data, proof, public_inputs })
    }

    /// Claim accumulated fees from a deployment with ZK proof
    pub fn claim_fees(
        &self,
        deployment_id: pallas::Base,
        backer_public: PublicKey,
        fee_share: u64,
        nonce: u64,
    ) -> Result<ClaimFeesResult, Box<dyn std::error::Error>> {
        let input = ClaimFeesV1CallData::new(deployment_id, backer_public, fee_share, nonce);

        let (proof, public_inputs) = claim_fees_v1_proof(
            &self.claim_fees_zkbin,
            &self.claim_fees_pk,
            &input,
        )?;

        let params = ClaimFeesParamsV1 { deployment_id };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(ClaimFeesResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for RelayerEndowmentHarness {
    fn name(&self) -> &str {
        "relayer_endowment"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["Initialize", "DeployCapital", "ClaimFees"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "Initialize" => Some(&self.initialize_zkbin),
            "DeployCapital" => Some(&self.deploy_capital_zkbin),
            "ClaimFees" => Some(&self.claim_fees_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "Initialize" => Some(&self.initialize_pk),
            "DeployCapital" => Some(&self.deploy_capital_pk),
            "ClaimFees" => Some(&self.claim_fees_pk),
            _ => None,
        }
    }
}

// ============================================================================
// Result Structs
// ============================================================================

/// Result of initialize
pub struct InitializeResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: InitializeV1PublicInputs,
}

/// Result of deploy_capital
pub struct DeployCapitalResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: DeployCapitalV1PublicInputs,
}

/// Result of claim_fees
pub struct ClaimFeesResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: ClaimFeesV1PublicInputs,
}
