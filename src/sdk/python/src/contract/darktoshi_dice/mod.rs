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

use dwow_darktoshi_dice_contract::{model as darktoshi_dice_model, DiceFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`DiceFunction::CommitBetV1`] function call parameter's bindings.
pub mod commit_bet_v1;
pub use commit_bet_v1::DiceCommitBetParamsV1;

/// [`DiceFunction::RevealRollV1`] function call parameter's bindings.
pub mod reveal_roll_v1;
pub use reveal_roll_v1::DiceRevealRollParamsV1;

/// [`DiceFunction::SettleBetV1`] function call parameter's bindings.
pub mod settle_bet_v1;
pub use settle_bet_v1::DiceSettleBetParamsV1;

/// [`DiceFunction::HouseCloseV1`] function call parameter's bindings.
pub mod house_close_v1;
pub use house_close_v1::DiceHouseCloseParamsV1;

/// Decodes the parameters of a DarkToshi Dice contract function call.
pub fn decode_darktoshi_dice_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match DiceFunction::try_from(function_index)? {
        DiceFunction::CommitBetV1 => {
            let params = darktoshi_dice_model::CommitBetParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DiceFunction::RevealRollV1 => {
            let params = darktoshi_dice_model::RevealRollParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DiceFunction::SettleBetV1 => {
            let params = darktoshi_dice_model::SettleBetParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DiceFunction::HouseCloseV1 => {
            let params = darktoshi_dice_model::HouseCloseParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        _ => return Err(dwow_core::Error::ParseFailed("unsupported DarkToshi Dice function")),
    };

    Ok(res)
}

/// Create darktoshi_dice module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "darktoshi_dice")?;

    submod.add_class::<DiceCommitBetParamsV1>()?;
    submod.add_class::<DiceRevealRollParamsV1>()?;
    submod.add_class::<DiceSettleBetParamsV1>()?;
    submod.add_class::<DiceHouseCloseParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.darktoshi_dice", &submod)?;

    Ok(submod)
}
