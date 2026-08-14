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

use dwow_auction_contract::{model as auction_model, AuctionFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`AuctionFunction::CreateAuctionV1`] function call parameter's bindings.
pub mod create_auction_v1;
pub use create_auction_v1::AuctionCreateAuctionParamsV1;

/// [`AuctionFunction::PlaceBidV1`] function call parameter's bindings.
pub mod place_bid_v1;
pub use place_bid_v1::AuctionPlaceBidParamsV1;

/// [`AuctionFunction::CloseAuctionV1`] function call parameter's bindings.
pub mod close_auction_v1;
pub use close_auction_v1::AuctionCloseAuctionParamsV1;

/// [`AuctionFunction::ClaimWinningsV1`] function call parameter's bindings.
pub mod claim_winnings_v1;
pub use claim_winnings_v1::AuctionClaimWinningsParamsV1;

/// [`AuctionFunction::SettleAuctionV1`] function call parameter's bindings.
pub mod settle_auction_v1;
pub use settle_auction_v1::AuctionSettleAuctionParamsV1;

/// [`AuctionFunction::RefundBidV1`] function call parameter's bindings.
pub mod refund_bid_v1;
pub use refund_bid_v1::AuctionRefundBidParamsV1;

/// Decodes the parameters of an Auction contract function call.
pub fn decode_auction_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match AuctionFunction::try_from(function_index)? {
        AuctionFunction::CreateAuctionV1 => {
            let params = auction_model::CreateAuctionParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        AuctionFunction::PlaceBidV1 => {
            let params = auction_model::PlaceBidParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        AuctionFunction::CloseAuctionV1 => {
            let params = auction_model::CloseAuctionParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        AuctionFunction::ClaimWinningsV1 => {
            let params = auction_model::ClaimWinningsParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        AuctionFunction::SettleAuctionV1 => {
            let params = auction_model::SettleAuctionParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        AuctionFunction::RefundBidV1 => {
            let params = auction_model::RefundBidParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create auction module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "auction")?;

    submod.add_class::<AuctionCreateAuctionParamsV1>()?;
    submod.add_class::<AuctionPlaceBidParamsV1>()?;
    submod.add_class::<AuctionCloseAuctionParamsV1>()?;
    submod.add_class::<AuctionClaimWinningsParamsV1>()?;
    submod.add_class::<AuctionSettleAuctionParamsV1>()?;
    submod.add_class::<AuctionRefundBidParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.auction", &submod)?;

    Ok(submod)
}
