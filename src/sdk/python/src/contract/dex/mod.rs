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

use dwow_dex_contract::{model as dex_model, DexFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`DexFunction::InitializeV1`] function call parameter's bindings.
pub mod initialize_v1;
pub use initialize_v1::DexInitializeParamsV1;

/// [`DexFunction::CreateSwapV1`] function call parameter's bindings.
pub mod create_swap_v1;
pub use create_swap_v1::DexCreateSwapParamsV1;

/// [`DexFunction::AcceptSwapV1`] function call parameter's bindings.
pub mod accept_swap_v1;
pub use accept_swap_v1::DexAcceptSwapParamsV1;

/// [`DexFunction::ExecuteSwapV1`] function call parameter's bindings.
pub mod execute_swap_v1;
pub use execute_swap_v1::DexExecuteSwapParamsV1;

/// [`DexFunction::CancelSwapV1`] function call parameter's bindings.
pub mod cancel_swap_v1;
pub use cancel_swap_v1::DexCancelSwapParamsV1;

/// [`DexFunction::UpdateConfigV1`] function call parameter's bindings.
pub mod update_config_v1;
pub use update_config_v1::DexUpdateConfigParamsV1;

/// [`DexFunction::SetTransparencyLevelV1`] function call parameter's bindings.
pub mod set_transparency_level_v1;
pub use set_transparency_level_v1::DexSetTransparencyLevelParamsV1;

/// [`DexFunction::ExecuteSwapFeeV1`] function call parameter's bindings.
pub mod execute_swap_fee_v1;
pub use execute_swap_fee_v1::DexExecuteSwapFeeParamsV1;

/// [`DexFunction::ExecuteSwapSlippageV1`] function call parameter's bindings.
pub mod execute_swap_slippage_v1;
pub use execute_swap_slippage_v1::DexExecuteSwapSlippageParamsV1;

/// Decodes the parameters of a DEX contract function call.
pub fn decode_dex_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match DexFunction::try_from(function_index)? {
        DexFunction::InitializeV1 => {
            let params = dex_model::InitializeParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::CreateSwapV1 => {
            let params = dex_model::CreateSwapParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::AcceptSwapV1 => {
            let params = dex_model::AcceptSwapParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::ExecuteSwapV1 => {
            let params = dex_model::ExecuteSwapParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::CancelSwapV1 => {
            let params = dex_model::CancelSwapParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::UpdateConfigV1 => {
            let params = dex_model::UpdateConfigParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::SetTransparencyLevelV1 => {
            let params = dex_model::SetTransparencyLevelParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::ExecuteSwapFeeV1 => {
            let params = dex_model::ExecuteSwapFeeParams::decode(&data[1..])?;
            Box::new(params)
        }
        DexFunction::ExecuteSwapSlippageV1 => {
            let params = dex_model::ExecuteSwapSlippageParams::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create dex module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "dex")?;

    submod.add_class::<DexInitializeParamsV1>()?;
    submod.add_class::<DexCreateSwapParamsV1>()?;
    submod.add_class::<DexAcceptSwapParamsV1>()?;
    submod.add_class::<DexExecuteSwapParamsV1>()?;
    submod.add_class::<DexCancelSwapParamsV1>()?;
    submod.add_class::<DexUpdateConfigParamsV1>()?;
    submod.add_class::<DexSetTransparencyLevelParamsV1>()?;
    submod.add_class::<DexExecuteSwapFeeParamsV1>()?;
    submod.add_class::<DexExecuteSwapSlippageParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.dex", &submod)?;

    Ok(submod)
}
