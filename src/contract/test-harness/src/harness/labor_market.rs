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

//! LaborMarket Test Harness
//!
//! Provides isolated testing for LaborMarket contract.

use dwow::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use dwow_serial::Encodable;

use darkfi_labor_market_contract::client::{
    accept_job_v1::{AcceptJobV1CallData, AcceptJobV1PublicInputs, accept_job_v1_proof},
    confirm_delivery_v1::{
        ConfirmDeliveryV1CallData, ConfirmDeliveryV1PublicInputs, confirm_delivery_v1_proof,
    },
    create_job_v1::{CreateJobV1CallData, CreateJobV1PublicInputs, create_job_v1_proof},
    dispute_v1::{DisputeV1CallData, DisputeV1PublicInputs, dispute_v1_proof},
    refund_v1::{RefundV1CallData, RefundV1PublicInputs, refund_v1_proof},
    submit_deliverable_v1::{
        SubmitDeliverableV1CallData, SubmitDeliverableV1PublicInputs, submit_deliverable_v1_proof,
    },
    submit_git_deliverable_v1::{
        SubmitGitDeliverableV1CallData, SubmitGitDeliverableV1PublicInputs,
        submit_git_deliverable_v1_proof,
    },
};
use darkfi_labor_market_contract::model::{
    AcceptJobParamsV1, ConfirmDeliveryParamsV1, CreateJobParamsV1, DisputeParamsV1,
    RefundParamsV1, SubmitDeliverableParamsV1, SubmitGitDeliverableParamsV1,
};

/// LaborMarket Harness for isolated testing
pub struct LaborMarketHarness {
    /// CreateJob_V1 ZkBinary
    create_job_zkbin: ZkBinary,
    /// CreateJob_V1 ProvingKey
    create_job_pk: ProvingKey,
    /// SubmitDeliverable_V1 ZkBinary
    submit_deliverable_zkbin: ZkBinary,
    /// SubmitDeliverable_V1 ProvingKey
    submit_deliverable_pk: ProvingKey,
    /// SubmitGitDeliverable_V1 ZkBinary
    submit_git_deliverable_zkbin: ZkBinary,
    /// SubmitGitDeliverable_V1 ProvingKey
    submit_git_deliverable_pk: ProvingKey,
    /// AcceptJob_V1 ZkBinary
    accept_job_zkbin: ZkBinary,
    /// AcceptJob_V1 ProvingKey
    accept_job_pk: ProvingKey,
    /// ConfirmDelivery_V1 ZkBinary
    confirm_delivery_zkbin: ZkBinary,
    /// ConfirmDelivery_V1 ProvingKey
    confirm_delivery_pk: ProvingKey,
    /// Dispute_V1 ZkBinary
    dispute_zkbin: ZkBinary,
    /// Dispute_V1 ProvingKey
    dispute_pk: ProvingKey,
    /// Refund_V1 ZkBinary
    refund_zkbin: ZkBinary,
    /// Refund_V1 ProvingKey
    refund_pk: ProvingKey,
}

impl LaborMarketHarness {
    /// Spawn a new LaborMarket harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../labor_market/proof/create_job_v1.zk.bin");
        let submit_bin = include_bytes!("../../../labor_market/proof/submit_deliverable_v1.zk.bin");
        let submit_git_bin =
            include_bytes!("../../../labor_market/proof/submit_git_deliverable_v1.zk.bin");
        let accept_bin = include_bytes!("../../../labor_market/proof/accept_job_v1.zk.bin");
        let confirm_bin = include_bytes!("../../../labor_market/proof/confirm_delivery_v1.zk.bin");
        let dispute_bin = include_bytes!("../../../labor_market/proof/dispute_v1.zk.bin");
        let refund_bin = include_bytes!("../../../labor_market/proof/refund_v1.zk.bin");

        let create_job_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let submit_deliverable_zkbin = ZkBinary::decode(submit_bin, false).unwrap();
        let submit_git_deliverable_zkbin = ZkBinary::decode(submit_git_bin, false).unwrap();
        let accept_job_zkbin = ZkBinary::decode(accept_bin, false).unwrap();
        let confirm_delivery_zkbin = ZkBinary::decode(confirm_bin, false).unwrap();
        let dispute_zkbin = ZkBinary::decode(dispute_bin, false).unwrap();
        let refund_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&create_job_zkbin).unwrap(),
            &create_job_zkbin,
        );
        let submit_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&submit_deliverable_zkbin).unwrap(),
            &submit_deliverable_zkbin,
        );
        let submit_git_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&submit_git_deliverable_zkbin).unwrap(),
            &submit_git_deliverable_zkbin,
        );
        let accept_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&accept_job_zkbin).unwrap(),
            &accept_job_zkbin,
        );
        let confirm_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&confirm_delivery_zkbin).unwrap(),
            &confirm_delivery_zkbin,
        );
        let dispute_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&dispute_zkbin).unwrap(),
            &dispute_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&refund_zkbin).unwrap(),
            &refund_zkbin,
        );

        let create_job_pk = ProvingKey::build(create_job_zkbin.k, &create_circuit);
        let submit_deliverable_pk = ProvingKey::build(submit_deliverable_zkbin.k, &submit_circuit);
        let submit_git_deliverable_pk =
            ProvingKey::build(submit_git_deliverable_zkbin.k, &submit_git_circuit);
        let accept_job_pk = ProvingKey::build(accept_job_zkbin.k, &accept_circuit);
        let confirm_delivery_pk = ProvingKey::build(confirm_delivery_zkbin.k, &confirm_circuit);
        let dispute_pk = ProvingKey::build(dispute_zkbin.k, &dispute_circuit);
        let refund_pk = ProvingKey::build(refund_zkbin.k, &refund_circuit);

        Self {
            create_job_zkbin,
            create_job_pk,
            submit_deliverable_zkbin,
            submit_deliverable_pk,
            submit_git_deliverable_zkbin,
            submit_git_deliverable_pk,
            accept_job_zkbin,
            accept_job_pk,
            confirm_delivery_zkbin,
            confirm_delivery_pk,
            dispute_zkbin,
            dispute_pk,
            refund_zkbin,
            refund_pk,
        }
    }

    /// Create a job (function code 0x00)
    pub fn create_job(
        &self,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
        attestation_id: pallas::Base,
        job_id: pallas::Base,
        delivery_type: u8,
        payment_amount: u64,
        payment_token: pallas::Base,
        payment_commit_x: pallas::Base,
        payment_commit_y: pallas::Base,
    ) -> Result<CreateJobResult, Box<dyn std::error::Error>> {
        let input = CreateJobV1CallData::new(employer_secret, employer_public, attestation_id);
        let (proof, public_inputs) = create_job_v1_proof(
            &self.create_job_zkbin,
            &self.create_job_pk,
            &input,
        )?;

        let params = CreateJobParamsV1 {
            proof: proof.as_ref().to_vec(),
            job_id,
            employer_pub_x: public_inputs.employer_pub_x,
            employer_pub_y: public_inputs.employer_pub_y,
            attestation_id: public_inputs.attestation_id,
            delivery_type,
            payment_amount,
            payment_token,
            payment_commit_x,
            payment_commit_y,
        };

        let mut call_data = vec![0x00];
        params.encode(&mut call_data)?;

        Ok(CreateJobResult { call_data, job_id, proof, public_inputs })
    }

    /// Accept a job (function code 0x01)
    pub fn accept_job(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
    ) -> Result<AcceptJobResult, Box<dyn std::error::Error>> {
        let input = AcceptJobV1CallData::new(worker_secret, worker_public, job_id);
        let (proof, public_inputs) = accept_job_v1_proof(
            &self.accept_job_zkbin,
            &self.accept_job_pk,
            &input,
        )?;

        let params = AcceptJobParamsV1 {
            proof: proof.as_ref().to_vec(),
            job_id: public_inputs.job_id,
            worker_pub_x: public_inputs.worker_pub_x,
            worker_pub_y: public_inputs.worker_pub_y,
        };

        let mut call_data = vec![0x01];
        params.encode(&mut call_data)?;

        Ok(AcceptJobResult { call_data, proof, public_inputs })
    }

    /// Submit a generic deliverable (function code 0x02)
    pub fn submit_deliverable(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        claim_id: pallas::Base,
        deadline_block: u64,
        current_block: u64,
    ) -> Result<SubmitDeliverableResult, Box<dyn std::error::Error>> {
        let input = SubmitDeliverableV1CallData::new(
            worker_secret,
            worker_public,
            job_id,
            claim_id,
            pallas::Base::from(deadline_block),
            pallas::Base::from(current_block),
        );
        let (proof, public_inputs) = submit_deliverable_v1_proof(
            &self.submit_deliverable_zkbin,
            &self.submit_deliverable_pk,
            &input,
        )?;

        let params = SubmitDeliverableParamsV1 {
            proof: proof.as_ref().to_vec(),
            job_id: public_inputs.job_id,
            claim_id: public_inputs.claim_id,
            worker_pub_x: public_inputs.worker_pub_x,
            worker_pub_y: public_inputs.worker_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
        };

        let mut call_data = vec![0x02];
        params.encode(&mut call_data)?;

        Ok(SubmitDeliverableResult { call_data, proof, public_inputs })
    }

    /// Submit a git deliverable (function code 0x03)
    pub fn submit_git_deliverable(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        claim_id: pallas::Base,
        deadline_block: u64,
        current_block: u64,
    ) -> Result<SubmitGitDeliverableResult, Box<dyn std::error::Error>> {
        let input = SubmitGitDeliverableV1CallData::new(
            worker_secret,
            worker_public,
            job_id,
            claim_id,
            pallas::Base::from(deadline_block),
            pallas::Base::from(current_block),
        );
        let (proof, public_inputs) = submit_git_deliverable_v1_proof(
            &self.submit_git_deliverable_zkbin,
            &self.submit_git_deliverable_pk,
            &input,
        )?;

        let params = SubmitGitDeliverableParamsV1 {
            proof: proof.as_ref().to_vec(),
            job_id: public_inputs.job_id,
            claim_id: public_inputs.claim_id,
            worker_pub_x: public_inputs.worker_pub_x,
            worker_pub_y: public_inputs.worker_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
        };

        let mut call_data = vec![0x03];
        params.encode(&mut call_data)?;

        Ok(SubmitGitDeliverableResult { call_data, proof, public_inputs })
    }

    /// Confirm delivery and release payment (function code 0x04)
    pub fn confirm_delivery(
        &self,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
        job_id: pallas::Base,
    ) -> Result<ConfirmDeliveryResult, Box<dyn std::error::Error>> {
        let input = ConfirmDeliveryV1CallData::new(employer_secret, employer_public, job_id);
        let (proof, public_inputs) = confirm_delivery_v1_proof(
            &self.confirm_delivery_zkbin,
            &self.confirm_delivery_pk,
            &input,
        )?;

        let params = ConfirmDeliveryParamsV1 {
            proof: proof.as_ref().to_vec(),
            job_id: public_inputs.job_id,
            employer_pub_x: public_inputs.employer_pub_x,
            employer_pub_y: public_inputs.employer_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
        };

        let mut call_data = vec![0x04];
        params.encode(&mut call_data)?;

        Ok(ConfirmDeliveryResult { call_data, proof, public_inputs })
    }

    /// Dispute a job (function code 0x05)
    pub fn dispute(
        &self,
        job_id: pallas::Base,
        disputer_secret: pallas::Base,
        dispute_reason_hash: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        disputer_public: PublicKey,
    ) -> Result<DisputeResult, Box<dyn std::error::Error>> {
        let input = DisputeV1CallData::new(
            job_id,
            disputer_secret,
            dispute_reason_hash,
            dao_escrow_bulla,
            disputer_public,
        );
        let (proof, public_inputs) = dispute_v1_proof(
            &self.dispute_zkbin,
            &self.dispute_pk,
            &input,
        )?;

        let params = DisputeParamsV1 {
            proof: proof.as_ref().to_vec(),
            job_id: public_inputs.job_id,
            disputer_pub_x: public_inputs.disputer_pub_x,
            disputer_pub_y: public_inputs.disputer_pub_y,
            dao_escrow_bulla: public_inputs.dao_escrow_bulla,
            spent_nullifier: public_inputs.spent_nullifier,
        };

        let mut call_data = vec![0x05];
        params.encode(&mut call_data)?;

        Ok(DisputeResult { call_data, proof, public_inputs })
    }

    /// Refund a job (function code 0x06)
    #[allow(clippy::too_many_arguments)]
    pub fn refund(
        &self,
        job_id: pallas::Base,
        employer_secret: pallas::Base,
        milestone_count: u64,
        completed_payment: u64,
        refund_amount: u64,
        deadline_block: u64,
        current_block: u64,
        total_payment: u64,
        employer_public: PublicKey,
    ) -> Result<RefundResult, Box<dyn std::error::Error>> {
        let input = RefundV1CallData::new(
            job_id,
            employer_secret,
            pallas::Base::from(milestone_count),
            pallas::Base::from(completed_payment),
            pallas::Base::from(refund_amount),
            pallas::Base::from(deadline_block),
            pallas::Base::from(current_block),
            pallas::Base::from(total_payment),
            employer_public,
        );
        let (proof, public_inputs) = refund_v1_proof(
            &self.refund_zkbin,
            &self.refund_pk,
            &input,
        )?;

        let params = RefundParamsV1 {
            proof: proof.as_ref().to_vec(),
            job_id: public_inputs.job_id,
            employer_pub_x: public_inputs.employer_pub_x,
            employer_pub_y: public_inputs.employer_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
        };

        let mut call_data = vec![0x06];
        params.encode(&mut call_data)?;

        Ok(RefundResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for LaborMarketHarness {
    fn name(&self) -> &str {
        "labor_market"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateJobV1",
            "SubmitDeliverableV1",
            "SubmitGitDeliverableV1",
            "AcceptJobV1",
            "ConfirmDeliveryV1",
            "DisputeV1",
            "RefundV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateJobV1" => Some(&self.create_job_zkbin),
            "SubmitDeliverableV1" => Some(&self.submit_deliverable_zkbin),
            "SubmitGitDeliverableV1" => Some(&self.submit_git_deliverable_zkbin),
            "AcceptJobV1" => Some(&self.accept_job_zkbin),
            "ConfirmDeliveryV1" => Some(&self.confirm_delivery_zkbin),
            "DisputeV1" => Some(&self.dispute_zkbin),
            "RefundV1" => Some(&self.refund_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateJobV1" => Some(&self.create_job_pk),
            "SubmitDeliverableV1" => Some(&self.submit_deliverable_pk),
            "SubmitGitDeliverableV1" => Some(&self.submit_git_deliverable_pk),
            "AcceptJobV1" => Some(&self.accept_job_pk),
            "ConfirmDeliveryV1" => Some(&self.confirm_delivery_pk),
            "DisputeV1" => Some(&self.dispute_pk),
            "RefundV1" => Some(&self.refund_pk),
            _ => None,
        }
    }
}

/// Result of create_job
pub struct CreateJobResult {
    pub call_data: Vec<u8>,
    pub job_id: pallas::Base,
    pub proof: dwow::zk::Proof,
    pub public_inputs: CreateJobV1PublicInputs,
}

/// Result of accept_job
pub struct AcceptJobResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: AcceptJobV1PublicInputs,
}

/// Result of submit_deliverable
pub struct SubmitDeliverableResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: SubmitDeliverableV1PublicInputs,
}

/// Result of submit_git_deliverable
pub struct SubmitGitDeliverableResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: SubmitGitDeliverableV1PublicInputs,
}

/// Result of confirm_delivery
pub struct ConfirmDeliveryResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: ConfirmDeliveryV1PublicInputs,
}

/// Result of dispute
pub struct DisputeResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: DisputeV1PublicInputs,
}

/// Result of refund
pub struct RefundResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: RefundV1PublicInputs,
}
