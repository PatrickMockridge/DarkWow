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

use dwow_relayer_endowment_contract::{model as relayer_endowment_model, RelayerEndowmentFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`RelayerEndowmentFunction::InitializeV1`] function call parameter's python bindings.
pub mod initialize_v1;
pub use initialize_v1::RelayerEndowmentInitializeParamsV1;

/// [`RelayerEndowmentFunction::DeployCapitalV1`] function call parameter's python bindings.
pub mod deploy_capital_v1;
pub use deploy_capital_v1::RelayerEndowmentDeployCapitalParamsV1;

/// [`RelayerEndowmentFunction::WithdrawDeploymentV1`] function call parameter's python bindings.
pub mod withdraw_deployment_v1;
pub use withdraw_deployment_v1::RelayerEndowmentWithdrawDeploymentParamsV1;

/// [`RelayerEndowmentFunction::ClaimRelayerFeesV1`] function call parameter's python bindings.
pub mod claim_relayer_fees_v1;
pub use claim_relayer_fees_v1::RelayerEndowmentClaimRelayerFeesParamsV1;

/// [`RelayerEndowmentFunction::SettleFeesV1`] function call parameter's python bindings.
pub mod settle_fees_v1;
pub use settle_fees_v1::RelayerEndowmentSettleFeesParamsV1;

/// [`RelayerEndowmentFunction::UpdateConfigV1`] function call parameter's python bindings.
pub mod update_config_v1;
pub use update_config_v1::RelayerEndowmentUpdateConfigParamsV1;

/// [`RelayerEndowmentFunction::ForceSettleV1`] function call parameter's python bindings.
pub mod force_settle_v1;
pub use force_settle_v1::RelayerEndowmentForceSettleParamsV1;

/// [`RelayerEndowmentFunction::DeactivateEndowmentV1`] function call parameter's python bindings.
pub mod deactivate_endowment_v1;
pub use deactivate_endowment_v1::RelayerEndowmentDeactivateEndowmentParamsV1;

/// Decodes the parameters of a Relayer-Endowment contract function call.
pub fn decode_relayer_endowment_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match RelayerEndowmentFunction::try_from(function_index)? {
        RelayerEndowmentFunction::InitializeV1 => {
            let params = relayer_endowment_model::InitializeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RelayerEndowmentFunction::DeployCapitalV1 => {
            let params = relayer_endowment_model::DeployCapitalParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RelayerEndowmentFunction::WithdrawDeploymentV1 => {
            let params = relayer_endowment_model::WithdrawDeploymentParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RelayerEndowmentFunction::ClaimRelayerFeesV1 => {
            let params = relayer_endowment_model::ClaimFeesParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RelayerEndowmentFunction::SettleFeesV1 => {
            let params = relayer_endowment_model::SettleFeesParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RelayerEndowmentFunction::UpdateConfigV1 => {
            let params = relayer_endowment_model::UpdateConfigParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RelayerEndowmentFunction::ForceSettleV1 => {
            let params = relayer_endowment_model::ForceSettleParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RelayerEndowmentFunction::DeactivateEndowmentV1 => {
            let params = relayer_endowment_model::DeactivateEndowmentParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create relayer_endowment module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "relayer_endowment")?;

    submod.add_class::<RelayerEndowmentInitializeParamsV1>()?;
    submod.add_class::<RelayerEndowmentDeployCapitalParamsV1>()?;
    submod.add_class::<RelayerEndowmentWithdrawDeploymentParamsV1>()?;
    submod.add_class::<RelayerEndowmentClaimRelayerFeesParamsV1>()?;
    submod.add_class::<RelayerEndowmentSettleFeesParamsV1>()?;
    submod.add_class::<RelayerEndowmentUpdateConfigParamsV1>()?;
    submod.add_class::<RelayerEndowmentForceSettleParamsV1>()?;
    submod.add_class::<RelayerEndowmentDeactivateEndowmentParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.relayer_endowment", &submod)?;

    Ok(submod)
}
