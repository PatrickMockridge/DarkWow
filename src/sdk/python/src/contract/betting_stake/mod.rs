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

use dwow_betting_stake_contract::{model as betting_stake_model, BettingStakeFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`BettingStakeFunction::InitializeV1`] function call parameter's python bindings.
pub mod initialize_v1;
pub use initialize_v1::BettingStakeInitializeParamsV1;

/// [`BettingStakeFunction::StakeV1`] function call parameter's python bindings.
pub mod stake_v1;
pub use stake_v1::BettingStakeStakeParamsV1;

/// [`BettingStakeFunction::UnstakeV1`] function call parameter's python bindings.
pub mod unstake_v1;
pub use unstake_v1::BettingStakeUnstakeParamsV1;

/// [`BettingStakeFunction::ClaimEarningsV1`] function call parameter's python bindings.
pub mod claim_earnings_v1;
pub use claim_earnings_v1::BettingStakeClaimEarningsParamsV1;

/// [`BettingStakeFunction::UpdateRiskV1`] function call parameter's python bindings.
pub mod update_risk_v1;
pub use update_risk_v1::BettingStakeUpdateRiskParamsV1;

/// Decodes the parameters of a Betting Stake contract function call.
pub fn decode_betting_stake_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match BettingStakeFunction::try_from(function_index)
        .map_err(|_| dwow_core::Error::ParseFailed("invalid BettingStake function"))?
    {
        BettingStakeFunction::InitializeV1 => {
            let params = betting_stake_model::InitializeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BettingStakeFunction::StakeV1 => {
            let params = betting_stake_model::StakeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BettingStakeFunction::UnstakeV1 => {
            let params = betting_stake_model::UnstakeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BettingStakeFunction::ClaimEarningsV1 => {
            let params = betting_stake_model::ClaimEarningsParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BettingStakeFunction::UpdateRiskV1 => {
            let params = betting_stake_model::UpdateRiskParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create betting_stake module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "betting_stake")?;

    submod.add_class::<BettingStakeInitializeParamsV1>()?;
    submod.add_class::<BettingStakeStakeParamsV1>()?;
    submod.add_class::<BettingStakeUnstakeParamsV1>()?;
    submod.add_class::<BettingStakeClaimEarningsParamsV1>()?;
    submod.add_class::<BettingStakeUpdateRiskParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.betting_stake", &submod)?;

    Ok(submod)
}
