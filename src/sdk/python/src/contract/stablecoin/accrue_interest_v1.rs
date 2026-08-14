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

use dwow_stablecoin_contract::model as stablecoin_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`stablecoin_model::AccrueInterestParams`] python binding.
#[pyclass]
pub struct StablecoinAccrueInterestParamsV1(stablecoin_model::AccrueInterestParams);
impl_py_methods!(StablecoinAccrueInterestParamsV1);

impl FunctionParams for stablecoin_model::AccrueInterestParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("old_total_debt", format!("{:?}", self.old_total_debt))?;
        dict.set_item("new_total_debt", format!("{:?}", self.new_total_debt))?;
        dict.set_item("interest_amount", format!("{:?}", self.interest_amount))?;
        dict.set_item("rate_per_second", format!("{:?}", self.rate_per_second))?;
        dict.set_item("time_elapsed", format!("{:?}", self.time_elapsed))?;
        dict.set_item("accumulator_pub", format!("{:?}", self.accumulator_pub))?;
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}old_total_debt: {:?}", self.old_total_debt).unwrap();
        writeln!(out, "{prefix}new_total_debt: {:?}", self.new_total_debt).unwrap();
        writeln!(out, "{prefix}interest_amount: {:?}", self.interest_amount).unwrap();
        writeln!(out, "{prefix}rate_per_second: {:?}", self.rate_per_second).unwrap();
        writeln!(out, "{prefix}time_elapsed: {:?}", self.time_elapsed).unwrap();
        writeln!(out, "{prefix}accumulator_pub: {:?}", self.accumulator_pub).unwrap();
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        Ok(())
    }
}
