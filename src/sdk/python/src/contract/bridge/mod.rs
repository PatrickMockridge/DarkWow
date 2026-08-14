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

/// [`BridgeFunction::UpdateConfigV1`] function call parameter's bindings.
pub mod update_config_v1;
pub use update_config_v1::BridgeUpdateConfigParamsV1;

/// [`BridgeFunction::CancelWithdrawV1`] function call parameter's bindings.
pub mod cancel_withdraw_v1;
pub use cancel_withdraw_v1::BridgeCancelWithdrawParamsV1;

/// [`BridgeFunction::ExecuteGuaranteedWithdrawV1`] function call parameter's bindings.
pub mod execute_guaranteed_withdraw_v1;
pub use execute_guaranteed_withdraw_v1::BridgeExecuteGuaranteedWithdrawParamsV1;

/// [`BridgeFunction::CreateHtlcV1`] function call parameter's bindings.
pub mod create_htlc_v1;
pub use create_htlc_v1::BridgeCreateHtlcParamsV1;

/// [`BridgeFunction::ClaimHtlcV1`] function call parameter's bindings.
pub mod claim_htlc_v1;
pub use claim_htlc_v1::BridgeClaimHtlcParamsV1;

/// [`BridgeFunction::RefundHtlcV1`] function call parameter's bindings.
pub mod refund_htlc_v1;
pub use refund_htlc_v1::BridgeRefundHtlcParamsV1;

/// [`BridgeFunction::ReassignWithdrawalV1`] function call parameter's bindings.
pub mod reassign_withdrawal_v1;
pub use reassign_withdrawal_v1::BridgeReassignWithdrawalParamsV1;

/// [`BridgeFunction::RegisterRelayerV1`] function call parameter's bindings.
pub mod register_relayer_v1;
pub use register_relayer_v1::BridgeRegisterRelayerParamsV1;

/// [`BridgeFunction::AcceptWithdrawalV1`] function call parameter's bindings.
pub mod accept_withdrawal_v1;
pub use accept_withdrawal_v1::BridgeAcceptWithdrawalParamsV1;

/// [`BridgeFunction::VerifyRelayerReputationV1`] function call parameter's bindings.
pub mod verify_relayer_reputation_v1;
pub use verify_relayer_reputation_v1::BridgeVerifyRelayerReputationParamsV1;

/// [`BridgeFunction::RegisterFeeScheduleV1`] function call parameter's bindings.
pub mod register_fee_schedule_v1;
pub use register_fee_schedule_v1::BridgeRegisterFeeScheduleParamsV1;

/// [`BridgeFunction::GovernanceReportV1`] function call parameter's bindings.
pub mod governance_report_v1;
pub use governance_report_v1::BridgeGovernanceReportParamsV1;

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
        BridgeFunction::UpdateConfigV1 => {
            let params = bridge_model::UpdateConfigParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::CancelWithdrawV1 => {
            let params = bridge_model::CancelWithdrawParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::ExecuteGuaranteedWithdrawV1 => {
            let params = bridge_model::ExecuteGuaranteedWithdrawParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::CreateHtlcV1 => {
            let params = bridge_model::CreateHtlcParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::ClaimHtlcV1 => {
            let params = bridge_model::ClaimHtlcParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::RefundHtlcV1 => {
            let params = bridge_model::RefundHtlcParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::ReassignWithdrawalV1 => {
            let params = bridge_model::ReassignWithdrawalParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::RegisterRelayerV1 => {
            let params = bridge_model::RegisterRelayerParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::AcceptWithdrawalV1 => {
            let params = bridge_model::AcceptWithdrawalParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::VerifyRelayerReputationV1 => {
            let params = bridge_model::VerifyRelayerReputationParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::RegisterFeeScheduleV1 => {
            let params = bridge_model::RegisterFeeScheduleParams::decode(&data[1..])?;
            Box::new(params)
        }
        BridgeFunction::GovernanceReportV1 => {
            let params = bridge_model::GovernanceReportParams::decode(&data[1..])?;
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
    submod.add_class::<BridgeUpdateConfigParamsV1>()?;
    submod.add_class::<BridgeCancelWithdrawParamsV1>()?;
    submod.add_class::<BridgeExecuteGuaranteedWithdrawParamsV1>()?;
    submod.add_class::<BridgeCreateHtlcParamsV1>()?;
    submod.add_class::<BridgeClaimHtlcParamsV1>()?;
    submod.add_class::<BridgeRefundHtlcParamsV1>()?;
    submod.add_class::<BridgeReassignWithdrawalParamsV1>()?;
    submod.add_class::<BridgeRegisterRelayerParamsV1>()?;
    submod.add_class::<BridgeAcceptWithdrawalParamsV1>()?;
    submod.add_class::<BridgeVerifyRelayerReputationParamsV1>()?;
    submod.add_class::<BridgeRegisterFeeScheduleParamsV1>()?;
    submod.add_class::<BridgeGovernanceReportParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.bridge", &submod)?;

    Ok(submod)
}
