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

use dwow_game_room_contract::{model as game_room_model, GameRoomFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`GameRoomFunction::CreateRoomV1`] function call parameter's python bindings.
pub mod create_room_v1;
pub use create_room_v1::GameRoomCreateRoomParamsV1;

/// [`GameRoomFunction::DepositV1`] function call parameter's python bindings.
pub mod deposit_v1;
pub use deposit_v1::GameRoomDepositParamsV1;

/// [`GameRoomFunction::WithdrawV1`] function call parameter's python bindings.
pub mod withdraw_v1;
pub use withdraw_v1::GameRoomWithdrawParamsV1;

/// [`GameRoomFunction::PlaceBetV1`] function call parameter's python bindings.
pub mod place_bet_v1;
pub use place_bet_v1::GameRoomPlaceBetParamsV1;

/// [`GameRoomFunction::RaiseV1`] function call parameter's python bindings.
pub mod raise_v1;
pub use raise_v1::GameRoomRaiseParamsV1;

/// [`GameRoomFunction::CallV1`] function call parameter's python bindings.
pub mod call_v1;
pub use call_v1::GameRoomCallParamsV1;

/// [`GameRoomFunction::FoldV1`] function call parameter's python bindings.
pub mod fold_v1;
pub use fold_v1::GameRoomFoldParamsV1;

/// [`GameRoomFunction::ClosePotV1`] function call parameter's python bindings.
pub mod close_pot_v1;
pub use close_pot_v1::GameRoomClosePotParamsV1;

/// [`GameRoomFunction::SettlePotV1`] function call parameter's python bindings.
pub mod settle_pot_v1;
pub use settle_pot_v1::GameRoomSettlePotParamsV1;

/// [`GameRoomFunction::ContributeEntropyV1`] function call parameter's python bindings.
pub mod contribute_entropy_v1;
pub use contribute_entropy_v1::GameRoomContributeEntropyParamsV1;

/// [`GameRoomFunction::ClaimV1`] function call parameter's python bindings.
pub mod claim_v1;
pub use claim_v1::GameRoomClaimParamsV1;

/// Decodes the parameters of a Game Room contract function call.
pub fn decode_game_room_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match GameRoomFunction::try_from(function_index)
        .map_err(|e| dwow_core::Error::ContractError(e.into()))?
    {
        GameRoomFunction::CreateRoomV1 => {
            let params = game_room_model::CreateRoomParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::DepositV1 => {
            let params = game_room_model::DepositParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::WithdrawV1 => {
            let params = game_room_model::WithdrawParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::PlaceBetV1 => {
            let params = game_room_model::PlaceBetParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::RaiseV1 => {
            let params = game_room_model::RaiseParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::CallV1 => {
            let params = game_room_model::CallParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::FoldV1 => {
            let params = game_room_model::FoldParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::ClosePotV1 => {
            let params = game_room_model::ClosePotParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::SettlePotV1 => {
            let params = game_room_model::SettlePotParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::ContributeEntropyV1 => {
            let params = game_room_model::ContributeEntropyParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        GameRoomFunction::ClaimV1 => {
            let params = game_room_model::ClaimParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create game_room module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "game_room")?;

    submod.add_class::<GameRoomCreateRoomParamsV1>()?;
    submod.add_class::<GameRoomDepositParamsV1>()?;
    submod.add_class::<GameRoomWithdrawParamsV1>()?;
    submod.add_class::<GameRoomPlaceBetParamsV1>()?;
    submod.add_class::<GameRoomRaiseParamsV1>()?;
    submod.add_class::<GameRoomCallParamsV1>()?;
    submod.add_class::<GameRoomFoldParamsV1>()?;
    submod.add_class::<GameRoomClosePotParamsV1>()?;
    submod.add_class::<GameRoomSettlePotParamsV1>()?;
    submod.add_class::<GameRoomContributeEntropyParamsV1>()?;
    submod.add_class::<GameRoomClaimParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.game_room", &submod)?;

    Ok(submod)
}
