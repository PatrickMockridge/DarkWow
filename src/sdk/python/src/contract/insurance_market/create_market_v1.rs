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

use dwow_insurance_market_contract::model as insurance_market_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`insurance_market_model::CreateMarketParamsV1`] python binding.
#[pyclass]
pub struct InsuranceMarketCreateMarketParamsV1(insurance_market_model::CreateMarketParamsV1);
impl_py_methods!(InsuranceMarketCreateMarketParamsV1);

impl FunctionParams for insurance_market_model::CreateMarketParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("risk_type_id", format!("{:?}", self.risk_type_id))?;
        dict.set_item("initial_premium_rate", format!("{:?}", self.initial_premium_rate))?;
        dict.set_item("total_coverage", format!("{:?}", self.total_coverage))?;
        dict.set_item("coverage_period", format!("{:?}", self.coverage_period))?;
        dict.set_item("deductible", format!("{:?}", self.deductible))?;
        dict.set_item("max_coverage_per_buyer", format!("{:?}", self.max_coverage_per_buyer))?;
        dict.set_item("closes_at", format!("{:?}", self.closes_at))?;
        dict.set_item("required_underwriter_capability", format!("{:?}", self.required_underwriter_capability))?;
        dict.set_item("required_buyer_capability", format!("{:?}", self.required_buyer_capability))?;
        dict.set_item("required_dag_id", format!("{:?}", self.required_dag_id))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}risk_type_id: {:?}", self.risk_type_id).unwrap();
        writeln!(out, "{prefix}initial_premium_rate: {:?}", self.initial_premium_rate).unwrap();
        writeln!(out, "{prefix}total_coverage: {:?}", self.total_coverage).unwrap();
        writeln!(out, "{prefix}coverage_period: {:?}", self.coverage_period).unwrap();
        writeln!(out, "{prefix}deductible: {:?}", self.deductible).unwrap();
        writeln!(out, "{prefix}max_coverage_per_buyer: {:?}", self.max_coverage_per_buyer).unwrap();
        writeln!(out, "{prefix}closes_at: {:?}", self.closes_at).unwrap();
        writeln!(out, "{prefix}required_underwriter_capability: {:?}", self.required_underwriter_capability).unwrap();
        writeln!(out, "{prefix}required_buyer_capability: {:?}", self.required_buyer_capability).unwrap();
        writeln!(out, "{prefix}required_dag_id: {:?}", self.required_dag_id).unwrap();
        Ok(())
    }
}
