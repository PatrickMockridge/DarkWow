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

use std::fmt::Write;

use dwow_auction_contract::model as auction_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`auction_model::PlaceBidParamsV1`] python binding.
#[pyclass]
pub struct AuctionPlaceBidParamsV1(auction_model::PlaceBidParamsV1);
impl_py_methods!(AuctionPlaceBidParamsV1);

impl FunctionParams for auction_model::PlaceBidParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("auction_id", format!("{:?}", self.auction_id))?;
        dict.set_item("bidder_pubkey", format!("{:?}", self.bidder_pubkey))?;
        dict.set_item("amount", format!("{:?}", self.amount))?;
        dict.set_item("bid_nonce", format!("{:?}", self.bid_nonce))?;
        dict.set_item("bid_id", format!("{:?}", self.bid_id))?;
        dict.set_item("escrow_id", format!("{:?}", self.escrow_id))?;
        dict.set_item("current_high_bid", format!("{:?}", self.current_high_bid))?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}auction_id: {:?}", self.auction_id).unwrap();
        writeln!(out, "{prefix}bidder_pubkey: {:?}", self.bidder_pubkey).unwrap();
        writeln!(out, "{prefix}amount: {:?}", self.amount).unwrap();
        writeln!(out, "{prefix}bid_nonce: {:?}", self.bid_nonce).unwrap();
        writeln!(out, "{prefix}bid_id: {:?}", self.bid_id).unwrap();
        writeln!(out, "{prefix}escrow_id: {:?}", self.escrow_id).unwrap();
        writeln!(out, "{prefix}current_high_bid: {:?}", self.current_high_bid).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
