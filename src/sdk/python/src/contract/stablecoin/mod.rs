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

use dwow_stablecoin_contract::{model as stablecoin_model, StablecoinFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`StablecoinFunction::InitializeV1`] function call parameter's bindings.
pub mod initialize_v1;
pub use initialize_v1::StablecoinInitializeParamsV1;

/// [`StablecoinFunction::OpenPositionV1`] function call parameter's bindings.
pub mod open_position_v1;
pub use open_position_v1::StablecoinOpenPositionParamsV1;

/// [`StablecoinFunction::AddCollateralV1`] function call parameter's bindings.
pub mod add_collateral_v1;
pub use add_collateral_v1::StablecoinAddCollateralParamsV1;

/// [`StablecoinFunction::RemoveCollateralV1`] function call parameter's bindings.
pub mod remove_collateral_v1;
pub use remove_collateral_v1::StablecoinRemoveCollateralParamsV1;

/// [`StablecoinFunction::MintStableV1`] function call parameter's bindings.
pub mod mint_stable_v1;
pub use mint_stable_v1::StablecoinMintStableParamsV1;

/// [`StablecoinFunction::RepayStableV1`] function call parameter's bindings.
pub mod repay_stable_v1;
pub use repay_stable_v1::StablecoinRepayStableParamsV1;

/// [`StablecoinFunction::LiquidateV1`] function call parameter's bindings.
pub mod liquidate_v1;
pub use liquidate_v1::StablecoinLiquidateParamsV1;

/// [`StablecoinFunction::UpdateConfigV1`] function call parameter's bindings.
pub mod update_config_v1;
pub use update_config_v1::StablecoinUpdateConfigParamsV1;

/// [`StablecoinFunction::GovernanceReportV1`] function call parameter's bindings.
pub mod governance_report_v1;
pub use governance_report_v1::StablecoinGovernanceReportParamsV1;

/// [`StablecoinFunction::AccrueInterestV1`] function call parameter's bindings.
pub mod accrue_interest_v1;
pub use accrue_interest_v1::StablecoinAccrueInterestParamsV1;

/// [`StablecoinFunction::RedeemStableV1`] function call parameter's bindings.
pub mod redeem_stable_v1;
pub use redeem_stable_v1::StablecoinRedeemStableParamsV1;

/// Decodes the parameters of a Stablecoin contract function call.
pub fn decode_stablecoin_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match StablecoinFunction::try_from(function_index)? {
        StablecoinFunction::InitializeV1 => {
            let params = stablecoin_model::InitializeParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::OpenPositionV1 => {
            let params = stablecoin_model::DepositCollateralParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::AddCollateralV1 => {
            let params = stablecoin_model::DepositCollateralParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::RemoveCollateralV1 => {
            let params = stablecoin_model::WithdrawCollateralParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::MintStableV1 => {
            let params = stablecoin_model::MintStableParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::RepayStableV1 => {
            let params = stablecoin_model::RepayStableParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::LiquidateV1 => {
            let params = stablecoin_model::LiquidateParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::UpdateConfigV1 => {
            let params = stablecoin_model::UpdateConfigParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::GovernanceReportV1 => {
            let params = stablecoin_model::GovernanceReportParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::AccrueInterestV1 => {
            let params = stablecoin_model::AccrueInterestParams::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::RedeemStableV1 => {
            let params = stablecoin_model::RedeemStableParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        StablecoinFunction::SpendHookCallback => {
            return Err(dwow_core::Error::ParseFailed(
                "unsupported Stablecoin function",
            ))
        }
    };

    Ok(res)
}

/// Create stablecoin module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "stablecoin")?;

    submod.add_class::<StablecoinInitializeParamsV1>()?;
    submod.add_class::<StablecoinOpenPositionParamsV1>()?;
    submod.add_class::<StablecoinAddCollateralParamsV1>()?;
    submod.add_class::<StablecoinRemoveCollateralParamsV1>()?;
    submod.add_class::<StablecoinMintStableParamsV1>()?;
    submod.add_class::<StablecoinRepayStableParamsV1>()?;
    submod.add_class::<StablecoinLiquidateParamsV1>()?;
    submod.add_class::<StablecoinUpdateConfigParamsV1>()?;
    submod.add_class::<StablecoinGovernanceReportParamsV1>()?;
    submod.add_class::<StablecoinAccrueInterestParamsV1>()?;
    submod.add_class::<StablecoinRedeemStableParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.stablecoin", &submod)?;

    Ok(submod)
}
