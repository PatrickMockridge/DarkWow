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

//! Stablecoin Test Harness
//!
//! Provides isolated testing for Stablecoin contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use darkfi_stablecoin_contract::client::{
    accrue_interest_v1::{AccrueInterestCallData, create_accrue_interest_proof},
    governance_report_v1::{GovernanceReportCallData, create_governance_report_proof},
    liquidate_v1::{LiquidateCallData, create_liquidate_proof},
    mint_stable_v1::{MintStableCallData, create_mint_stable_proof},
    open_position_v1::{OpenPositionCallData, create_open_position_proof},
};

/// Stablecoin Harness for isolated testing
pub struct StablecoinHarness {
    /// AccrueInterest_V1 ZkBinary
    accrue_interest_zkbin: ZkBinary,
    /// AccrueInterest_V1 ProvingKey
    accrue_interest_pk: ProvingKey,
    /// GovernanceReport_V1 ZkBinary
    governance_report_zkbin: ZkBinary,
    /// GovernanceReport_V1 ProvingKey
    governance_report_pk: ProvingKey,
    /// Liquidate_V1 ZkBinary
    liquidate_zkbin: ZkBinary,
    /// Liquidate_V1 ProvingKey
    liquidate_pk: ProvingKey,
    /// MintStable_V1 ZkBinary
    mint_stable_zkbin: ZkBinary,
    /// MintStable_V1 ProvingKey
    mint_stable_pk: ProvingKey,
    /// OpenPosition_V1 ZkBinary
    open_position_zkbin: ZkBinary,
    /// OpenPosition_V1 ProvingKey
    open_position_pk: ProvingKey,
}

impl StablecoinHarness {
    /// Spawn a new Stablecoin harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let accrue_bin = include_bytes!("../../../stablecoin/proof/accrue_interest_v1.zk.bin");
        let gov_bin = include_bytes!("../../../stablecoin/proof/governance_report_v1.zk.bin");
        let liquidate_bin = include_bytes!("../../../stablecoin/proof/liquidate_v1.zk.bin");
        let mint_bin = include_bytes!("../../../stablecoin/proof/mint_stable_v1.zk.bin");
        let open_bin = include_bytes!("../../../stablecoin/proof/open_position_v1.zk.bin");

        let accrue_interest_zkbin = ZkBinary::decode(accrue_bin, false).unwrap();
        let governance_report_zkbin = ZkBinary::decode(gov_bin, false).unwrap();
        let liquidate_zkbin = ZkBinary::decode(liquidate_bin, false).unwrap();
        let mint_stable_zkbin = ZkBinary::decode(mint_bin, false).unwrap();
        let open_position_zkbin = ZkBinary::decode(open_bin, false).unwrap();

        let accrue_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&accrue_interest_zkbin).unwrap(),
            &accrue_interest_zkbin,
        );
        let gov_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&governance_report_zkbin).unwrap(),
            &governance_report_zkbin,
        );
        let liquidate_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&liquidate_zkbin).unwrap(),
            &liquidate_zkbin,
        );
        let mint_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&mint_stable_zkbin).unwrap(),
            &mint_stable_zkbin,
        );
        let open_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&open_position_zkbin).unwrap(),
            &open_position_zkbin,
        );

        let accrue_interest_pk = ProvingKey::build(accrue_interest_zkbin.k, &accrue_circuit);
        let governance_report_pk = ProvingKey::build(governance_report_zkbin.k, &gov_circuit);
        let liquidate_pk = ProvingKey::build(liquidate_zkbin.k, &liquidate_circuit);
        let mint_stable_pk = ProvingKey::build(mint_stable_zkbin.k, &mint_circuit);
        let open_position_pk = ProvingKey::build(open_position_zkbin.k, &open_circuit);

        Self {
            accrue_interest_zkbin,
            accrue_interest_pk,
            governance_report_zkbin,
            governance_report_pk,
            liquidate_zkbin,
            liquidate_pk,
            mint_stable_zkbin,
            mint_stable_pk,
            open_position_zkbin,
            open_position_pk,
        }
    }

    /// Accrue interest on a position
    pub fn accrue_interest(
        &self,
        accumulator_secret: pallas::Base,
        old_total_debt: u64,
        rate_per_second: u64,
        time_elapsed: u64,
    ) -> Result<AccrueInterestResult, Box<dyn std::error::Error>> {
        let input = AccrueInterestCallData::new(accumulator_secret, old_total_debt, rate_per_second, time_elapsed);

        let (proof, public_inputs) = create_accrue_interest_proof(
            &self.accrue_interest_zkbin,
            &self.accrue_interest_pk,
            &input,
        )?;

        Ok(AccrueInterestResult {
            old_total_debt: public_inputs.old_total_debt,
            new_total_debt: public_inputs.new_total_debt,
            interest_amount: public_inputs.interest_amount,
            proof,
        })
    }

    /// Open a collateral position
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

        Ok(OpenPositionResult {
            position_commitment: public_inputs.position_commitment,
            position_nullifier: public_inputs.position_nullifier,
            proof,
        })
    }
}

impl super::ContractHarness for StablecoinHarness {
    fn name(&self) -> &str {
        "stablecoin"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "AccrueInterestV1",
            "GovernanceReportV1",
            "LiquidateV1",
            "MintStableV1",
            "OpenPositionV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "AccrueInterestV1" => Some(&self.accrue_interest_zkbin),
            "GovernanceReportV1" => Some(&self.governance_report_zkbin),
            "LiquidateV1" => Some(&self.liquidate_zkbin),
            "MintStableV1" => Some(&self.mint_stable_zkbin),
            "OpenPositionV1" => Some(&self.open_position_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "AccrueInterestV1" => Some(&self.accrue_interest_pk),
            "GovernanceReportV1" => Some(&self.governance_report_pk),
            "LiquidateV1" => Some(&self.liquidate_pk),
            "MintStableV1" => Some(&self.mint_stable_pk),
            "OpenPositionV1" => Some(&self.open_position_pk),
            _ => None,
        }
    }
}

/// Result of accrue_interest
pub struct AccrueInterestResult {
    pub old_total_debt: pallas::Base,
    pub new_total_debt: pallas::Base,
    pub interest_amount: pallas::Base,
    pub proof: darkfi::zk::Proof,
}

/// Result of open_position
pub struct OpenPositionResult {
    pub position_commitment: pallas::Base,
    pub position_nullifier: pallas::Base,
    pub proof: darkfi::zk::Proof,
}