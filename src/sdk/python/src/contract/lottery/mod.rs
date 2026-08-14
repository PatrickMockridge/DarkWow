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

use dwow_lottery_contract::{model as lottery_model, LotteryFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`LotteryFunction::InitializeV1`] function call parameter's bindings.
pub mod initialize_v1;
pub use initialize_v1::LotteryInitializeParamsV1;

/// [`LotteryFunction::BuyTicketV1`] function call parameter's bindings.
pub mod buy_ticket_v1;
pub use buy_ticket_v1::LotteryBuyTicketParamsV1;

/// [`LotteryFunction::DrawWinnersV1`] function call parameter's bindings.
pub mod draw_winners_v1;
pub use draw_winners_v1::LotteryDrawWinnersParamsV1;

/// [`LotteryFunction::RevealTicketV1`] function call parameter's bindings.
pub mod reveal_ticket_v1;
pub use reveal_ticket_v1::LotteryRevealTicketParamsV1;

/// [`LotteryFunction::ClaimPrizeV1`] function call parameter's bindings.
pub mod claim_prize_v1;
pub use claim_prize_v1::LotteryClaimPrizeParamsV1;

/// [`LotteryFunction::ExpireLotteryV1`] function call parameter's bindings.
pub mod expire_lottery_v1;
pub use expire_lottery_v1::LotteryExpireLotteryParamsV1;

/// Decodes the parameters of a Lottery contract function call.
pub fn decode_lottery_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match LotteryFunction::try_from(function_index)? {
        LotteryFunction::InitializeV1 => {
            let params = lottery_model::InitializeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LotteryFunction::BuyTicketV1 => {
            let params = lottery_model::BuyTicketParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LotteryFunction::DrawWinnersV1 => {
            let params = lottery_model::DrawWinnersParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LotteryFunction::RevealTicketV1 => {
            let params = lottery_model::RevealTicketParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LotteryFunction::ClaimPrizeV1 => {
            let params = lottery_model::ClaimPrizeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        LotteryFunction::ExpireLotteryV1 => {
            let params = lottery_model::ExpireLotteryParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create lottery module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "lottery")?;

    submod.add_class::<LotteryInitializeParamsV1>()?;
    submod.add_class::<LotteryBuyTicketParamsV1>()?;
    submod.add_class::<LotteryDrawWinnersParamsV1>()?;
    submod.add_class::<LotteryRevealTicketParamsV1>()?;
    submod.add_class::<LotteryClaimPrizeParamsV1>()?;
    submod.add_class::<LotteryExpireLotteryParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.lottery", &submod)?;

    Ok(submod)
}
