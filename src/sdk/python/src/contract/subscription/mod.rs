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

use dwow_subscription_contract::{model as subscription_model, SubscriptionFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`SubscriptionFunction::SubscribeV1`] function call parameter's python bindings.
pub mod subscribe_v1;
pub use subscribe_v1::SubscriptionSubscribeParamsV1;

/// [`SubscriptionFunction::CancelV1`] function call parameter's python bindings.
pub mod cancel_v1;
pub use cancel_v1::SubscriptionCancelParamsV1;

/// [`SubscriptionFunction::RenewV1`] function call parameter's python bindings.
pub mod renew_v1;
pub use renew_v1::SubscriptionRenewParamsV1;

/// [`SubscriptionFunction::UpdateUsageV1`] function call parameter's python bindings.
pub mod update_usage_v1;
pub use update_usage_v1::SubscriptionUpdateUsageParamsV1;

/// [`SubscriptionFunction::DaoControlV1`] function call parameter's python bindings.
pub mod dao_control_v1;
pub use dao_control_v1::SubscriptionDaoControlParamsV1;

/// Decodes the parameters of a Subscription contract function call.
pub fn decode_subscription_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match SubscriptionFunction::try_from(function_index)? {
        SubscriptionFunction::SubscribeV1 => {
            let params = subscription_model::SubscribeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        SubscriptionFunction::CancelV1 => {
            let params = subscription_model::CancelParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        SubscriptionFunction::RenewV1 => {
            let params = subscription_model::RenewParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        SubscriptionFunction::UpdateUsageV1 => {
            let params = subscription_model::UpdateUsageParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        SubscriptionFunction::DaoControlV1 => {
            let params = subscription_model::DaoControlParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        _ => return Err(dwow_core::Error::ParseFailed("unsupported Subscription function")),
    };

    Ok(res)
}

/// Create subscription module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "subscription")?;

    submod.add_class::<SubscriptionSubscribeParamsV1>()?;
    submod.add_class::<SubscriptionCancelParamsV1>()?;
    submod.add_class::<SubscriptionRenewParamsV1>()?;
    submod.add_class::<SubscriptionUpdateUsageParamsV1>()?;
    submod.add_class::<SubscriptionDaoControlParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.subscription", &submod)?;

    Ok(submod)
}
