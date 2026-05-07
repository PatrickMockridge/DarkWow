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

//! PoolStake Test Harness
//!
//! Provides isolated testing for PoolStake contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_serial::Encodable;

use darkfi_pool_stake_contract::client::{
    allocate_coverage_v1::{AllocateCoverageV1CallData, allocate_coverage_v1_proof, AllocateCoverageV1PublicInputs},
    create_pool_v1::{CreatePoolV1CallData, create_pool_v1_proof, CreatePoolV1PublicInputs},
    join_pool_v1::{JoinPoolV1CallData, join_pool_v1_proof, JoinPoolV1PublicInputs},
    slash_coverage_v1::{SlashCoverageV1CallData, slash_coverage_v1_proof, SlashCoverageV1PublicInputs},
};
use darkfi_pool_stake_contract::model::{
    CreatePoolParamsV1, JoinPoolParamsV1, LeavePoolParamsV1,
    AllocateCoverageParamsV1, SlashCoverageParamsV1,
};

/// PoolStake Harness for isolated testing
pub struct PoolStakeHarness {
    create_pool_zkbin: ZkBinary,
    create_pool_pk: ProvingKey,
    join_pool_zkbin: ZkBinary,
    join_pool_pk: ProvingKey,
    allocate_coverage_zkbin: ZkBinary,
    allocate_coverage_pk: ProvingKey,
    slash_coverage_zkbin: ZkBinary,
    slash_coverage_pk: ProvingKey,
}

impl PoolStakeHarness {
    pub fn spawn() -> Self {
        let create_pool_bin =
            include_bytes!("../../../pool_stake/proof/create_pool_v1.zk.bin");
        let join_pool_bin =
            include_bytes!("../../../pool_stake/proof/join_pool_v1.zk.bin");
        let allocate_coverage_bin =
            include_bytes!("../../../pool_stake/proof/allocate_coverage_v1.zk.bin");
        let slash_coverage_bin =
            include_bytes!("../../../pool_stake/proof/slash_coverage_v1.zk.bin");

        let create_pool_zkbin = ZkBinary::decode(create_pool_bin, false).unwrap();
        let join_pool_zkbin = ZkBinary::decode(join_pool_bin, false).unwrap();
        let allocate_coverage_zkbin = ZkBinary::decode(allocate_coverage_bin, false).unwrap();
        let slash_coverage_zkbin = ZkBinary::decode(slash_coverage_bin, false).unwrap();

        let create_pool_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_pool_zkbin).unwrap(),
            &create_pool_zkbin,
        );
        let join_pool_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&join_pool_zkbin).unwrap(),
            &join_pool_zkbin,
        );
        let allocate_coverage_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&allocate_coverage_zkbin).unwrap(),
            &allocate_coverage_zkbin,
        );
        let slash_coverage_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&slash_coverage_zkbin).unwrap(),
            &slash_coverage_zkbin,
        );

        let create_pool_pk = ProvingKey::build(create_pool_zkbin.k, &create_pool_circuit);
        let join_pool_pk = ProvingKey::build(join_pool_zkbin.k, &join_pool_circuit);
        let allocate_coverage_pk =
            ProvingKey::build(allocate_coverage_zkbin.k, &allocate_coverage_circuit);
        let slash_coverage_pk =
            ProvingKey::build(slash_coverage_zkbin.k, &slash_coverage_circuit);

        Self {
            create_pool_zkbin,
            create_pool_pk,
            join_pool_zkbin,
            join_pool_pk,
            allocate_coverage_zkbin,
            allocate_coverage_pk,
            slash_coverage_zkbin,
            slash_coverage_pk,
        }
    }

    /// Create a new staking pool (function code 0x00)
    pub fn create_pool(
        &self,
        owner_pub: PublicKey,
        max_coverage_ratio: u32,
        operator_fee_bp: u32,
    ) -> Result<CreatePoolResult, Box<dyn std::error::Error>> {
        let pool_config_hash = pallas::Base::from(
            (max_coverage_ratio as u64) ^ ((operator_fee_bp as u64) << 32)
        );
        let nonce = 0u64;
        let call_data_input = CreatePoolV1CallData::new(owner_pub, pool_config_hash, nonce);
        let (proof, public_inputs) = create_pool_v1_proof(
            &self.create_pool_zkbin,
            &self.create_pool_pk,
            &call_data_input,
        )?;

        let params = CreatePoolParamsV1 {
            owner_pub,
            max_coverage_ratio,
            operator_fee_bp,
            pool_config_hash,
            nonce,
            derived_pool_id: public_inputs.derived_pool_id,
        };
        let mut call_data = vec![0x00];
        params.encode(&mut call_data)?;

        let pool_id = public_inputs.derived_pool_id;
        Ok(CreatePoolResult { call_data, proof, public_inputs, pool_id })
    }

    /// Join an existing pool (function code 0x01)
    pub fn join_pool(
        &self,
        pool_id: pallas::Base,
        amount: u64,
        relayer_id: [u8; 32],
        member_pub: PublicKey,
    ) -> Result<JoinPoolResult, Box<dyn std::error::Error>> {
        let token_id = pallas::Base::from(1u64);
        let nonce = 0u64;
        let value_blind = pallas::Scalar::zero();
        let call_data_input = JoinPoolV1CallData::new(
            pool_id, member_pub, amount, token_id, nonce, value_blind,
        );
        let (proof, public_inputs) = join_pool_v1_proof(
            &self.join_pool_zkbin,
            &self.join_pool_pk,
            &call_data_input,
        )?;

        let params = JoinPoolParamsV1 {
            pool_id,
            amount,
            relayer_id,
            member_pub,
            token_id,
            nonce,
            derived_member_id: public_inputs.derived_member_id,
            value_commit_x: public_inputs.value_commit_x,
            value_commit_y: public_inputs.value_commit_y,
        };
        let mut call_data = vec![0x01];
        params.encode(&mut call_data)?;

        let stake_id = public_inputs.derived_member_id;
        Ok(JoinPoolResult { call_data, proof, public_inputs, stake_id })
    }

    /// Leave a pool (function code 0x02) - no ZK proof required
    pub fn leave_pool(
        &self,
        stake_id: pallas::Base,
    ) -> Result<LeavePoolResult, Box<dyn std::error::Error>> {
        let params = LeavePoolParamsV1 { stake_id };
        let mut call_data = vec![0x02];
        params.encode(&mut call_data)?;

        Ok(LeavePoolResult { call_data })
    }

    /// Allocate coverage for a withdrawal (function code 0x03)
    pub fn allocate_coverage(
        &self,
        pool_id: pallas::Base,
        member_pub: PublicKey,
        coverage_amount: u64,
        withdrawal_id: pallas::Base,
        withdrawal_nullifier: [u8; 32],
        timeout_height: u64,
    ) -> Result<AllocateCoverageResult, Box<dyn std::error::Error>> {
        let nonce = 0u64;
        let call_data_input = AllocateCoverageV1CallData::new(
            pool_id, member_pub, coverage_amount, withdrawal_id, nonce,
        );
        let (proof, public_inputs) = allocate_coverage_v1_proof(
            &self.allocate_coverage_zkbin,
            &self.allocate_coverage_pk,
            &call_data_input,
        )?;

        let params = AllocateCoverageParamsV1 {
            pool_id,
            withdrawal_nullifier,
            amount: coverage_amount,
            timeout_height,
            member_pub,
            withdrawal_id,
            nonce,
            derived_allocation_id: public_inputs.derived_allocation_id,
        };
        let mut call_data = vec![0x03];
        params.encode(&mut call_data)?;

        let allocation_id = public_inputs.derived_allocation_id;
        Ok(AllocateCoverageResult { call_data, proof, public_inputs, allocation_id })
    }

    /// Slash coverage after failure (function code 0x05)
    pub fn slash_coverage(
        &self,
        allocation_id: pallas::Base,
        slash_amount: u64,
        user_pub: PublicKey,
    ) -> Result<SlashCoverageResult, Box<dyn std::error::Error>> {
        let nonce = 0u64;
        let call_data_input = SlashCoverageV1CallData::new(
            allocation_id, slash_amount, user_pub, nonce,
        );
        let (proof, public_inputs) = slash_coverage_v1_proof(
            &self.slash_coverage_zkbin,
            &self.slash_coverage_pk,
            &call_data_input,
        )?;

        let params = SlashCoverageParamsV1 {
            allocation_id,
            slash_amount,
            user_pub,
            nonce,
            derived_slash_id: public_inputs.derived_slash_id,
        };
        let mut call_data = vec![0x05];
        params.encode(&mut call_data)?;

        let slash_id = public_inputs.derived_slash_id;
        Ok(SlashCoverageResult { call_data, proof, public_inputs, slash_id })
    }
}

impl super::ContractHarness for PoolStakeHarness {
    fn name(&self) -> &str {
        "pool_stake"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreatePool", "JoinPool", "AllocateCoverage", "SlashCoverage"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreatePool" => Some(&self.create_pool_zkbin),
            "JoinPool" => Some(&self.join_pool_zkbin),
            "AllocateCoverage" => Some(&self.allocate_coverage_zkbin),
            "SlashCoverage" => Some(&self.slash_coverage_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreatePool" => Some(&self.create_pool_pk),
            "JoinPool" => Some(&self.join_pool_pk),
            "AllocateCoverage" => Some(&self.allocate_coverage_pk),
            "SlashCoverage" => Some(&self.slash_coverage_pk),
            _ => None,
        }
    }
}

/// Result of create_pool
pub struct CreatePoolResult {
    pub call_data: Vec<u8>,
    pub pool_id: pallas::Base,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: CreatePoolV1PublicInputs,
}

/// Result of join_pool
pub struct JoinPoolResult {
    pub call_data: Vec<u8>,
    pub stake_id: pallas::Base,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: JoinPoolV1PublicInputs,
}

/// Result of leave_pool
pub struct LeavePoolResult {
    pub call_data: Vec<u8>,
}

/// Result of allocate_coverage
pub struct AllocateCoverageResult {
    pub call_data: Vec<u8>,
    pub allocation_id: pallas::Base,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: AllocateCoverageV1PublicInputs,
}

/// Result of slash_coverage
pub struct SlashCoverageResult {
    pub call_data: Vec<u8>,
    pub slash_id: pallas::Base,
    pub proof: darkfi::zk::Proof,
    pub public_inputs: SlashCoverageV1PublicInputs,
}
