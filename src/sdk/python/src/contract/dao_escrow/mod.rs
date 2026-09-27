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

use dwow_dao_escrow_contract::{model as dao_escrow_model, DaoEscrowFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`DaoEscrowFunction::InitializeV1`] function call parameter's python bindings.
pub mod initialize_v1;
pub use initialize_v1::DaoEscrowInitializeParamsV1;

/// [`DaoEscrowFunction::UpdateV1`] function call parameter's python bindings.
pub mod update_v1;
pub use update_v1::DaoEscrowUpdateParamsV1;

/// [`DaoEscrowFunction::PayPremiumV1`] function call parameter's python bindings.
pub mod pay_premium_v1;
pub use pay_premium_v1::DaoEscrowPayPremiumParamsV1;

/// [`DaoEscrowFunction::WithdrawV1`] function call parameter's python bindings.
pub mod withdraw_v1;
pub use withdraw_v1::DaoEscrowWithdrawParamsV1;

/// [`DaoEscrowFunction::EndowmentWithdrawV1`] function call parameter's python bindings.
pub mod endowment_withdraw_v1;
pub use endowment_withdraw_v1::DaoEscrowEndowmentWithdrawParamsV1;

/// [`DaoEscrowFunction::TreasurySpendV1`] function call parameter's python bindings.
pub mod treasury_spend_v1;
pub use treasury_spend_v1::DaoEscrowTreasurySpendParamsV1;

// Five binding modules were removed here, one per retired selector:
// `enable_drain_protection_v1` (0x06), `register_capability_requirement_v1` (0x0a),
// `verify_member_capability_v1` (0x0b), `resolve_dispute_v1` (0x0c) and
// `deactivate_capability_requirement_v1` (0x10). Their Rust param structs are gone, so a binding for
// them could not decode anything; each kept a `pub use` and an `add_class` naming a type that no longer
// existed.

/// [`DaoEscrowFunction::ProposeClaimV1`] function call parameter's python bindings.
pub mod propose_claim_v1;
pub use propose_claim_v1::DaoEscrowProposeClaimParamsV1;

/// [`DaoEscrowFunction::VoteClaimV1`] function call parameter's python bindings.
pub mod vote_claim_v1;
pub use vote_claim_v1::DaoEscrowVoteClaimParamsV1;

/// [`DaoEscrowFunction::ExecuteClaimV1`] function call parameter's python bindings.
pub mod execute_claim_v1;
pub use execute_claim_v1::DaoEscrowExecuteClaimParamsV1;

/// [`DaoEscrowFunction::CancelClaimV1`] function call parameter's python bindings.
pub mod cancel_claim_v1;
pub use cancel_claim_v1::DaoEscrowCancelClaimParamsV1;

/// Decodes the parameters of a DAO-Escrow contract function call.
pub fn decode_dao_escrow_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match DaoEscrowFunction::try_from(function_index)? {
        DaoEscrowFunction::InitializeV1 => {
            let params = dao_escrow_model::InitializeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::UpdateV1 => {
            let params = dao_escrow_model::UpdateParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::PayPremiumV1 => {
            let params = dao_escrow_model::PayPremiumParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let params = dao_escrow_model::WithdrawParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            let params = dao_escrow_model::EndowmentWithdrawParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            let params = dao_escrow_model::TreasurySpendParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::ProposeClaimV1 => {
            let params = dao_escrow_model::ProposeClaimParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::VoteClaimV1 => {
            let params = dao_escrow_model::VoteClaimParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::ExecuteClaimV1 => {
            let params = dao_escrow_model::ExecuteClaimParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DaoEscrowFunction::CancelClaimV1 => {
            let params = dao_escrow_model::CancelClaimParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        _ => return Err(dwow_core::Error::ParseFailed("unsupported DAO-Escrow function")),
    };

    Ok(res)
}

/// Create dao_escrow module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "dao_escrow")?;

    submod.add_class::<DaoEscrowInitializeParamsV1>()?;
    submod.add_class::<DaoEscrowUpdateParamsV1>()?;
    submod.add_class::<DaoEscrowPayPremiumParamsV1>()?;
    submod.add_class::<DaoEscrowWithdrawParamsV1>()?;
    submod.add_class::<DaoEscrowEndowmentWithdrawParamsV1>()?;
    submod.add_class::<DaoEscrowTreasurySpendParamsV1>()?;
    submod.add_class::<DaoEscrowProposeClaimParamsV1>()?;
    submod.add_class::<DaoEscrowVoteClaimParamsV1>()?;
    submod.add_class::<DaoEscrowExecuteClaimParamsV1>()?;
    submod.add_class::<DaoEscrowCancelClaimParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.dao_escrow", &submod)?;

    Ok(submod)
}
