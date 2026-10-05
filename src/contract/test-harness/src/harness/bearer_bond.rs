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
use dwow_sdk::crypto::{ContractId, MerkleNode};
use dwow_sdk::pasta::pallas;
use dwow_serial::Encodable;

use dwow_bearer_bond_contract::client::{
    burn_stake::{BurnStakeCallBuilder, BurnStakeCallInput, BurnStakeCallPlan},
    emergency_unstake::{
        EmergencyUnstakeCallBuilder, EmergencyUnstakeCallInput, EmergencyUnstakeCallOutput,
        EmergencyUnstakeCallPlan,
    },
    issue_stake::{IssueStakeCallBuilder, IssueStakeCallInput, IssueStakeCallPlan},
    pay_interest::{PayInterestCallBuilder, PayInterestCallInput, PayInterestCallPlan},
    prove_coverage::{ProveCoverageCallBuilder, ProveCoverageCallInput, ProveCoverageCallPlan},
    request_interest::{
        RequestInterestCallBuilder, RequestInterestCallInput, RequestInterestCallPlan,
    },
    transfer_stake::{
        TransferStakeCallBuilder, TransferStakeCallInput, TransferStakeCallOutput,
        TransferStakeCallPlan,
    },
    unstake::{UnstakeCallBuilder, UnstakeCallInput, UnstakeCallOutput, UnstakeCallPlan},
};

/// The commitment over an ordered call set (`OBL-C198`). The order is DFS post-order — children
/// before the parent — because that is the order the host hashes, and **one** commitment is taken
/// for the whole transaction so a child's proof and its parent's bind to the same value.
///
/// One helper because every builder in this harness needs the same value, and a second derivation
/// would be a second value waiting to drift (`safety.md` RC5). It is derived over the call
/// *including* the contract id — the id is part of the call, and the node recomputes over the same
/// bytes — which is why the harness is given the deployed id rather than a placeholder.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

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
    /// The contract's deployed id (`OBL-C198`): the commitment is over the call set, and a call
    /// carries the contract it addresses, so a prover must know this id to derive the commitment
    /// its proof binds to. The spec supplies it.
    contract_id: ContractId,
}

impl BearerBondHarness {
    pub fn spawn(contract_id: ContractId) -> Self {
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
            contract_id,
        }
    }

    /// The commitment over a single-call endpoint of this harness — the call, and the node's own
    /// derivation over it (`OBL-C198`). One helper, so a second derivation cannot drift.
    fn commitment(&self, call_data: &[u8]) -> pallas::Base {
        commitment_of(&[dwow_sdk::tx::ContractCall {
            contract_id: self.contract_id,
            data: call_data.to_vec(),
        }])
    }

    // ========================================================================
    // IssueStakeV1 — selector 0x00 (BlindOutput_V2)
    // ========================================================================

    /// Assemble the IssueStake call's data and stop, **before** the proof (`OBL-C198`).
    pub fn issue_stake_prepare(
        &self,
        input: IssueStakeCallInput,
    ) -> Result<IssueStakeCallPlan, Box<dyn std::error::Error>> {
        let plan = IssueStakeCallBuilder {
            input,
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared IssueStake call against `commitment`/`nonce`.
    pub fn issue_stake_prove(
        &self,
        plan: IssueStakeCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<IssueStakeResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x00]; // IssueStakeV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(IssueStakeResult { call_data, proofs: debris.proofs })
    }

    /// Issue stake as the **only** call in its transaction. The shortcut is the name: the
    /// commitment is derived over this call alone, correct only because the endpoint is childless
    /// (`OBL-C198`). A caller with children uses `_prepare`, assembles the whole ordered set,
    /// derives over it with `dwow_sdk::crypto::util::tx_commitment`, then `_prove`.
    pub fn issue_stake_solo(
        &self,
        input: IssueStakeCallInput,
    ) -> Result<IssueStakeResult, Box<dyn std::error::Error>> {
        let plan = self.issue_stake_prepare(input)?;
        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.issue_stake_prove(plan, commitment, pallas::Base::zero())
    }

    // ========================================================================
    // BurnStakeV1 — selector 0x05 (Burn_V2)
    // ========================================================================

    /// Assemble the BurnStake call's data and stop, **before** the proofs (`OBL-C198`).
    pub fn burn_stake_prepare(
        &self,
        inputs: Vec<BurnStakeCallInput>,
    ) -> Result<BurnStakeCallPlan, Box<dyn std::error::Error>> {
        let plan = BurnStakeCallBuilder {
            inputs,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared BurnStake call against `commitment`/`nonce`.
    pub fn burn_stake_prove(
        &self,
        plan: BurnStakeCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<BurnStakeResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x05]; // BurnStakeV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(BurnStakeResult { call_data, proofs: debris.proofs })
    }

    /// Burn stake as the **only** call in its transaction — see `issue_stake_solo` for the caveat.
    pub fn burn_stake_solo(
        &self,
        inputs: Vec<BurnStakeCallInput>,
    ) -> Result<BurnStakeResult, Box<dyn std::error::Error>> {
        let plan = self.burn_stake_prepare(inputs)?;
        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.burn_stake_prove(plan, commitment, pallas::Base::zero())
    }

    // ========================================================================
    // TransferStakeV1 — selector 0x01 (Burn_V2 + BlindOutput_V2)
    // ========================================================================

    /// Assemble the TransferStake call's data and stop, **before** the proofs (`OBL-C198`).
    pub fn transfer_stake_prepare(
        &self,
        inputs: Vec<TransferStakeCallInput>,
        outputs: Vec<TransferStakeCallOutput>,
    ) -> Result<TransferStakeCallPlan, Box<dyn std::error::Error>> {
        let plan = TransferStakeCallBuilder {
            inputs,
            outputs,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared TransferStake call against `commitment`/`nonce`.
    pub fn transfer_stake_prove(
        &self,
        plan: TransferStakeCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<TransferStakeResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x01]; // TransferStakeV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(TransferStakeResult { call_data, proofs: debris.proofs })
    }

    /// Transfer stake as the **only** call in its transaction — see `issue_stake_solo`.
    pub fn transfer_stake_solo(
        &self,
        inputs: Vec<TransferStakeCallInput>,
        outputs: Vec<TransferStakeCallOutput>,
    ) -> Result<TransferStakeResult, Box<dyn std::error::Error>> {
        let plan = self.transfer_stake_prepare(inputs, outputs)?;
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.transfer_stake_prove(plan, commitment, pallas::Base::zero())
    }

    // ========================================================================
    // RequestInterestV1 — selector 0x02 (Burn_V2)
    // ========================================================================

    /// Assemble the RequestInterest call's data and stop, **before** the proof (`OBL-C198`).
    pub fn request_interest_prepare(
        &self,
        input: RequestInterestCallInput,
    ) -> Result<RequestInterestCallPlan, Box<dyn std::error::Error>> {
        let plan = RequestInterestCallBuilder {
            input,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared RequestInterest call against `commitment`/`nonce`.
    pub fn request_interest_prove(
        &self,
        plan: RequestInterestCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<RequestInterestResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x02]; // RequestInterestV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(RequestInterestResult { call_data, proofs: debris.proofs })
    }

    /// Request interest as the **only** call in its transaction — see `issue_stake_solo`.
    pub fn request_interest_solo(
        &self,
        input: RequestInterestCallInput,
    ) -> Result<RequestInterestResult, Box<dyn std::error::Error>> {
        let plan = self.request_interest_prepare(input)?;
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.request_interest_prove(plan, commitment, pallas::Base::zero())
    }

    // ========================================================================
    // UnstakeV1 — selector 0x04 (Burn_V2 + Redeem_V2)
    // ========================================================================

    /// Assemble the Unstake call's data and stop, **before** the proofs (`OBL-C198`).
    pub fn unstake_prepare(
        &self,
        input: UnstakeCallInput,
        output: UnstakeCallOutput,
    ) -> Result<UnstakeCallPlan, Box<dyn std::error::Error>> {
        let plan = UnstakeCallBuilder {
            input,
            output,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            redeem_zkbin: self.redeem_zkbin.clone(),
            redeem_pk: self.redeem_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared Unstake call against `commitment`/`nonce`.
    pub fn unstake_prove(
        &self,
        plan: UnstakeCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<UnstakeResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x04]; // UnstakeV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(UnstakeResult { call_data, proofs: debris.proofs })
    }

    /// Unstake as the **only** call in its transaction — see `issue_stake_solo`.
    pub fn unstake_solo(
        &self,
        input: UnstakeCallInput,
        output: UnstakeCallOutput,
    ) -> Result<UnstakeResult, Box<dyn std::error::Error>> {
        let plan = self.unstake_prepare(input, output)?;
        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.unstake_prove(plan, commitment, pallas::Base::zero())
    }

    // ========================================================================
    // EmergencyUnstakeV1 — selector 0x03 (Burn_V2 + Redeem_V2)
    // ========================================================================

    /// Assemble the EmergencyUnstake call's data and stop, **before** the proofs (`OBL-C198`).
    pub fn emergency_unstake_prepare(
        &self,
        input: EmergencyUnstakeCallInput,
        output: EmergencyUnstakeCallOutput,
    ) -> Result<EmergencyUnstakeCallPlan, Box<dyn std::error::Error>> {
        let plan = EmergencyUnstakeCallBuilder {
            input,
            output,
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            redeem_zkbin: self.redeem_zkbin.clone(),
            redeem_pk: self.redeem_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared EmergencyUnstake call against `commitment`/`nonce`.
    pub fn emergency_unstake_prove(
        &self,
        plan: EmergencyUnstakeCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<EmergencyUnstakeResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x03]; // EmergencyUnstakeV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(EmergencyUnstakeResult { call_data, proofs: debris.proofs })
    }

    /// Emergency-unstake as the **only** call in its transaction — see `issue_stake_solo`.
    pub fn emergency_unstake_solo(
        &self,
        input: EmergencyUnstakeCallInput,
        output: EmergencyUnstakeCallOutput,
    ) -> Result<EmergencyUnstakeResult, Box<dyn std::error::Error>> {
        let plan = self.emergency_unstake_prepare(input, output)?;
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.emergency_unstake_prove(plan, commitment, pallas::Base::zero())
    }

    // ========================================================================
    // PayInterestV1 — selector 0x08 (BlindOutput_V2)
    // ========================================================================

    /// Assemble the PayInterest call's data and stop, **before** the proof (`OBL-C198`).
    pub fn pay_interest_prepare(
        &self,
        input: PayInterestCallInput,
    ) -> Result<PayInterestCallPlan, Box<dyn std::error::Error>> {
        let plan = PayInterestCallBuilder {
            input,
            blind_output_zkbin: self.blind_output_zkbin.clone(),
            blind_output_pk: self.blind_output_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared PayInterest call against `commitment`/`nonce`.
    pub fn pay_interest_prove(
        &self,
        plan: PayInterestCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<PayInterestResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x08]; // PayInterestV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(PayInterestResult { call_data, proofs: debris.proofs })
    }

    /// Pay interest as the **only** call in its transaction — see `issue_stake_solo`.
    pub fn pay_interest_solo(
        &self,
        input: PayInterestCallInput,
    ) -> Result<PayInterestResult, Box<dyn std::error::Error>> {
        let plan = self.pay_interest_prepare(input)?;
        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.pay_interest_prove(plan, commitment, pallas::Base::zero())
    }

    // ========================================================================
    // ProveCoverageV1 — selector 0x06 (ProveCoverage_V2)
    // ========================================================================

    /// Assemble the ProveCoverage call's data and stop, **before** the proof (`OBL-C198`).
    pub fn prove_coverage_prepare(
        &self,
        input: ProveCoverageCallInput,
    ) -> Result<ProveCoverageCallPlan, Box<dyn std::error::Error>> {
        let plan = ProveCoverageCallBuilder {
            input,
            prove_coverage_zkbin: self.prove_coverage_zkbin.clone(),
            prove_coverage_pk: self.prove_coverage_pk.clone(),
        }
        .prepare()?;
        Ok(plan)
    }

    /// Prove a prepared ProveCoverage call against `commitment`/`nonce`.
    pub fn prove_coverage_prove(
        &self,
        plan: ProveCoverageCallPlan,
        commitment: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<ProveCoverageResult, Box<dyn std::error::Error>> {
        let mut call_data = vec![0x06]; // ProveCoverageV1
        call_data.extend_from_slice(&plan.params().encode());
        let debris = plan.prove(commitment, nonce)?;
        Ok(ProveCoverageResult { call_data, proofs: debris.proofs })
    }

    /// Prove coverage as the **only** call in its transaction — see `issue_stake_solo`.
    pub fn prove_coverage_solo(
        &self,
        input: ProveCoverageCallInput,
    ) -> Result<ProveCoverageResult, Box<dyn std::error::Error>> {
        let plan = self.prove_coverage_prepare(input)?;
        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&plan.params().encode());
        let commitment = self.commitment(&call_data);
        self.prove_coverage_prove(plan, commitment, pallas::Base::zero())
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
