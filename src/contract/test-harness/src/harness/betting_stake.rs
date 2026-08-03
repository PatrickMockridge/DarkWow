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

//! BettingStake Test Harness
//!
//! Provides isolated testing for BettingStake contract.

use dwow_core::{
    zk::{ProvingKey, Proof, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::{Field, Group},
        PublicKey, SecretKey,
    },
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;

use dwow_betting_stake_contract::client::proof_gen::{
    init_v1_proof, stake_v1_proof, unstake_v1_proof, claim_v1_proof, update_risk_v1_proof,
    InitV1CallData, StakeV1CallData, UnstakeV1CallData, ClaimV1CallData, UpdateRiskV1CallData,
};
use dwow_betting_stake_contract::model::{
    ClaimEarningsParamsV1, InitializeParamsV1, StakeParamsV1, UnstakeParamsV1, UpdateRiskParamsV1,
};

/// BettingStake Harness for isolated testing
pub struct BettingStakeHarness {
    /// Init_V1 ZkBinary
    init_zkbin: ZkBinary,
    /// Init_V1 ProvingKey
    init_pk: ProvingKey,
    /// Stake_V1 ZkBinary
    stake_zkbin: ZkBinary,
    /// Stake_V1 ProvingKey
    stake_pk: ProvingKey,
    /// Unstake_V1 ZkBinary
    unstake_zkbin: ZkBinary,
    /// Unstake_V1 ProvingKey
    unstake_pk: ProvingKey,
    /// Claim_V1 ZkBinary
    claim_zkbin: ZkBinary,
    /// Claim_V1 ProvingKey
    claim_pk: ProvingKey,
    /// UpdateRisk_V1 ZkBinary
    update_risk_zkbin: ZkBinary,
    /// UpdateRisk_V1 ProvingKey
    update_risk_pk: ProvingKey,
}

impl BettingStakeHarness {
    /// Spawn a new BettingStake harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let init_bin = include_bytes!("../../../betting_stake/proof/init.zk.bin");
        let stake_bin = include_bytes!("../../../betting_stake/proof/stake.zk.bin");
        let unstake_bin = include_bytes!("../../../betting_stake/proof/unstake.zk.bin");
        let claim_bin = include_bytes!("../../../betting_stake/proof/claim.zk.bin");
        let update_risk_bin = include_bytes!("../../../betting_stake/proof/update_risk.zk.bin");

        let init_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let stake_zkbin = ZkBinary::decode(stake_bin, false).unwrap();
        let unstake_zkbin = ZkBinary::decode(unstake_bin, false).unwrap();
        let claim_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let update_risk_zkbin = ZkBinary::decode(update_risk_bin, false).unwrap();

        let init_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&init_zkbin).unwrap(), &init_zkbin);
        let stake_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&stake_zkbin).unwrap(), &stake_zkbin);
        let unstake_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&unstake_zkbin).unwrap(), &unstake_zkbin);
        let claim_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&claim_zkbin).unwrap(), &claim_zkbin);
        let update_risk_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&update_risk_zkbin).unwrap(), &update_risk_zkbin);

        let init_pk = ProvingKey::build(init_zkbin.k, &init_circuit).expect("ProvingKey::build failed");
        let stake_pk = ProvingKey::build(stake_zkbin.k, &stake_circuit).expect("ProvingKey::build failed");
        let unstake_pk = ProvingKey::build(unstake_zkbin.k, &unstake_circuit).expect("ProvingKey::build failed");
        let claim_pk = ProvingKey::build(claim_zkbin.k, &claim_circuit).expect("ProvingKey::build failed");
        let update_risk_pk = ProvingKey::build(update_risk_zkbin.k, &update_risk_circuit).expect("ProvingKey::build failed");

        Self {
            init_zkbin,
            init_pk,
            stake_zkbin,
            stake_pk,
            unstake_zkbin,
            unstake_pk,
            claim_zkbin,
            claim_pk,
            update_risk_zkbin,
            update_risk_pk,
        }
    }

    /// Initialize staking for a betting table (0x00)
    pub fn initialize(
        &self,
        betting_contract_id: pallas::Base,
        house_edge_bp: u32,
        risk_profile: u8,
    ) -> Result<InitializeResult> {
        let nonce = 0u64;

        // Generate ZK proof for Init circuit
        let input = InitV1CallData::new(betting_contract_id, house_edge_bp, risk_profile, nonce);
        let (proof, _public_inputs) = init_v1_proof(&self.init_zkbin, &self.init_pk, &input)?;

        let params = InitializeParamsV1 {
            betting_contract_id,
            house_edge_bp,
            risk_profile,
            nonce: pallas::Base::from(nonce),
            signature: dwow_sdk::crypto::schnorr::Signature::dummy(),
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(InitializeResult { call_data, proof })
    }

    /// Stake capital against a table (0x01)
    pub fn stake(
        &self,
        table_id: pallas::Base,
        staker_pub: PublicKey,
        staker_secret: SecretKey,
        amount: u64,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
    ) -> Result<StakeResult> {
        let nonce = 0u64;
        let token_id = pallas::Base::zero();
        let value_blind = pallas::Scalar::random(&mut OsRng);

        // Generate ZK proof for Stake circuit
        let input = StakeV1CallData::new(
            table_id,
            staker_pub,
            *staker_secret.inner(),
            amount,
            token_id,
            nonce,
            value_blind,
        );
        let (proof, _public_inputs) = stake_v1_proof(&self.stake_zkbin, &self.stake_pk, &input)?;

        let params = StakeParamsV1 {
            table_id,
            staker_pub,
            amount,
            nonce: pallas::Base::from(nonce),
            value_commit: pallas::Point::identity(),
            staker_nullifier: pallas::Base::from(3u64),
            spend_hook,
            user_data,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(StakeResult { call_data, proof })
    }

    /// Unstake and withdraw (0x02)
    pub fn unstake(
        &self,
        stake_id: pallas::Base,
        stake: &UnstakeStakeInfo,
        staker_secret: SecretKey,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
    ) -> Result<UnstakeResult> {
        let value_blind = pallas::Scalar::random(&mut OsRng);

        // Generate ZK proof for Unstake circuit
        let input = UnstakeV1CallData::new(
            stake.table_id,
            stake.staker_pub,
            *staker_secret.inner(),
            stake.original_amount,
            stake.current_amount,
            stake.accumulated_earnings,
            stake.token_id,
            stake.nonce,
            value_blind,
        );
        let (proof, _public_inputs) = unstake_v1_proof(&self.unstake_zkbin, &self.unstake_pk, &input)?;

        let params = UnstakeParamsV1 {
            stake_id,
            table_id: stake.table_id,
            staker_pub: stake.staker_pub,
            original_amount: stake.original_amount,
            nonce: pallas::Base::from(stake.nonce),
            value_commit: pallas::Point::identity(),
            staker_nullifier: pallas::Base::from(3u64),
            spend_hook,
            user_data,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(UnstakeResult { call_data, proof })
    }

    /// Claim accumulated earnings (0x03)
    pub fn claim_earnings(
        &self,
        stake_id: pallas::Base,
        stake: &ClaimStakeInfo,
        staker_secret: SecretKey,
    ) -> Result<ClaimEarningsResult> {
        let value_blind = pallas::Scalar::random(&mut OsRng);

        // Generate ZK proof for Claim circuit
        let input = ClaimV1CallData::new(
            stake.table_id,
            stake.staker_pub,
            *staker_secret.inner(),
            stake.current_amount,
            stake.accumulated_earnings,
            stake.token_id,
            stake.nonce,
            value_blind,
        );
        let (proof, _public_inputs) = claim_v1_proof(&self.claim_zkbin, &self.claim_pk, &input)?;

        let params = ClaimEarningsParamsV1 {
            stake_id,
            table_id: stake.table_id,
            staker_pub: stake.staker_pub,
            current_amount: stake.current_amount,
            nonce: pallas::Base::from(stake.nonce),
            value_commit: pallas::Point::identity(),
            staker_nullifier: pallas::Base::from(3u64),
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(ClaimEarningsResult { call_data, proof })
    }

    /// Update risk after payout (0x04)
    pub fn update_risk(
        &self,
        table_id: pallas::Base,
        betting_contract_id: pallas::Base,
        total_stake: u64,
        accumulated_losses: u64,
        house_edge_bp: u32,
        risk_profile: u8,
    ) -> Result<UpdateRiskResult> {
        let nonce = 0u64;

        // Generate ZK proof for UpdateRisk circuit
        let input = UpdateRiskV1CallData::new(
            betting_contract_id,
            total_stake,
            accumulated_losses,
            house_edge_bp,
            risk_profile,
            nonce,
        );
        let (proof, _public_inputs) = update_risk_v1_proof(&self.update_risk_zkbin, &self.update_risk_pk, &input)?;

        let params = UpdateRiskParamsV1 {
            table_id,
            payout_amount: 0,  // Not used in circuit, just for params
            house_share: 0,
            betting_contract_id,
            nonce: pallas::Base::from(nonce),
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(UpdateRiskResult { call_data, proof })
    }
}

/// Stake information needed for unstake
#[derive(Debug, Clone)]
pub struct UnstakeStakeInfo {
    pub table_id: pallas::Base,
    pub staker_pub: PublicKey,
    pub original_amount: u64,
    pub current_amount: u64,
    pub accumulated_earnings: u64,
    pub token_id: pallas::Base,
    pub nonce: u64,
}

impl UnstakeStakeInfo {
    pub fn new(
        table_id: pallas::Base,
        staker_pub: PublicKey,
        original_amount: u64,
        current_amount: u64,
        accumulated_earnings: u64,
        token_id: pallas::Base,
        nonce: u64,
    ) -> Self {
        Self { table_id, staker_pub, original_amount, current_amount, accumulated_earnings, token_id, nonce }
    }
}

/// Stake information needed for claim
#[derive(Debug, Clone)]
pub struct ClaimStakeInfo {
    pub table_id: pallas::Base,
    pub staker_pub: PublicKey,
    pub current_amount: u64,
    pub accumulated_earnings: u64,
    pub token_id: pallas::Base,
    pub nonce: u64,
}

impl ClaimStakeInfo {
    pub fn new(
        table_id: pallas::Base,
        staker_pub: PublicKey,
        current_amount: u64,
        accumulated_earnings: u64,
        token_id: pallas::Base,
        nonce: u64,
    ) -> Self {
        Self { table_id, staker_pub, current_amount, accumulated_earnings, token_id, nonce }
    }
}

/// Result of InitializeV1
pub struct InitializeResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

/// Result of StakeV1
pub struct StakeResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

/// Result of UnstakeV1
pub struct UnstakeResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

/// Result of ClaimEarningsV1
pub struct ClaimEarningsResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

/// Result of UpdateRiskV1
pub struct UpdateRiskResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

impl super::ContractHarness for BettingStakeHarness {
    fn name(&self) -> &str {
        "betting_stake"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["Init", "Stake", "Unstake", "Claim", "UpdateRisk"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "Init" => Some(&self.init_zkbin),
            "Stake" => Some(&self.stake_zkbin),
            "Unstake" => Some(&self.unstake_zkbin),
            "Claim" => Some(&self.claim_zkbin),
            "UpdateRisk" => Some(&self.update_risk_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "Init" => Some(&self.init_pk),
            "Stake" => Some(&self.stake_pk),
            "Unstake" => Some(&self.unstake_pk),
            "Claim" => Some(&self.claim_pk),
            "UpdateRisk" => Some(&self.update_risk_pk),
            _ => None,
        }
    }
}