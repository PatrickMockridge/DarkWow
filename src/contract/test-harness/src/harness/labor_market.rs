/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or
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

//! LaborMarket Test Harness
//!
//! Provides isolated testing for LaborMarket contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use darkfi_labor_market_contract::client::{
    accept_job_v1::{AcceptJobV1CallData, accept_job_v1_proof},
    confirm_delivery_v1::{ConfirmDeliveryV1CallData, confirm_delivery_v1_proof},
    create_job_v1::{CreateJobV1CallData, create_job_v1_proof},
    dispute_v1::{DisputeV1CallData, dispute_v1_proof},
    refund_v1::{RefundV1CallData, refund_v1_proof},
    submit_deliverable_v1::{SubmitDeliverableV1CallData, submit_deliverable_v1_proof},
    submit_git_deliverable_v1::{SubmitGitDeliverableV1CallData, submit_git_deliverable_v1_proof},
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
        let submit_git_bin = include_bytes!("../../../labor_market/proof/submit_git_deliverable_v1.zk.bin");
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
            darkfi::zk::empty_witnesses(&create_job_zkbin).unwrap(),
            &create_job_zkbin,
        );
        let submit_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&submit_deliverable_zkbin).unwrap(),
            &submit_deliverable_zkbin,
        );
        let submit_git_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&submit_git_deliverable_zkbin).unwrap(),
            &submit_git_deliverable_zkbin,
        );
        let accept_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&accept_job_zkbin).unwrap(),
            &accept_job_zkbin,
        );
        let confirm_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&confirm_delivery_zkbin).unwrap(),
            &confirm_delivery_zkbin,
        );
        let dispute_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&dispute_zkbin).unwrap(),
            &dispute_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&refund_zkbin).unwrap(),
            &refund_zkbin,
        );

        let create_job_pk = ProvingKey::build(create_job_zkbin.k, &create_circuit);
        let submit_deliverable_pk = ProvingKey::build(submit_deliverable_zkbin.k, &submit_circuit);
        let submit_git_deliverable_pk = ProvingKey::build(submit_git_deliverable_zkbin.k, &submit_git_circuit);
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