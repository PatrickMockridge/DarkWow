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

/// [`lottery_model::RevealTicketParamsV1`] python binding.
#[pyclass]
pub struct LotteryRevealTicketParamsV1(lottery_model::RevealTicketParamsV1);
impl_py_methods!(LotteryRevealTicketParamsV1);

impl FunctionParams for lottery_model::RevealTicketParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("ticket_id", format!("{:?}", self.ticket_id))?;
        dict.set_item("numbers", format!("{:?}", self.numbers))?;
        dict.set_item("nonce", format!("{:?}", self.nonce))?;
        dict.set_item("revealed_commitment", format!("{:?}", self.revealed_commitment))?;
        dict.set_item("matches", self.matches)?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}ticket_id: {:?}", self.ticket_id).unwrap();
        writeln!(out, "{prefix}numbers: {:?}", self.numbers).unwrap();
        writeln!(out, "{prefix}nonce: {:?}", self.nonce).unwrap();
        writeln!(out, "{prefix}revealed_commitment: {:?}", self.revealed_commitment).unwrap();
        writeln!(out, "{prefix}matches: {}", self.matches).unwrap();
        Ok(())
    }
}
