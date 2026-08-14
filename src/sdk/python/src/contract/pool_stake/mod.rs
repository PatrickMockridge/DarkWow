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

use dwow_pool_stake_contract::{model as pool_stake_model, PoolStakeFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`PoolStakeFunction::CreatePoolV1`] function call parameter's python bindings.
pub mod create_pool_v1;
pub use create_pool_v1::PoolStakeCreatePoolParamsV1;

/// [`PoolStakeFunction::JoinPoolV1`] function call parameter's python bindings.
pub mod join_pool_v1;
pub use join_pool_v1::PoolStakeJoinPoolParamsV1;

/// [`PoolStakeFunction::LeavePoolV1`] function call parameter's python bindings.
pub mod leave_pool_v1;
pub use leave_pool_v1::PoolStakeLeavePoolParamsV1;

/// [`PoolStakeFunction::AllocateCoverageV1`] function call parameter's python bindings.
pub mod allocate_coverage_v1;
pub use allocate_coverage_v1::PoolStakeAllocateCoverageParamsV1;

/// [`PoolStakeFunction::ReleaseCoverageV1`] function call parameter's python bindings.
pub mod release_coverage_v1;
pub use release_coverage_v1::PoolStakeReleaseCoverageParamsV1;

/// [`PoolStakeFunction::SlashCoverageV1`] function call parameter's python bindings.
pub mod slash_coverage_v1;
pub use slash_coverage_v1::PoolStakeSlashCoverageParamsV1;

/// [`PoolStakeFunction::ClaimFeesV1`] function call parameter's python bindings.
pub mod claim_fees_v1;
pub use claim_fees_v1::PoolStakeClaimFeesParamsV1;

/// [`PoolStakeFunction::UpdatePoolConfigV1`] function call parameter's python bindings.
pub mod update_pool_config_v1;
pub use update_pool_config_v1::PoolStakeUpdatePoolConfigParamsV1;

/// [`PoolStakeFunction::RebalancePoolSharesV1`] function call parameter's python bindings.
pub mod rebalance_pool_shares_v1;
pub use rebalance_pool_shares_v1::PoolStakeRebalancePoolSharesParamsV1;

/// Decodes the parameters of a Pool-Stake contract function call.
pub fn decode_pool_stake_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match PoolStakeFunction::try_from(function_index)? {
        PoolStakeFunction::CreatePoolV1 => {
            let params = pool_stake_model::CreatePoolParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::JoinPoolV1 => {
            let params = pool_stake_model::JoinPoolParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::LeavePoolV1 => {
            let params = pool_stake_model::LeavePoolParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::AllocateCoverageV1 => {
            let params = pool_stake_model::AllocateCoverageParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::ReleaseCoverageV1 => {
            let params = pool_stake_model::ReleaseCoverageParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::SlashCoverageV1 => {
            let params = pool_stake_model::SlashCoverageParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::ClaimFeesV1 => {
            let params = pool_stake_model::ClaimFeesParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::UpdatePoolConfigV1 => {
            let params = pool_stake_model::UpdatePoolConfigParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        PoolStakeFunction::RebalancePoolSharesV1 => {
            let params = pool_stake_model::RebalancePoolSharesParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create pool_stake module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "pool_stake")?;

    submod.add_class::<PoolStakeCreatePoolParamsV1>()?;
    submod.add_class::<PoolStakeJoinPoolParamsV1>()?;
    submod.add_class::<PoolStakeLeavePoolParamsV1>()?;
    submod.add_class::<PoolStakeAllocateCoverageParamsV1>()?;
    submod.add_class::<PoolStakeReleaseCoverageParamsV1>()?;
    submod.add_class::<PoolStakeSlashCoverageParamsV1>()?;
    submod.add_class::<PoolStakeClaimFeesParamsV1>()?;
    submod.add_class::<PoolStakeUpdatePoolConfigParamsV1>()?;
    submod.add_class::<PoolStakeRebalancePoolSharesParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.pool_stake", &submod)?;

    Ok(submod)
}
