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

use dwow_otc_swap_contract::{model as otc_swap_model, OtcSwapFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`OtcSwapFunction::CreateSwapV1`] function call parameter's bindings.
pub mod create_swap_v1;
pub use create_swap_v1::OtcSwapCreateSwapParamsV1;

/// [`OtcSwapFunction::FundSwapV1`] function call parameter's bindings.
pub mod fund_swap_v1;
pub use fund_swap_v1::OtcSwapFundSwapParamsV1;

/// [`OtcSwapFunction::ExecuteSwapV1`] function call parameter's bindings.
pub mod execute_swap_v1;
pub use execute_swap_v1::OtcSwapExecuteSwapParamsV1;

/// [`OtcSwapFunction::CancelSwapV1`] function call parameter's bindings.
pub mod cancel_swap_v1;
pub use cancel_swap_v1::OtcSwapCancelSwapParamsV1;

/// Decodes the parameters of an OTC Swap contract function call.
pub fn decode_otc_swap_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match OtcSwapFunction::try_from(function_index)? {
        OtcSwapFunction::InitializeV1 => {
            return Err(dwow_core::Error::ParseFailed(
                "unsupported OtcSwap function",
            ))
        }
        OtcSwapFunction::CreateSwapV1 => {
            let params = otc_swap_model::CreateSwapParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        OtcSwapFunction::FundSwapV1 => {
            let params = otc_swap_model::FundSwapParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        OtcSwapFunction::ExecuteSwapV1 => {
            let params = otc_swap_model::ExecuteSwapParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        OtcSwapFunction::CancelSwapV1 => {
            let params = otc_swap_model::CancelSwapParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create otc_swap module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "otc_swap")?;

    submod.add_class::<OtcSwapCreateSwapParamsV1>()?;
    submod.add_class::<OtcSwapFundSwapParamsV1>()?;
    submod.add_class::<OtcSwapExecuteSwapParamsV1>()?;
    submod.add_class::<OtcSwapCancelSwapParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.otc_swap", &submod)?;

    Ok(submod)
}
