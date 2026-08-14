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

use dwow_drain_protection_contract::{model as drain_protection_model, DrainProtectionFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`DrainProtectionFunction::InitializeV1`] function call parameter's python bindings.
pub mod initialize_v1;
pub use initialize_v1::DrainProtectionInitializeParamsV1;

/// [`DrainProtectionFunction::ProposeV1`] function call parameter's python bindings.
pub mod propose_v1;
pub use propose_v1::DrainProtectionProposeParamsV1;

/// [`DrainProtectionFunction::VoteV1`] function call parameter's python bindings.
pub mod vote_v1;
pub use vote_v1::DrainProtectionVoteParamsV1;

/// [`DrainProtectionFunction::ExecuteV1`] function call parameter's python bindings.
pub mod execute_v1;
pub use execute_v1::DrainProtectionExecuteParamsV1;

/// [`DrainProtectionFunction::ExitV1`] function call parameter's python bindings.
pub mod exit_v1;
pub use exit_v1::DrainProtectionExitParamsV1;

/// [`DrainProtectionFunction::TransferV1`] function call parameter's python bindings.
pub mod transfer_v1;
pub use transfer_v1::DrainProtectionTransferParamsV1;

/// [`DrainProtectionFunction::LockV1`] function call parameter's python bindings.
pub mod lock_v1;
pub use lock_v1::DrainProtectionLockParamsV1;

/// [`DrainProtectionFunction::UnlockV1`] function call parameter's python bindings.
pub mod unlock_v1;
pub use unlock_v1::DrainProtectionUnlockParamsV1;

/// [`DrainProtectionFunction::UpdateConfigV1`] function call parameter's python bindings.
pub mod update_config_v1;
pub use update_config_v1::DrainProtectionUpdateConfigParamsV1;

/// Decodes the parameters of a Drain Protection contract function call.
pub fn decode_drain_protection_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match DrainProtectionFunction::try_from(function_index)
        .map_err(|e| dwow_core::Error::ContractError(e.into()))?
    {
        DrainProtectionFunction::InitializeV1 => {
            let params = drain_protection_model::InitializeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::ProposeV1 => {
            let params = drain_protection_model::ProposeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::VoteV1 => {
            let params = drain_protection_model::VoteParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::ExecuteV1 => {
            let params = drain_protection_model::ExecuteParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::ExitV1 => {
            let params = drain_protection_model::ExitParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::TransferV1 => {
            let params = drain_protection_model::TransferParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::LockV1 => {
            let params = drain_protection_model::LockParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::UnlockV1 => {
            let params = drain_protection_model::UnlockParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DrainProtectionFunction::UpdateConfigV1 => {
            let params = drain_protection_model::UpdateConfigParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create drain_protection module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "drain_protection")?;

    submod.add_class::<DrainProtectionInitializeParamsV1>()?;
    submod.add_class::<DrainProtectionProposeParamsV1>()?;
    submod.add_class::<DrainProtectionVoteParamsV1>()?;
    submod.add_class::<DrainProtectionExecuteParamsV1>()?;
    submod.add_class::<DrainProtectionExitParamsV1>()?;
    submod.add_class::<DrainProtectionTransferParamsV1>()?;
    submod.add_class::<DrainProtectionLockParamsV1>()?;
    submod.add_class::<DrainProtectionUnlockParamsV1>()?;
    submod.add_class::<DrainProtectionUpdateConfigParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.drain_protection", &submod)?;

    Ok(submod)
}
