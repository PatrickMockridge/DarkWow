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

use dwow_roulette_contract::{model as roulette_model, RouletteFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`RouletteFunction::InitializeV1`] function call parameter's bindings.
pub mod initialize_v1;
pub use initialize_v1::RouletteInitializeParamsV1;

/// [`RouletteFunction::PlaceBetV1`] function call parameter's bindings.
pub mod place_bet_v1;
pub use place_bet_v1::RoulettePlaceBetParamsV1;

/// [`RouletteFunction::SpinWheelV1`] function call parameter's bindings.
pub mod spin_wheel_v1;
pub use spin_wheel_v1::RouletteSpinWheelParamsV1;

/// [`RouletteFunction::SettleBetsV1`] function call parameter's bindings.
pub mod settle_bets_v1;
pub use settle_bets_v1::RouletteSettleBetsParamsV1;

/// [`RouletteFunction::HouseCloseV1`] function call parameter's bindings.
pub mod house_close_v1;
pub use house_close_v1::RouletteHouseCloseParamsV1;

/// Decodes the parameters of a Roulette contract function call.
pub fn decode_roulette_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let func = RouletteFunction::try_from(function_index)
        .map_err(|_| dwow_core::Error::ParseFailed("unsupported Roulette function"))?;

    let res: Box<dyn FunctionParams> = match func {
        RouletteFunction::InitializeV1 => {
            let params = roulette_model::InitializeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RouletteFunction::PlaceBetV1 => {
            let params = roulette_model::PlaceBetParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RouletteFunction::SpinWheelV1 => {
            let params = roulette_model::SpinWheelParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RouletteFunction::SettleBetsV1 => {
            let params = roulette_model::SettleBetsParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        RouletteFunction::HouseCloseV1 => {
            let params = roulette_model::HouseCloseParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create roulette module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "roulette")?;

    submod.add_class::<RouletteInitializeParamsV1>()?;
    submod.add_class::<RoulettePlaceBetParamsV1>()?;
    submod.add_class::<RouletteSpinWheelParamsV1>()?;
    submod.add_class::<RouletteSettleBetsParamsV1>()?;
    submod.add_class::<RouletteHouseCloseParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.roulette", &submod)?;

    Ok(submod)
}
