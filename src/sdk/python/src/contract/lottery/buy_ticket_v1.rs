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

use dwow_lottery_contract::model as lottery_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`lottery_model::BuyTicketParamsV1`] python binding.
#[pyclass]
pub struct LotteryBuyTicketParamsV1(lottery_model::BuyTicketParamsV1);
impl_py_methods!(LotteryBuyTicketParamsV1);

impl FunctionParams for lottery_model::BuyTicketParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("player_pub", format!("{:?}", self.player_pub))?;
        dict.set_item("commitment", format!("{:?}", self.commitment))?;
        dict.set_item("asset_id", format!("{:?}", self.asset_id))?;
        dict.set_item("value", self.value)?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("signature", format!("{:?}", self.signature))?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}player_pub: {:?}", self.player_pub).unwrap();
        writeln!(out, "{prefix}commitment: {:?}", self.commitment).unwrap();
        writeln!(out, "{prefix}asset_id: {:?}", self.asset_id).unwrap();
        writeln!(out, "{prefix}value: {}", self.value).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}signature: {:?}", self.signature).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
