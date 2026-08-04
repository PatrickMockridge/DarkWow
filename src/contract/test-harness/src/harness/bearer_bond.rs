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

//! Bearer Bond Test Harness

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::crypto::MerkleNode;
use dwow_serial::Encodable;

use dwow_bearer_bond_contract::client::{
    burn_stake::{BurnStakeCallBuilder, BurnStakeCallInput},
    emergency_unstake::{EmergencyUnstakeCallBuilder, EmergencyUnstakeCallInput, EmergencyUnstakeCallOutput},
    issue_stake::{IssueStakeCallBuilder, IssueStakeCallInput},
    pay_interest::{PayInterestCallBuilder, PayInterestCallInput},
    prove_coverage::{ProveCoverageCallBuilder, ProveCoverageCallInput},
    request_interest::{RequestInterestCallBuilder, RequestInterestCallInput},
    transfer_stake::{TransferStakeCallBuilder, TransferStakeCallInput, TransferStakeCallOutput},
    unstake::{UnstakeCallBuilder, UnstakeCallInput, UnstakeCallOutput},
};

/// Bearer Bond Harness for isolated testing
pub struct BearerBondHarness {
    blind_output_zkbin: ZkBinary,
    blind_output_pk: ProvingKey,
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    redeem_zkbin: ZkBinary,
    redeem_pk: ProvingKey,
    prove_coverage_zkbin: ZkBinary,
    prove_coverage_pk: ProvingKey,
}

impl BearerBondHarness {
    pub fn spawn() -> Self {
        let blind_output_bin = include_bytes!("../../../bearer_bond/proof/blind_output.zk.bin");
        let burn_bin = include_bytes!("../../../bearer_bond/proof/burn.zk.bin");
        let redeem_bin = include_bytes!("../../../bearer_bond/proof/redeem.zk.bin");
        let prove_coverage_bin = include_bytes!("../../../bearer_bond/proof/prove_coverage.zk.bin");

        let blind_output_zkbin = ZkBinary::decode(blind_output_bin, false).unwrap();
        let blind_output_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&blind_output_zkbin).unwrap(), &blind_output_zkbin,
        );
        let blind_output_pk = ProvingKey::build(blind_output_zkbin.k, &blind_output_circuit)
            .expect("ProvingKey::build failed");
        let burn_zkbin = ZkBinary::decode(burn_bin, false).unwrap();
        let burn_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin,
        );
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit)
            .expect("ProvingKey::build failed");
        let redeem_zkbin = ZkBinary::decode(redeem_bin, false).unwrap();
        let redeem_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&redeem_zkbin).unwrap(), &redeem_zkbin,
        );
        let redeem_pk = ProvingKey::build(redeem_zkbin.k, &redeem_circuit)
            .expect("ProvingKey::build failed");
        let prove_coverage_zkbin = ZkBinary::decode(prove_coverage_bin, false).unwrap();
        let prove_coverage_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&prove_coverage_zkbin).unwrap(), &prove_coverage_zkbin,
        );
        let prove_coverage_pk = ProvingKey::build(prove_coverage_zkbin.k, &prove_coverage_circuit)
            .expect("ProvingKey::build failed");

        Self {
            blind_output_zkbin, blind_output_pk,
            burn_zkbin, burn_pk,
            redeem_zkbin, redeem_pk,
            prove_coverage_zkbin, prove_coverage_pk,
        }
    }

    /// Issue stake (BlindOutput_V1, function code 0x01)
    pub fn issue_stake(
        &self,
        input: IssueStakeCallInput,
    ) -> Result<IssueStakeResult, Box<dyn std::error::Error>> {
        let debris = IssueStakeCallBuilder {
            input,
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x00]; // IssueStakeV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(IssueStakeResult { call_data, proofs: debris.proofs })
    }

    /// Burn stake (Burn_V1, function code 0x02)
    pub fn burn_stake(
        &self,
        inputs: Vec<BurnStakeCallInput>,
    ) -> Result<BurnStakeResult, Box<dyn std::error::Error>> {
        let debris = BurnStakeCallBuilder {
            inputs,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x05]; // BurnStakeV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(BurnStakeResult { call_data, proofs: debris.proofs })
    }

    /// Transfer stake (Burn_V1 + BlindOutput_V1, function code 0x03)
    pub fn transfer_stake(
        &self,
        inputs: Vec<TransferStakeCallInput>,
        outputs: Vec<TransferStakeCallOutput>,
    ) -> Result<TransferStakeResult, Box<dyn std::error::Error>> {
        let debris = TransferStakeCallBuilder {
            inputs,
            outputs,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x01]; // TransferStakeV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(TransferStakeResult { call_data, proofs: debris.proofs })
    }

    /// Request interest (Burn_V1, function code 0x04)
    pub fn request_interest(
        &self,
        input: RequestInterestCallInput,
    ) -> Result<RequestInterestResult, Box<dyn std::error::Error>> {
        let debris = RequestInterestCallBuilder {
            input,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x02]; // RequestInterestV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(RequestInterestResult { call_data, proofs: debris.proofs })
    }

    /// Unstake (Burn_V1 + Redeem_V1, function code 0x05)
    pub fn unstake(
        &self,
        input: UnstakeCallInput,
        output: UnstakeCallOutput,
    ) -> Result<UnstakeResult, Box<dyn std::error::Error>> {
        let debris = UnstakeCallBuilder {
            input,
            output,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            redeem_zkbin: self.redeem_zkbin.clone(),
            redeem_pk: self.redeem_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x04]; // UnstakeV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(UnstakeResult { call_data, proofs: debris.proofs })
    }

    /// Emergency unstake (Burn_V1 + Redeem_V1, function code 0x06)
    pub fn emergency_unstake(
        &self,
        input: EmergencyUnstakeCallInput,
        output: EmergencyUnstakeCallOutput,
    ) -> Result<EmergencyUnstakeResult, Box<dyn std::error::Error>> {
        let debris = EmergencyUnstakeCallBuilder {
            input,
            output,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            redeem_zkbin: self.redeem_zkbin.clone(),
            redeem_pk: self.redeem_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x03]; // EmergencyUnstakeV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(EmergencyUnstakeResult { call_data, proofs: debris.proofs })
    }

    /// Pay interest (BlindOutput_V1, function code 0x07)
    pub fn pay_interest(
        &self,
        input: PayInterestCallInput,
    ) -> Result<PayInterestResult, Box<dyn std::error::Error>> {
        let debris = PayInterestCallBuilder {
            input,
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x08]; // PayInterestV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(PayInterestResult { call_data, proofs: debris.proofs })
    }

    /// Prove coverage (ProveCoverage_V1, function code 0x08)
    pub fn prove_coverage(
        &self,
        input: ProveCoverageCallInput,
    ) -> Result<ProveCoverageResult, Box<dyn std::error::Error>> {
        let debris = ProveCoverageCallBuilder {
            input,
            prove_coverage_zkbin: self.prove_coverage_zkbin.clone(),
            prove_coverage_pk: self.prove_coverage_pk.clone(),
        }
        .build()?;

        let mut call_data = vec![0x06]; // ProveCoverageV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(ProveCoverageResult { call_data, proofs: debris.proofs })
    }
}

impl super::ContractHarness for BearerBondHarness {
    fn name(&self) -> &str {
        "bearer_bond"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["Burn_V2", "BlindOutput_V2", "Redeem_V2", "ProveCoverage_V2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "Burn_V2" => Some(&self.burn_zkbin),
            "BlindOutput_V2" => Some(&self.blind_output_zkbin),
            "Redeem_V2" => Some(&self.redeem_zkbin),
            "ProveCoverage_V2" => Some(&self.prove_coverage_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "Burn_V2" => Some(&self.burn_pk),
            "BlindOutput_V2" => Some(&self.blind_output_pk),
            "Redeem_V2" => Some(&self.redeem_pk),
            "ProveCoverage_V2" => Some(&self.prove_coverage_pk),
            _ => None,
        }
    }
}

/// Result of issue_stake
pub struct IssueStakeResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Result of burn_stake
pub struct BurnStakeResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Result of transfer_stake
pub struct TransferStakeResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Result of request_interest
pub struct RequestInterestResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Result of unstake
pub struct UnstakeResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Result of emergency_unstake
pub struct EmergencyUnstakeResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Result of pay_interest
pub struct PayInterestResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Result of prove_coverage
pub struct ProveCoverageResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}
