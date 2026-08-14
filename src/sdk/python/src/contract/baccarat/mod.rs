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

use dwow_baccarat_contract::{model as baccarat_model, BaccaratFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`BaccaratFunction::CommitBetV1`] function call parameter's bindings.
pub mod commit_bet_v1;
pub use commit_bet_v1::BaccaratCommitBetParamsV1;

/// [`BaccaratFunction::DrawCardsV1`] function call parameter's bindings.
pub mod draw_cards_v1;
pub use draw_cards_v1::BaccaratDrawCardsParamsV1;

/// [`BaccaratFunction::SettleBetV1`] function call parameter's bindings.
pub mod settle_bet_v1;
pub use settle_bet_v1::BaccaratSettleBetParamsV1;

/// [`BaccaratFunction::HouseCloseV1`] function call parameter's bindings.
pub mod house_close_v1;
pub use house_close_v1::BaccaratHouseCloseParamsV1;

/// Decodes the parameters of a Baccarat contract function call.
pub fn decode_baccarat_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match BaccaratFunction::try_from(function_index)? {
        BaccaratFunction::CommitBetV1 => {
            let params = baccarat_model::CommitBetParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BaccaratFunction::DrawCardsV1 => {
            let params = baccarat_model::DrawCardsParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BaccaratFunction::SettleBetV1 => {
            let params = baccarat_model::SettleBetParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        BaccaratFunction::HouseCloseV1 => {
            let params = baccarat_model::HouseCloseParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        _ => return Err(dwow_core::Error::ParseFailed("unsupported Baccarat function")),
    };

    Ok(res)
}

/// Create baccarat module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "baccarat")?;

    submod.add_class::<BaccaratCommitBetParamsV1>()?;
    submod.add_class::<BaccaratDrawCardsParamsV1>()?;
    submod.add_class::<BaccaratSettleBetParamsV1>()?;
    submod.add_class::<BaccaratHouseCloseParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.baccarat", &submod)?;

    Ok(submod)
}
