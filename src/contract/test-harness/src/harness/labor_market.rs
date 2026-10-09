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

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{ContractId, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_labor_market_contract::client::{
    accept_job::{AcceptJobV1CallData, AcceptJobV1PublicInputs, accept_job_v1_proof},
    accept_job_with_capability::{
        AcceptJobWithCapabilityV1CallData, AcceptJobWithCapabilityV1PublicInputs,
        accept_job_with_capability_v1_proof,
    },
    confirm_delivery::{
        ConfirmDeliveryV1CallData, ConfirmDeliveryV1PublicInputs, confirm_delivery_v1_proof,
    },
    create_job::{CreateJobV1CallData, CreateJobV1PublicInputs, create_job_v1_proof},
    dispute::{DisputeV1CallData, DisputeV1PublicInputs, dispute_v1_proof},
    milestone_payment::{
        MilestonePaymentV1CallData, MilestonePaymentV1PublicInputs, milestone_payment_v1_proof,
    },
    refund::{RefundV1CallData, RefundV1PublicInputs, refund_v1_proof},
    submit_deliverable::{
        SubmitDeliverableV1CallData, SubmitDeliverableV1PublicInputs, submit_deliverable_v1_proof,
    },
    submit_git_deliverable::{
        SubmitGitDeliverableV1CallData, SubmitGitDeliverableV1PublicInputs,
        submit_git_deliverable_v1_proof,
    },
};
use dwow_labor_market_contract::model::{
    AcceptJobParamsV1, AcceptJobWithCapabilityParamsV1, ConfirmDeliveryParamsV1,
    ConfirmMilestoneParamsV1, CreateJobParamsV1, DisputeParamsV1, RefundParamsV1,
    SubmitDeliverableParamsV1, SubmitGitDeliverableParamsV1,
};

/// LaborMarket Harness for isolated testing
pub struct LaborMarketHarness {
    /// The deployed contract's id — the frame commitment covers it (`OBL-C198`).
    contract_id: ContractId,
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
    /// AcceptJobWithCapability_V1 ZkBinary
    accept_job_with_capability_zkbin: ZkBinary,
    /// AcceptJobWithCapability_V1 ProvingKey
    accept_job_with_capability_pk: ProvingKey,
    /// MilestonePayment_V1 ZkBinary
    milestone_payment_zkbin: ZkBinary,
    /// MilestonePayment_V1 ProvingKey
    milestone_payment_pk: ProvingKey,
}

impl LaborMarketHarness {
    /// Spawn a new LaborMarket harness with pre-loaded circuits, for `contract_id` (`OBL-C198`).
    pub fn spawn(contract_id: ContractId) -> Self {
        let create_bin = include_bytes!("../../../labor_market/proof/create_job.zk.bin");
        let submit_bin = include_bytes!("../../../labor_market/proof/submit_deliverable.zk.bin");
        let submit_git_bin =
            include_bytes!("../../../labor_market/proof/submit_git_deliverable.zk.bin");
        let accept_bin = include_bytes!("../../../labor_market/proof/accept_job.zk.bin");
        let confirm_bin = include_bytes!("../../../labor_market/proof/confirm_delivery.zk.bin");
        let dispute_bin = include_bytes!("../../../labor_market/proof/dispute.zk.bin");
        let refund_bin = include_bytes!("../../../labor_market/proof/refund.zk.bin");
        let accept_with_cap_bin =
            include_bytes!("../../../labor_market/proof/accept_job_with_capability.zk.bin");
        let milestone_payment_bin =
            include_bytes!("../../../labor_market/proof/milestone_payment.zk.bin");

        let create_job_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let submit_deliverable_zkbin = ZkBinary::decode(submit_bin, false).unwrap();
        let submit_git_deliverable_zkbin = ZkBinary::decode(submit_git_bin, false).unwrap();
        let accept_job_zkbin = ZkBinary::decode(accept_bin, false).unwrap();
        let confirm_delivery_zkbin = ZkBinary::decode(confirm_bin, false).unwrap();
        let dispute_zkbin = ZkBinary::decode(dispute_bin, false).unwrap();
        let refund_zkbin = ZkBinary::decode(refund_bin, false).unwrap();
        let accept_job_with_capability_zkbin = ZkBinary::decode(accept_with_cap_bin, false).unwrap();
        let milestone_payment_zkbin = ZkBinary::decode(milestone_payment_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_job_zkbin).unwrap(),
            &create_job_zkbin,
        );
        let submit_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&submit_deliverable_zkbin).unwrap(),
            &submit_deliverable_zkbin,
        );
        let submit_git_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&submit_git_deliverable_zkbin).unwrap(),
            &submit_git_deliverable_zkbin,
        );
        let accept_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&accept_job_zkbin).unwrap(),
            &accept_job_zkbin,
        );
        let confirm_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&confirm_delivery_zkbin).unwrap(),
            &confirm_delivery_zkbin,
        );
        let dispute_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&dispute_zkbin).unwrap(),
            &dispute_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&refund_zkbin).unwrap(),
            &refund_zkbin,
        );
        let accept_job_with_capability_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&accept_job_with_capability_zkbin).unwrap(),
            &accept_job_with_capability_zkbin,
        );
        let milestone_payment_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&milestone_payment_zkbin).unwrap(),
            &milestone_payment_zkbin,
        );

        let create_job_pk = ProvingKey::build(create_job_zkbin.k, &create_circuit).expect("ProvingKey::build failed");
        let submit_deliverable_pk = ProvingKey::build(submit_deliverable_zkbin.k, &submit_circuit).expect("ProvingKey::build failed");
        let submit_git_deliverable_pk =
            ProvingKey::build(submit_git_deliverable_zkbin.k, &submit_git_circuit).expect("ProvingKey::build failed");
        let accept_job_pk = ProvingKey::build(accept_job_zkbin.k, &accept_circuit).expect("ProvingKey::build failed");
        let confirm_delivery_pk = ProvingKey::build(confirm_delivery_zkbin.k, &confirm_circuit).expect("ProvingKey::build failed");
        let dispute_pk = ProvingKey::build(dispute_zkbin.k, &dispute_circuit).expect("ProvingKey::build failed");
        let refund_pk = ProvingKey::build(refund_zkbin.k, &refund_circuit).expect("ProvingKey::build failed");
        let accept_job_with_capability_pk =
            ProvingKey::build(accept_job_with_capability_zkbin.k, &accept_job_with_capability_circuit).expect("ProvingKey::build failed");
        let milestone_payment_pk =
            ProvingKey::build(milestone_payment_zkbin.k, &milestone_payment_circuit).expect("ProvingKey::build failed");

        Self {
            contract_id,
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
            accept_job_with_capability_zkbin,
            accept_job_with_capability_pk,
            milestone_payment_zkbin,
            milestone_payment_pk,
        }
    }

    /// The frame commitment over a call set that is this call alone (`OBL-C198`). Correct only for a
    /// **childless** endpoint: a caller with children assembles `[children…, parent]` and derives with
    /// `dwow_sdk::crypto::util::tx_commitment`.
    fn commitment(&self, call_data: &[u8]) -> pallas::Base {
        dwow_sdk::crypto::util::tx_commitment([&dwow_sdk::tx::ContractCall {
            contract_id: self.contract_id,
            data: call_data.to_vec(),
        }])
    }

    /// Assemble `create_job`'s call data and stop, before the proof (`OBL-C198`).
    pub fn create_job_prepare(
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
    ) -> Result<CreateJobPlan, Box<dyn std::error::Error>> {
        let input = CreateJobV1CallData::new(employer_secret, employer_public, attestation_id);
        let public_inputs = input.compute_public_inputs();

        let params = CreateJobParamsV1 {
            job_id,
            employer_pub_x: public_inputs.employer_pub_x,
            employer_pub_y: public_inputs.employer_pub_y,
            attestation_id: public_inputs.attestation_id,
            delivery_type,
            payment_amount,
            payment_token,
            payment_commit_x,
            payment_commit_y,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode()?);

        Ok(CreateJobPlan { call_data, input, job_id, zkbin: self.create_job_zkbin.clone(), pk: self.create_job_pk.clone() })
    }

    /// `create_job` as the **only** call in its transaction (childless fast path, `OBL-C198`).
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
        let plan = self.create_job_prepare(employer_secret, employer_public, attestation_id, job_id, delivery_type, payment_amount, payment_token, payment_commit_x, payment_commit_y)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Accept a job (function code 0x01)
    /// Assemble `accept_job`'s call data and stop, before the proof (`OBL-C198`).
    pub fn accept_job_prepare(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
    ) -> Result<AcceptJobPlan, Box<dyn std::error::Error>> {
        let input = AcceptJobV1CallData::new(worker_secret, worker_public, job_id);
        let public_inputs = input.compute_public_inputs();

        let params = AcceptJobParamsV1 {
            job_id: public_inputs.job_id,
            worker_pub_x: public_inputs.worker_pub_x,
            worker_pub_y: public_inputs.worker_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode()?);

        Ok(AcceptJobPlan { call_data, input, zkbin: self.accept_job_zkbin.clone(), pk: self.accept_job_pk.clone() })
    }

    /// `accept_job` as the **only** call in its transaction (childless fast path, `OBL-C198`).
    pub fn accept_job(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
    ) -> Result<AcceptJobResult, Box<dyn std::error::Error>> {
        let plan = self.accept_job_prepare(worker_secret, worker_public, job_id)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Assemble `submit_deliverable`'s call data and stop, before the proof (`OBL-C198`). Carries a
    /// `CheckAttestationV1` child, so the caller frames `[child, parent]`.
    pub fn submit_deliverable_prepare(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        claim_id: pallas::Base,
    ) -> Result<SubmitDeliverablePlan, Box<dyn std::error::Error>> {
        let input = SubmitDeliverableV1CallData::new(worker_secret, worker_public, job_id);
        let public_inputs = input.compute_public_inputs();

        let params = SubmitDeliverableParamsV1 {
            job_id: public_inputs.job_id,
            claim_id,
            worker_pub_x: public_inputs.worker_pub_x,
            worker_pub_y: public_inputs.worker_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode()?);

        Ok(SubmitDeliverablePlan { call_data, input, zkbin: self.submit_deliverable_zkbin.clone(), pk: self.submit_deliverable_pk.clone() })
    }

    /// `submit_deliverable` as the **only** call in its transaction (childless fast path).
    pub fn submit_deliverable(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        claim_id: pallas::Base,
    ) -> Result<SubmitDeliverableResult, Box<dyn std::error::Error>> {
        let plan = self.submit_deliverable_prepare(worker_secret, worker_public, job_id, claim_id)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Assemble `submit_git_deliverable`'s call data and stop, before the proof (`OBL-C198`).
    pub fn submit_git_deliverable_prepare(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        claim_id: pallas::Base,
    ) -> Result<SubmitGitDeliverablePlan, Box<dyn std::error::Error>> {
        let input = SubmitGitDeliverableV1CallData::new(worker_secret, worker_public, job_id);
        let public_inputs = input.compute_public_inputs();

        let params = SubmitGitDeliverableParamsV1 {
            job_id: public_inputs.job_id,
            claim_id,
            worker_pub_x: public_inputs.worker_pub_x,
            worker_pub_y: public_inputs.worker_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode()?);

        Ok(SubmitGitDeliverablePlan { call_data, input, zkbin: self.submit_git_deliverable_zkbin.clone(), pk: self.submit_git_deliverable_pk.clone() })
    }

    /// `submit_git_deliverable` as the **only** call in its transaction (childless fast path).
    pub fn submit_git_deliverable(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        claim_id: pallas::Base,
    ) -> Result<SubmitGitDeliverableResult, Box<dyn std::error::Error>> {
        let plan = self.submit_git_deliverable_prepare(worker_secret, worker_public, job_id, claim_id)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Assemble `confirm_delivery`'s call data and stop, before the proof (`OBL-C198`).
    pub fn confirm_delivery_prepare(
        &self,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
        job_id: pallas::Base,
    ) -> Result<ConfirmDeliveryPlan, Box<dyn std::error::Error>> {
        let input = ConfirmDeliveryV1CallData::new(employer_secret, employer_public, job_id);
        let public_inputs = input.compute_public_inputs();

        let params = ConfirmDeliveryParamsV1 {
            job_id: public_inputs.job_id,
            employer_pub_x: public_inputs.employer_pub_x,
            employer_pub_y: public_inputs.employer_pub_y,
            spent_nullifier: public_inputs.spent_nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode()?);

        Ok(ConfirmDeliveryPlan { call_data, input, zkbin: self.confirm_delivery_zkbin.clone(), pk: self.confirm_delivery_pk.clone() })
    }

    /// `confirm_delivery` as the **only** call in its transaction (childless fast path).
    pub fn confirm_delivery(
        &self,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
        job_id: pallas::Base,
    ) -> Result<ConfirmDeliveryResult, Box<dyn std::error::Error>> {
        let plan = self.confirm_delivery_prepare(employer_secret, employer_public, job_id)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Assemble `dispute`'s call data and stop, before the proof (`OBL-C198`). Carries a
    /// `dao_escrow` (and, beneath it, a `multisig`) child set, so the caller frames `[child…, parent]`.
    pub fn dispute_prepare(
        &self,
        job_id: pallas::Base,
        disputer_secret: pallas::Base,
        dispute_reason_hash: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        disputer_public: PublicKey,
    ) -> Result<DisputePlan, Box<dyn std::error::Error>> {
        let input = DisputeV1CallData::new(
            job_id,
            disputer_secret,
            dispute_reason_hash,
            dao_escrow_bulla,
            disputer_public,
        );
        let public_inputs = input.compute_public_inputs();

        let params = DisputeParamsV1 {
            job_id: public_inputs.job_id,
            disputer_pub_x: public_inputs.disputer_pub_x,
            disputer_pub_y: public_inputs.disputer_pub_y,
            dao_escrow_bulla,
            spent_nullifier: public_inputs.spent_nullifier,
            dispute_reason_hash: public_inputs.dispute_reason_hash,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode()?);

        Ok(DisputePlan { call_data, input, zkbin: self.dispute_zkbin.clone(), pk: self.dispute_pk.clone() })
    }

    /// `dispute` as the **only** call in its transaction (fast path).
    pub fn dispute(
        &self,
        job_id: pallas::Base,
        disputer_secret: pallas::Base,
        dispute_reason_hash: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        disputer_public: PublicKey,
    ) -> Result<DisputeResult, Box<dyn std::error::Error>> {
        let plan = self.dispute_prepare(job_id, disputer_secret, dispute_reason_hash, dao_escrow_bulla, disputer_public)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Assemble `refund`'s call data and stop, before the proof (`OBL-C198`).
    #[allow(clippy::too_many_arguments)]
    pub fn refund_prepare(
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
    ) -> Result<RefundPlan, Box<dyn std::error::Error>> {
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
        let public_inputs = input.compute_public_inputs();

        let params = RefundParamsV1 {
            job_id: public_inputs.job_id,
            employer_pub_x: public_inputs.employer_pub_x,
            employer_pub_y: public_inputs.employer_pub_y,
            milestone_count,
            completed_payment,
            refund_amount,
            spent_nullifier: public_inputs.spent_nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode()?);

        Ok(RefundPlan { call_data, input, zkbin: self.refund_zkbin.clone(), pk: self.refund_pk.clone() })
    }

    /// `refund` as the **only** call in its transaction (fast path).
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
        let plan = self.refund_prepare(job_id, employer_secret, milestone_count, completed_payment, refund_amount, deadline_block, current_block, total_payment, employer_public)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Assemble `accept_job_with_capability`'s call data and stop, before the proof (`OBL-C198`).
    /// Carries an `identity` capability child, so the caller frames `[child, parent]`.
    #[allow(clippy::too_many_arguments)]
    pub fn accept_job_with_capability_prepare(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        required_capability_id: pallas::Base,
        capability_proof: Vec<u8>,
        capability_secret: [u8; 32],
    ) -> Result<AcceptJobWithCapabilityPlan, Box<dyn std::error::Error>> {
        let input = AcceptJobWithCapabilityV1CallData::new(
            worker_secret,
            worker_public,
            job_id,
            required_capability_id,
        );
        let public_inputs = input.compute_public_inputs();

        let params = AcceptJobWithCapabilityParamsV1 {
            job_id: public_inputs.job_id,
            worker_pub_x: public_inputs.worker_pub_x,
            worker_pub_y: public_inputs.worker_pub_y,
            required_capability_id: public_inputs.required_capability_id,
            capability_proof,
            capability_secret,
            spent_nullifier: public_inputs.spent_nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x0d];
        call_data.extend_from_slice(&params.encode()?);

        Ok(AcceptJobWithCapabilityPlan { call_data, input, zkbin: self.accept_job_with_capability_zkbin.clone(), pk: self.accept_job_with_capability_pk.clone() })
    }

    /// `accept_job_with_capability` as the **only** call in its transaction (fast path).
    #[allow(clippy::too_many_arguments)]
    pub fn accept_job_with_capability(
        &self,
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        required_capability_id: pallas::Base,
        capability_proof: Vec<u8>,
        capability_secret: [u8; 32],
    ) -> Result<AcceptJobWithCapabilityResult, Box<dyn std::error::Error>> {
        let plan = self.accept_job_with_capability_prepare(worker_secret, worker_public, job_id, required_capability_id, capability_proof, capability_secret)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }

    /// Assemble `confirm_milestone`'s call data and stop, before the proof (`OBL-C198`).
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_milestone_prepare(
        &self,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
        job_id: pallas::Base,
        milestone_index: u32,
        milestone_payment_amount: u64,
        payment_release: u64,
    ) -> Result<ConfirmMilestonePlan, Box<dyn std::error::Error>> {
        let input = MilestonePaymentV1CallData::new(
            job_id,
            pallas::Base::from(milestone_payment_amount),
            employer_secret,
            employer_public,
        );
        let public_inputs = input.compute_public_inputs();

        let params = ConfirmMilestoneParamsV1 {
            job_id: public_inputs.job_id,
            milestone_index,
            employer_pub_x: public_inputs.employer_pub_x,
            employer_pub_y: public_inputs.employer_pub_y,
            payment_release,
            spent_nullifier: public_inputs.spent_nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x0a];
        call_data.extend_from_slice(&params.encode()?);

        Ok(ConfirmMilestonePlan { call_data, input, zkbin: self.milestone_payment_zkbin.clone(), pk: self.milestone_payment_pk.clone() })
    }

    /// `confirm_milestone` as the **only** call in its transaction (fast path).
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_milestone(
        &self,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
        job_id: pallas::Base,
        milestone_index: u32,
        milestone_payment_amount: u64,
        payment_release: u64,
    ) -> Result<ConfirmMilestoneResult, Box<dyn std::error::Error>> {
        let plan = self.confirm_milestone_prepare(employer_secret, employer_public, job_id, milestone_index, milestone_payment_amount, payment_release)?;
        let commitment = self.commitment(&plan.call_data);
        plan.prove(commitment)
    }
}

impl super::ContractHarness for LaborMarketHarness {
    fn name(&self) -> &str {
        "labor_market"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateJobV2",
            "SubmitDeliverableV2",
            "SubmitGitDeliverableV2",
            "AcceptJobV2",
            "ConfirmDeliveryV2",
            "DisputeV2",
            "RefundV2",
            "AcceptJobWithCapability",
            "MilestonePayment",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateJobV2" => Some(&self.create_job_zkbin),
            "SubmitDeliverableV2" => Some(&self.submit_deliverable_zkbin),
            "SubmitGitDeliverableV2" => Some(&self.submit_git_deliverable_zkbin),
            "AcceptJobV2" => Some(&self.accept_job_zkbin),
            "ConfirmDeliveryV2" => Some(&self.confirm_delivery_zkbin),
            "DisputeV2" => Some(&self.dispute_zkbin),
            "RefundV2" => Some(&self.refund_zkbin),
            "AcceptJobWithCapability" => Some(&self.accept_job_with_capability_zkbin),
            "MilestonePayment" => Some(&self.milestone_payment_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateJobV2" => Some(&self.create_job_pk),
            "SubmitDeliverableV2" => Some(&self.submit_deliverable_pk),
            "SubmitGitDeliverableV2" => Some(&self.submit_git_deliverable_pk),
            "AcceptJobV2" => Some(&self.accept_job_pk),
            "ConfirmDeliveryV2" => Some(&self.confirm_delivery_pk),
            "DisputeV2" => Some(&self.dispute_pk),
            "RefundV2" => Some(&self.refund_pk),
            "AcceptJobWithCapability" => Some(&self.accept_job_with_capability_pk),
            "MilestonePayment" => Some(&self.milestone_payment_pk),
            _ => None,
        }
    }
}

/// Result of create_job
pub struct CreateJobResult {
    pub call_data: Vec<u8>,
    pub job_id: pallas::Base,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CreateJobV1PublicInputs,
}

/// Result of accept_job
pub struct AcceptJobResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AcceptJobV1PublicInputs,
}

/// Result of submit_deliverable
pub struct SubmitDeliverableResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SubmitDeliverableV1PublicInputs,
}

/// Result of submit_git_deliverable
pub struct SubmitGitDeliverableResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SubmitGitDeliverableV1PublicInputs,
}

/// Result of confirm_delivery
pub struct ConfirmDeliveryResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ConfirmDeliveryV1PublicInputs,
}

/// Result of dispute
pub struct DisputeResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: DisputeV1PublicInputs,
}

/// Result of refund
pub struct RefundResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: RefundV1PublicInputs,
}

/// Result of accept_job_with_capability
pub struct AcceptJobWithCapabilityResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AcceptJobWithCapabilityV1PublicInputs,
}

/// Result of confirm_milestone (MilestonePayment circuit)
pub struct ConfirmMilestoneResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: MilestonePaymentV1PublicInputs,
}

// ---------------------------------------------------------------------------
// `OBL-C198`: prepared calls. Each `*_prepare` fixes the call data — the frame commitment is a
// derivation over it, and over the children when a row carries any — and `prove(commitment)` binds
// the proof to that frame. The childless convenience (`create_job`, …) derives the frame over the
// call alone with `commitment`.
// ---------------------------------------------------------------------------

/// A prepared `create_job` call (`OBL-C198`).
pub struct CreateJobPlan {
    /// The encoded call data, the same bytes the host will execute.
    pub call_data: Vec<u8>,
    input: CreateJobV1CallData,
    job_id: pallas::Base,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl CreateJobPlan {
    /// The public inputs as prepared (the coords, nullifier and nonce — `tx_binding` is the one field
    /// that moves when the frame is supplied).
    pub fn public_inputs(&self) -> CreateJobV1PublicInputs {
        self.input.compute_public_inputs()
    }

    /// Prove the call, binding its proof to `tx_commitment`.
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<CreateJobResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = create_job_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(CreateJobResult { call_data: self.call_data, job_id: self.job_id, proof, public_inputs })
    }
}

/// A prepared `accept_job` call (`OBL-C198`).
pub struct AcceptJobPlan {
    pub call_data: Vec<u8>,
    input: AcceptJobV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl AcceptJobPlan {
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<AcceptJobResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = accept_job_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(AcceptJobResult { call_data: self.call_data, proof, public_inputs })
    }
}

/// A prepared `submit_deliverable` call (`OBL-C198`).
pub struct SubmitDeliverablePlan {
    pub call_data: Vec<u8>,
    input: SubmitDeliverableV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl SubmitDeliverablePlan {
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<SubmitDeliverableResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = submit_deliverable_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(SubmitDeliverableResult { call_data: self.call_data, proof, public_inputs })
    }
}

/// A prepared `submit_git_deliverable` call (`OBL-C198`).
pub struct SubmitGitDeliverablePlan {
    pub call_data: Vec<u8>,
    input: SubmitGitDeliverableV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl SubmitGitDeliverablePlan {
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<SubmitGitDeliverableResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = submit_git_deliverable_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(SubmitGitDeliverableResult { call_data: self.call_data, proof, public_inputs })
    }
}

/// A prepared `confirm_delivery` call (`OBL-C198`).
pub struct ConfirmDeliveryPlan {
    pub call_data: Vec<u8>,
    input: ConfirmDeliveryV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl ConfirmDeliveryPlan {
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<ConfirmDeliveryResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = confirm_delivery_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(ConfirmDeliveryResult { call_data: self.call_data, proof, public_inputs })
    }
}

/// A prepared `dispute` call (`OBL-C198`).
pub struct DisputePlan {
    pub call_data: Vec<u8>,
    input: DisputeV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl DisputePlan {
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<DisputeResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = dispute_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(DisputeResult { call_data: self.call_data, proof, public_inputs })
    }
}

/// A prepared `refund` call (`OBL-C198`).
pub struct RefundPlan {
    pub call_data: Vec<u8>,
    input: RefundV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl RefundPlan {
    /// The public inputs as prepared (`tx_binding` is the one field that moves at `prove`).
    pub fn public_inputs(&self) -> RefundV1PublicInputs {
        self.input.compute_public_inputs()
    }

    pub fn prove(self, tx_commitment: pallas::Base) -> Result<RefundResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = refund_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(RefundResult { call_data: self.call_data, proof, public_inputs })
    }
}

/// A prepared `accept_job_with_capability` call (`OBL-C198`).
pub struct AcceptJobWithCapabilityPlan {
    pub call_data: Vec<u8>,
    input: AcceptJobWithCapabilityV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl AcceptJobWithCapabilityPlan {
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<AcceptJobWithCapabilityResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = accept_job_with_capability_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(AcceptJobWithCapabilityResult { call_data: self.call_data, proof, public_inputs })
    }
}

/// A prepared `confirm_milestone` call (`OBL-C198`; the `MilestonePayment_V2` circuit).
pub struct ConfirmMilestonePlan {
    pub call_data: Vec<u8>,
    input: MilestonePaymentV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}
impl ConfirmMilestonePlan {
    /// The public inputs as prepared (`tx_binding` is the one field that moves at `prove`).
    pub fn public_inputs(&self) -> MilestonePaymentV1PublicInputs {
        self.input.compute_public_inputs()
    }

    pub fn prove(self, tx_commitment: pallas::Base) -> Result<ConfirmMilestoneResult, Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = milestone_payment_v1_proof(&self.zkbin, &self.pk, &input)?;
        Ok(ConfirmMilestoneResult { call_data: self.call_data, proof, public_inputs })
    }
}
