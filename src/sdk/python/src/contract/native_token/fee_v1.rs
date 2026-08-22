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

use dwow_native_token_contract::model as native_token_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`native_token_model::FeeParamsV3`] python binding.
#[pyclass]
pub struct FeeParamsV3(native_token_model::FeeParamsV3);
impl_py_methods!(FeeParamsV3);

impl FunctionParams for native_token_model::FeeParamsV3 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let res = PyDict::new(py);
        res.set_item("input", self.input.to_pydict(py)?)?;
        res.set_item("output", self.output.to_pydict(py)?)?;
        res.set_item("fee", self.fee.get())?;
        res.set_item("tier", self.tier.tier_multiplier())?;
        Ok(res.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}input:").unwrap();
        self.input.fmt_pretty(out, depth + 2)?;

        writeln!(out, "{prefix}output:").unwrap();
        self.output.fmt_pretty(out, depth + 2)?;

        writeln!(out, "{prefix}fee: {}", self.fee).unwrap();
        writeln!(out, "{prefix}tier: {}", self.tier.tier_multiplier()).unwrap();
        Ok(())
    }
}
