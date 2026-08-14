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

use dwow_roulette_contract::model as roulette_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`roulette_model::PlaceBetParamsV1`] python binding.
#[pyclass]
pub struct RoulettePlaceBetParamsV1(roulette_model::PlaceBetParamsV1);
impl_py_methods!(RoulettePlaceBetParamsV1);

impl FunctionParams for roulette_model::PlaceBetParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("table_id", format!("{:?}", self.table_id))?;
        dict.set_item("player_pub", format!("{:?}", self.player_pub))?;
        dict.set_item("bet_type", format!("{:?}", self.bet_type))?;
        dict.set_item("numbers", format!("{:?}", self.numbers))?;
        dict.set_item("amount", self.amount)?;
        dict.set_item("signature", format!("{:?}", self.signature))?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}table_id: {:?}", self.table_id).unwrap();
        writeln!(out, "{prefix}player_pub: {:?}", self.player_pub).unwrap();
        writeln!(out, "{prefix}bet_type: {:?}", self.bet_type).unwrap();
        writeln!(out, "{prefix}numbers: {:?}", self.numbers).unwrap();
        writeln!(out, "{prefix}amount: {}", self.amount).unwrap();
        writeln!(out, "{prefix}signature: {:?}", self.signature).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
