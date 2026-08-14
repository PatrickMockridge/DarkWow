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

use dwow_labor_market_contract::model as labor_market_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`labor_market_model::ConfirmMilestoneParamsV1`] python binding.
#[pyclass]
pub struct LaborMarketConfirmMilestoneParamsV1(labor_market_model::ConfirmMilestoneParamsV1);
impl_py_methods!(LaborMarketConfirmMilestoneParamsV1);

impl FunctionParams for labor_market_model::ConfirmMilestoneParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("job_id", format!("{:?}", self.job_id))?;
        dict.set_item("milestone_index", format!("{:?}", self.milestone_index))?;
        dict.set_item("employer_pub_x", format!("{:?}", self.employer_pub_x))?;
        dict.set_item("employer_pub_y", format!("{:?}", self.employer_pub_y))?;
        dict.set_item("payment_release", format!("{:?}", self.payment_release))?;
        dict.set_item("spent_nullifier", format!("{:?}", self.spent_nullifier))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}job_id: {:?}", self.job_id).unwrap();
        writeln!(out, "{prefix}milestone_index: {:?}", self.milestone_index).unwrap();
        writeln!(out, "{prefix}employer_pub_x: {:?}", self.employer_pub_x).unwrap();
        writeln!(out, "{prefix}employer_pub_y: {:?}", self.employer_pub_y).unwrap();
        writeln!(out, "{prefix}payment_release: {:?}", self.payment_release).unwrap();
        writeln!(out, "{prefix}spent_nullifier: {:?}", self.spent_nullifier).unwrap();
        Ok(())
    }
}
