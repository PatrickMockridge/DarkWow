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

/// [`auction_model::ClaimWinningsParamsV1`] python binding.
#[pyclass]
pub struct AuctionClaimWinningsParamsV1(auction_model::ClaimWinningsParamsV1);
impl_py_methods!(AuctionClaimWinningsParamsV1);

impl FunctionParams for auction_model::ClaimWinningsParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("auction_id", format!("{:?}", self.auction_id))?;
        dict.set_item("winner_bid_id", format!("{:?}", self.winner_bid_id))?;
        dict.set_item("winner_pubkey", format!("{:?}", self.winner_pubkey))?;
        dict.set_item("winner_secret", format!("{:?}", self.winner_secret))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}auction_id: {:?}", self.auction_id).unwrap();
        writeln!(out, "{prefix}winner_bid_id: {:?}", self.winner_bid_id).unwrap();
        writeln!(out, "{prefix}winner_pubkey: {:?}", self.winner_pubkey).unwrap();
        writeln!(out, "{prefix}winner_secret: {:?}", self.winner_secret).unwrap();
        Ok(())
    }
}
