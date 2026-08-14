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

/// [`auction_model::CreateAuctionParamsV1`] python binding.
#[pyclass]
pub struct AuctionCreateAuctionParamsV1(auction_model::CreateAuctionParamsV1);
impl_py_methods!(AuctionCreateAuctionParamsV1);

impl FunctionParams for auction_model::CreateAuctionParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("seller_pubkey", format!("{:?}", self.seller_pubkey))?;
        dict.set_item("item_commitment", format!("{:?}", self.item_commitment))?;
        dict.set_item("reserve_price", format!("{:?}", self.reserve_price))?;
        dict.set_item("token_id", format!("{:?}", self.token_id))?;
        dict.set_item("deadline_block", format!("{:?}", self.deadline_block))?;
        dict.set_item("auction_id", format!("{:?}", self.auction_id))?;
        dict.set_item("seller_commitment", format!("{:?}", self.seller_commitment))?;
        dict.set_item("merkle_proof", format!("{:?}", self.merkle_proof))?;
        dict.set_item("merkle_root", format!("{:?}", self.merkle_root))?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}seller_pubkey: {:?}", self.seller_pubkey).unwrap();
        writeln!(out, "{prefix}item_commitment: {:?}", self.item_commitment).unwrap();
        writeln!(out, "{prefix}reserve_price: {:?}", self.reserve_price).unwrap();
        writeln!(out, "{prefix}token_id: {:?}", self.token_id).unwrap();
        writeln!(out, "{prefix}deadline_block: {:?}", self.deadline_block).unwrap();
        writeln!(out, "{prefix}auction_id: {:?}", self.auction_id).unwrap();
        writeln!(out, "{prefix}seller_commitment: {:?}", self.seller_commitment).unwrap();
        writeln!(out, "{prefix}merkle_proof: {:?}", self.merkle_proof).unwrap();
        writeln!(out, "{prefix}merkle_root: {:?}", self.merkle_root).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
