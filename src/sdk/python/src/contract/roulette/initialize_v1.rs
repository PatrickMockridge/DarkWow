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

/// [`roulette_model::InitializeParamsV1`] python binding.
#[pyclass]
pub struct RouletteInitializeParamsV1(roulette_model::InitializeParamsV1);
impl_py_methods!(RouletteInitializeParamsV1);

impl FunctionParams for roulette_model::InitializeParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("house_pub", format!("{:?}", self.house_pub))?;
        dict.set_item("american_wheel", self.american_wheel)?;
        dict.set_item("house_capital", self.house_capital)?;
        dict.set_item("max_straight_bet", self.max_straight_bet)?;
        dict.set_item("duration_blocks", self.duration_blocks)?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}house_pub: {:?}", self.house_pub).unwrap();
        writeln!(out, "{prefix}american_wheel: {}", self.american_wheel).unwrap();
        writeln!(out, "{prefix}house_capital: {}", self.house_capital).unwrap();
        writeln!(out, "{prefix}max_straight_bet: {}", self.max_straight_bet).unwrap();
        writeln!(out, "{prefix}duration_blocks: {}", self.duration_blocks).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
