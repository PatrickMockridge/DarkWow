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

/// [`insurance_market_model::UnderwriteWithCapabilityParamsV1`] python binding.
#[pyclass]
pub struct InsuranceMarketUnderwriteWithCapabilityParamsV1(
    insurance_market_model::UnderwriteWithCapabilityParamsV1,
);
impl_py_methods!(InsuranceMarketUnderwriteWithCapabilityParamsV1);

impl FunctionParams for insurance_market_model::UnderwriteWithCapabilityParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("market_id", format!("{:?}", self.market_id))?;
        dict.set_item("bond_amount", format!("{:?}", self.bond_amount))?;
        dict.set_item("coverage_limit", format!("{:?}", self.coverage_limit))?;
        dict.set_item("underwriter", self.underwriter.to_string())?;
        dict.set_item("capability_proof", format!("{:?}", self.capability_proof))?;
        dict.set_item("capability_secret", format!("{:?}", self.capability_secret))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}market_id: {:?}", self.market_id).unwrap();
        writeln!(out, "{prefix}bond_amount: {:?}", self.bond_amount).unwrap();
        writeln!(out, "{prefix}coverage_limit: {:?}", self.coverage_limit).unwrap();
        writeln!(out, "{prefix}underwriter: {}", self.underwriter).unwrap();
        writeln!(out, "{prefix}capability_proof: {:?}", self.capability_proof).unwrap();
        writeln!(out, "{prefix}capability_secret: {:?}", self.capability_secret).unwrap();
        Ok(())
    }
}
