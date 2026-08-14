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

use dwow_labor_market_contract::{model as labor_market_model, LaborMarketFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`LaborMarketFunction::CreateJobV1`] function call parameter's python bindings.
pub mod create_job_v1;
pub use create_job_v1::LaborMarketCreateJobParamsV1;

/// [`LaborMarketFunction::AcceptJobV1`] function call parameter's python bindings.
pub mod accept_job_v1;
pub use accept_job_v1::LaborMarketAcceptJobParamsV1;

/// [`LaborMarketFunction::SubmitDeliverableV1`] function call parameter's python bindings.
pub mod submit_deliverable_v1;
pub use submit_deliverable_v1::LaborMarketSubmitDeliverableParamsV1;

/// [`LaborMarketFunction::SubmitGitDeliverableV1`] function call parameter's python bindings.
pub mod submit_git_deliverable_v1;
pub use submit_git_deliverable_v1::LaborMarketSubmitGitDeliverableParamsV1;

/// [`LaborMarketFunction::ConfirmDeliveryV1`] function call parameter's python bindings.
pub mod confirm_delivery_v1;
pub use confirm_delivery_v1::LaborMarketConfirmDeliveryParamsV1;

/// [`LaborMarketFunction::DisputeV1`] function call parameter's python bindings.
pub mod dispute_v1;
pub use dispute_v1::LaborMarketDisputeParamsV1;

/// [`LaborMarketFunction::RefundV1`] function call parameter's python bindings.
pub mod refund_v1;
pub use refund_v1::LaborMarketRefundParamsV1;

/// [`LaborMarketFunction::CancelV1`] function call parameter's python bindings.
pub mod cancel_v1;
pub use cancel_v1::LaborMarketCancelParamsV1;

/// [`LaborMarketFunction::CreateJobWithMilestonesV1`] function call parameter's python bindings.
pub mod create_job_with_milestones_v1;
pub use create_job_with_milestones_v1::LaborMarketCreateJobWithMilestonesParamsV1;

/// [`LaborMarketFunction::SubmitMilestoneV1`] function call parameter's python bindings.
pub mod submit_milestone_v1;
pub use submit_milestone_v1::LaborMarketSubmitMilestoneParamsV1;

/// [`LaborMarketFunction::ConfirmMilestoneV1`] function call parameter's python bindings.
pub mod confirm_milestone_v1;
pub use confirm_milestone_v1::LaborMarketConfirmMilestoneParamsV1;

/// [`LaborMarketFunction::InitiateDisputeV1`] function call parameter's python bindings.
pub mod initiate_dispute_v1;
pub use initiate_dispute_v1::LaborMarketInitiateDisputeParamsV1;

/// [`LaborMarketFunction::CreateJobWithCapabilityV1`] function call parameter's python bindings.
pub mod create_job_with_capability_v1;
pub use create_job_with_capability_v1::LaborMarketCreateJobWithCapabilityParamsV1;

/// [`LaborMarketFunction::AcceptJobWithCapabilityV1`] function call parameter's python bindings.
pub mod accept_job_with_capability_v1;
pub use accept_job_with_capability_v1::LaborMarketAcceptJobWithCapabilityParamsV1;

/// [`LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1`] function call parameter's python bindings.
pub mod create_job_with_milestones_and_capability_v1;
pub use create_job_with_milestones_and_capability_v1::LaborMarketCreateJobWithMilestonesAndCapabilityParamsV1;

/// Decodes the parameters of a Labor-Market contract function call.
pub fn decode_labor_market_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match LaborMarketFunction::try_from(function_index)? {
        LaborMarketFunction::CreateJobV1 => {
            let params = labor_market_model::CreateJobParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::AcceptJobV1 => {
            let params = labor_market_model::AcceptJobParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::SubmitDeliverableV1 => {
            let params = labor_market_model::SubmitDeliverableParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::SubmitGitDeliverableV1 => {
            let params = labor_market_model::SubmitGitDeliverableParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::ConfirmDeliveryV1 => {
            let params = labor_market_model::ConfirmDeliveryParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::DisputeV1 => {
            let params = labor_market_model::DisputeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::RefundV1 => {
            let params = labor_market_model::RefundParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::CancelV1 => {
            let params = labor_market_model::CancelJobParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::CreateJobWithMilestonesV1 => {
            let params = labor_market_model::CreateJobWithMilestonesParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::SubmitMilestoneV1 => {
            let params = labor_market_model::SubmitMilestoneDeliverableParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::ConfirmMilestoneV1 => {
            let params = labor_market_model::ConfirmMilestoneParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::InitiateDisputeV1 => {
            let params = labor_market_model::InitiateDisputeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::CreateJobWithCapabilityV1 => {
            let params = labor_market_model::CreateJobWithCapabilityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::AcceptJobWithCapabilityV1 => {
            let params = labor_market_model::AcceptJobWithCapabilityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1 => {
            let params = labor_market_model::CreateJobWithMilestonesAndCapabilityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create labor_market module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "labor_market")?;

    submod.add_class::<LaborMarketCreateJobParamsV1>()?;
    submod.add_class::<LaborMarketAcceptJobParamsV1>()?;
    submod.add_class::<LaborMarketSubmitDeliverableParamsV1>()?;
    submod.add_class::<LaborMarketSubmitGitDeliverableParamsV1>()?;
    submod.add_class::<LaborMarketConfirmDeliveryParamsV1>()?;
    submod.add_class::<LaborMarketDisputeParamsV1>()?;
    submod.add_class::<LaborMarketRefundParamsV1>()?;
    submod.add_class::<LaborMarketCancelParamsV1>()?;
    submod.add_class::<LaborMarketCreateJobWithMilestonesParamsV1>()?;
    submod.add_class::<LaborMarketSubmitMilestoneParamsV1>()?;
    submod.add_class::<LaborMarketConfirmMilestoneParamsV1>()?;
    submod.add_class::<LaborMarketInitiateDisputeParamsV1>()?;
    submod.add_class::<LaborMarketCreateJobWithCapabilityParamsV1>()?;
    submod.add_class::<LaborMarketAcceptJobWithCapabilityParamsV1>()?;
    submod.add_class::<LaborMarketCreateJobWithMilestonesAndCapabilityParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.labor_market", &submod)?;

    Ok(submod)
}
