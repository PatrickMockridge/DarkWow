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

use dwow_slot_contract::{model as slot_model, SlotFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`SlotFunction::CommitSpinV1`] function call parameter's bindings.
pub mod commit_spin_v1;
pub use commit_spin_v1::SlotCommitSpinParamsV1;

/// [`SlotFunction::RevealSpinV1`] function call parameter's bindings.
pub mod reveal_spin_v1;
pub use reveal_spin_v1::SlotRevealSpinParamsV1;

/// [`SlotFunction::SettleSpinV1`] function call parameter's bindings.
pub mod settle_spin_v1;
pub use settle_spin_v1::SlotSettleSpinParamsV1;

/// [`SlotFunction::CancelSpinV1`] function call parameter's bindings.
pub mod cancel_spin_v1;
pub use cancel_spin_v1::SlotCancelSpinParamsV1;

/// Decodes the parameters of a Slot contract function call.
pub fn decode_slot_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match SlotFunction::try_from(function_index)? {
        SlotFunction::CommitSpinV1 => {
            let params = slot_model::CommitSpinParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        SlotFunction::RevealSpinV1 => {
            let params = slot_model::RevealSpinParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        SlotFunction::SettleSpinV1 => {
            let params = slot_model::SettleSpinParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        SlotFunction::CancelSpinV1 => {
            let params = slot_model::CancelSpinParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        _ => return Err(dwow_core::Error::ParseFailed("unsupported Slot function")),
    };

    Ok(res)
}

/// Create slot module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "slot")?;

    submod.add_class::<SlotCommitSpinParamsV1>()?;
    submod.add_class::<SlotRevealSpinParamsV1>()?;
    submod.add_class::<SlotSettleSpinParamsV1>()?;
    submod.add_class::<SlotCancelSpinParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.slot", &submod)?;

    Ok(submod)
}
