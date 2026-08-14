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

use dwow_darkbet_exchange_contract::{model as darkbet_exchange_model, DarkbetFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`DarkbetFunction::CreateMarketV1`] function call parameter's python bindings.
pub mod create_market_v1;
pub use create_market_v1::DarkbetCreateMarketParamsV1;

/// [`DarkbetFunction::PlaceBackV1`] function call parameter's python bindings.
pub mod place_back_v1;
pub use place_back_v1::DarkbetPlaceBackParamsV1;

/// [`DarkbetFunction::PlaceLayV1`] function call parameter's python bindings.
pub mod place_lay_v1;
pub use place_lay_v1::DarkbetPlaceLayParamsV1;

/// [`DarkbetFunction::MatchOrdersV1`] function call parameter's python bindings.
pub mod match_orders_v1;
pub use match_orders_v1::DarkbetMatchOrdersParamsV1;

/// [`DarkbetFunction::BuyPositionV1`] function call parameter's python bindings.
pub mod buy_position_v1;
pub use buy_position_v1::DarkbetBuyPositionParamsV1;

/// [`DarkbetFunction::AddLiquidityV1`] function call parameter's python bindings.
pub mod add_liquidity_v1;
pub use add_liquidity_v1::DarkbetAddLiquidityParamsV1;

/// [`DarkbetFunction::RemoveLiquidityV1`] function call parameter's python bindings.
pub mod remove_liquidity_v1;
pub use remove_liquidity_v1::DarkbetRemoveLiquidityParamsV1;

/// [`DarkbetFunction::ClaimWinningsV1`] function call parameter's python bindings.
pub mod claim_winnings_v1;
pub use claim_winnings_v1::DarkbetClaimWinningsParamsV1;

/// [`DarkbetFunction::ResolveMarketV1`] function call parameter's python bindings.
pub mod resolve_market_v1;
pub use resolve_market_v1::DarkbetResolveMarketParamsV1;

/// [`DarkbetFunction::SettleMarketV1`] function call parameter's python bindings.
pub mod settle_market_v1;
pub use settle_market_v1::DarkbetSettleMarketParamsV1;

/// [`DarkbetFunction::CancelOrderV1`] function call parameter's python bindings.
pub mod cancel_order_v1;
pub use cancel_order_v1::DarkbetCancelOrderParamsV1;

/// Decodes the parameters of a Darkbet Exchange contract function call.
pub fn decode_darkbet_exchange_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match DarkbetFunction::try_from(function_index)? {
        DarkbetFunction::CreateMarketV1 => {
            let params = darkbet_exchange_model::CreateMarketParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::PlaceBackV1 => {
            let params = darkbet_exchange_model::PlaceBackParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::PlaceLayV1 => {
            let params = darkbet_exchange_model::PlaceLayParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::MatchOrdersV1 => {
            let params = darkbet_exchange_model::MatchOrdersParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::BuyPositionV1 => {
            let params = darkbet_exchange_model::BuyPositionParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::AddLiquidityV1 => {
            let params = darkbet_exchange_model::AddLiquidityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::RemoveLiquidityV1 => {
            let params = darkbet_exchange_model::RemoveLiquidityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::ClaimWinningsV1 => {
            let params = darkbet_exchange_model::ClaimWinningsParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::ResolveMarketV1 => {
            let params = darkbet_exchange_model::ResolveMarketParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::SettleMarketV1 => {
            let params = darkbet_exchange_model::SettleMarketParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        DarkbetFunction::CancelOrderV1 => {
            let params = darkbet_exchange_model::CancelOrderParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create darkbet_exchange module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "darkbet_exchange")?;

    submod.add_class::<DarkbetCreateMarketParamsV1>()?;
    submod.add_class::<DarkbetPlaceBackParamsV1>()?;
    submod.add_class::<DarkbetPlaceLayParamsV1>()?;
    submod.add_class::<DarkbetMatchOrdersParamsV1>()?;
    submod.add_class::<DarkbetBuyPositionParamsV1>()?;
    submod.add_class::<DarkbetAddLiquidityParamsV1>()?;
    submod.add_class::<DarkbetRemoveLiquidityParamsV1>()?;
    submod.add_class::<DarkbetClaimWinningsParamsV1>()?;
    submod.add_class::<DarkbetResolveMarketParamsV1>()?;
    submod.add_class::<DarkbetSettleMarketParamsV1>()?;
    submod.add_class::<DarkbetCancelOrderParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.darkbet_exchange", &submod)?;

    Ok(submod)
}
