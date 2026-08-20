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

use dwow_bridge_contract::{model as bridge_model, BridgeFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`BridgeFunction::DepositV1`] function call parameter's bindings.
pub mod deposit_v1;
pub use deposit_v1::BridgeDepositParamsV1;

/// [`BridgeFunction::WithdrawV1`] function call parameter's bindings.
pub mod withdraw_v1;
pub use withdraw_v1::BridgeWithdrawParamsV1;

/// Decodes the parameters of a Bridge contract function call.
pub fn decode_bridge_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match BridgeFunction::try_from(function_index)? {
        BridgeFunction::InitializeV1 => {
            return Err(dwow_core::Error::ParseFailed("unsupported Bridge function"))
        }
        BridgeFunction::DepositV1 => {
            let params = bridge_model::DepositParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::WithdrawV1 => {
            let params = bridge_model::WithdrawParams::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create bridge module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "bridge")?;

    submod.add_class::<BridgeDepositParamsV1>()?;
    submod.add_class::<BridgeWithdrawParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.bridge", &submod)?;

    Ok(submod)
}
