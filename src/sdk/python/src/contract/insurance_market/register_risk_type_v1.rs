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

/// [`insurance_market_model::RegisterRiskTypeParamsV1`] python binding.
#[pyclass]
pub struct InsuranceMarketRegisterRiskTypeParamsV1(insurance_market_model::RegisterRiskTypeParamsV1);
impl_py_methods!(InsuranceMarketRegisterRiskTypeParamsV1);

impl FunctionParams for insurance_market_model::RegisterRiskTypeParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("category", format!("{:?}", self.category))?;
        dict.set_item("description", format!("{:?}", self.description))?;
        dict.set_item("base_premium_rate", format!("{:?}", self.base_premium_rate))?;
        dict.set_item("min_bond_rate", format!("{:?}", self.min_bond_rate))?;
        dict.set_item("oracle_pubkey", self.oracle_pubkey.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}category: {:?}", self.category).unwrap();
        writeln!(out, "{prefix}description: {:?}", self.description).unwrap();
        writeln!(out, "{prefix}base_premium_rate: {:?}", self.base_premium_rate).unwrap();
        writeln!(out, "{prefix}min_bond_rate: {:?}", self.min_bond_rate).unwrap();
        writeln!(out, "{prefix}oracle_pubkey: {}", self.oracle_pubkey).unwrap();
        Ok(())
    }
}
