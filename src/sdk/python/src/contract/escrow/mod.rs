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

use dwow_escrow_contract::{model as escrow_model, EscrowFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`EscrowFunction::CreateEscrowV1`] function call parameter's bindings.
pub mod create_escrow_v1;
pub use create_escrow_v1::EscrowCreateEscrowParamsV1;

/// [`EscrowFunction::FundV1`] function call parameter's bindings.
pub mod fund_v1;
pub use fund_v1::EscrowFundParamsV1;

/// [`EscrowFunction::ClaimV1`] function call parameter's bindings.
pub mod claim_v1;
pub use claim_v1::EscrowClaimParamsV1;

/// [`EscrowFunction::RefundV1`] function call parameter's bindings.
pub mod refund_v1;
pub use refund_v1::EscrowRefundParamsV1;

/// [`EscrowFunction::CancelV1`] function call parameter's bindings.
pub mod cancel_v1;
pub use cancel_v1::EscrowCancelParamsV1;

/// Decodes the parameters of an Escrow contract function call.
pub fn decode_escrow_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match EscrowFunction::try_from(function_index)? {
        EscrowFunction::InitializeV1 => {
            return Err(dwow_core::Error::ParseFailed("unsupported Escrow function"))
        }
        EscrowFunction::CreateEscrowV1 => {
            let params = escrow_model::CreateEscrowParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        EscrowFunction::FundV1 => {
            let params = escrow_model::FundEscrowParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        EscrowFunction::ClaimV1 => {
            let params = escrow_model::ClaimEscrowParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        EscrowFunction::RefundV1 => {
            let params = escrow_model::RefundEscrowParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        EscrowFunction::CancelV1 => {
            let params = escrow_model::CancelEscrowParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create escrow module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "escrow")?;

    submod.add_class::<EscrowCreateEscrowParamsV1>()?;
    submod.add_class::<EscrowFundParamsV1>()?;
    submod.add_class::<EscrowClaimParamsV1>()?;
    submod.add_class::<EscrowRefundParamsV1>()?;
    submod.add_class::<EscrowCancelParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.escrow", &submod)?;

    Ok(submod)
}
