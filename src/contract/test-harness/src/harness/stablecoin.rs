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

//! Stablecoin Test Harness
//!
//! Provides isolated testing for Stablecoin contract.

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, IntentCommitment, BaseBlind, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;
use dwow_stablecoin_contract::client::{
    open_position_v1::{OpenPositionCallData, create_open_position_proof},
    mint_stable_v1::{MintStableCallData, create_mint_stable_proof, MintStablePublicInputs},
    liquidate_v1::{LiquidateCallData, create_liquidate_proof, LiquidatePublicInputs},
    governance_report_v1::{GovernanceReportCallData, create_governance_report_proof, GovernanceReportPublicInputs},
    accrue_interest_v1::{AccrueInterestCallData, create_accrue_interest_proof, AccrueInterestPublicInputs},
    initialize_v1::{InitV1CallData, create_initialize_proof, InitV1PublicInputs},
};
use dwow_stablecoin_contract::model::{DepositCollateralParams, MintStableParams, LiquidateParams, GovernanceReportParams, AccrueInterestParams};

/// Helper to convert pallas::Base to IntentCommitment
fn to_intent_commitment(base: pallas::Base) -> IntentCommitment {
    IntentCommitment::from_bytes(base.to_repr()).unwrap()
}

/// Helper to convert pallas::Base to [u8; 32]
#[allow(dead_code)]
fn base_to_bytes(base: pallas::Base) -> [u8; 32] {
    base.to_repr()
}

/// Stablecoin Harness for isolated testing
pub struct StablecoinHarness {
    /// Init_V1 ZkBinary
    init_zkbin: ZkBinary,
    /// Init_V1 ProvingKey
    init_pk: ProvingKey,
    /// OpenPosition_V1 ZkBinary
    open_position_zkbin: ZkBinary,
    /// OpenPosition_V1 ProvingKey
    open_position_pk: ProvingKey,
    /// MintStable_V1 ZkBinary
    mint_stable_zkbin: ZkBinary,
    /// MintStable_V1 ProvingKey
    mint_stable_pk: ProvingKey,
    /// Liquidate_V1 ZkBinary
    liquidate_zkbin: ZkBinary,
    /// Liquidate_V1 ProvingKey
    liquidate_pk: ProvingKey,
    /// GovernanceReport_V1 ZkBinary
    governance_report_zkbin: ZkBinary,
    /// GovernanceReport_V1 ProvingKey
    governance_report_pk: ProvingKey,
    /// AccrueInterest_V1 ZkBinary
    accrue_interest_zkbin: ZkBinary,
    /// AccrueInterest_V1 ProvingKey
    accrue_interest_pk: ProvingKey,
    /// AddCollateral_V1 ZkBinary
    add_collateral_zkbin: ZkBinary,
    /// AddCollateral_V1 ProvingKey
    add_collateral_pk: ProvingKey,
    /// RemoveCollateral_V1 ZkBinary
    remove_collateral_zkbin: ZkBinary,
    /// RemoveCollateral_V1 ProvingKey
    remove_collateral_pk: ProvingKey,
    /// RepayStable_V1 ZkBinary
    repay_stable_zkbin: ZkBinary,
    /// RepayStable_V1 ProvingKey
    repay_stable_pk: ProvingKey,
    /// UpdateConfig_V1 ZkBinary
    update_config_zkbin: ZkBinary,
    /// UpdateConfig_V1 ProvingKey
    update_config_pk: ProvingKey,
}

impl StablecoinHarness {
    /// Spawn a new Stablecoin harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let init_bin = include_bytes!("../../../stablecoin/proof/init_v2.zk.bin");
        let open_bin = include_bytes!("../../../stablecoin/proof/open_position_v2.zk.bin");
        let mint_bin = include_bytes!("../../../stablecoin/proof/mint_stable_v2.zk.bin");
        let liquidate_bin = include_bytes!("../../../stablecoin/proof/liquidate_v2.zk.bin");
        let governance_bin = include_bytes!("../../../stablecoin/proof/governance_report_v2.zk.bin");
        let accrue_bin = include_bytes!("../../../stablecoin/proof/accrue_interest_v2.zk.bin");
        let add_collateral_bin = include_bytes!("../../../stablecoin/proof/add_collateral_v2.zk.bin");
        let remove_collateral_bin = include_bytes!("../../../stablecoin/proof/remove_collateral_v2.zk.bin");
        let repay_stable_bin = include_bytes!("../../../stablecoin/proof/repay_stable_v2.zk.bin");
        let update_config_bin = include_bytes!("../../../stablecoin/proof/update_config_v2.zk.bin");

        let init_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let open_position_zkbin = ZkBinary::decode(open_bin, false).unwrap();
        let mint_stable_zkbin = ZkBinary::decode(mint_bin, false).unwrap();
        let liquidate_zkbin = ZkBinary::decode(liquidate_bin, false).unwrap();
        let governance_report_zkbin = ZkBinary::decode(governance_bin, false).unwrap();
        let accrue_interest_zkbin = ZkBinary::decode(accrue_bin, false).unwrap();
        let add_collateral_zkbin = ZkBinary::decode(add_collateral_bin, false).unwrap();
        let remove_collateral_zkbin = ZkBinary::decode(remove_collateral_bin, false).unwrap();
        let repay_stable_zkbin = ZkBinary::decode(repay_stable_bin, false).unwrap();
        let update_config_zkbin = ZkBinary::decode(update_config_bin, false).unwrap();

        let init_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&init_zkbin).unwrap(),
            &init_zkbin,
        );
        let open_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&open_position_zkbin).unwrap(),
            &open_position_zkbin,
        );
        let mint_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&mint_stable_zkbin).unwrap(),
            &mint_stable_zkbin,
        );
        let liquidate_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&liquidate_zkbin).unwrap(),
            &liquidate_zkbin,
        );
        let governance_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&governance_report_zkbin).unwrap(),
            &governance_report_zkbin,
        );
        let accrue_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&accrue_interest_zkbin).unwrap(),
            &accrue_interest_zkbin,
        );
        let add_collateral_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&add_collateral_zkbin).unwrap(),
            &add_collateral_zkbin,
        );
        let remove_collateral_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&remove_collateral_zkbin).unwrap(),
            &remove_collateral_zkbin,
        );
        let repay_stable_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&repay_stable_zkbin).unwrap(),
            &repay_stable_zkbin,
        );
        let update_config_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&update_config_zkbin).unwrap(),
            &update_config_zkbin,
        );

        let init_pk = ProvingKey::build(init_zkbin.k, &init_circuit).expect("ProvingKey::build failed");
        let open_position_pk = ProvingKey::build(open_position_zkbin.k, &open_circuit).expect("ProvingKey::build failed");
        let mint_stable_pk = ProvingKey::build(mint_stable_zkbin.k, &mint_circuit).expect("ProvingKey::build failed");
        let liquidate_pk = ProvingKey::build(liquidate_zkbin.k, &liquidate_circuit).expect("ProvingKey::build failed");
        let governance_report_pk = ProvingKey::build(governance_report_zkbin.k, &governance_circuit).expect("ProvingKey::build failed");
        let accrue_interest_pk = ProvingKey::build(accrue_interest_zkbin.k, &accrue_circuit).expect("ProvingKey::build failed");
        let add_collateral_pk = ProvingKey::build(add_collateral_zkbin.k, &add_collateral_circuit).expect("ProvingKey::build failed");
        let remove_collateral_pk = ProvingKey::build(remove_collateral_zkbin.k, &remove_collateral_circuit).expect("ProvingKey::build failed");
        let repay_stable_pk = ProvingKey::build(repay_stable_zkbin.k, &repay_stable_circuit).expect("ProvingKey::build failed");
        let update_config_pk = ProvingKey::build(update_config_zkbin.k, &update_config_circuit).expect("ProvingKey::build failed");

        Self {
            init_zkbin,
            init_pk,
            open_position_zkbin,
            open_position_pk,
            mint_stable_zkbin,
            mint_stable_pk,
            liquidate_zkbin,
            liquidate_pk,
            governance_report_zkbin,
            governance_report_pk,
            accrue_interest_zkbin,
            accrue_interest_pk,
            add_collateral_zkbin,
            add_collateral_pk,
            remove_collateral_zkbin,
            remove_collateral_pk,
            repay_stable_zkbin,
            repay_stable_pk,
            update_config_zkbin,
            update_config_pk,
        }
    }

    /// Create an open position proof (deposit collateral)
    pub fn open_position(
        &self,
        owner_secret: pallas::Base,
        collateral_amount: u64,
        debt_amount: u64,
        collateral_type: pallas::Base,
    ) -> Result<OpenPositionResult, Box<dyn std::error::Error>> {
        let input = OpenPositionCallData::new(
            owner_secret,
            collateral_amount,
            debt_amount,
            collateral_type,
        );

        let (proof, public_inputs) = create_open_position_proof(
            &self.open_position_zkbin,
            &self.open_position_pk,
            &input,
        )?;

        // Build DepositCollateralParams (OpenPositionV1 uses this internally)
        let params = DepositCollateralParams {
            deposit_commitment: to_intent_commitment(public_inputs.position_commitment),
            collateral_amount,
            collateral_type: dwow_stablecoin_contract::model::CollateralType::Xmr,
            proof: vec![],
            fee: 0,
            zk_public_inputs: public_inputs.to_vec(),
        };

        let mut call_data = vec![0x01]; // OpenPositionV1
        call_data.extend_from_slice(&params.encode());

        Ok(OpenPositionResult {
            call_data,
            position_commitment: public_inputs.position_commitment,
            position_nullifier: public_inputs.position_nullifier,
            owner_public_key: input.owner_public_key(),
            collateral_commitment: input.collateral_commitment(),
            debt_commitment: input.debt_commitment(),
            proof,
        })
    }

    /// Mint stablecoin against a position
    pub fn mint_stable(
        &self,
        owner_secret: pallas::Base,
        old_collateral: u64,
        old_debt: u64,
        mint_amount: u64,
        collateral_blind: BaseBlind,
        debt_blind: BaseBlind,
        old_commitment: pallas::Base,
    ) -> Result<MintStableResult, Box<dyn std::error::Error>> {
        let input = MintStableCallData::new(
            owner_secret,
            old_collateral,
            old_debt,
            mint_amount,
            collateral_blind,
            debt_blind,
            old_commitment,
        );

        let (proof, public_inputs) = create_mint_stable_proof(
            &self.mint_stable_zkbin,
            &self.mint_stable_pk,
            &input,
        )?;

        // Build MintStableParams
        let params = MintStableParams {
            mint_commitment: to_intent_commitment(public_inputs.new_commitment),
            mint_amount,
            total_debt: old_debt + mint_amount,
            total_collateral: old_collateral,
            proof: vec![],
            fee: 0,
            zk_public_inputs: public_inputs.to_vec(),
        };

        let mut call_data = vec![0x04]; // MintStableV1
        call_data.extend_from_slice(&params.encode());

        Ok(MintStableResult {
            call_data,
            proof,
            public_inputs,
        })
    }

    /// Liquidate an underwater position
    pub fn liquidate(
        &self,
        owner_secret: pallas::Base,
        collateral_amount: u64,
        debt_amount: u64,
        liquidation_penalty: u64,
        current_price: u64,
        liquidator_reward: u64,
        collateral_blind: BaseBlind,
        debt_blind: BaseBlind,
        old_commitment: pallas::Base,
    ) -> Result<LiquidateResult, Box<dyn std::error::Error>> {
        let input = LiquidateCallData::new(
            owner_secret,
            collateral_amount,
            debt_amount,
            liquidation_penalty,
            current_price,
            liquidator_reward,
            collateral_blind,
            debt_blind,
            old_commitment,
        );

        let (proof, public_inputs) = create_liquidate_proof(
            &self.liquidate_zkbin,
            &self.liquidate_pk,
            &input,
        )?;

        // Build LiquidateParams (pooled debt model)
        let params = LiquidateParams {
            liquidation_commitment: to_intent_commitment(public_inputs.new_commitment),
            total_debt: debt_amount,
            total_collateral: collateral_amount,
            current_price,
            debt_to_cover: debt_amount,
            proof: vec![],
            liquidation_reward: liquidator_reward,
            fee: 0,
            zk_public_inputs: public_inputs.to_vec(),
        };

        let mut call_data = vec![0x06]; // LiquidateV1
        call_data.extend_from_slice(&params.encode());

        Ok(LiquidateResult {
            call_data,
            proof,
            public_inputs,
        })
    }

    /// Governance report for precise collateral/debt ratio
    pub fn governance_report(
        &self,
        reporter_secret: pallas::Base,
        total_collateral: u64,
        total_debt: u64,
        rate_per_second: u64,
        time_elapsed: u64,
        report_timestamp: u64,
    ) -> Result<GovernanceReportResult, Box<dyn std::error::Error>> {
        let input = GovernanceReportCallData::new(
            reporter_secret,
            total_collateral,
            total_debt,
            rate_per_second,
            time_elapsed,
            report_timestamp,
        );

        let (proof, public_inputs) = create_governance_report_proof(
            &self.governance_report_zkbin,
            &self.governance_report_pk,
            &input,
        )?;

        // Build GovernanceReportParams
        // Use reporter public key from secret
        let reporter_pub = PublicKey::from_secret(dwow_sdk::crypto::SecretKey::from_bytes(reporter_secret.to_repr()).unwrap());
        let (_reporter_pub_x, _reporter_pub_y) = reporter_pub.xy().expect("pk not identity");

        let params = GovernanceReportParams {
            token_id: pallas::Base::zero(),
            total_collateral,
            total_debt,
            total_redeemed: 0,
            outstanding: total_debt,
            collateral_ratio_bps: input.collateral_ratio_bps,
            interest_accrued: input.interest_accrued,
            report_timestamp,
            reporter_pub: reporter_pub,
            proof: vec![],
            fee: 0,
        };

        let mut call_data = vec![0x08]; // GovernanceReportV1
        call_data.extend_from_slice(&params.encode());

        Ok(GovernanceReportResult {
            call_data,
            proof,
            public_inputs,
        })
    }

    /// Accrue interest on a position
    pub fn accrue_interest(
        &self,
        accumulator_secret: pallas::Base,
        old_total_debt: u64,
        rate_per_second: u64,
        time_elapsed: u64,
    ) -> Result<AccrueInterestResult, Box<dyn std::error::Error>> {
        let input = AccrueInterestCallData::new(
            accumulator_secret,
            old_total_debt,
            rate_per_second,
            time_elapsed,
        );

        let (proof, public_inputs) = create_accrue_interest_proof(
            &self.accrue_interest_zkbin,
            &self.accrue_interest_pk,
            &input,
        )?;

        // Build AccrueInterestParams
        // Use accumulator public key from secret
        let accumulator_pub = PublicKey::from_secret(dwow_sdk::crypto::SecretKey::from_bytes(accumulator_secret.to_repr()).unwrap());
        let (_accumulator_pub_x, _accumulator_pub_y) = accumulator_pub.xy().expect("pk not identity");

        let params = AccrueInterestParams {
            old_total_debt,
            new_total_debt: input.compute_new_total_debt(),
            interest_amount: input.compute_interest(),
            rate_per_second,
            time_elapsed,
            accumulator_pub: accumulator_pub,
            proof: vec![],
            fee: 0,
        };

        let mut call_data = vec![0x09]; // AccrueInterestV1
        call_data.extend_from_slice(&params.encode());

        Ok(AccrueInterestResult {
            call_data,
            proof,
            public_inputs,
        })
    }

    /// Initialize the stablecoin contract (function code 0x00)
    pub fn initialize(
        &self,
        deployer_secret: pallas::Base,
        contract_salt: pallas::Base,
        params: &dwow_stablecoin_contract::model::InitializeParams,
    ) -> Result<InitializeResult, Box<dyn std::error::Error>> {
        let input = InitV1CallData::new(deployer_secret, contract_salt);

        let (proof, public_inputs) = create_initialize_proof(
            &self.init_zkbin,
            &self.init_pk,
            &input,
        )?;

        // Override deployer_auth from proof
        let mut params = params.clone();
        params.deployer_auth = public_inputs.deployer_auth;

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());

        Ok(InitializeResult { call_data, proof })
    }

    /// Add collateral with ZK proof (function code 0x02)
    pub fn add_collateral(
        &self,
        params: &dwow_stablecoin_contract::model::DepositCollateralParams,
    ) -> Result<AddCollateralResult, Box<dyn std::error::Error>> {
        let witnesses = dwow_core::zk::empty_witnesses(&self.add_collateral_zkbin)?;
        let circuit = ZkCircuit::new(witnesses, &self.add_collateral_zkbin);
        let proof = Proof::create(&self.add_collateral_pk, &[circuit], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        Ok(AddCollateralResult { call_data, proof })
    }

    /// Remove collateral with ZK proof (function code 0x03)
    pub fn remove_collateral(
        &self,
        params: &dwow_stablecoin_contract::model::WithdrawCollateralParams,
    ) -> Result<RemoveCollateralResult, Box<dyn std::error::Error>> {
        let witnesses = dwow_core::zk::empty_witnesses(&self.remove_collateral_zkbin)?;
        let circuit = ZkCircuit::new(witnesses, &self.remove_collateral_zkbin);
        let proof = Proof::create(&self.remove_collateral_pk, &[circuit], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(RemoveCollateralResult { call_data, proof })
    }

    /// Repay stable debt with ZK proof (function code 0x05)
    pub fn repay_stable(
        &self,
        params: &dwow_stablecoin_contract::model::RepayStableParams,
    ) -> Result<RepayStableResult, Box<dyn std::error::Error>> {
        let witnesses = dwow_core::zk::empty_witnesses(&self.repay_stable_zkbin)?;
        let circuit = ZkCircuit::new(witnesses, &self.repay_stable_zkbin);
        let proof = Proof::create(&self.repay_stable_pk, &[circuit], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());

        Ok(RepayStableResult { call_data, proof })
    }

    /// Update configuration (function code 0x07)
    pub fn update_config(
        &self,
        params: &dwow_stablecoin_contract::model::UpdateConfigParams,
    ) -> Result<UpdateConfigResult, Box<dyn std::error::Error>> {
        let witnesses = dwow_core::zk::empty_witnesses(&self.update_config_zkbin)?;
        let circuit = ZkCircuit::new(witnesses, &self.update_config_zkbin);
        let proof = Proof::create(&self.update_config_pk, &[circuit], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode());

        Ok(UpdateConfigResult { call_data, proof })
    }
}

impl super::ContractHarness for StablecoinHarness {
    fn name(&self) -> &str {
        "stablecoin"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "InitV1",
            "OpenPositionV1",
            "MintStableV1",
            "LiquidateV1",
            "GovernanceReportV1",
            "AccrueInterestV1",
            "AddCollateralV1",
            "RemoveCollateralV1",
            "RepayStableV1",
            "UpdateConfigV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "InitV1" => Some(&self.init_zkbin),
            "OpenPositionV1" => Some(&self.open_position_zkbin),
            "MintStableV1" => Some(&self.mint_stable_zkbin),
            "LiquidateV1" => Some(&self.liquidate_zkbin),
            "GovernanceReportV1" => Some(&self.governance_report_zkbin),
            "AccrueInterestV1" => Some(&self.accrue_interest_zkbin),
            "AddCollateralV1" => Some(&self.add_collateral_zkbin),
            "RemoveCollateralV1" => Some(&self.remove_collateral_zkbin),
            "RepayStableV1" => Some(&self.repay_stable_zkbin),
            "UpdateConfigV1" => Some(&self.update_config_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "InitV1" => Some(&self.init_pk),
            "OpenPositionV1" => Some(&self.open_position_pk),
            "MintStableV1" => Some(&self.mint_stable_pk),
            "LiquidateV1" => Some(&self.liquidate_pk),
            "GovernanceReportV1" => Some(&self.governance_report_pk),
            "AccrueInterestV1" => Some(&self.accrue_interest_pk),
            "AddCollateralV1" => Some(&self.add_collateral_pk),
            "RemoveCollateralV1" => Some(&self.remove_collateral_pk),
            "RepayStableV1" => Some(&self.repay_stable_pk),
            "UpdateConfigV1" => Some(&self.update_config_pk),
            _ => None,
        }
    }
}

/// Result of open_position
pub struct OpenPositionResult {
    pub call_data: Vec<u8>,
    pub position_commitment: pallas::Base,
    pub position_nullifier: pallas::Base,
    pub owner_public_key: pallas::Base,
    pub collateral_commitment: pallas::Base,
    pub debt_commitment: pallas::Base,
    pub proof: dwow_core::zk::Proof,
}

/// Result of mint_stable
pub struct MintStableResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: MintStablePublicInputs,
}

/// Result of liquidate
pub struct LiquidateResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: LiquidatePublicInputs,
}

/// Result of governance_report
pub struct GovernanceReportResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: GovernanceReportPublicInputs,
}

/// Result of accrue_interest
pub struct AccrueInterestResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AccrueInterestPublicInputs,
}

/// Result of initialize
pub struct InitializeResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}

/// Result of add_collateral
pub struct AddCollateralResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}

/// Result of remove_collateral
pub struct RemoveCollateralResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}

/// Result of repay_stable
pub struct RepayStableResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}

/// Result of update_config
pub struct UpdateConfigResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}